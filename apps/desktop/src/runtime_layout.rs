use std::collections::HashSet;
use std::sync::Arc;

use nana_ui::runtime::{
    AlignSpec, AppContext, Button, FrameworkError, IconButton, JustifySpec, LengthSpec,
    MutationQueue, SemanticColorRole, StableNodeId, Stack, TextArea,
};
use nana_ui::{ButtonKind, ControlSize, Icon, UI_METRICS};

const COMPOSER_SEND_SIZE: f32 = 30.0;
const PILL_RADIUS: f32 = 999.0;
pub(crate) const COMPOSER_CARD_RADIUS: f32 = 16.0;

pub(crate) fn composer_card() -> Stack {
    Stack::column(7.0)
        .padding(UI_METRICS.control_padding_x)
        .surface(SemanticColorRole::Surface)
        .outline(SemanticColorRole::Border, 1.0)
        .radius(COMPOSER_CARD_RADIUS)
}

/// Holds the empty-state headline centered in the leftover pane height, and
/// collapses to nothing once a timeline takes over the pane.
pub(crate) fn headline_slot(active: bool) -> Stack {
    if active {
        Stack::fill_column(0.0)
            .align(AlignSpec::Center)
            .justify(JustifySpec::Center)
    } else {
        Stack::column(0.0).height(LengthSpec::Px(0.0))
    }
}

pub(crate) fn trigger_slot(width: f32, height: f32) -> Stack {
    Stack::row(0.0)
        .align(AlignSpec::Center)
        .justify(JustifySpec::Center)
        .width(LengthSpec::Px(width))
        .height(LengthSpec::Px(height))
        .min_width(LengthSpec::Px(width))
        .min_height(LengthSpec::Px(height))
        .grow(0.0)
        .shrink(0.0)
}

pub(crate) fn pending_interaction_card() -> Stack {
    Stack::column(8.0)
        .padding(UI_METRICS.control_padding_x)
        .surface(SemanticColorRole::Surface)
        .outline(SemanticColorRole::Border, 1.0)
        .radius(COMPOSER_CARD_RADIUS)
}

pub(crate) fn pending_actions_row() -> Stack {
    Stack::row(6.0)
}

pub(crate) fn inspector_header_bar() -> Stack {
    Stack::bar(6.0).justify(JustifySpec::SpaceBetween)
}

pub(crate) fn reconcile_children(
    context: &mut AppContext,
    parent: StableNodeId,
    ordered: &[StableNodeId],
) -> Result<(), FrameworkError> {
    let ordered = ordered
        .iter()
        .copied()
        .filter(|id| *id != parent && context.world().contains(*id))
        .collect::<Vec<_>>();
    let current = context
        .world()
        .node(parent)
        .map(|node| node.children.clone())
        .unwrap_or_default();
    if current.as_slice() == ordered.as_slice() {
        return Ok(());
    }
    let keep = ordered.iter().copied().collect::<HashSet<_>>();
    let mut mutations = MutationQueue::new();
    for child in &current {
        if !keep.contains(child) {
            mutations.park_subtree(*child);
        }
    }
    for child in ordered {
        mutations.insert(parent, child, None);
    }
    context.commit_mutations(mutations)?;
    Ok(())
}

fn round_icon_button(icon: Icon, label: &'static str, kind: ButtonKind) -> IconButton {
    let mut button = IconButton::new(icon, label).kind(kind).with_tooltip(label);
    let layout = Arc::make_mut(&mut button.style.layout);
    let edge = LengthSpec::Px(COMPOSER_SEND_SIZE);
    layout.min_width = Some(edge);
    layout.min_height = Some(edge);
    layout.width = Some(edge);
    layout.height = Some(edge);
    layout.padding_left = Some(LengthSpec::Px(0.0));
    layout.padding_right = Some(LengthSpec::Px(0.0));
    layout.border_radius = Some(COMPOSER_SEND_SIZE * 0.5);
    button
}

pub(crate) fn composer_send_button(enabled: bool) -> IconButton {
    round_icon_button(Icon::ArrowUp, "发送", ButtonKind::Primary).disabled(!enabled)
}

pub(crate) fn composer_interrupt_button(enabled: bool) -> IconButton {
    round_icon_button(Icon::Close, "停止", ButtonKind::Danger).disabled(!enabled)
}

pub(crate) fn sidebar_icon_button(icon: Icon, label: &'static str) -> IconButton {
    sized_icon_button(icon, label, ButtonKind::Text, UI_METRICS.icon_button_size)
}

pub(crate) fn window_control(icon: Icon, label: &'static str, kind: ButtonKind) -> IconButton {
    sized_icon_button(icon, label, kind, UI_METRICS.icon_button_size)
}

fn sized_icon_button(icon: Icon, label: &'static str, kind: ButtonKind, edge: f32) -> IconButton {
    let mut button = IconButton::new(icon, label)
        .kind(kind)
        .size(ControlSize::Small)
        .with_tooltip(label);
    let layout = Arc::make_mut(&mut button.style.layout);
    let edge = LengthSpec::Px(edge);
    layout.min_width = Some(edge);
    layout.min_height = Some(edge);
    layout.width = Some(edge);
    layout.height = Some(edge);
    layout.padding_left = Some(LengthSpec::Px(0.0));
    layout.padding_right = Some(LengthSpec::Px(0.0));
    layout.border_radius = Some(UI_METRICS.radius_sm);
    button
}

pub(crate) fn pill_button(label: &str, kind: ButtonKind) -> Button {
    let mut button = Button::new(label).kind(kind).size(ControlSize::Small);
    let layout = Arc::make_mut(&mut button.style.layout);
    layout.min_height = Some(LengthSpec::Px(UI_METRICS.compact_control_height));
    layout.padding_left = Some(LengthSpec::Px(UI_METRICS.compact_control_padding_x));
    layout.padding_right = Some(LengthSpec::Px(UI_METRICS.compact_control_padding_x));
    layout.border_radius = Some(PILL_RADIUS);
    button
}

pub(crate) fn flatten_composer_textarea(area: TextArea) -> TextArea {
    let mut style = area.style.clone();
    style.background = None;
    style.border = None;
    style.interaction.hovered.border = None;
    style.interaction.focused.border = None;
    let layout = Arc::make_mut(&mut style.layout);
    layout.border_width = Some(0.0);
    layout.border_radius = Some(0.0);
    layout.min_height = Some(LengthSpec::Px(ControlSize::Medium.height()));
    layout.padding_left = Some(LengthSpec::Px(UI_METRICS.field_padding_x));
    layout.padding_right = Some(LengthSpec::Px(UI_METRICS.field_padding_x));
    layout.padding_top = Some(LengthSpec::Px(UI_METRICS.field_padding_y));
    layout.padding_bottom = Some(LengthSpec::Px(UI_METRICS.field_padding_y));
    area.style(style)
}
