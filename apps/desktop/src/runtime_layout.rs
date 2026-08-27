use std::collections::HashSet;
use std::sync::Arc;

use nana_ui::runtime::{
    AlignSpec, AppContext, Button, ComponentView, FlexDirection, FrameworkError, IconButton,
    InteractionState, JustifySpec, LengthSpec, MutationQueue, NodeKind, NodeStyle,
    SemanticColorRole, StableNodeId, TextArea,
};
use nana_ui::{ButtonKind, ControlSize, Icon, UI_METRICS};

const COMPOSER_SEND_SIZE: f32 = 30.0;
const PILL_RADIUS: f32 = 999.0;
pub(crate) const COMPOSER_CARD_RADIUS: f32 = 16.0;

#[derive(Clone)]
pub(crate) struct HostStack {
    direction: FlexDirection,
    gap: f32,
    align: AlignSpec,
    justify: JustifySpec,
    width: Option<LengthSpec>,
    height: Option<LengthSpec>,
    min_width: Option<LengthSpec>,
    min_height: Option<LengthSpec>,
    grow: Option<f32>,
    shrink: Option<f32>,
    padding: Option<f32>,
    padding_x: Option<f32>,
    padding_y: Option<f32>,
    max_width: Option<LengthSpec>,
    background: Option<SemanticColorRole>,
    border: Option<SemanticColorRole>,
    border_width: Option<f32>,
    radius: Option<f32>,
}

impl HostStack {
    fn base(direction: FlexDirection, gap: f32, align: AlignSpec, justify: JustifySpec) -> Self {
        Self {
            direction,
            gap,
            align,
            justify,
            width: None,
            height: None,
            min_width: None,
            min_height: None,
            grow: None,
            shrink: None,
            padding: None,
            padding_x: None,
            padding_y: None,
            max_width: None,
            background: None,
            border: None,
            border_width: None,
            radius: None,
        }
    }

    pub(crate) fn composer_card() -> Self {
        Self::column(7.0)
            .padding(UI_METRICS.control_padding_x)
            .surface(SemanticColorRole::Surface)
            .outline(SemanticColorRole::Border, 1.0)
            .radius(COMPOSER_CARD_RADIUS)
    }

    /// Holds the empty-state headline centered in the leftover pane height, and
    /// collapses to nothing once a timeline takes over the pane.
    pub(crate) fn headline_slot(active: bool) -> Self {
        if active {
            Self::fill_column(0.0)
                .align(AlignSpec::Center)
                .justify(JustifySpec::Center)
        } else {
            Self::column(0.0).height(LengthSpec::Px(0.0))
        }
    }

    pub(crate) fn row(gap: f32) -> Self {
        Self::base(FlexDirection::Row, gap, AlignSpec::Center, JustifySpec::End)
            .width(LengthSpec::Shrink)
    }

    pub(crate) fn leading_row(gap: f32) -> Self {
        Self::base(
            FlexDirection::Row,
            gap,
            AlignSpec::Center,
            JustifySpec::Start,
        )
        .width(LengthSpec::Shrink)
    }

    pub(crate) fn fill_column(gap: f32) -> Self {
        Self::base(
            FlexDirection::Column,
            gap,
            AlignSpec::Stretch,
            JustifySpec::Start,
        )
        .width(LengthSpec::Fill)
        .height(LengthSpec::Fill)
        .min_width(LengthSpec::Px(0.0))
        .min_height(LengthSpec::Px(0.0))
        .grow(1.0)
        .shrink(1.0)
    }

    pub(crate) fn column(gap: f32) -> Self {
        Self::base(
            FlexDirection::Column,
            gap,
            AlignSpec::Stretch,
            JustifySpec::Start,
        )
        .width(LengthSpec::Fill)
        .height(LengthSpec::Shrink)
        .min_width(LengthSpec::Px(0.0))
        .grow(0.0)
        .shrink(0.0)
    }

    pub(crate) fn bar(gap: f32) -> Self {
        Self::base(
            FlexDirection::Row,
            gap,
            AlignSpec::Center,
            JustifySpec::Start,
        )
        .width(LengthSpec::Fill)
        .grow(0.0)
        .shrink(0.0)
    }

    pub(crate) fn trigger_slot(width: f32, height: f32) -> Self {
        Self::base(
            FlexDirection::Row,
            0.0,
            AlignSpec::Center,
            JustifySpec::Center,
        )
        .width(LengthSpec::Px(width))
        .height(LengthSpec::Px(height))
        .min_width(LengthSpec::Px(width))
        .min_height(LengthSpec::Px(height))
        .grow(0.0)
        .shrink(0.0)
    }

    pub(crate) fn fill_row(gap: f32) -> Self {
        Self::base(
            FlexDirection::Row,
            gap,
            AlignSpec::Center,
            JustifySpec::Start,
        )
        .width(LengthSpec::Fill)
        .grow(1.0)
        .shrink(1.0)
    }

    fn width(mut self, width: LengthSpec) -> Self {
        self.width = Some(width);
        self
    }

    fn height(mut self, height: LengthSpec) -> Self {
        self.height = Some(height);
        self
    }

    fn min_width(mut self, width: LengthSpec) -> Self {
        self.min_width = Some(width);
        self
    }

    fn min_height(mut self, height: LengthSpec) -> Self {
        self.min_height = Some(height);
        self
    }

    fn grow(mut self, grow: f32) -> Self {
        self.grow = Some(grow);
        self
    }

    fn shrink(mut self, shrink: f32) -> Self {
        self.shrink = Some(shrink);
        self
    }

    pub(crate) fn padding(mut self, padding: f32) -> Self {
        self.padding = Some(padding);
        self
    }

    pub(crate) fn padding_xy(mut self, padding_x: f32, padding_y: f32) -> Self {
        self.padding_x = Some(padding_x);
        self.padding_y = Some(padding_y);
        self
    }

    pub(crate) fn justify(mut self, justify: JustifySpec) -> Self {
        self.justify = justify;
        self
    }

    fn surface(mut self, background: SemanticColorRole) -> Self {
        self.background = Some(background);
        self
    }

    fn outline(mut self, border: SemanticColorRole, width: f32) -> Self {
        self.border = Some(border);
        self.border_width = Some(width);
        self
    }

    pub(crate) fn radius(mut self, radius: f32) -> Self {
        self.radius = Some(radius);
        self
    }

    pub(crate) fn max_width(mut self, width: f32) -> Self {
        self.max_width = Some(LengthSpec::Px(width));
        self
    }

    pub(crate) fn align(mut self, align: AlignSpec) -> Self {
        self.align = align;
        self
    }
}

impl ComponentView for HostStack {
    fn node_kind(&self) -> NodeKind {
        NodeKind::Element {
            tag: "stack".into(),
        }
    }

    fn project(
        &self,
        id: StableNodeId,
        world: &nana_ui::runtime::UiWorld,
        mutations: &mut MutationQueue,
    ) {
        let mut style = NodeStyle::default();
        let layout = Arc::make_mut(&mut style.layout);
        layout.direction = Some(self.direction);
        layout.gap = Some(LengthSpec::Px(self.gap));
        layout.align_items = self.align;
        layout.justify_content = self.justify;
        layout.width = self.width;
        layout.height = self.height;
        layout.min_width = self.min_width;
        layout.min_height = self.min_height;
        if let Some(padding) = self.padding {
            layout.padding = Some(LengthSpec::Px(padding));
        }
        if let Some(padding_x) = self.padding_x {
            let value = LengthSpec::Px(padding_x);
            layout.padding_left = Some(value);
            layout.padding_right = Some(value);
        }
        if let Some(padding_y) = self.padding_y {
            let value = LengthSpec::Px(padding_y);
            layout.padding_top = Some(value);
            layout.padding_bottom = Some(value);
        }
        if let Some(width) = self.border_width {
            layout.border_width = Some(width);
        }
        if let Some(radius) = self.radius {
            layout.border_radius = Some(radius);
        }
        layout.max_width = self.max_width;
        if let Some(grow) = self.grow {
            layout.flex_grow = Some(grow);
        }
        if let Some(shrink) = self.shrink {
            layout.flex_shrink = Some(shrink);
        }
        style.background = self.background;
        style.border = self.border;
        if world.node_style(id) != Some(&style) {
            mutations.set_style(id, style);
        }
        let interaction = InteractionState {
            pointer_events: false,
            focusable: false,
        };
        if world.interaction(id) != Some(interaction) {
            mutations.set_interaction(id, interaction);
        }
    }
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
    let mut button = IconButton::new(icon, label).kind(kind);
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
        .size(ControlSize::Small);
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
