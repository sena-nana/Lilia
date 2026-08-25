use crate::{
    Contribution, Event, EventBus, FeatureId, JobProtocol, Jobs, Journal, Kernel, KernelError,
    ServiceKey, ServiceRef, SubscriptionId,
};

/// A unit of product capability. Everything LiliaCode does beyond the kernel is
/// a feature: it declares the services it needs, publishes the services it owns,
/// and appends its contributions during a single mount pass.
pub trait Feature: Send + Sync + 'static {
    fn id(&self) -> FeatureId;

    /// Services that must already be provided when this feature mounts. Used to
    /// order the mount pass and to fail fast on a missing or cyclic dependency.
    fn requires(&self) -> Vec<ServiceRef> {
        Vec::new()
    }

    /// Services this feature will provide. Declared separately from [`Self::mount`]
    /// so the kernel can order mounts before running any feature code.
    fn provides(&self) -> Vec<ServiceRef> {
        Vec::new()
    }

    /// Long-operation protocols this feature owns. Collected before the task
    /// runtime is built, so a feature never spawns its own worker thread.
    fn protocols(&self) -> Vec<JobProtocol> {
        Vec::new()
    }

    fn mount(&self, cx: &mut FeatureContext<'_>) -> Result<(), KernelError>;
}

/// Mount-time capability handed to a feature. Every registration made through it
/// is recorded so [`Kernel::unmount`] can reverse it.
pub struct FeatureContext<'a> {
    kernel: &'a Kernel,
    feature: FeatureId,
    provided: Vec<ServiceRef>,
    subscriptions: Vec<SubscriptionId>,
}

impl<'a> FeatureContext<'a> {
    pub(crate) fn new(kernel: &'a Kernel, feature: FeatureId) -> Self {
        Self {
            kernel,
            feature,
            provided: Vec::new(),
            subscriptions: Vec::new(),
        }
    }

    pub fn feature_id(&self) -> &FeatureId {
        &self.feature
    }

    pub fn provide<K>(&mut self, value: K::Value) -> Result<(), KernelError>
    where
        K: ServiceKey + ?Sized,
    {
        self.kernel.provide_service::<K>(self.feature.clone(), value)?;
        self.provided.push(ServiceRef::of::<K>());
        Ok(())
    }

    pub fn require<K>(&self) -> Result<K::Value, KernelError>
    where
        K: ServiceKey + ?Sized,
    {
        self.kernel.service::<K>()
    }

    pub fn on<E, F>(&mut self, handler: F)
    where
        E: Event,
        F: Fn(&E) + Send + Sync + 'static,
    {
        let id = self
            .kernel
            .events()
            .on::<E, _>(Some(self.feature.clone()), handler);
        self.subscriptions.push(id);
    }

    pub fn contribute<C>(&mut self, item: C::Item)
    where
        C: Contribution,
    {
        self.kernel.contribute::<C>(self.feature.clone(), item);
    }

    pub fn events(&self) -> &EventBus {
        self.kernel.events()
    }

    pub fn journal(&self) -> &Journal {
        self.kernel.journal()
    }

    pub fn jobs(&self) -> &Jobs {
        self.kernel.jobs()
    }

    pub(crate) fn into_parts(self) -> (Vec<ServiceRef>, Vec<SubscriptionId>) {
        (self.provided, self.subscriptions)
    }
}
