use std::fs;
use std::path::{Component, Path, PathBuf};

use ignore::WalkBuilder;
use lilia_contracts::{ChatContextSearchMatch, ChatContextSearchResult, ProjectId, TaskId};

use crate::application::{describe_attachment_path, DesktopApplication, DesktopApplicationError};

const DEFAULT_CONTEXT_SEARCH_LIMIT: usize = 12;
const MAX_CONTEXT_SEARCH_LIMIT: usize = 50;
const CONTEXT_SEARCH_SCAN_LIMIT: usize = 4_000;

pub fn search_context_attachments(
    root: impl AsRef<Path>,
    query: &str,
    limit: usize,
) -> Vec<ChatContextSearchResult> {
    let root = root.as_ref();
    if !root.is_dir() {
        return Vec::new();
    }
    let limit = if limit == 0 {
        DEFAULT_CONTEXT_SEARCH_LIMIT
    } else {
        limit.clamp(1, MAX_CONTEXT_SEARCH_LIMIT)
    };
    let query = query.trim();
    if context_query_is_path_like(query) {
        search_context_browse_dir(root, query, limit)
    } else {
        search_context_project(root, query, limit)
    }
}

impl DesktopApplication {
    pub fn search_task_context_attachments(
        &self,
        task_id: &TaskId,
        query: &str,
        limit: usize,
    ) -> Result<Vec<ChatContextSearchResult>, DesktopApplicationError> {
        let task = self.get_task(task_id)?;
        let project_id = task
            .project_id
            .ok_or_else(|| DesktopApplicationError::InvalidInput {
                field: "taskId",
                message: format!(
                    "inbox task `{}` has no project workspace context",
                    task_id.as_str()
                ),
            })?;
        self.search_project_context_attachments(&project_id, query, limit)
    }

    pub fn search_project_context_attachments(
        &self,
        project_id: &ProjectId,
        query: &str,
        limit: usize,
    ) -> Result<Vec<ChatContextSearchResult>, DesktopApplicationError> {
        let context = self.project_context(project_id)?;
        Ok(search_context_attachments(
            context.active_root(),
            query,
            limit,
        ))
    }
}

fn should_skip_context_search_dir(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if matches!(
        name.as_str(),
        ".git" | "node_modules" | "dist" | "target" | ".cache" | "build"
    ) {
        return true;
    }
    name == "cache"
        && path
            .parent()
            .and_then(Path::file_name)
            .and_then(|parent| parent.to_str())
            .is_some_and(|parent| parent.eq_ignore_ascii_case(".yarn"))
}

fn context_query_is_path_like(query: &str) -> bool {
    query.contains('/') || query.contains('\\')
}

fn query_allows_hidden(query: &str) -> bool {
    query.contains('.')
}

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with('.'))
}

fn relative_path_text(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn normalized_path_query(query: &str) -> String {
    let mut normalized = query.trim().replace('\\', "/");
    while let Some(rest) = normalized.strip_prefix("./") {
        normalized = rest.to_owned();
    }
    normalized
}

fn relative_path_buf(value: &str) -> Option<PathBuf> {
    let mut path = PathBuf::new();
    for component in Path::new(value).components() {
        match component {
            Component::Normal(part) => path.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::Prefix(_) | Component::RootDir => return None,
        }
    }
    Some(path)
}

fn browse_dir_from_query(query: &str) -> Option<(PathBuf, String)> {
    let normalized = normalized_path_query(query);
    if normalized.ends_with('/') {
        let directory = normalized.trim_end_matches('/');
        return relative_path_buf(directory).map(|path| (path, normalized));
    }
    let slash = normalized.rfind('/')?;
    relative_path_buf(&normalized[..slash]).map(|path| (path, normalized))
}

fn sorted_child_paths(directory: &Path) -> Option<Vec<PathBuf>> {
    let mut paths = fs::read_dir(directory)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    paths.sort_by_key(|path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default()
    });
    Some(paths)
}

fn match_path(root: &Path, path: &Path, query: &str) -> Option<(String, ChatContextSearchMatch)> {
    let relative_path = relative_path_text(root, path);
    if query.is_empty() {
        return Some((relative_path, ChatContextSearchMatch::Name));
    }
    let query = query.to_ascii_lowercase().replace('\\', "/");
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if name.contains(&query) {
        Some((relative_path, ChatContextSearchMatch::Name))
    } else if relative_path.to_ascii_lowercase().contains(&query) {
        Some((relative_path, ChatContextSearchMatch::Path))
    } else {
        None
    }
}

fn push_result(root: &Path, path: &Path, query: &str, results: &mut Vec<ChatContextSearchResult>) {
    let Some((relative_path, matched_by)) = match_path(root, path, query) else {
        return;
    };
    results.push(ChatContextSearchResult {
        attachment: describe_attachment_path(path),
        relative_path,
        matched_by,
    });
}

fn search_context_browse_dir(
    root: &Path,
    query: &str,
    limit: usize,
) -> Vec<ChatContextSearchResult> {
    let Some((relative_dir, normalized_query)) = browse_dir_from_query(query) else {
        return Vec::new();
    };
    let Some(children) = sorted_child_paths(&root.join(relative_dir)) else {
        return Vec::new();
    };
    let allow_hidden = query_allows_hidden(query);
    let mut results = Vec::new();
    for path in children.into_iter().take(CONTEXT_SEARCH_SCAN_LIMIT) {
        if results.len() >= limit {
            break;
        }
        if !allow_hidden && is_hidden(&path) {
            continue;
        }
        push_result(root, &path, &normalized_query, &mut results);
    }
    results
}

fn search_context_project(root: &Path, query: &str, limit: usize) -> Vec<ChatContextSearchResult> {
    let allow_hidden = query_allows_hidden(query);
    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(!allow_hidden)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(true)
        .ignore(true)
        .parents(true);
    let filter_root = root.to_path_buf();
    builder.filter_entry(move |entry| {
        entry.path() == filter_root || !should_skip_context_search_dir(entry.path())
    });
    let mut results = Vec::new();
    for entry in builder
        .build()
        .filter_map(Result::ok)
        .take(CONTEXT_SEARCH_SCAN_LIMIT)
    {
        if results.len() >= limit {
            break;
        }
        if entry.path() != root {
            push_result(root, entry.path(), query, &mut results);
        }
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_file(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    fn paths(results: &[ChatContextSearchResult]) -> Vec<&str> {
        results
            .iter()
            .map(|result| result.relative_path.as_str())
            .collect()
    }

    #[test]
    fn project_search_honors_hidden_and_gitignore_rules() {
        let root = tempfile::tempdir().unwrap();
        write_file(&root.path().join(".env"), "secret");
        write_file(&root.path().join(".gitignore"), "dist/\n");
        write_file(&root.path().join("dist/app.js"), "ignored");
        write_file(&root.path().join("src/env.ts"), "visible");

        let visible = search_context_attachments(root.path(), "env", 20);
        assert_eq!(paths(&visible), vec!["src/env.ts"]);
        let hidden = search_context_attachments(root.path(), ".", 20);
        assert!(paths(&hidden).contains(&".env"));
        assert!(
            !paths(&search_context_attachments(root.path(), "dist", 20)).contains(&"dist/app.js")
        );
    }

    #[test]
    fn path_query_lists_only_direct_children_and_rejects_parent_escape() {
        let root = tempfile::tempdir().unwrap();
        write_file(&root.path().join("big/inside.md"), "inside");
        write_file(&root.path().join("big/nested/deep.md"), "deep");

        let results = search_context_attachments(root.path(), "big/", 20);
        assert!(paths(&results).contains(&"big/inside.md"));
        assert!(paths(&results).contains(&"big/nested"));
        assert!(!paths(&results).contains(&"big/nested/deep.md"));
        assert!(search_context_attachments(root.path(), "../", 20).is_empty());
    }
}
