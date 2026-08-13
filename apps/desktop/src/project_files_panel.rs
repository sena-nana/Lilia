use iced::widget::{button, column, container, row, text, Space};
use iced::{Alignment, Element, Length, Padding};
use lilia_desktop_application::{ProjectFileEntry, ProjectFileKind, ProjectFilesSnapshot};
use nana_ui::widgets::button_style;
use nana_ui::{ui_font, ButtonKind, Icon, ThemeTokens, TreeNode, TreeView, TreeViewEvent};

use crate::target_ids;

#[derive(Debug, Clone)]
pub enum ProjectFilesPanelMessage {
    ToggleExpand(String),
    OpenPath(String),
    Refresh,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectFileNodeId {
    relative_path: String,
    kind: ProjectFileKind,
}

pub fn project_files_tree(
    snapshot: &ProjectFilesSnapshot,
    tokens: ThemeTokens,
) -> Element<'static, ProjectFilesPanelMessage> {
    let colors = tokens.colors;
    let mut body = column![
        row![
            text(format!("{} · 文件", snapshot.root_name))
                .size(12)
                .font(ui_font(iced::font::Weight::Semibold))
                .color(colors.text),
            Space::new().width(Length::Fill),
            button(text("刷新").size(10))
                .on_press(ProjectFilesPanelMessage::Refresh)
                .style(button_style(tokens, ButtonKind::Text)),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .width(Length::Fill),
        text(snapshot.workspace_root.display().to_string())
            .size(10)
            .color(colors.muted),
    ]
    .spacing(6)
    .width(Length::Fill);

    if snapshot.entries.is_empty() {
        body = body.push(text("工作区为空或不可读。").size(11).color(colors.muted));
    } else {
        let nodes = snapshot
            .entries
            .iter()
            .map(|entry| project_file_tree_node(entry, snapshot.view.selected_path.as_deref()));
        body = body.push(TreeView::new(nodes, project_file_tree_event, tokens).view());
    }

    container(body)
        .width(Length::Fill)
        .padding(Padding::from([8, 10]))
        .into()
}

pub fn project_files_center(
    snapshot: &ProjectFilesSnapshot,
    opened_path: Option<&str>,
    opened_preview: Option<&str>,
    tokens: ThemeTokens,
) -> Element<'static, ProjectFilesPanelMessage> {
    let colors = tokens.colors;
    let mut content = column![project_files_tree(snapshot, tokens)].spacing(12);
    if let Some(path) = opened_path {
        let mut detail = column![text(format!("已打开 · {path}"))
            .size(13)
            .font(ui_font(iced::font::Weight::Semibold))
            .color(colors.text),]
        .spacing(6);
        if let Some(preview) = opened_preview {
            let clipped = if preview.chars().count() > 400 {
                format!("{}…", preview.chars().take(400).collect::<String>())
            } else {
                preview.to_owned()
            };
            detail = detail.push(
                container(text(clipped).size(11).color(colors.text))
                    .width(Length::Fill)
                    .padding(10)
                    .style(move |_| {
                        iced::widget::container::Style::default()
                            .background(colors.surface)
                            .border(iced::Border {
                                color: colors.border,
                                width: 1.0,
                                radius: 6.0.into(),
                            })
                    }),
            );
        }
        content = content.push(detail);
    } else {
        content = content.push(text("选择文件以打开文档。").size(12).color(colors.muted));
    }
    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding::from([14, 16]))
        .into()
}

fn project_file_tree_node(
    entry: &ProjectFileEntry,
    selected: Option<&str>,
) -> TreeNode<ProjectFileNodeId> {
    let id = ProjectFileNodeId {
        relative_path: entry.relative_path.clone(),
        kind: entry.kind,
    };
    let node = match entry.kind {
        ProjectFileKind::Directory => TreeNode::branch(
            id,
            entry.name.clone(),
            entry.children.is_some(),
            entry
                .children
                .iter()
                .flatten()
                .map(|child| project_file_tree_node(child, selected)),
        )
        .icon(Icon::Folder),
        ProjectFileKind::File => TreeNode::leaf(id, entry.name.clone()).icon(Icon::File),
    };
    node.selected(selected == Some(entry.relative_path.as_str()))
}

fn project_file_tree_event(event: TreeViewEvent<ProjectFileNodeId>) -> ProjectFilesPanelMessage {
    let node = match event {
        TreeViewEvent::Toggle(node) | TreeViewEvent::Select(node) => node,
    };
    match node.kind {
        ProjectFileKind::Directory => ProjectFilesPanelMessage::ToggleExpand(node.relative_path),
        ProjectFileKind::File => ProjectFilesPanelMessage::OpenPath(node.relative_path),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_events_preserve_directory_and_file_actions() {
        let directory = ProjectFileNodeId {
            relative_path: "src".to_owned(),
            kind: ProjectFileKind::Directory,
        };
        let file = ProjectFileNodeId {
            relative_path: "src/main.rs".to_owned(),
            kind: ProjectFileKind::File,
        };

        assert!(matches!(
            project_file_tree_event(TreeViewEvent::Select(directory)),
            ProjectFilesPanelMessage::ToggleExpand(path) if path == "src"
        ));
        assert!(matches!(
            project_file_tree_event(TreeViewEvent::Select(file)),
            ProjectFilesPanelMessage::OpenPath(path) if path == "src/main.rs"
        ));
    }

    #[test]
    fn tree_nodes_reflect_snapshot_expansion_and_selection() {
        let entry = ProjectFileEntry {
            name: "src".to_owned(),
            relative_path: "src".to_owned(),
            kind: ProjectFileKind::Directory,
            children: Some(vec![ProjectFileEntry {
                name: "main.rs".to_owned(),
                relative_path: "src/main.rs".to_owned(),
                kind: ProjectFileKind::File,
                children: None,
            }]),
        };

        let node = project_file_tree_node(&entry, Some("src/main.rs"));
        assert!(node.branch);
        assert!(node.expanded);
        assert!(!node.selected);
        assert_eq!(node.children.len(), 1);
        assert!(node.children[0].selected);
    }
}
