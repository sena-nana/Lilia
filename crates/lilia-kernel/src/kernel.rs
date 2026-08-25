use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex, MutexGuard, RwLock, RwLockReadGuard};

use crate::{
    Contribution, ContributionRegistry, EventBus, Feature, FeatureContext, FeatureId, Jobs, Journal,
    KernelError, RecordKind, ServiceKey, ServiceRef, ServiceRegistry, SubscriptionId,
};

struct MountRecord {
    feature: Arc<dyn Feature>,
    services: Vec<ServiceRef>,
    subscriptions: Vec<SubscriptionId>,
}

/// Composition root. Holds no product knowledge: it resolves services, orders
/// feature mounts, fans out typed events, keeps the append-only journal, and
/// owns the single job facade.
pub struct Kernel {
    services: RwLock<ServiceRegistry>,
    contributions: RwLock<ContributionRegistry>,
    events: EventBus,
    journal: Journal,
    jobs: Jobs,
    mounted: Mutex<Vec<MountRecord>>,
}

impl Default for Kernel {
    fn default() -> Self {
        Self::new()
    }
}

impl Kernel {
    pub fn new() -> Self {
        let events = EventBus::new();
        let journal = Journal::new();
        let jobs = Jobs::new(events.clone(), journal.clone());
        Self {
            services: RwLock::new(ServiceRegistry::new()),
            contributions: RwLock::new(ContributionRegistry::new()),
            events,
            journal,
            jobs,
            mounted: Mutex::new(Vec::new()),
        }
    }

    pub fn events(&self) -> &EventBus {
        &self.events
    }

    pub fn journal(&self) -> &Journal {
        &self.journal
    }

    pub fn jobs(&self) -> &Jobs {
        &self.jobs
    }

    pub fn service<K>(&self) -> Result<K::Value, KernelError>
    where
        K: ServiceKey + ?Sized,
    {
        self.services_read().resolve::<K>()
    }

    pub fn has_service(&self, slot: ServiceRef) -> bool {
        self.services_read().contains(slot)
    }

    pub fn contributions(&self) -> RwLockReadGuard<'_, ContributionRegistry> {
        self.contributions
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Removes and returns one contribution collection. Hosts use it for items
    /// they must own, such as UI modules bound to the render thread.
    pub fn take_contributions<C>(&self) -> Vec<(FeatureId, C::Item)>
    where
        C: Contribution,
    {
        self.contributions
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take::<C>()
    }

    pub fn mounted_features(&self) -> Vec<FeatureId> {
        self.mounted()
            .iter()
            .map(|record| record.feature.id())
            .collect()
    }

    /// Mounts one feature immediately. Its requirements must already be
    /// satisfied; use [`Self::mount_all`] to let the kernel order a set.
    pub fn mount(&self, feature: Arc<dyn Feature>) -> Result<(), KernelError> {
        let id = feature.id();
        if self
            .mounted()
            .iter()
            .any(|record| record.feature.id() == id)
        {
            return Err(KernelError::DuplicateFeature(id));
        }
        for requirement in feature.requires() {
            if !self.has_service(requirement) {
                return Err(KernelError::UnsatisfiedRequirement {
                    feature: id.clone(),
                    service: requirement.name(),
                });
            }
        }

        let declared = feature.provides();
        let mut context = FeatureContext::new(self, id.clone());
        feature.mount(&mut context)?;
        let (services, subscriptions) = context.into_parts();

        if let Some(missing) = declared
            .iter()
            .find(|slot| !services.contains(slot) && !self.has_service(**slot))
        {
            self.revoke(&id, &services, &subscriptions);
            return Err(KernelError::MissingService(missing.name()));
        }

        self.mounted().push(MountRecord {
            feature,
            services,
            subscriptions,
        });
        self.journal.append(
            RecordKind::Lifecycle,
            "feature.mounted",
            Some(id.to_string()),
            serde_json::json!({ "feature": id.as_str() }),
        );
        Ok(())
    }

    /// Orders `features` by their declared dependencies and mounts them.
    ///
    /// Ordering happens before any feature code runs, so a missing provider or a
    /// dependency cycle fails without partially initialising the process.
    pub fn mount_all(
        &self,
        features: impl IntoIterator<Item = Arc<dyn Feature>>,
    ) -> Result<(), KernelError> {
        let features = features.into_iter().collect::<Vec<_>>();
        for feature in resolve_mount_order(features, |slot| self.has_service(slot))? {
            self.mount(feature)?;
        }
        Ok(())
    }

    /// Reverses every registration the feature made at mount time.
    pub fn unmount(&self, feature: &FeatureId) -> Result<(), KernelError> {
        let record = {
            let mut mounted = self.mounted();
            let position = mounted
                .iter()
                .position(|record| &record.feature.id() == feature)
                .ok_or_else(|| KernelError::UnknownFeature {
                    feature: feature.clone(),
                })?;
            mounted.remove(position)
        };
        self.revoke(feature, &record.services, &record.subscriptions);
        self.journal.append(
            RecordKind::Lifecycle,
            "feature.unmounted",
            Some(feature.to_string()),
            serde_json::json!({ "feature": feature.as_str() }),
        );
        Ok(())
    }

    pub fn shutdown(&self) {
        self.jobs.shutdown();
        for feature in self.mounted_features().into_iter().rev() {
            let _ = self.unmount(&feature);
        }
    }

    pub(crate) fn provide_service<K>(
        &self,
        provider: FeatureId,
        value: K::Value,
    ) -> Result<(), KernelError>
    where
        K: ServiceKey + ?Sized,
    {
        self.services
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .provide::<K>(provider, value)
    }

    pub(crate) fn contribute<C>(&self, contributor: FeatureId, item: C::Item)
    where
        C: Contribution,
    {
        self.contributions
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert::<C>(contributor, item);
    }

    fn revoke(
        &self,
        feature: &FeatureId,
        services: &[ServiceRef],
        subscriptions: &[SubscriptionId],
    ) {
        {
            let mut registry = self
                .services
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            for slot in services {
                registry.revoke(*slot);
            }
        }
        for subscription in subscriptions {
            self.events.unsubscribe(*subscription);
        }
        self.events.unsubscribe_owner(feature);
        self.contributions
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .revoke_all(feature);
    }

    fn services_read(&self) -> RwLockReadGuard<'_, ServiceRegistry> {
        self.services
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn mounted(&self) -> MutexGuard<'_, Vec<MountRecord>> {
        self.mounted
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Kahn ordering over declared `provides`/`requires`. `already_available`
/// reports services satisfied outside this batch, such as by an earlier mount.
fn resolve_mount_order(
    features: Vec<Arc<dyn Feature>>,
    already_available: impl Fn(ServiceRef) -> bool,
) -> Result<Vec<Arc<dyn Feature>>, KernelError> {
    let mut providers: HashMap<ServiceRef, FeatureId> = HashMap::new();
    for feature in &features {
        for slot in feature.provides() {
            if let Some(existing) = providers.insert(slot, feature.id()) {
                return Err(KernelError::duplicate_service(slot, existing));
            }
        }
    }

    let mut pending: BTreeMap<FeatureId, BTreeSet<FeatureId>> = BTreeMap::new();
    let mut by_id: BTreeMap<FeatureId, Arc<dyn Feature>> = BTreeMap::new();
    for feature in features {
        let id = feature.id();
        if by_id.insert(id.clone(), Arc::clone(&feature)).is_some() {
            return Err(KernelError::DuplicateFeature(id));
        }
        let mut dependencies = BTreeSet::new();
        for slot in feature.requires() {
            match providers.get(&slot) {
                Some(provider) if provider != &id => {
                    dependencies.insert(provider.clone());
                }
                Some(_) => {}
                None if already_available(slot) => {}
                None => {
                    return Err(KernelError::UnsatisfiedRequirement {
                        feature: id,
                        service: slot.name(),
                    })
                }
            }
        }
        pending.insert(id, dependencies);
    }

    let mut ordered = Vec::with_capacity(pending.len());
    while !pending.is_empty() {
        let ready = pending
            .iter()
            .filter(|(_, dependencies)| dependencies.is_empty())
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        if ready.is_empty() {
            let cycle = pending
                .keys()
                .map(FeatureId::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            return Err(KernelError::DependencyCycle(cycle));
        }
        for id in ready {
            pending.remove(&id);
            for dependencies in pending.values_mut() {
                dependencies.remove(&id);
            }
            if let Some(feature) = by_id.remove(&id) {
                ordered.push(feature);
            }
        }
    }
    Ok(ordered)
}
