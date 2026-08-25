use std::any::{Any, TypeId};
use std::collections::HashMap;

use crate::{FeatureId, KernelError, ServiceRef};

/// Declares one service slot. Implement it for the trait object a feature hands
/// to consumers so the slot carries a diagnosable name:
///
/// ```
/// use std::sync::Arc;
/// use lilia_kernel::ServiceKey;
///
/// trait ProjectStore: Send + Sync {
///     fn count(&self) -> usize;
/// }
///
/// impl ServiceKey for dyn ProjectStore {
///     type Value = Arc<dyn ProjectStore>;
///     const NAME: &'static str = "lilia.project.store";
/// }
/// ```
pub trait ServiceKey: 'static {
    /// Value handed to consumers, typically `Arc<dyn Trait>`.
    type Value: Clone + Send + Sync + 'static;

    const NAME: &'static str;
}

struct ServiceEntry {
    name: &'static str,
    provider: FeatureId,
    value: Box<dyn Any + Send + Sync>,
}

#[derive(Default)]
pub struct ServiceRegistry {
    entries: HashMap<TypeId, ServiceEntry>,
}

impl ServiceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn provide<K>(
        &mut self,
        provider: FeatureId,
        value: K::Value,
    ) -> Result<(), KernelError>
    where
        K: ServiceKey + ?Sized,
    {
        let slot = ServiceRef::of::<K>();
        if let Some(existing) = self.entries.get(&slot.type_id()) {
            return Err(KernelError::duplicate_service(slot, existing.provider.clone()));
        }
        self.entries.insert(
            slot.type_id(),
            ServiceEntry {
                name: slot.name(),
                provider,
                value: Box::new(value),
            },
        );
        Ok(())
    }

    pub fn resolve<K>(&self) -> Result<K::Value, KernelError>
    where
        K: ServiceKey + ?Sized,
    {
        let slot = ServiceRef::of::<K>();
        let entry = self
            .entries
            .get(&slot.type_id())
            .ok_or(KernelError::MissingService(slot.name()))?;
        entry
            .value
            .downcast_ref::<K::Value>()
            .cloned()
            .ok_or(KernelError::ServiceTypeMismatch {
                service: slot.name(),
            })
    }

    pub fn contains(&self, slot: ServiceRef) -> bool {
        self.entries.contains_key(&slot.type_id())
    }

    pub fn provider(&self, slot: ServiceRef) -> Option<&FeatureId> {
        self.entries
            .get(&slot.type_id())
            .map(|entry| &entry.provider)
    }

    pub(crate) fn revoke(&mut self, slot: ServiceRef) {
        self.entries.remove(&slot.type_id());
    }

    /// Slot names in stable order, for diagnostics and debug snapshots.
    pub fn slot_names(&self) -> Vec<&'static str> {
        let mut names = self
            .entries
            .values()
            .map(|entry| entry.name)
            .collect::<Vec<_>>();
        names.sort_unstable();
        names
    }
}
