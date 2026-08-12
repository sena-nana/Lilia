use lilia_desktop_application::search_context_attachments as search_application_context;

use crate::chat::types::ChatContextSearchResult;

pub(super) fn search_context_attachments(
    project_cwd: String,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<ChatContextSearchResult>, String> {
    Ok(search_application_context(
        project_cwd.trim(),
        &query,
        limit.unwrap_or(12),
    ))
}
