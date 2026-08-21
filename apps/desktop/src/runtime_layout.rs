use std::collections::HashSet;

use nana_ui::runtime::{
    AlignSpec, AppContext, ComponentView, FlexDirection, FrameworkError, InteractionState,
    JustifySpec, LengthSpec, MutationQueue, NodeKind, NodeStyle, StableNodeId,
};

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
    max_width: Option<LengthSpec>,
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
            max_width: None,
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
        let layout = std::sync::Arc::make_mut(&mut style.layout);
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
        layout.max_width = self.max_width;
        if let Some(grow) = self.grow {
            layout.flex_grow = Some(grow);
        }
        if let Some(shrink) = self.shrink {
            layout.flex_shrink = Some(shrink);
        }
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
