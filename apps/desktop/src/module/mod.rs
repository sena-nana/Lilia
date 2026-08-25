//! UI modules: one per domain that has moved out of the shell.

pub mod architecture;
pub mod memory;
pub mod roadmap;

use lilia_kernel::{Feature, FeatureContext, FeatureId, KernelError};

use crate::ui_module::UiModules;

/// Contributes the modules whose domains live in the shell's crate.
///
/// A feature crate cannot contribute one yet: `UiModule` is host vocabulary
/// declared in `apps/desktop`, and a feature crate depending on the host would
/// invert the dependency. So the shell contributes on their behalf, through the
/// same registry a feature would use, which is what keeps the collection path
/// identical once the contract moves to a shared crate.
pub struct ShellUiFeature;

impl Feature for ShellUiFeature {
    fn id(&self) -> FeatureId {
        FeatureId::new("lilia.shell.ui").expect("the shell ui feature id is not blank")
    }

    fn mount(&self, cx: &mut FeatureContext<'_>) -> Result<(), KernelError> {
        cx.contribute::<UiModules>(Box::new(|| {
            Box::new(architecture::ArchitectureModule::default())
        }));
        cx.contribute::<UiModules>(Box::new(|| Box::new(roadmap::RoadmapModule::default())));
        cx.contribute::<UiModules>(Box::new(|| Box::new(memory::MemoryModule::default())));
        Ok(())
    }
}
