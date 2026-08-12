//! Storage writer authority reservation (#60).
//!
//! ## Guarantees (honest boundary)
//!
//! - **Process-local registry**: same process cannot claim the same `storage_key`
//!   twice (epoch + owner).
//! - **Single-machine file lock** (optional): when a lock path is provided,
//!   another process on the same host cannot acquire the same writer lock while
//!   this guard is alive. Lock is released on process exit (including crash).
//! - **Not provided yet**: distributed fencing, epoch rejection of late commands
//!   from a previous writer after takeover, or cluster-wide lease renewal.
//!
//! Embedded Desktop ↔ Service must coordinate through this lock before both open
//! the product DB for writes. Full cross-process epoch fencing remains a follow-up.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::Serialize;
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WriterMode {
    Embedded,
    Service,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageWriterLease {
    pub storage_key: String,
    pub owner_id: String,
    pub mode: WriterMode,
    pub epoch: u64,
    /// Absolute path of the on-disk writer lock file, when file locking is enabled.
    pub file_lock_path: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriterLeaseHealth {
    pub held: bool,
    pub storage_key: Option<String>,
    pub owner_id: Option<String>,
    pub mode: Option<WriterMode>,
    pub epoch: Option<u64>,
    pub single_writer: bool,
    /// True when this holder also owns a single-machine file lock.
    pub file_lock_held: bool,
    pub file_lock_path: Option<String>,
    /// Cross-process epoch fencing of late commands is **not** implemented.
    pub cross_process_epoch_fencing: bool,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum WriterLeaseError {
    #[error(
        "storage writer for `{storage_key}` already held by `{holder}` (epoch {epoch}, mode {mode:?})"
    )]
    AlreadyHeld {
        storage_key: String,
        holder: String,
        epoch: u64,
        mode: WriterMode,
    },
    #[error("storage writer file lock busy at `{path}`")]
    FileLockBusy { path: String },
    #[error("storage writer file lock I/O at `{path}`: {message}")]
    FileLockIo { path: String, message: String },
    #[error("storage writer lease epoch mismatch")]
    EpochMismatch,
}

struct ActiveWriter {
    lease: StorageWriterLease,
}

fn registry() -> &'static Mutex<HashMap<String, ActiveWriter>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, ActiveWriter>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Held exclusive file lock (released when dropped / process exits).
#[derive(Debug)]
struct FileWriterLock {
    path: PathBuf,
    _file: File,
}

impl FileWriterLock {
    fn try_acquire(path: impl AsRef<Path>) -> Result<Self, WriterLeaseError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| WriterLeaseError::FileLockIo {
                path: path.display().to_string(),
                message: err.to_string(),
            })?;
        }
        let file = open_lock_file(&path).map_err(|err| {
            if err.kind() == io::ErrorKind::WouldBlock
                || err.raw_os_error() == Some(16) // EBUSY
                || err.raw_os_error() == Some(32) // ERROR_SHARING_VIOLATION
                || err.raw_os_error() == Some(33) // ERROR_LOCK_VIOLATION
                || err.kind() == io::ErrorKind::PermissionDenied
            {
                WriterLeaseError::FileLockBusy {
                    path: path.display().to_string(),
                }
            } else {
                WriterLeaseError::FileLockIo {
                    path: path.display().to_string(),
                    message: err.to_string(),
                }
            }
        })?;
        Ok(Self { path, _file: file })
    }
}

#[cfg(unix)]
fn open_lock_file(path: &Path) -> io::Result<File> {
    use std::os::unix::io::AsRawFd;

    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(path)?;
    // flock(2): exclusive, non-blocking. Released automatically on process exit.
    extern "C" {
        fn flock(fd: i32, operation: i32) -> i32;
    }
    const LOCK_EX: i32 = 2;
    const LOCK_NB: i32 = 4;
    let rc = unsafe { flock(file.as_raw_fd() as i32, LOCK_EX | LOCK_NB) };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    let mut file = file;
    file.set_len(0)?;
    writeln!(file, "lilia-writer-lock")?;
    file.flush()?;
    Ok(file)
}

#[cfg(windows)]
fn open_lock_file(path: &Path) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;

    // share_mode(0) → exclusive open; another process gets a sharing violation.
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .share_mode(0)
        .open(path)?;
    file.set_len(0)?;
    writeln!(file, "lilia-writer-lock")?;
    file.flush()?;
    Ok(file)
}

#[cfg(not(any(unix, windows)))]
fn open_lock_file(path: &Path) -> io::Result<File> {
    let _ = path;
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "writer file lock unsupported on this platform",
    ))
}

/// RAII guard: releasing clears the process-local writer claim (and file lock).
#[derive(Debug)]
pub struct StorageWriterGuard {
    lease: StorageWriterLease,
    _file_lock: Option<FileWriterLock>,
}

impl StorageWriterGuard {
    pub fn lease(&self) -> &StorageWriterLease {
        &self.lease
    }

    pub fn try_acquire(
        storage_key: impl Into<String>,
        owner_id: impl Into<String>,
        mode: WriterMode,
    ) -> Result<Self, WriterLeaseError> {
        Self::try_acquire_inner(storage_key, owner_id, mode, None)
    }

    /// Acquire process-local lease **and** a single-machine exclusive file lock.
    pub fn try_acquire_with_file_lock(
        storage_key: impl Into<String>,
        owner_id: impl Into<String>,
        mode: WriterMode,
        lock_path: impl AsRef<Path>,
    ) -> Result<Self, WriterLeaseError> {
        Self::try_acquire_inner(
            storage_key,
            owner_id,
            mode,
            Some(lock_path.as_ref().to_path_buf()),
        )
    }

    fn try_acquire_inner(
        storage_key: impl Into<String>,
        owner_id: impl Into<String>,
        mode: WriterMode,
        lock_path: Option<PathBuf>,
    ) -> Result<Self, WriterLeaseError> {
        let storage_key = storage_key.into();
        let owner_id = owner_id.into();
        let file_lock = match lock_path {
            Some(path) => Some(FileWriterLock::try_acquire(path)?),
            None => None,
        };
        let mut guard = registry().lock().expect("writer lease registry poisoned");
        if let Some(active) = guard.get(&storage_key) {
            return Err(WriterLeaseError::AlreadyHeld {
                storage_key: storage_key.clone(),
                holder: active.lease.owner_id.clone(),
                epoch: active.lease.epoch,
                mode: active.lease.mode,
            });
        }
        let epoch = next_epoch();
        let lease = StorageWriterLease {
            storage_key: storage_key.clone(),
            owner_id,
            mode,
            epoch,
            file_lock_path: file_lock
                .as_ref()
                .map(|lock| lock.path.display().to_string()),
        };
        guard.insert(
            storage_key,
            ActiveWriter {
                lease: lease.clone(),
            },
        );
        Ok(Self {
            lease,
            _file_lock: file_lock,
        })
    }
}

impl Drop for StorageWriterGuard {
    fn drop(&mut self) {
        let mut guard = registry().lock().expect("writer lease registry poisoned");
        if let Some(active) = guard.get(&self.lease.storage_key) {
            if active.lease.epoch == self.lease.epoch
                && active.lease.owner_id == self.lease.owner_id
            {
                guard.remove(&self.lease.storage_key);
            }
        }
        // FileWriterLock drops after this and releases the OS lock.
    }
}

pub fn writer_lease_health(storage_key: &str) -> WriterLeaseHealth {
    let guard = registry().lock().expect("writer lease registry poisoned");
    match guard.get(storage_key) {
        Some(active) => WriterLeaseHealth {
            held: true,
            storage_key: Some(active.lease.storage_key.clone()),
            owner_id: Some(active.lease.owner_id.clone()),
            mode: Some(active.lease.mode),
            epoch: Some(active.lease.epoch),
            single_writer: true,
            file_lock_held: active.lease.file_lock_path.is_some(),
            file_lock_path: active.lease.file_lock_path.clone(),
            cross_process_epoch_fencing: false,
        },
        None => WriterLeaseHealth {
            held: false,
            storage_key: Some(storage_key.to_string()),
            owner_id: None,
            mode: None,
            epoch: None,
            single_writer: true,
            file_lock_held: false,
            file_lock_path: None,
            cross_process_epoch_fencing: false,
        },
    }
}

fn next_epoch() -> u64 {
    static EPOCH: OnceLock<Mutex<u64>> = OnceLock::new();
    let counter = EPOCH.get_or_init(|| Mutex::new(0));
    let mut value = counter.lock().expect("writer epoch poisoned");
    *value = value.saturating_add(1);
    *value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_acquire_same_key_is_rejected_until_first_drops() {
        let key = "test:second-acquire";
        let first = StorageWriterGuard::try_acquire(key, "svc-a", WriterMode::Service).unwrap();
        let err =
            StorageWriterGuard::try_acquire(key, "embedded-b", WriterMode::Embedded).unwrap_err();
        assert!(matches!(
            err,
            WriterLeaseError::AlreadyHeld {
                mode: WriterMode::Service,
                ..
            }
        ));
        drop(first);
        let second =
            StorageWriterGuard::try_acquire(key, "embedded-b", WriterMode::Embedded).unwrap();
        assert_eq!(second.lease().mode, WriterMode::Embedded);
        assert!(writer_lease_health(key).held);
        drop(second);
        assert!(!writer_lease_health(key).held);
    }

    #[test]
    fn different_storage_keys_may_hold_writers_concurrently() {
        let a = StorageWriterGuard::try_acquire("test:key-a", "a", WriterMode::Service).unwrap();
        let b = StorageWriterGuard::try_acquire("test:key-b", "b", WriterMode::Embedded).unwrap();
        assert!(writer_lease_health("test:key-a").held);
        assert!(writer_lease_health("test:key-b").held);
        drop(a);
        drop(b);
    }

    #[test]
    fn file_lock_blocks_second_holder_and_documents_no_epoch_fencing() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("lilia-writer-lock-{nanos}"));
        let lock_path = dir.join("writer.lock");
        let key = format!("file:{}", lock_path.display());

        let first = StorageWriterGuard::try_acquire_with_file_lock(
            &key,
            "svc-file-a",
            WriterMode::Service,
            &lock_path,
        )
        .unwrap();
        let health = writer_lease_health(&key);
        assert!(health.file_lock_held);
        assert!(!health.cross_process_epoch_fencing);
        assert_eq!(
            health.file_lock_path.as_deref(),
            Some(lock_path.to_string_lossy().as_ref())
        );

        let err = StorageWriterGuard::try_acquire_with_file_lock(
            format!("{key}-other-registry-key"),
            "svc-file-b",
            WriterMode::Embedded,
            &lock_path,
        )
        .unwrap_err();
        assert!(matches!(err, WriterLeaseError::FileLockBusy { .. }));

        drop(first);
        let second = StorageWriterGuard::try_acquire_with_file_lock(
            format!("{key}-reclaim"),
            "svc-file-b",
            WriterMode::Embedded,
            &lock_path,
        )
        .unwrap();
        assert_eq!(second.lease().mode, WriterMode::Embedded);
        drop(second);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
