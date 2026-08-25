use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use lilia_contracts::TaskId;
use crate::application::{
    DesktopIabSnapshotInput, DesktopIabSubmission, DesktopTurnDispatchKind,
};
use uuid::Uuid;

use crate::iab_panel::{HostedBrowserId, IabPanelMessage, IabPanelState};
use crate::runtime_compat::{HostedWindowCaptureId, HostedWindowId};
use nana_ui_platform::WindowId;

#[derive(Debug, Clone, PartialEq)]
pub enum IabWindowMessage {
    Browser(IabPanelMessage),
    NoteChanged(String),
    Submit,
}

#[derive(Debug, Clone)]
struct PendingIabCapture {
    id: HostedWindowCaptureId,
    captured_at: u64,
    url: String,
    title: Option<String>,
    note: Option<String>,
    path: PathBuf,
}

pub struct IabWindowState {
    pub id: HostedWindowId,
    pub task_id: TaskId,
    browser: IabPanelState,
    note: String,
    pending_capture: Option<PendingIabCapture>,
    next_capture_id: u64,
    notice: Option<String>,
    error: Option<String>,
}

impl IabWindowState {
    pub fn new(
        id: HostedWindowId,
        browser_id: HostedBrowserId,
        task_id: TaskId,
        initial_url: impl Into<String>,
    ) -> Self {
        Self {
            id,
            task_id,
            browser: IabPanelState::new(browser_id, initial_url),
            note: String::new(),
            pending_capture: None,
            next_capture_id: 1,
            notice: None,
            error: None,
        }
    }

    pub fn browser_ready(&self) -> bool {
        self.browser.browser_ready()
    }

    pub fn active_url(&self) -> &str {
        self.browser.active_url()
    }

    pub fn capture_pending(&self) -> bool {
        self.pending_capture.is_some()
    }

    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref().or_else(|| self.browser.error())
    }

    pub fn update_browser(&mut self, message: IabPanelMessage) {
        self.notice = None;
        self.error = None;
        self.browser.update(message, self.id);
    }

    pub fn set_note(&mut self, note: String) {
        self.note = note;
        self.notice = None;
        self.error = None;
    }

    pub fn set_visible(&mut self, visible: bool) {
        self.browser.set_panel_visible(visible, self.id);
    }

    pub fn begin_capture(&mut self, home: &Path) -> Option<(HostedWindowCaptureId, PathBuf)> {
        if self.pending_capture.is_some() {
            return None;
        }
        let capture_id = HostedWindowCaptureId(self.next_capture_id);
        self.next_capture_id = self.next_capture_id.saturating_add(1);
        let captured_at = now_millis();
        let path = capture_path(home, &self.task_id, captured_at);
        self.pending_capture = Some(PendingIabCapture {
            id: capture_id,
            captured_at,
            url: self.browser.active_url().to_owned(),
            title: self.browser.title().map(str::to_owned),
            note: normalized_note(&self.note),
            path: path.clone(),
        });
        self.notice = Some("正在提交浏览结果…".to_owned());
        self.error = None;
        Some((capture_id, path))
    }

    pub fn finish_capture(
        &mut self,
        capture_id: HostedWindowCaptureId,
        screenshot_path: Option<PathBuf>,
        warning: Option<String>,
    ) -> Option<DesktopIabSnapshotInput> {
        let pending = self
            .pending_capture
            .take_if(|pending| pending.id == capture_id)?;
        let screenshot_path = screenshot_path.filter(|path| path == &pending.path);
        Some(DesktopIabSnapshotInput {
            task_id: self.task_id.clone(),
            url: pending.url,
            title: pending.title,
            note: pending.note,
            captured_at: pending.captured_at,
            screenshot_path,
            warning,
        })
    }

    pub fn cancel_pending_capture(&mut self) -> Option<(HostedWindowCaptureId, PathBuf)> {
        self.pending_capture
            .take()
            .map(|pending| (pending.id, pending.path))
    }

    pub fn complete_submission(&mut self, submission: &DesktopIabSubmission) {
        self.error = None;
        self.notice = Some(match submission.dispatch.kind {
            DesktopTurnDispatchKind::Started => "浏览结果已提交。".to_owned(),
            DesktopTurnDispatchKind::Queued { .. } => "浏览结果已加入对话队列。".to_owned(),
        });
    }

    pub fn fail_submission(&mut self, message: impl Into<String>) {
        self.notice = None;
        self.error = Some(message.into());
    }
}

fn capture_path(home: &Path, task_id: &TaskId, captured_at: u64) -> PathBuf {
    home.join("attachments").join("iab-snapshots").join(format!(
        "iab-{captured_at}-{}-{}.png",
        safe_filename_segment(task_id.as_str()),
        Uuid::new_v4()
    ))
}

fn safe_filename_segment(value: &str) -> String {
    let value = value
        .chars()
        .take(40)
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    if value.is_empty() {
        "task".to_owned()
    } else {
        value
    }
}

fn normalized_note(note: &str) -> Option<String> {
    let note = note.trim();
    (!note.is_empty()).then(|| note.to_owned())
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_completion_preserves_the_page_metadata_from_submit_time() {
        let root = Path::new("C:/lilia");
        let mut state = IabWindowState::new(
            WindowId(10),
            HostedBrowserId(11),
            TaskId::new("task/1").unwrap(),
            "https://example.com/",
        );
        state.set_note("  inspect the modal  ".to_owned());
        let (capture_id, path) = state.begin_capture(root).unwrap();

        let input = state
            .finish_capture(capture_id, Some(path.clone()), None)
            .unwrap();

        assert_eq!(input.url, "https://example.com/");
        assert_eq!(input.note.as_deref(), Some("inspect the modal"));
        assert_eq!(input.screenshot_path.as_deref(), Some(path.as_path()));
        assert!(path.to_string_lossy().contains("task-1"));
        assert!(!state.capture_pending());
    }

    #[test]
    fn mismatched_capture_result_does_not_consume_the_pending_submission() {
        let mut state = IabWindowState::new(
            WindowId(10),
            HostedBrowserId(11),
            TaskId::new("task-1").unwrap(),
            "about:blank",
        );
        let _ = state.begin_capture(Path::new("C:/lilia")).unwrap();

        assert!(state
            .finish_capture(HostedWindowCaptureId(99), None, None)
            .is_none());
        assert!(state.capture_pending());
    }

    #[test]
    fn closing_a_pending_capture_returns_the_uncommitted_attachment_path() {
        let mut state = IabWindowState::new(
            WindowId(10),
            HostedBrowserId(11),
            TaskId::new("task-1").unwrap(),
            "about:blank",
        );
        let (capture_id, path) = state.begin_capture(Path::new("C:/lilia")).unwrap();

        assert_eq!(
            state.cancel_pending_capture(),
            Some((capture_id, path.clone()))
        );
        assert!(path
            .to_string_lossy()
            .replace('\\', "/")
            .contains("attachments/iab-snapshots"));
        assert!(!state.capture_pending());
    }
}
