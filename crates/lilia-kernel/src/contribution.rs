use std::any::{Any, TypeId};
use std::collections::HashMap;

use crate::FeatureId;

/// Declares one collection features append to during mount. Collections keep
/// host-specific vocabulary out of the kernel: the UI crate declares a
/// `UiModules` contribution, the agent crate declares `AgentTools`, and the
/// kernel only stores and orders them.
pub trait Contribution: 'static {
    type Item: Send + 'static;

    const NAME: &'static str;
}

struct ContributionEntry {
    contributor: FeatureId,
    item: Box<dyn Any + Send>,
}

#[derive(Default)]
pub struct ContributionRegistry {
    entries: HashMap<TypeId, Vec<ContributionEntry>>,
}

impl ContributionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn insert<C>(&mut self, contributor: FeatureId, item: C::Item)
    where
        C: Contribution,
    {
        self.entries
            .entry(TypeId::of::<C>())
            .or_default()
            .push(ContributionEntry {
                contributor,
                item: Box::new(item),
            });
    }

    /// Items in mount order, paired with the feature that contributed them.
    pub fn items<C>(&self) -> Vec<(&FeatureId, &C::Item)>
    where
        C: Contribution,
    {
        self.entries
            .get(&TypeId::of::<C>())
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|entry| {
                        entry
                            .item
                            .downcast_ref::<C::Item>()
                            .map(|item| (&entry.contributor, item))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn count<C>(&self) -> usize
    where
        C: Contribution,
    {
        self.entries
            .get(&TypeId::of::<C>())
            .map(Vec::len)
            .unwrap_or(0)
    }

    /// Removes every item contributed by `contributor`, preserving the relative
    /// order of the remaining items.
    pub(crate) fn revoke_all(&mut self, contributor: &FeatureId) {
        for entries in self.entries.values_mut() {
            entries.retain(|entry| &entry.contributor != contributor);
        }
    }

    /// Drains items of one collection so the host can take ownership. Used by
    /// hosts that must move non-`Sync` items, such as UI modules.
    pub fn take<C>(&mut self) -> Vec<(FeatureId, C::Item)>
    where
        C: Contribution,
    {
        self.entries
            .remove(&TypeId::of::<C>())
            .map(|entries| {
                entries
                    .into_iter()
                    .filter_map(|entry| {
                        entry
                            .item
                            .downcast::<C::Item>()
                            .ok()
                            .map(|item| (entry.contributor, *item))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}
