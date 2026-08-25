//! Durable export of the kernel journal.
//!
//! The kernel keeps a bounded in-memory ring, which is enough for a running
//! process but gone once it exits. A post-mortem reader — `cargo xtask
//! agent-debug`, or a user reporting a wedged turn — needs the ordered log after
//! the fact, so the host persists it as JSON Lines when a path is configured.

use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use lilia_kernel::{Journal, JournalRecord, JournalSink};

/// Path the journal is exported to, when set.
pub const JOURNAL_PATH_ENV: &str = "LILIA_JOURNAL_PATH";

/// Installs a file export on `journal` when [`JOURNAL_PATH_ENV`] names a path.
///
/// Returns the writer, which must be kept alive for the process lifetime: it
/// stops the writer thread and flushes on drop. Reports the reason on failure so
/// a missing export never silently reduces post-mortem evidence.
pub fn install_from_env(journal: &Journal) -> Option<JournalExport> {
    let path = PathBuf::from(std::env::var_os(JOURNAL_PATH_ENV)?);
    match JournalExport::create(&path) {
        Ok(export) => {
            journal.set_sink(export.sink());
            Some(export)
        }
        Err(error) => {
            eprintln!(
                "failed to export the kernel journal to {}: {error}",
                path.display()
            );
            None
        }
    }
}

/// Owns the export writer thread. Dropping it drains and flushes.
pub struct JournalExport {
    sink: Arc<FileJournalSink>,
    writer: Option<JoinHandle<()>>,
}

impl JournalExport {
    pub fn create(path: &Path) -> std::io::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        let (records, incoming) = mpsc::channel::<JournalRecord>();
        // Records arrive on the UI thread and on job workers. Writing inline
        // would put a file syscall on the frame path, so the append is handed to
        // a dedicated thread and the publisher only pays a channel send.
        let writer = std::thread::Builder::new()
            .name("lilia-journal-export".to_owned())
            .spawn(move || {
                let mut file = BufWriter::new(file);
                while let Ok(record) = incoming.recv() {
                    if write_record(&mut file, &record).is_err() {
                        break;
                    }
                }
                let _ = file.flush();
            })?;
        Ok(Self {
            sink: Arc::new(FileJournalSink {
                records: Mutex::new(records),
                reported: AtomicBool::new(false),
            }),
            writer: Some(writer),
        })
    }

    fn sink(&self) -> Arc<dyn JournalSink> {
        Arc::clone(&self.sink) as Arc<dyn JournalSink>
    }
}

impl Drop for JournalExport {
    fn drop(&mut self) {
        self.sink.close();
        if let Some(writer) = self.writer.take() {
            let _ = writer.join();
        }
    }
}

/// Hands records to the writer thread. Wrapped in `Option` so [`JournalExport`]
/// can close the channel and let the thread finish before the process exits.
pub struct FileJournalSink {
    records: Mutex<Sender<JournalRecord>>,
    reported: AtomicBool,
}

impl FileJournalSink {
    fn close(&self) {
        // Replacing the sender with a disconnected one ends the writer loop
        // without needing a sentinel record in the log.
        let (dead, _) = mpsc::channel();
        *self
            .records
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = dead;
    }
}

impl JournalSink for FileJournalSink {
    fn write(&self, record: &JournalRecord) {
        let sent = self
            .records
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .send(record.clone());
        // A dead writer would otherwise print once per record and drown the log.
        if sent.is_err() && !self.reported.swap(true, Ordering::Relaxed) {
            eprintln!("the kernel journal export stopped accepting records");
        }
    }
}

fn write_record(file: &mut BufWriter<File>, record: &JournalRecord) -> std::io::Result<()> {
    let line = serde_json::to_string(record)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    file.write_all(line.as_bytes())?;
    file.write_all(b"\n")?;
    // A killed process is the normal end of an agent-debug run, so each record
    // is flushed instead of waiting for the buffer to fill.
    file.flush()
}

#[cfg(test)]
mod tests {
    use lilia_kernel::RecordKind;

    use super::*;

    #[test]
    fn every_appended_record_survives_the_export() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let path = directory.path().join("nested/journal.jsonl");
        let journal = Journal::new();
        let export = JournalExport::create(&path).expect("the export opens");
        journal.set_sink(export.sink());

        for index in 0..64 {
            journal.append(
                RecordKind::Mutation,
                "task.created",
                Some(format!("task-{index}")),
                serde_json::json!({ "duplicate": false }),
            );
        }
        drop(export);

        let exported = std::fs::read_to_string(&path).expect("the export is readable");
        let records: Vec<JournalRecord> = exported
            .lines()
            .map(|line| serde_json::from_str(line).expect("each line is one record"))
            .collect();
        assert_eq!(records.len(), 64);
        assert!(
            records
                .windows(2)
                .all(|pair| pair[1].sequence == pair[0].sequence + 1),
            "the export lost the journal's ordering"
        );
        assert_eq!(records[0].subject.as_deref(), Some("task-0"));
    }
}
