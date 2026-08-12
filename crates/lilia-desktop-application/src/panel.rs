use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

pub const RESOURCES_PANEL_ID: &str = "resources";
pub const TASK_INSPECTOR_PANEL_ID: &str = "task-inspector";
pub const CODING_TOOLS_PANEL_ID: &str = "coding-tools";
pub const DIAGNOSTICS_PANEL_ID: &str = "diagnostics";

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PanelId(String);

impl PanelId {
    pub fn new(value: impl Into<String>) -> Result<Self, PanelLayoutError> {
        let value = value.into();
        let value = value.trim();
        if value.is_empty() || value.chars().any(char::is_control) {
            return Err(PanelLayoutError::InvalidIdentifier);
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DockSlot {
    Left,
    Right,
    Bottom,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelState {
    pub id: PanelId,
    pub slot: DockSlot,
    pub visible: bool,
    pub active: bool,
    pub extent: f32,
}

impl PanelState {
    pub fn new(id: PanelId, slot: DockSlot, extent: f32) -> Self {
        Self {
            id,
            slot,
            visible: true,
            active: false,
            extent,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PaneId(String);

impl PaneId {
    pub fn new(value: impl Into<String>) -> Result<Self, PanelLayoutError> {
        let value = value.into();
        let value = value.trim();
        if value.is_empty() || value.chars().any(char::is_control) {
            return Err(PanelLayoutError::InvalidIdentifier);
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkspaceItemId(String);

impl WorkspaceItemId {
    pub fn new(value: impl Into<String>) -> Result<Self, PanelLayoutError> {
        let value = value.into();
        let value = value.trim();
        if value.is_empty() || value.chars().any(char::is_control) {
            return Err(PanelLayoutError::InvalidIdentifier);
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitAxis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PaneNode {
    Leaf {
        id: PaneId,
        items: Vec<WorkspaceItemId>,
        active_item: Option<WorkspaceItemId>,
    },
    Split {
        axis: SplitAxis,
        ratio: f32,
        first: Box<PaneNode>,
        second: Box<PaneNode>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelLayoutSnapshot {
    pub schema_version: u32,
    pub panels: Vec<PanelState>,
    #[serde(default = "default_primary_pane_id")]
    pub active_pane: PaneId,
    pub panes: PaneNode,
}

impl Default for PanelLayoutSnapshot {
    fn default() -> Self {
        Self {
            schema_version: 1,
            panels: default_panel_states(),
            active_pane: default_primary_pane_id(),
            panes: PaneNode::Leaf {
                id: default_primary_pane_id(),
                items: Vec::new(),
                active_item: None,
            },
        }
    }
}

impl PanelLayoutSnapshot {
    pub fn validate(&self) -> Result<(), PanelLayoutError> {
        if self.schema_version != 1 {
            return Err(PanelLayoutError::UnsupportedSchemaVersion(
                self.schema_version,
            ));
        }
        let mut panel_ids = BTreeSet::new();
        let mut active_slots = BTreeSet::new();
        for panel in &self.panels {
            if !panel.extent.is_finite() || panel.extent <= 0.0 {
                return Err(PanelLayoutError::InvalidPanelExtent(
                    panel.id.as_str().to_owned(),
                ));
            }
            if !panel_ids.insert(panel.id.clone()) {
                return Err(PanelLayoutError::DuplicatePanel(
                    panel.id.as_str().to_owned(),
                ));
            }
            if panel.active && (!panel.visible || !active_slots.insert(panel.slot)) {
                return Err(PanelLayoutError::InvalidActivePanel(
                    panel.id.as_str().to_owned(),
                ));
            }
        }

        let mut pane_ids = BTreeSet::new();
        let mut item_ids = BTreeSet::new();
        validate_pane_node(&self.panes, &mut pane_ids, &mut item_ids)?;
        if !pane_ids.contains(&self.active_pane) {
            return Err(PanelLayoutError::PaneNotFound(
                self.active_pane.as_str().to_owned(),
            ));
        }
        Ok(())
    }

    pub fn panel(&self, panel_id: &PanelId) -> Option<&PanelState> {
        self.panels.iter().find(|panel| &panel.id == panel_id)
    }

    pub fn active_panel(&self, slot: DockSlot) -> Option<&PanelState> {
        self.panels
            .iter()
            .find(|panel| panel.slot == slot && panel.visible && panel.active)
    }

    pub fn ensure_panel(&mut self, panel: PanelState) -> Result<bool, PanelLayoutError> {
        if self.panel(&panel.id).is_some() {
            return Ok(false);
        }
        let mut next = self.clone();
        next.panels.push(panel);
        next.validate()?;
        *self = next;
        Ok(true)
    }

    pub fn activate_panel(&mut self, panel_id: &PanelId) -> Result<(), PanelLayoutError> {
        let slot = self
            .panel(panel_id)
            .map(|panel| panel.slot)
            .ok_or_else(|| PanelLayoutError::PanelNotFound(panel_id.as_str().to_owned()))?;
        let mut next = self.clone();
        for panel in &mut next.panels {
            if panel.slot == slot {
                panel.active = &panel.id == panel_id;
                if panel.active {
                    panel.visible = true;
                }
            }
        }
        next.validate()?;
        *self = next;
        Ok(())
    }

    pub fn set_panel_visible(
        &mut self,
        panel_id: &PanelId,
        visible: bool,
    ) -> Result<(), PanelLayoutError> {
        let index = self
            .panels
            .iter()
            .position(|panel| &panel.id == panel_id)
            .ok_or_else(|| PanelLayoutError::PanelNotFound(panel_id.as_str().to_owned()))?;
        let mut next = self.clone();
        let slot = next.panels[index].slot;
        let was_active = next.panels[index].active;
        next.panels[index].visible = visible;
        if !visible {
            next.panels[index].active = false;
        } else if !next
            .panels
            .iter()
            .any(|panel| panel.slot == slot && panel.visible && panel.active)
        {
            next.panels[index].active = true;
        }
        if was_active && !visible {
            if let Some(panel) = next
                .panels
                .iter_mut()
                .find(|panel| panel.slot == slot && panel.visible)
            {
                panel.active = true;
            }
        }
        next.validate()?;
        *self = next;
        Ok(())
    }

    pub fn resize_panel(
        &mut self,
        panel_id: &PanelId,
        extent: f32,
    ) -> Result<(), PanelLayoutError> {
        let mut next = self.clone();
        let panel = next
            .panels
            .iter_mut()
            .find(|panel| &panel.id == panel_id)
            .ok_or_else(|| PanelLayoutError::PanelNotFound(panel_id.as_str().to_owned()))?;
        panel.extent = extent;
        next.validate()?;
        *self = next;
        Ok(())
    }

    pub fn open_item(
        &mut self,
        pane_id: &PaneId,
        item_id: WorkspaceItemId,
    ) -> Result<(), PanelLayoutError> {
        if let Some(owner) = pane_for_item(&self.panes, &item_id) {
            if owner != pane_id {
                return Err(PanelLayoutError::DuplicateWorkspaceItem(
                    item_id.as_str().to_owned(),
                ));
            }
        }
        let PaneNode::Leaf {
            items, active_item, ..
        } = pane_leaf_mut(&mut self.panes, pane_id)?
        else {
            unreachable!("pane_leaf_mut returns only leaf panes")
        };
        if !items.contains(&item_id) {
            items.push(item_id.clone());
        }
        *active_item = Some(item_id);
        self.active_pane = pane_id.clone();
        self.validate()
    }

    pub fn activate_item(
        &mut self,
        pane_id: &PaneId,
        item_id: &WorkspaceItemId,
    ) -> Result<(), PanelLayoutError> {
        let PaneNode::Leaf {
            items, active_item, ..
        } = pane_leaf_mut(&mut self.panes, pane_id)?
        else {
            unreachable!("pane_leaf_mut returns only leaf panes")
        };
        if !items.contains(item_id) {
            return Err(PanelLayoutError::WorkspaceItemNotFound(
                item_id.as_str().to_owned(),
            ));
        }
        *active_item = Some(item_id.clone());
        self.active_pane = pane_id.clone();
        self.validate()
    }

    pub fn close_item(
        &mut self,
        pane_id: &PaneId,
        item_id: &WorkspaceItemId,
    ) -> Result<(), PanelLayoutError> {
        let PaneNode::Leaf {
            items, active_item, ..
        } = pane_leaf_mut(&mut self.panes, pane_id)?
        else {
            unreachable!("pane_leaf_mut returns only leaf panes")
        };
        let index = items
            .iter()
            .position(|item| item == item_id)
            .ok_or_else(|| PanelLayoutError::WorkspaceItemNotFound(item_id.as_str().to_owned()))?;
        items.remove(index);
        if active_item.as_ref() == Some(item_id) {
            *active_item = items.get(index.min(items.len().saturating_sub(1))).cloned();
        }
        self.validate()
    }

    pub fn split_pane(
        &mut self,
        pane_id: &PaneId,
        new_pane_id: PaneId,
        axis: SplitAxis,
        ratio: f32,
    ) -> Result<(), PanelLayoutError> {
        if !ratio.is_finite() || ratio <= 0.0 || ratio >= 1.0 {
            return Err(PanelLayoutError::InvalidSplitRatio);
        }
        if pane_exists(&self.panes, &new_pane_id) {
            return Err(PanelLayoutError::DuplicatePane(
                new_pane_id.as_str().to_owned(),
            ));
        }
        split_pane_node(&mut self.panes, pane_id, new_pane_id, axis, ratio)?;
        self.validate()
    }

    pub fn focus_pane(&mut self, pane_id: &PaneId) -> Result<(), PanelLayoutError> {
        pane_leaf(&self.panes, pane_id)?;
        self.active_pane = pane_id.clone();
        self.validate()
    }

    pub fn close_empty_pane(&mut self, pane_id: &PaneId) -> Result<(), PanelLayoutError> {
        let pane_ids = self.pane_ids();
        if !pane_ids.contains(pane_id) {
            return Err(PanelLayoutError::PaneNotFound(pane_id.as_str().to_owned()));
        }
        if pane_ids.len() == 1 {
            return Err(PanelLayoutError::CannotCloseLastPane);
        }
        if !self.pane_items(pane_id)?.is_empty() {
            return Err(PanelLayoutError::PaneNotEmpty(pane_id.as_str().to_owned()));
        }

        let mut next = self.clone();
        close_empty_pane_node(&mut next.panes, pane_id)?;
        if next.active_pane == *pane_id {
            next.active_pane = first_leaf_pane_id(&next.panes).clone();
        }
        next.validate()?;
        *self = next;
        Ok(())
    }

    pub fn resize_split(
        &mut self,
        first_pane_id: &PaneId,
        second_pane_id: &PaneId,
        ratio: f32,
    ) -> Result<(), PanelLayoutError> {
        if !ratio.is_finite() || ratio <= 0.0 || ratio >= 1.0 {
            return Err(PanelLayoutError::InvalidSplitRatio);
        }
        resize_split_node(&mut self.panes, first_pane_id, second_pane_id, ratio)?;
        self.validate()
    }

    pub fn move_item(
        &mut self,
        item_id: &WorkspaceItemId,
        target_pane_id: &PaneId,
        before: Option<&WorkspaceItemId>,
    ) -> Result<(), PanelLayoutError> {
        let source_pane_id = self
            .pane_for_item(item_id)
            .cloned()
            .ok_or_else(|| PanelLayoutError::WorkspaceItemNotFound(item_id.as_str().to_owned()))?;
        pane_leaf(&self.panes, target_pane_id)?;
        if source_pane_id == *target_pane_id && before == Some(item_id) {
            return self.activate_item(target_pane_id, item_id);
        }
        if let Some(before) = before {
            let PaneNode::Leaf { items, .. } = pane_leaf(&self.panes, target_pane_id)? else {
                unreachable!("pane_leaf returns only leaf panes")
            };
            if !items.contains(before) {
                return Err(PanelLayoutError::WorkspaceItemNotFound(
                    before.as_str().to_owned(),
                ));
            }
        }

        let mut next = self.clone();
        remove_pane_item(&mut next.panes, &source_pane_id, item_id)?;
        let PaneNode::Leaf {
            items, active_item, ..
        } = pane_leaf_mut(&mut next.panes, target_pane_id)?
        else {
            unreachable!("pane_leaf_mut returns only leaf panes")
        };
        let insert_at = before
            .and_then(|before| items.iter().position(|item| item == before))
            .unwrap_or(items.len());
        items.insert(insert_at, item_id.clone());
        *active_item = Some(item_id.clone());
        next.active_pane = target_pane_id.clone();
        next.validate()?;
        *self = next;
        Ok(())
    }

    pub fn item_ids(&self) -> Vec<WorkspaceItemId> {
        let mut items = Vec::new();
        collect_item_ids(&self.panes, &mut items);
        items
    }

    pub fn contains_item(&self, item_id: &WorkspaceItemId) -> bool {
        pane_for_item(&self.panes, item_id).is_some()
    }

    pub fn active_item(
        &self,
        pane_id: &PaneId,
    ) -> Result<Option<&WorkspaceItemId>, PanelLayoutError> {
        let PaneNode::Leaf { active_item, .. } = pane_leaf(&self.panes, pane_id)? else {
            unreachable!("pane_leaf returns only leaf panes")
        };
        Ok(active_item.as_ref())
    }

    pub fn active_pane(&self) -> &PaneId {
        &self.active_pane
    }

    pub fn active_workspace_item(&self) -> Result<Option<&WorkspaceItemId>, PanelLayoutError> {
        self.active_item(&self.active_pane)
    }

    pub fn deactivate_active_item(&mut self) -> Result<(), PanelLayoutError> {
        let active_pane = self.active_pane.clone();
        let PaneNode::Leaf { active_item, .. } = pane_leaf_mut(&mut self.panes, &active_pane)?
        else {
            unreachable!("pane_leaf_mut returns only leaf panes")
        };
        *active_item = None;
        self.validate()
    }

    pub fn pane_for_item(&self, item_id: &WorkspaceItemId) -> Option<&PaneId> {
        pane_for_item(&self.panes, item_id)
    }

    pub fn pane_items(&self, pane_id: &PaneId) -> Result<&[WorkspaceItemId], PanelLayoutError> {
        let PaneNode::Leaf { items, .. } = pane_leaf(&self.panes, pane_id)? else {
            unreachable!("pane_leaf returns only leaf panes")
        };
        Ok(items)
    }

    pub fn pane_ids(&self) -> Vec<PaneId> {
        let mut panes = Vec::new();
        collect_pane_ids(&self.panes, &mut panes);
        panes
    }

    pub fn retain_items(&mut self, retained: &BTreeSet<WorkspaceItemId>) {
        retain_pane_items(&mut self.panes, retained);
    }
}

fn default_primary_pane_id() -> PaneId {
    PaneId("primary".to_owned())
}

pub fn default_panel_states() -> Vec<PanelState> {
    vec![
        PanelState {
            id: PanelId(RESOURCES_PANEL_ID.to_owned()),
            slot: DockSlot::Left,
            visible: true,
            active: true,
            extent: 232.0,
        },
        PanelState {
            id: PanelId(TASK_INSPECTOR_PANEL_ID.to_owned()),
            slot: DockSlot::Right,
            visible: false,
            active: false,
            extent: 300.0,
        },
        PanelState {
            id: PanelId(CODING_TOOLS_PANEL_ID.to_owned()),
            slot: DockSlot::Right,
            visible: false,
            active: false,
            extent: 360.0,
        },
        PanelState {
            id: PanelId(DIAGNOSTICS_PANEL_ID.to_owned()),
            slot: DockSlot::Bottom,
            visible: false,
            active: false,
            extent: 240.0,
        },
    ]
}

fn remove_pane_item(
    node: &mut PaneNode,
    pane_id: &PaneId,
    item_id: &WorkspaceItemId,
) -> Result<(), PanelLayoutError> {
    let PaneNode::Leaf {
        items, active_item, ..
    } = pane_leaf_mut(node, pane_id)?
    else {
        unreachable!("pane_leaf_mut returns only leaf panes")
    };
    let index = items
        .iter()
        .position(|item| item == item_id)
        .ok_or_else(|| PanelLayoutError::WorkspaceItemNotFound(item_id.as_str().to_owned()))?;
    items.remove(index);
    if active_item.as_ref() == Some(item_id) {
        *active_item = items.get(index.min(items.len().saturating_sub(1))).cloned();
    }
    Ok(())
}

fn collect_item_ids(node: &PaneNode, items: &mut Vec<WorkspaceItemId>) {
    match node {
        PaneNode::Leaf {
            items: pane_items, ..
        } => items.extend(pane_items.iter().cloned()),
        PaneNode::Split { first, second, .. } => {
            collect_item_ids(first, items);
            collect_item_ids(second, items);
        }
    }
}

fn collect_pane_ids(node: &PaneNode, panes: &mut Vec<PaneId>) {
    match node {
        PaneNode::Leaf { id, .. } => panes.push(id.clone()),
        PaneNode::Split { first, second, .. } => {
            collect_pane_ids(first, panes);
            collect_pane_ids(second, panes);
        }
    }
}

fn first_leaf_pane_id(node: &PaneNode) -> &PaneId {
    match node {
        PaneNode::Leaf { id, .. } => id,
        PaneNode::Split { first, .. } => first_leaf_pane_id(first),
    }
}

fn close_empty_pane_node(node: &mut PaneNode, pane_id: &PaneId) -> Result<(), PanelLayoutError> {
    let PaneNode::Split { first, second, .. } = node else {
        return Err(PanelLayoutError::PaneNotFound(pane_id.as_str().to_owned()));
    };
    if matches!(first.as_ref(), PaneNode::Leaf { id, .. } if id == pane_id) {
        *node = second.as_ref().clone();
        return Ok(());
    }
    if matches!(second.as_ref(), PaneNode::Leaf { id, .. } if id == pane_id) {
        *node = first.as_ref().clone();
        return Ok(());
    }
    if pane_exists(first, pane_id) {
        close_empty_pane_node(first, pane_id)
    } else if pane_exists(second, pane_id) {
        close_empty_pane_node(second, pane_id)
    } else {
        Err(PanelLayoutError::PaneNotFound(pane_id.as_str().to_owned()))
    }
}

fn resize_split_node(
    node: &mut PaneNode,
    first_pane_id: &PaneId,
    second_pane_id: &PaneId,
    next_ratio: f32,
) -> Result<(), PanelLayoutError> {
    let PaneNode::Split {
        ratio,
        first,
        second,
        ..
    } = node
    else {
        return Err(PanelLayoutError::SplitNotFound {
            first: first_pane_id.as_str().to_owned(),
            second: second_pane_id.as_str().to_owned(),
        });
    };
    if first_pane_id == first_leaf_pane_id(first) && second_pane_id == first_leaf_pane_id(second) {
        *ratio = next_ratio;
        return Ok(());
    }
    if pane_exists(first, first_pane_id) && pane_exists(first, second_pane_id) {
        resize_split_node(first, first_pane_id, second_pane_id, next_ratio)
    } else if pane_exists(second, first_pane_id) && pane_exists(second, second_pane_id) {
        resize_split_node(second, first_pane_id, second_pane_id, next_ratio)
    } else {
        Err(PanelLayoutError::SplitNotFound {
            first: first_pane_id.as_str().to_owned(),
            second: second_pane_id.as_str().to_owned(),
        })
    }
}

fn retain_pane_items(node: &mut PaneNode, retained: &BTreeSet<WorkspaceItemId>) {
    match node {
        PaneNode::Leaf {
            items, active_item, ..
        } => {
            let active_index = active_item
                .as_ref()
                .and_then(|active| items.iter().position(|item| item == active));
            items.retain(|item| retained.contains(item));
            if active_item
                .as_ref()
                .is_some_and(|active| !retained.contains(active))
            {
                *active_item = active_index
                    .and_then(|index| items.get(index.min(items.len().saturating_sub(1))))
                    .cloned();
            }
        }
        PaneNode::Split { first, second, .. } => {
            retain_pane_items(first, retained);
            retain_pane_items(second, retained);
        }
    }
}

fn pane_exists(node: &PaneNode, pane_id: &PaneId) -> bool {
    match node {
        PaneNode::Leaf { id, .. } => id == pane_id,
        PaneNode::Split { first, second, .. } => {
            pane_exists(first, pane_id) || pane_exists(second, pane_id)
        }
    }
}

fn pane_for_item<'a>(node: &'a PaneNode, item_id: &WorkspaceItemId) -> Option<&'a PaneId> {
    match node {
        PaneNode::Leaf { id, items, .. } => items.contains(item_id).then_some(id),
        PaneNode::Split { first, second, .. } => {
            pane_for_item(first, item_id).or_else(|| pane_for_item(second, item_id))
        }
    }
}

fn pane_leaf_mut<'a>(
    node: &'a mut PaneNode,
    pane_id: &PaneId,
) -> Result<&'a mut PaneNode, PanelLayoutError> {
    match node {
        PaneNode::Leaf { id, .. } if id == pane_id => Ok(node),
        PaneNode::Leaf { .. } => Err(PanelLayoutError::PaneNotFound(pane_id.as_str().to_owned())),
        PaneNode::Split { first, second, .. } => {
            if pane_exists(first, pane_id) {
                pane_leaf_mut(first, pane_id)
            } else if pane_exists(second, pane_id) {
                pane_leaf_mut(second, pane_id)
            } else {
                Err(PanelLayoutError::PaneNotFound(pane_id.as_str().to_owned()))
            }
        }
    }
}

fn pane_leaf<'a>(node: &'a PaneNode, pane_id: &PaneId) -> Result<&'a PaneNode, PanelLayoutError> {
    match node {
        PaneNode::Leaf { id, .. } if id == pane_id => Ok(node),
        PaneNode::Leaf { .. } => Err(PanelLayoutError::PaneNotFound(pane_id.as_str().to_owned())),
        PaneNode::Split { first, second, .. } => {
            if pane_exists(first, pane_id) {
                pane_leaf(first, pane_id)
            } else if pane_exists(second, pane_id) {
                pane_leaf(second, pane_id)
            } else {
                Err(PanelLayoutError::PaneNotFound(pane_id.as_str().to_owned()))
            }
        }
    }
}

fn split_pane_node(
    node: &mut PaneNode,
    pane_id: &PaneId,
    new_pane_id: PaneId,
    axis: SplitAxis,
    ratio: f32,
) -> Result<(), PanelLayoutError> {
    match node {
        PaneNode::Leaf { id, .. } if id == pane_id => {
            let first = Box::new(node.clone());
            *node = PaneNode::Split {
                axis,
                ratio,
                first,
                second: Box::new(PaneNode::Leaf {
                    id: new_pane_id,
                    items: Vec::new(),
                    active_item: None,
                }),
            };
            Ok(())
        }
        PaneNode::Leaf { .. } => Err(PanelLayoutError::PaneNotFound(pane_id.as_str().to_owned())),
        PaneNode::Split { first, second, .. } => {
            if pane_exists(first, pane_id) {
                split_pane_node(first, pane_id, new_pane_id, axis, ratio)
            } else if pane_exists(second, pane_id) {
                split_pane_node(second, pane_id, new_pane_id, axis, ratio)
            } else {
                Err(PanelLayoutError::PaneNotFound(pane_id.as_str().to_owned()))
            }
        }
    }
}

fn validate_pane_node(
    node: &PaneNode,
    pane_ids: &mut BTreeSet<PaneId>,
    item_ids: &mut BTreeSet<WorkspaceItemId>,
) -> Result<(), PanelLayoutError> {
    match node {
        PaneNode::Leaf {
            id,
            items,
            active_item,
        } => {
            if !pane_ids.insert(id.clone()) {
                return Err(PanelLayoutError::DuplicatePane(id.as_str().to_owned()));
            }
            for item in items {
                if !item_ids.insert(item.clone()) {
                    return Err(PanelLayoutError::DuplicateWorkspaceItem(
                        item.as_str().to_owned(),
                    ));
                }
            }
            if active_item
                .as_ref()
                .is_some_and(|active| !items.contains(active))
            {
                return Err(PanelLayoutError::MissingActiveWorkspaceItem(
                    id.as_str().to_owned(),
                ));
            }
            Ok(())
        }
        PaneNode::Split {
            ratio,
            first,
            second,
            ..
        } => {
            if !ratio.is_finite() || *ratio <= 0.0 || *ratio >= 1.0 {
                return Err(PanelLayoutError::InvalidSplitRatio);
            }
            validate_pane_node(first, pane_ids, item_ids)?;
            validate_pane_node(second, pane_ids, item_ids)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PanelLayoutError {
    #[error("panel, pane, and item identifiers must not be empty or contain control characters")]
    InvalidIdentifier,
    #[error("unsupported panel layout schema version {0}")]
    UnsupportedSchemaVersion(u32),
    #[error("panel `{0}` has an invalid extent")]
    InvalidPanelExtent(String),
    #[error("panel `{0}` appears more than once")]
    DuplicatePanel(String),
    #[error("panel `{0}` does not exist")]
    PanelNotFound(String),
    #[error("panel `{0}` cannot be active in its current dock state")]
    InvalidActivePanel(String),
    #[error("pane `{0}` appears more than once")]
    DuplicatePane(String),
    #[error("pane `{0}` does not exist")]
    PaneNotFound(String),
    #[error("the last workspace pane cannot be closed")]
    CannotCloseLastPane,
    #[error("pane `{0}` must be empty before it can be closed")]
    PaneNotEmpty(String),
    #[error("pane split between `{first}` and `{second}` does not exist")]
    SplitNotFound { first: String, second: String },
    #[error("workspace item `{0}` appears in more than one pane")]
    DuplicateWorkspaceItem(String),
    #[error("workspace item `{0}` does not exist in the target pane")]
    WorkspaceItemNotFound(String),
    #[error("pane `{0}` references an active item that it does not contain")]
    MissingActiveWorkspaceItem(String),
    #[error("pane split ratio must be finite and strictly between zero and one")]
    InvalidSplitRatio,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_rejects_duplicate_items_across_panes() {
        let item = WorkspaceItemId::new("document:readme").unwrap();
        let layout = PanelLayoutSnapshot {
            panes: PaneNode::Split {
                axis: SplitAxis::Horizontal,
                ratio: 0.5,
                first: Box::new(PaneNode::Leaf {
                    id: PaneId::new("left").unwrap(),
                    items: vec![item.clone()],
                    active_item: Some(item.clone()),
                }),
                second: Box::new(PaneNode::Leaf {
                    id: PaneId::new("right").unwrap(),
                    items: vec![item.clone()],
                    active_item: Some(item),
                }),
            },
            ..PanelLayoutSnapshot::default()
        };

        assert!(matches!(
            layout.validate(),
            Err(PanelLayoutError::DuplicateWorkspaceItem(item)) if item == "document:readme"
        ));
    }

    #[test]
    fn default_layout_is_a_valid_ide_shell_layout() {
        let layout = PanelLayoutSnapshot::default();
        layout.validate().unwrap();
        assert_eq!(layout.panels.len(), 4);
        assert_eq!(
            layout
                .active_panel(DockSlot::Left)
                .map(|panel| panel.id.as_str()),
            Some(RESOURCES_PANEL_ID)
        );
        assert!(layout.active_panel(DockSlot::Right).is_none());
        assert!(layout.active_panel(DockSlot::Bottom).is_none());
    }

    #[test]
    fn dock_panels_activate_resize_and_fall_back_within_their_slot() {
        let mut layout = PanelLayoutSnapshot::default();
        let task = PanelId::new(TASK_INSPECTOR_PANEL_ID).unwrap();
        let tools = PanelId::new(CODING_TOOLS_PANEL_ID).unwrap();

        layout.activate_panel(&task).unwrap();
        assert_eq!(layout.active_panel(DockSlot::Right).unwrap().id, task);
        layout.activate_panel(&tools).unwrap();
        assert_eq!(layout.active_panel(DockSlot::Right).unwrap().id, tools);
        assert!(layout.panel(&task).unwrap().visible);

        layout.resize_panel(&tools, 412.0).unwrap();
        assert_eq!(layout.panel(&tools).unwrap().extent, 412.0);
        layout.set_panel_visible(&tools, false).unwrap();
        assert_eq!(layout.active_panel(DockSlot::Right).unwrap().id, task);
        layout.set_panel_visible(&task, false).unwrap();
        assert!(layout.active_panel(DockSlot::Right).is_none());
        layout.validate().unwrap();
    }

    #[test]
    fn ensure_panel_migrates_older_snapshots_without_replacing_existing_state() {
        let mut layout = PanelLayoutSnapshot::default();
        layout.panels.truncate(1);
        let tools = PanelState::new(
            PanelId::new(CODING_TOOLS_PANEL_ID).unwrap(),
            DockSlot::Right,
            360.0,
        );

        assert!(layout.ensure_panel(tools.clone()).unwrap());
        assert!(!layout.ensure_panel(tools).unwrap());
        assert_eq!(layout.panels[0].extent, 232.0);
        layout.validate().unwrap();
    }

    #[test]
    fn item_commands_keep_tabs_unique_and_choose_a_neighbor_when_closing() {
        let mut layout = PanelLayoutSnapshot::default();
        let pane = PaneId::new("primary").unwrap();
        let first = WorkspaceItemId::new("task:first").unwrap();
        let second = WorkspaceItemId::new("task:second").unwrap();
        layout.open_item(&pane, first.clone()).unwrap();
        layout.open_item(&pane, second.clone()).unwrap();
        layout.activate_item(&pane, &first).unwrap();
        layout.close_item(&pane, &first).unwrap();

        assert!(matches!(
            &layout.panes,
            PaneNode::Leaf { items, active_item, .. }
                if items == &vec![second.clone()] && active_item.as_ref() == Some(&second)
        ));
    }

    #[test]
    fn activation_tracks_the_focused_pane_and_overview_keeps_open_tabs() {
        let mut layout = PanelLayoutSnapshot::default();
        let primary = PaneId::new("primary").unwrap();
        let secondary = PaneId::new("secondary").unwrap();
        let first = WorkspaceItemId::new("task:first").unwrap();
        let second = WorkspaceItemId::new("task:second").unwrap();
        layout.open_item(&primary, first.clone()).unwrap();
        layout
            .split_pane(&primary, secondary.clone(), SplitAxis::Horizontal, 0.5)
            .unwrap();
        layout.open_item(&secondary, second.clone()).unwrap();

        assert_eq!(layout.active_pane(), &secondary);
        assert_eq!(layout.active_workspace_item(), Ok(Some(&second)));
        layout.deactivate_active_item().unwrap();
        assert_eq!(layout.active_workspace_item(), Ok(None));
        assert_eq!(layout.item_ids(), vec![first, second]);
    }

    #[test]
    fn moving_items_preserves_source_neighbors_and_supports_target_reordering() {
        let mut layout = PanelLayoutSnapshot::default();
        let primary = PaneId::new("primary").unwrap();
        let secondary = PaneId::new("secondary").unwrap();
        let first = WorkspaceItemId::new("task:first").unwrap();
        let second = WorkspaceItemId::new("task:second").unwrap();
        layout.open_item(&primary, first.clone()).unwrap();
        layout.open_item(&primary, second.clone()).unwrap();
        layout
            .split_pane(&primary, secondary.clone(), SplitAxis::Horizontal, 0.5)
            .unwrap();

        layout.move_item(&second, &secondary, None).unwrap();
        assert_eq!(layout.pane_items(&primary), Ok([first.clone()].as_slice()));
        assert_eq!(layout.active_item(&primary), Ok(Some(&first)));
        assert_eq!(
            layout.pane_items(&secondary),
            Ok([second.clone()].as_slice())
        );
        assert_eq!(layout.active_workspace_item(), Ok(Some(&second)));

        layout.move_item(&first, &secondary, Some(&second)).unwrap();
        assert_eq!(
            layout.pane_items(&secondary),
            Ok([first.clone(), second.clone()].as_slice())
        );
        layout.move_item(&second, &secondary, Some(&first)).unwrap();
        assert_eq!(
            layout.pane_items(&secondary),
            Ok([second.clone(), first].as_slice())
        );
        assert_eq!(layout.active_workspace_item(), Ok(Some(&second)));
        assert!(layout.pane_items(&primary).unwrap().is_empty());
    }

    #[test]
    fn pane_split_preserves_the_existing_leaf_and_creates_an_empty_peer() {
        let mut layout = PanelLayoutSnapshot::default();
        let primary = PaneId::new("primary").unwrap();
        let secondary = PaneId::new("secondary").unwrap();
        let item = WorkspaceItemId::new("automation:one").unwrap();
        layout.open_item(&primary, item.clone()).unwrap();
        layout
            .split_pane(&primary, secondary.clone(), SplitAxis::Horizontal, 0.6)
            .unwrap();

        assert!(matches!(
            &layout.panes,
            PaneNode::Split { first, second, .. }
                if matches!(first.as_ref(), PaneNode::Leaf { items, .. } if items == &vec![item])
                    && matches!(second.as_ref(), PaneNode::Leaf { id, items, .. } if id == &secondary && items.is_empty())
        ));
    }

    #[test]
    fn nested_split_ratios_resize_by_stable_child_pane_anchors() {
        let mut layout = PanelLayoutSnapshot::default();
        let primary = PaneId::new("primary").unwrap();
        let secondary = PaneId::new("secondary").unwrap();
        let nested = PaneId::new("nested").unwrap();
        layout
            .split_pane(&primary, secondary.clone(), SplitAxis::Horizontal, 0.5)
            .unwrap();
        layout
            .split_pane(&primary, nested.clone(), SplitAxis::Vertical, 0.4)
            .unwrap();

        layout.resize_split(&primary, &nested, 0.25).unwrap();
        layout.resize_split(&primary, &secondary, 0.7).unwrap();

        let PaneNode::Split {
            ratio: root_ratio,
            first,
            ..
        } = &layout.panes
        else {
            panic!("root remains split");
        };
        let PaneNode::Split {
            ratio: nested_ratio,
            ..
        } = first.as_ref()
        else {
            panic!("first child remains split");
        };
        assert_eq!((*root_ratio, *nested_ratio), (0.7, 0.25));
        assert!(matches!(
            layout.resize_split(&nested, &secondary, 0.5),
            Err(PanelLayoutError::SplitNotFound { .. })
        ));
    }

    #[test]
    fn empty_panes_can_be_closed_and_the_tree_collapses_to_the_remaining_peer() {
        let mut layout = PanelLayoutSnapshot::default();
        let primary = PaneId::new("primary").unwrap();
        let secondary = PaneId::new("secondary").unwrap();
        let item = WorkspaceItemId::new("task:one").unwrap();
        layout.open_item(&primary, item.clone()).unwrap();
        layout
            .split_pane(&primary, secondary.clone(), SplitAxis::Vertical, 0.5)
            .unwrap();
        layout.focus_pane(&secondary).unwrap();

        layout.close_empty_pane(&secondary).unwrap();

        assert_eq!(layout.pane_ids(), vec![primary.clone()]);
        assert_eq!(layout.active_pane(), &primary);
        assert_eq!(layout.active_workspace_item(), Ok(Some(&item)));
        assert_eq!(
            layout.close_empty_pane(&primary),
            Err(PanelLayoutError::CannotCloseLastPane)
        );
    }
}
