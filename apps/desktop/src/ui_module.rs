//! The UI module contract.
//!
//! A module owns one domain: its state is private, its message vocabulary is its
//! own, and it reaches shared facts only through the kernel. That last part is
//! what makes the state private in practice — a module that had to be handed the
//! project list would keep a copy of it, and the copy is what drifts.
//!
//! Modules fold their slice into the window projection rather than returning a
//! projection of their own. The shell still assembles one snapshot per frame, so
//! a domain can move into a module without touching the reconciler or the
//! domains that have not moved yet.

use std::any::Any;
use std::collections::HashMap;

use lilia_kernel::{Contribution, FeatureId, Kernel};

use crate::runtime_shell::PrimaryShellSnapshot;

/// What the shell owes a module: the kernel, and nothing else.
///
/// Everything a domain needs to read is a service slot, so widening this struct
/// is the signal that a fact has no owner yet.
pub struct UiModuleContext<'a> {
    kernel: &'a Kernel,
}

impl<'a> UiModuleContext<'a> {
    pub fn new(kernel: &'a Kernel) -> Self {
        Self { kernel }
    }

    pub fn kernel(&self) -> &Kernel {
        self.kernel
    }
}

/// What a module asks the shell to do after handling a message.
///
/// A module cannot reach the window, the persistence path or another domain, so
/// anything crossing those lines leaves as a request the shell fulfils.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct UiModuleOutcome {
    /// Whether this module's slice of the projection changed.
    pub dirty: bool,
    /// Message to surface in the shell's error strip.
    pub error: Option<String>,
}

impl UiModuleOutcome {
    /// The message was handled and changed nothing worth redrawing.
    pub fn clean() -> Self {
        Self::default()
    }

    /// The module's slice changed and has to be reprojected.
    pub fn dirty() -> Self {
        Self {
            dirty: true,
            error: None,
        }
    }

    pub fn failed(error: impl Into<String>) -> Self {
        Self {
            dirty: true,
            error: Some(error.into()),
        }
    }
}

/// One UI domain.
///
/// Implemented with the domain's own message type; the shell stores modules
/// erased and routes to them by [`FeatureId`], so a module can come from a
/// feature crate without the shell naming its type.
pub trait UiModule: 'static {
    /// The domain's slice of the shell's message vocabulary.
    type Message: 'static;

    /// The feature that owns this domain. Messages are routed by it.
    fn feature(&self) -> FeatureId;

    fn reduce(&mut self, message: Self::Message, cx: &UiModuleContext<'_>) -> UiModuleOutcome;

    /// Writes this module's own fields of the projection and no others.
    fn project(&self, cx: &UiModuleContext<'_>, into: &mut PrimaryShellSnapshot);
}

/// Object-safe face of [`UiModule`], so the shell can hold a heterogeneous set.
///
/// Not implemented by hand: the blanket impl below derives it from every
/// [`UiModule`], which keeps the typed message in the implementation and the
/// downcast confined to this file.
pub trait ErasedUiModule {
    fn feature(&self) -> FeatureId;

    /// Applies a message already routed to this module. A payload of the wrong
    /// type means the shell's routing table disagrees with the module's message
    /// type, which is a wiring bug rather than a runtime condition.
    fn reduce_erased(
        &mut self,
        message: Box<dyn Any>,
        cx: &UiModuleContext<'_>,
    ) -> UiModuleOutcome;

    fn project(&self, cx: &UiModuleContext<'_>, into: &mut PrimaryShellSnapshot);
}

impl<M> ErasedUiModule for M
where
    M: UiModule,
{
    fn feature(&self) -> FeatureId {
        UiModule::feature(self)
    }

    fn reduce_erased(
        &mut self,
        message: Box<dyn Any>,
        cx: &UiModuleContext<'_>,
    ) -> UiModuleOutcome {
        match message.downcast::<M::Message>() {
            Ok(message) => self.reduce(*message, cx),
            Err(_) => UiModuleOutcome::failed(format!(
                "a message reached {} that its module cannot read",
                UiModule::feature(self).as_str()
            )),
        }
    }

    fn project(&self, cx: &UiModuleContext<'_>, into: &mut PrimaryShellSnapshot) {
        UiModule::project(self, cx, into)
    }
}

/// The collection features append their UI modules to during mount.
///
/// Declared by the shell rather than the kernel because a module is host
/// vocabulary: the kernel stores the items and preserves mount order without
/// knowing what a projection is.
pub enum UiModules {}

impl Contribution for UiModules {
    /// `Send` because a module is built on the mounting thread and then moved to
    /// the UI thread, where it stays. It is never shared across threads, which
    /// is why this is not `Sync`.
    type Item = Box<dyn ErasedUiModule + Send>;

    const NAME: &'static str = "lilia.shell.ui_modules";
}

/// The modules a shell window hosts, indexed for routing.
///
/// Registration order is projection order, so a module added later cannot
/// silently overwrite an earlier one's fields — it would have to claim the same
/// fields, which is the conflict this ordering makes visible.
#[derive(Default)]
pub struct UiModuleHost {
    modules: Vec<Box<dyn ErasedUiModule>>,
    routes: HashMap<FeatureId, usize>,
}

impl UiModuleHost {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a module. Two modules claiming one feature is a composition bug, so
    /// the second is refused rather than shadowing the first.
    pub fn register(&mut self, module: Box<dyn ErasedUiModule>) -> Result<(), String> {
        let feature = module.feature();
        if self.routes.contains_key(&feature) {
            return Err(format!(
                "{} already has a UI module registered",
                feature.as_str()
            ));
        }
        self.routes.insert(feature, self.modules.len());
        self.modules.push(module);
        Ok(())
    }

    pub fn hosts(&self, feature: &FeatureId) -> bool {
        self.routes.contains_key(feature)
    }

    /// Routes one message to the module owning `feature`.
    ///
    /// Returns `None` when no module owns the domain yet, which is how the shell
    /// tells a migrated domain from one it still handles itself.
    pub fn reduce(
        &mut self,
        feature: &FeatureId,
        message: Box<dyn Any>,
        cx: &UiModuleContext<'_>,
    ) -> Option<UiModuleOutcome> {
        let index = *self.routes.get(feature)?;
        Some(self.modules[index].reduce_erased(message, cx))
    }

    /// Folds every module's slice into the projection the shell is building.
    pub fn project(&self, cx: &UiModuleContext<'_>, into: &mut PrimaryShellSnapshot) {
        for module in &self.modules {
            module.project(cx, into);
        }
    }

    /// Takes every module contributed during mount, in mount order.
    ///
    /// Draining rather than borrowing is what lets a module be `!Sync`: the host
    /// owns it outright for the rest of the process.
    pub fn from_kernel(kernel: &Kernel) -> Result<Self, String> {
        let mut host = Self::new();
        for (contributor, module) in kernel.take_contributions::<UiModules>() {
            if module.feature() != contributor {
                return Err(format!(
                    "{} contributed a UI module claiming {}",
                    contributor.as_str(),
                    module.feature().as_str()
                ));
            }
            host.register(module)?;
        }
        Ok(host)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes one snapshot field, so a test can tell whose slice landed.
    struct Titler {
        feature: &'static str,
        title: String,
    }

    enum TitlerMessage {
        Set(String),
    }

    impl UiModule for Titler {
        type Message = TitlerMessage;

        fn feature(&self) -> FeatureId {
            FeatureId::new(self.feature).expect("the test feature id is not blank")
        }

        fn reduce(&mut self, message: Self::Message, _cx: &UiModuleContext<'_>) -> UiModuleOutcome {
            match message {
                TitlerMessage::Set(title) if title == self.title => UiModuleOutcome::clean(),
                TitlerMessage::Set(title) => {
                    self.title = title;
                    UiModuleOutcome::dirty()
                }
            }
        }

        fn project(&self, _cx: &UiModuleContext<'_>, into: &mut PrimaryShellSnapshot) {
            into.title = self.title.clone();
        }
    }

    /// Claims a different field, to prove folding composes instead of clobbering.
    struct Header {
        heading: String,
    }

    impl UiModule for Header {
        type Message = ();

        fn feature(&self) -> FeatureId {
            FeatureId::new("test.header").expect("the test feature id is not blank")
        }

        fn reduce(&mut self, _message: (), _cx: &UiModuleContext<'_>) -> UiModuleOutcome {
            UiModuleOutcome::clean()
        }

        fn project(&self, _cx: &UiModuleContext<'_>, into: &mut PrimaryShellSnapshot) {
            into.heading = self.heading.clone();
        }
    }

    fn titler(feature: &'static str) -> Box<dyn ErasedUiModule> {
        Box::new(Titler {
            feature,
            title: String::new(),
        })
    }

    #[test]
    fn a_message_reaches_only_the_module_that_owns_its_domain() {
        let kernel = Kernel::new();
        let cx = UiModuleContext::new(&kernel);
        let mut host = UiModuleHost::new();
        host.register(titler("test.titler"))
            .expect("the slot is free");
        host.register(Box::new(Header {
            heading: "untouched".to_owned(),
        }))
        .expect("the slot is free");

        let titler = FeatureId::new("test.titler").unwrap();
        let outcome = host
            .reduce(
                &titler,
                Box::new(TitlerMessage::Set("moved".to_owned())),
                &cx,
            )
            .expect("the titler is hosted");
        assert!(outcome.dirty);

        let mut snapshot = crate::runtime_shell::empty_snapshot();
        host.project(&cx, &mut snapshot);
        assert_eq!(snapshot.title, "moved");
        assert_eq!(snapshot.heading, "untouched");
    }

    #[test]
    fn an_unclaimed_domain_reports_no_module_so_the_shell_keeps_handling_it() {
        let kernel = Kernel::new();
        let cx = UiModuleContext::new(&kernel);
        let mut host = UiModuleHost::new();
        host.register(titler("test.left")).expect("the slot is free");

        let absent = FeatureId::new("test.absent").unwrap();
        assert!(host
            .reduce(&absent, Box::new(TitlerMessage::Set(String::new())), &cx)
            .is_none());
    }

    /// Contributes a module the way a feature crate would, so the test covers
    /// the mount-time path rather than direct registration.
    struct HeaderFeature;

    impl lilia_kernel::Feature for HeaderFeature {
        fn id(&self) -> FeatureId {
            FeatureId::new("test.header").expect("the test feature id is not blank")
        }

        fn mount(
            &self,
            cx: &mut lilia_kernel::FeatureContext<'_>,
        ) -> Result<(), lilia_kernel::KernelError> {
            cx.contribute::<UiModules>(Box::new(Header {
                heading: "from the feature".to_owned(),
            }));
            Ok(())
        }
    }

    #[test]
    fn a_module_contributed_at_mount_time_reaches_the_host() {
        let kernel = Kernel::new();
        kernel
            .mount_all(vec![
                std::sync::Arc::new(HeaderFeature) as std::sync::Arc<dyn lilia_kernel::Feature>,
            ])
            .expect("the feature mounts");

        let host = UiModuleHost::from_kernel(&kernel).expect("the contributed module is accepted");
        assert!(host.hosts(&FeatureId::new("test.header").unwrap()));

        let cx = UiModuleContext::new(&kernel);
        let mut snapshot = crate::runtime_shell::empty_snapshot();
        host.project(&cx, &mut snapshot);
        assert_eq!(snapshot.heading, "from the feature");
    }

    #[test]
    fn two_modules_cannot_claim_one_domain() {
        let mut host = UiModuleHost::new();
        host.register(titler("test.left")).expect("the slot is free");

        assert!(host.register(titler("test.left")).is_err());
    }

    #[test]
    fn each_module_folds_its_own_slice_without_erasing_the_others() {
        let kernel = Kernel::new();
        let cx = UiModuleContext::new(&kernel);
        let mut host = UiModuleHost::new();
        host.register(Box::new(Titler {
            feature: "test.titler",
            title: "a title".to_owned(),
        }))
        .expect("the slot is free");
        host.register(Box::new(Header {
            heading: "a heading".to_owned(),
        }))
        .expect("the slot is free");

        let mut snapshot = crate::runtime_shell::empty_snapshot();
        host.project(&cx, &mut snapshot);

        assert_eq!(snapshot.title, "a title");
        assert_eq!(snapshot.heading, "a heading");
    }
}
