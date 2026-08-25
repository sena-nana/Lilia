use std::path::PathBuf;

use lilia_contracts::{ChatAttachment, TaskId};
use serde::{Deserialize, Serialize};

use crate::application::{
    describe_attachment_path, DesktopApplication, DesktopApplicationError, DesktopTurnDispatch,
    DesktopTurnRequest,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopIabSnapshotStatus {
    Captured,
    MetadataOnly,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopIabSnapshotInput {
    pub task_id: TaskId,
    pub url: String,
    pub title: Option<String>,
    pub note: Option<String>,
    pub captured_at: u64,
    pub screenshot_path: Option<PathBuf>,
    pub warning: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopIabSnapshot {
    pub task_id: TaskId,
    pub url: String,
    pub title: Option<String>,
    pub note: Option<String>,
    pub captured_at: u64,
    pub screenshot_path: Option<String>,
    pub screenshot_attachment: Option<ChatAttachment>,
    pub status: DesktopIabSnapshotStatus,
    pub warning: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopIabSubmission {
    pub snapshot: DesktopIabSnapshot,
    pub dispatch: DesktopTurnDispatch,
}

impl DesktopApplication {
    pub fn submit_iab_snapshot(
        &self,
        input: DesktopIabSnapshotInput,
    ) -> Result<DesktopIabSubmission, DesktopApplicationError> {
        let (snapshot, request) = build_iab_turn(input)?;
        let dispatch = self.start_task_turn(request)?;
        Ok(DesktopIabSubmission { snapshot, dispatch })
    }
}

fn build_iab_turn(
    input: DesktopIabSnapshotInput,
) -> Result<(DesktopIabSnapshot, DesktopTurnRequest), DesktopApplicationError> {
    let url = required_trimmed(input.url, "url", "IAB page URL must not be empty")?;
    let title = optional_trimmed(input.title);
    let note = optional_trimmed(input.note);
    let warning = optional_trimmed(input.warning);
    let (screenshot_path, screenshot_attachment, status) = match input.screenshot_path {
        Some(path) => {
            let attachment = describe_attachment_path(&path);
            if !attachment.exists || !attachment.is_image() {
                return Err(DesktopApplicationError::InvalidInput {
                    field: "screenshot_path",
                    message: "IAB screenshot must reference an existing image".to_owned(),
                });
            }
            (
                Some(attachment.path.clone()),
                Some(attachment),
                DesktopIabSnapshotStatus::Captured,
            )
        }
        None => (None, None, DesktopIabSnapshotStatus::MetadataOnly),
    };
    let snapshot = DesktopIabSnapshot {
        task_id: input.task_id.clone(),
        url,
        title,
        note,
        captured_at: input.captured_at,
        screenshot_path,
        screenshot_attachment,
        status,
        warning,
    };
    let mut request = DesktopTurnRequest::new(input.task_id, render_iab_context(&snapshot));
    if let Some(attachment) = snapshot.screenshot_attachment.clone() {
        request.attachments.push(attachment);
    }
    Ok((snapshot, request))
}

fn render_iab_context(snapshot: &DesktopIabSnapshot) -> String {
    let mut lines = vec![
        "IAB 浏览结果已提交，请结合页面信息继续当前任务。".to_owned(),
        format!("地址：{}", snapshot.url),
    ];
    if let Some(title) = &snapshot.title {
        lines.push(format!("页面标题：{title}"));
    }
    if let Some(note) = &snapshot.note {
        lines.push(format!("用户备注：{note}"));
    }
    if snapshot.status == DesktopIabSnapshotStatus::MetadataOnly {
        lines.push("本次仅包含页面信息，没有附加截图。".to_owned());
    }
    lines.join("\n")
}

fn required_trimmed(
    value: String,
    field: &'static str,
    message: &'static str,
) -> Result<String, DesktopApplicationError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(DesktopApplicationError::InvalidInput {
            field,
            message: message.to_owned(),
        });
    }
    Ok(value.to_owned())
}

fn optional_trimmed(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use lilia_contracts::ChatAttachmentKind;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn captured_page_becomes_a_durable_turn_with_an_image_reference() {
        let root = tempdir().unwrap();
        let screenshot = root.path().join("iab.png");
        fs::write(&screenshot, b"captured pixels").unwrap();

        let (snapshot, request) = build_iab_turn(DesktopIabSnapshotInput {
            task_id: TaskId::new("task-1").unwrap(),
            url: " https://example.com/final ".to_owned(),
            title: Some(" Example ".to_owned()),
            note: Some(" Check the dialog ".to_owned()),
            captured_at: 42,
            screenshot_path: Some(screenshot),
            warning: None,
        })
        .unwrap();

        assert_eq!(snapshot.status, DesktopIabSnapshotStatus::Captured);
        assert_eq!(snapshot.title.as_deref(), Some("Example"));
        assert_eq!(snapshot.note.as_deref(), Some("Check the dialog"));
        assert_eq!(request.attachments.len(), 1);
        assert_eq!(request.attachments[0].kind, ChatAttachmentKind::File);
        assert_eq!(request.attachments[0].mime.as_deref(), Some("image/png"));
        assert!(request.content.contains("https://example.com/final"));
        assert!(request.content.contains("Check the dialog"));
    }

    #[test]
    fn metadata_only_page_is_still_deliverable_without_a_fake_attachment() {
        let (snapshot, request) = build_iab_turn(DesktopIabSnapshotInput {
            task_id: TaskId::new("task-1").unwrap(),
            url: "about:blank".to_owned(),
            title: None,
            note: None,
            captured_at: 42,
            screenshot_path: None,
            warning: Some("capture unavailable".to_owned()),
        })
        .unwrap();

        assert_eq!(snapshot.status, DesktopIabSnapshotStatus::MetadataOnly);
        assert!(request.attachments.is_empty());
        assert!(request.content.contains("没有附加截图"));
    }
}
