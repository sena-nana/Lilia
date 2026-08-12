use lilia_desktop_application::describe_attachment_path as describe_path;

use crate::chat::types::ChatAttachment;

pub(crate) fn describe_attachment_path(path: String) -> ChatAttachment {
    describe_path(path)
}
