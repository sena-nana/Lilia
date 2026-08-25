use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use lilia_contracts::{
    SidebarNavigationContribution, SidebarNavigationContributionError,
    SidebarNavigationContributionSet, SidebarNavigationIcon, SidebarNavigationTarget,
    SIDEBAR_NAVIGATION_EXTENSION_ID, SIDEBAR_NAVIGATION_SCHEMA_VERSION,
};
use mutsuki_runtime_contracts::{
    ExtensionProjection, PluginDeploymentKind, PluginExtensionDescriptor, RuntimeProfile,
    RuntimeProfileMode,
};
use mutsuki_runtime_host::{HostRuntime, HostRuntimeConfig, RuntimeBootstrapper};
use mutsuki_runtime_sdk::{
    HostEffect, HostEffectFuture, HostEffectKind, PluginBuilder, RuntimeResult,
};

const WORKBENCH_PLUGIN_ID: &str = "lilia.workbench";
const DEFAULT_PROFILE_ID: &str = "liliacode.default";
const BUILTIN_GENERATION: u64 = 1;

pub(crate) struct LiliaContributionHost {
    registry: Arc<SidebarNavigationRegistry>,
    _runtime: HostRuntime,
}

impl LiliaContributionHost {
    pub(crate) fn bootstrap() -> Result<Self, LiliaContributionError> {
        let registry = Arc::new(SidebarNavigationRegistry::default());
        let contributions = builtin_sidebar_navigation();
        let payload = serde_json::to_value(&contributions)
            .map_err(|error| LiliaContributionError::Encode(error.to_string()))?;
        let registration =
            registry.register(WORKBENCH_PLUGIN_ID, BUILTIN_GENERATION, contributions)?;
        let extension = PluginExtensionDescriptor {
            extension_id: SIDEBAR_NAVIGATION_EXTENSION_ID.into(),
            version: SIDEBAR_NAVIGATION_SCHEMA_VERSION,
            projection: ExtensionProjection::Required,
            payload,
        };
        let plugin = PluginBuilder::new(WORKBENCH_PLUGIN_ID)
            .extension(extension)
            .host_effect(HostEffectKind::HostLocal, Box::new(registration))
            .build();
        let mut bootstrapper = RuntimeBootstrapper::new();
        bootstrapper.register_loaded_plugin(plugin);
        let runtime = bootstrapper
            .into_host_runtime_with_config(default_profile(), contribution_host_config())
            .map_err(|error| LiliaContributionError::Runtime(error.to_string()))?;

        Ok(Self {
            registry,
            _runtime: runtime,
        })
    }

    pub(crate) fn sidebar_navigation(&self) -> Vec<SidebarNavigationContribution> {
        self.registry.snapshot()
    }
}

fn default_profile() -> RuntimeProfile {
    RuntimeProfile {
        profile_id: DEFAULT_PROFILE_ID.into(),
        mode: RuntimeProfileMode::LockedBuiltin,
        enabled_plugins: vec![WORKBENCH_PLUGIN_ID.into()],
        bindings: BTreeMap::new(),
        surface_bindings: BTreeMap::new(),
        supported_extensions: vec![SIDEBAR_NAVIGATION_EXTENSION_ID.into()],
        plugin_deployments: BTreeMap::from([(
            WORKBENCH_PLUGIN_ID.into(),
            PluginDeploymentKind::Builtin,
        )]),
        observability: Default::default(),
        allow_dynamic_registration: false,
        allow_hot_reload: false,
    }
}

fn contribution_host_config() -> HostRuntimeConfig {
    HostRuntimeConfig {
        event_driven: true,
        worker_threads: 1,
        blocking_threads: 1,
        management_threads: 1,
        ..HostRuntimeConfig::default()
    }
}

fn builtin_sidebar_navigation() -> SidebarNavigationContributionSet {
    SidebarNavigationContributionSet {
        items: vec![
            SidebarNavigationContribution {
                id: "lilia.sidebar.footer.settings".into(),
                label: "设置".into(),
                icon: SidebarNavigationIcon::Settings,
                target: SidebarNavigationTarget::Settings,
                order: 100,
            },
            SidebarNavigationContribution {
                id: "lilia.sidebar.footer.automations".into(),
                label: "自动化".into(),
                icon: SidebarNavigationIcon::Automations,
                target: SidebarNavigationTarget::Automations,
                order: 200,
            },
        ],
    }
}

#[derive(Default)]
struct SidebarNavigationRegistry {
    state: Mutex<SidebarNavigationRegistryState>,
    next_token: AtomicU64,
}

#[derive(Default)]
struct SidebarNavigationRegistryState {
    entries: BTreeMap<String, OwnedSidebarNavigationContribution>,
}

struct OwnedSidebarNavigationContribution {
    owner_plugin_id: String,
    generation: u64,
    token: u64,
    contribution: SidebarNavigationContribution,
}

impl SidebarNavigationRegistry {
    fn register(
        self: &Arc<Self>,
        owner_plugin_id: &str,
        generation: u64,
        contributions: SidebarNavigationContributionSet,
    ) -> Result<SidebarNavigationRegistration, LiliaContributionError> {
        contributions.validate()?;
        let token = self.next_token.fetch_add(1, Ordering::Relaxed) + 1;
        let mut state = self.state.lock().expect("sidebar registry lock poisoned");
        if let Some(id) = contributions
            .items
            .iter()
            .map(|item| item.id.as_str())
            .find(|id| state.entries.contains_key(*id))
        {
            return Err(LiliaContributionError::DuplicateActiveId(id.into()));
        }
        for contribution in contributions.items {
            state.entries.insert(
                contribution.id.clone(),
                OwnedSidebarNavigationContribution {
                    owner_plugin_id: owner_plugin_id.into(),
                    generation,
                    token,
                    contribution,
                },
            );
        }
        drop(state);
        Ok(SidebarNavigationRegistration {
            registry: Arc::clone(self),
            owner_plugin_id: owner_plugin_id.into(),
            generation,
            token,
            active: true,
        })
    }

    fn snapshot(&self) -> Vec<SidebarNavigationContribution> {
        let mut entries = self
            .state
            .lock()
            .expect("sidebar registry lock poisoned")
            .entries
            .values()
            .map(|entry| entry.contribution.clone())
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| {
            left.order
                .cmp(&right.order)
                .then_with(|| left.id.cmp(&right.id))
        });
        entries
    }

    fn remove(&self, owner_plugin_id: &str, generation: u64, token: u64) {
        let mut state = self.state.lock().expect("sidebar registry lock poisoned");
        state.entries.retain(|_, entry| {
            entry.owner_plugin_id != owner_plugin_id
                || entry.generation != generation
                || entry.token != token
        });
    }
}

struct SidebarNavigationRegistration {
    registry: Arc<SidebarNavigationRegistry>,
    owner_plugin_id: String,
    generation: u64,
    token: u64,
    active: bool,
}

impl SidebarNavigationRegistration {
    fn unregister(&mut self) {
        if self.active {
            self.registry
                .remove(&self.owner_plugin_id, self.generation, self.token);
            self.active = false;
        }
    }
}

impl HostEffect for SidebarNavigationRegistration {
    fn dispose(&mut self) -> HostEffectFuture<'_> {
        self.unregister();
        Box::pin(async { RuntimeResult::Ok(()) })
    }
}

impl Drop for SidebarNavigationRegistration {
    fn drop(&mut self) {
        self.unregister();
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum LiliaContributionError {
    #[error(transparent)]
    Contract(#[from] SidebarNavigationContributionError),
    #[error("sidebar navigation contribution id `{0}` is already active")]
    DuplicateActiveId(String),
    #[error("failed to encode Lilia contribution: {0}")]
    Encode(String),
    #[error("failed to start Lilia contribution Host: {0}")]
    Runtime(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_host_projects_ordered_sidebar_navigation_and_disposes_scope_effects() {
        let host = LiliaContributionHost::bootstrap().unwrap();
        let registry = Arc::clone(&host.registry);

        let snapshot = host.sidebar_navigation();
        assert_eq!(
            snapshot.iter().map(|item| item.target).collect::<Vec<_>>(),
            [
                SidebarNavigationTarget::Settings,
                SidebarNavigationTarget::Automations,
            ]
        );

        drop(host);
        assert!(registry.snapshot().is_empty());
    }

    #[test]
    fn registration_is_atomic_and_stale_cleanup_cannot_remove_a_new_generation() {
        let registry = Arc::new(SidebarNavigationRegistry::default());
        let contributions = builtin_sidebar_navigation();
        let mut first = registry
            .register(WORKBENCH_PLUGIN_ID, 1, contributions.clone())
            .unwrap();
        assert!(matches!(
            registry.register("lilia.other", 1, contributions.clone()),
            Err(LiliaContributionError::DuplicateActiveId(_))
        ));
        assert_eq!(registry.snapshot().len(), 2);

        registry.remove(&first.owner_plugin_id, first.generation, first.token);
        let second = registry
            .register(WORKBENCH_PLUGIN_ID, 2, contributions)
            .unwrap();
        first.unregister();
        assert_eq!(registry.snapshot().len(), 2);

        drop(second);
        assert!(registry.snapshot().is_empty());

        let mut same_order = builtin_sidebar_navigation();
        same_order.items[0].id = "lilia.sidebar.footer.z".into();
        same_order.items[0].order = 10;
        same_order.items[1].id = "lilia.sidebar.footer.a".into();
        same_order.items[1].order = 10;
        let registration = registry
            .register(WORKBENCH_PLUGIN_ID, 3, same_order)
            .unwrap();
        assert_eq!(
            registry
                .snapshot()
                .into_iter()
                .map(|item| item.id)
                .collect::<Vec<_>>(),
            ["lilia.sidebar.footer.a", "lilia.sidebar.footer.z"]
        );
        drop(registration);
    }

    #[test]
    fn failed_profile_activation_rolls_back_registered_contributions() {
        let registry = Arc::new(SidebarNavigationRegistry::default());
        let contributions = builtin_sidebar_navigation();
        let payload = serde_json::to_value(&contributions).unwrap();
        let registration = registry
            .register(WORKBENCH_PLUGIN_ID, BUILTIN_GENERATION, contributions)
            .unwrap();
        let plugin = PluginBuilder::new(WORKBENCH_PLUGIN_ID)
            .extension(PluginExtensionDescriptor {
                extension_id: SIDEBAR_NAVIGATION_EXTENSION_ID.into(),
                version: SIDEBAR_NAVIGATION_SCHEMA_VERSION,
                projection: ExtensionProjection::Required,
                payload,
            })
            .host_effect(HostEffectKind::HostLocal, Box::new(registration))
            .build();
        let mut bootstrapper = RuntimeBootstrapper::new();
        bootstrapper.register_loaded_plugin(plugin);
        let mut unsupported_profile = default_profile();
        unsupported_profile.supported_extensions.clear();

        assert!(bootstrapper
            .into_host_runtime_with_config(unsupported_profile, contribution_host_config())
            .is_err());
        assert!(registry.snapshot().is_empty());
    }
}
