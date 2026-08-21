use lilia_desktop_application::{ProjectFileEntry, ProjectFileKind, ProjectFilesSnapshot};
use nana_ui::runtime::TreeView;
use nana_ui::{Icon, TreeNode};

use crate::target_ids;

pub fn project_files_tree(
    snapshot: &ProjectFilesSnapshot,
    selected: Option<&str>,
) -> TreeView {
    TreeView::new(
        snapshot
            .entries
            .iter()
            .map(|entry| project_file_runtime_node(entry, selected)),
    )
}

fn project_file_runtime_node(entry: &ProjectFileEntry, selected: Option<&str>) -> TreeNode<std::sync::Arc<str>> {
    let id = std::sync::Arc::<str>::from(entry.relative_path.as_str());
    let node = match entry.kind {
        ProjectFileKind::Directory => TreeNode::branch(
            id,
            entry.name.clone(),
            entry.children.is_some(),
            entry
                .children
                .iter()
                .flatten()
                .map(|child| project_file_runtime_node(child, selected)),
        )
        .icon(Icon::Folder),
        ProjectFileKind::File => TreeNode::leaf(id, entry.name.clone()).icon(Icon::File),
    };
    node.selected(selected == Some(entry.relative_path.as_str()))
}

pub fn project_files_debug_targets(snapshot: &ProjectFilesSnapshot) -> Vec<String> {
    let mut targets = vec![
        target_ids::PROJECT_FILES_OPEN.to_owned(),
        target_ids::PROJECT_FILES_REFRESH.to_owned(),
    ];
    collect_entry_targets(&snapshot.entries, &mut targets);
    targets
}

fn collect_entry_targets(entries: &[ProjectFileEntry], targets: &mut Vec<String>) {
    for entry in entries {
        targets.push(format!(
            "lilia.project-files.entry.{}",
            entry.relative_path.replace('/', ".")
        ));
        if let Some(children) = &entry.children {
            collect_entry_targets(children, targets);
        }
    }
}
