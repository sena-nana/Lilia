//! UI modules: one per domain that has moved out of the shell.

pub mod architecture;
pub mod composer;
pub mod documents;
pub mod extensions;
pub mod memory;
pub mod roadmap;
pub mod settings;
pub mod task;
pub mod timeline;

use lilia_kernel::{Feature, FeatureContext, FeatureId, KernelError};

use crate::application::ApplicationWorkspaceSurface;
use crate::runtime_shell::ShellProjectPage;
use crate::ui_module::{UiModuleContext, UiModules};

pub(crate) fn conversation_is_visible(cx: &UiModuleContext<'_>) -> bool {
    if cx.shows_surface(ApplicationWorkspaceSurface::Settings)
        || cx.shows_surface(ApplicationWorkspaceSurface::Automations)
        || cx.shows_surface(ApplicationWorkspaceSurface::Projects)
    {
        return false;
    }
    !(cx.shows(ShellProjectPage::Overview)
        || cx.shows(ShellProjectPage::Clone)
        || cx.shows(ShellProjectPage::Roadmap)
        || cx.shows(ShellProjectPage::Memory)
        || cx.shows(ShellProjectPage::Architecture)
        || cx.shows(ShellProjectPage::Settings)
        || cx.shows(ShellProjectPage::Files))
}

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
        cx.contribute::<UiModules>(Box::new(|| Box::new(composer::ComposerModule::default())));
        cx.contribute::<UiModules>(Box::new(|| {
            Box::new(extensions::ExtensionsModule::default())
        }));
        cx.contribute::<UiModules>(Box::new(|| Box::new(task::TaskModule::default())));
        cx.contribute::<UiModules>(Box::new(|| Box::new(timeline::TimelineModule::default())));
        cx.contribute::<UiModules>(Box::new(|| Box::new(settings::SettingsModule::default())));
        Ok(())
    }
}
