use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatAttachment {
    pub id: String,
    pub name: String,
    pub path: String,
    pub kind: ChatAttachmentKind,
    pub size: Option<u64>,
    #[serde(default = "default_attachment_exists")]
    pub exists: bool,
    #[serde(default)]
    pub mime: Option<String>,
    #[serde(default)]
    pub directory: Option<ChatAttachmentDirectoryMeta>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatAttachmentKind {
    File,
    Directory,
    Unknown,
}

impl ChatAttachmentKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Directory => "directory",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatAttachmentDirectoryMeta {
    pub file_count: u64,
    pub directory_count: u64,
    pub total_size: u64,
    pub truncated: bool,
    pub unreadable_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatContextSearchMatch {
    Name,
    Path,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatContextSearchResult {
    pub attachment: ChatAttachment,
    pub relative_path: String,
    pub matched_by: ChatContextSearchMatch,
}

impl ChatAttachment {
    pub fn is_image(&self) -> bool {
        self.exists
            && self
                .mime
                .as_deref()
                .is_some_and(|mime| mime.starts_with("image/"))
    }

    pub fn reference_label(&self) -> &'static str {
        if self.is_image() {
            "图片引用"
        } else if self.kind == ChatAttachmentKind::Directory {
            "目录引用"
        } else {
            "文件引用"
        }
    }

    pub fn reference_text(&self) -> String {
        format!(
            "[{}: {} | {}]",
            self.reference_label(),
            self.name,
            self.path
        )
    }
}

fn default_attachment_exists() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn attachment_serialization_matches_the_frontend_contract() {
        let attachment = ChatAttachment {
            id: "att-1".to_owned(),
            name: "src".to_owned(),
            path: "C:/repo/src".to_owned(),
            kind: ChatAttachmentKind::Directory,
            size: None,
            exists: true,
            mime: None,
            directory: Some(ChatAttachmentDirectoryMeta {
                file_count: 2,
                directory_count: 1,
                total_size: 42,
                truncated: false,
                unreadable_count: 0,
            }),
        };

        assert_eq!(
            serde_json::to_value(attachment).unwrap(),
            json!({
                "id": "att-1",
                "name": "src",
                "path": "C:/repo/src",
                "kind": "directory",
                "size": null,
                "exists": true,
                "mime": null,
                "directory": {
                    "fileCount": 2,
                    "directoryCount": 1,
                    "totalSize": 42,
                    "truncated": false,
                    "unreadableCount": 0,
                },
            })
        );
    }

    #[test]
    fn attachment_reference_matches_the_frontend_wire_format() {
        let attachment = ChatAttachment {
            id: "att-image".to_owned(),
            name: "capture.png".to_owned(),
            path: "C:/repo/capture.png".to_owned(),
            kind: ChatAttachmentKind::File,
            size: Some(42),
            exists: true,
            mime: Some("image/png".to_owned()),
            directory: None,
        };

        assert_eq!(
            attachment.reference_text(),
            "[图片引用: capture.png | C:/repo/capture.png]"
        );
    }

    #[test]
    fn context_search_result_matches_the_frontend_wire_format() {
        let result = ChatContextSearchResult {
            attachment: ChatAttachment {
                id: "att-file".to_owned(),
                name: "main.rs".to_owned(),
                path: "C:/repo/src/main.rs".to_owned(),
                kind: ChatAttachmentKind::File,
                size: Some(42),
                exists: true,
                mime: None,
                directory: None,
            },
            relative_path: "src/main.rs".to_owned(),
            matched_by: ChatContextSearchMatch::Path,
        };

        assert_eq!(serde_json::to_value(result).unwrap()["matchedBy"], "path");
    }
}
