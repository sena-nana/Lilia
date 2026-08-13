#[cfg(test)]
mod command_contract;
mod contract;
mod model;
mod types;

pub(crate) use types::{SuggestionItem, SuggestionSettings};

use lilia_desktop_application::{
    ConversationSuggestionModelPort, DesktopApplication, DesktopConversationSuggestionSettings,
    DesktopConversationSuggestionSource, DesktopSuggestionItem, DesktopSuggestionItemSource,
    DesktopSuggestionLocalGitProbe, DesktopSuggestionSourceProbe,
};
use tauri::{AppHandle, Manager, State};

use crate::store::LiliaStore;

use model::TauriSuggestionModelPort;
use types::{
    SuggestionItemSource, SuggestionLocalGitProbe, SuggestionSource, SuggestionSourceProbe,
};

fn suggestion_settings_from_shared(
    settings: DesktopConversationSuggestionSettings,
) -> SuggestionSettings {
    SuggestionSettings {
        enabled: settings.enabled,
        source: match settings.source {
            DesktopConversationSuggestionSource::Provider => SuggestionSource::Provider,
            DesktopConversationSuggestionSource::AssistantAi => SuggestionSource::AssistantAi,
        },
    }
}

fn shared_from_suggestion_settings(
    settings: &SuggestionSettings,
) -> DesktopConversationSuggestionSettings {
    DesktopConversationSuggestionSettings {
        enabled: settings.enabled,
        source: match settings.source {
            SuggestionSource::Provider => DesktopConversationSuggestionSource::Provider,
            SuggestionSource::AssistantAi => DesktopConversationSuggestionSource::AssistantAi,
        },
    }
}

fn map_item_source(source: DesktopSuggestionItemSource) -> SuggestionItemSource {
    match source {
        DesktopSuggestionItemSource::Task => SuggestionItemSource::Task,
        DesktopSuggestionItemSource::Github => SuggestionItemSource::Github,
        DesktopSuggestionItemSource::LocalGit => SuggestionItemSource::LocalGit,
        DesktopSuggestionItemSource::SessionThread => SuggestionItemSource::SessionThread,
        DesktopSuggestionItemSource::Provider => SuggestionItemSource::Provider,
    }
}

fn map_item(item: DesktopSuggestionItem) -> SuggestionItem {
    SuggestionItem {
        id: item.id,
        project_id: item.project_id,
        task_ids: item.task_ids,
        source: map_item_source(item.source),
        github_activities: item
            .github_activities
            .into_iter()
            .map(|activity| types::SuggestionGitHubActivityRef {
                id: activity.id,
                repo_full_name: activity.repo_full_name,
                kind: activity.kind,
                title: activity.title,
                url: activity.url,
            })
            .collect(),
        local_git_contexts: item
            .local_git_contexts
            .into_iter()
            .map(|context| types::SuggestionLocalGitContextRef {
                id: context.id,
                branch: context.branch,
                status: context.status,
                changed_files: context.changed_files,
                recent_commits: context.recent_commits,
            })
            .collect(),
        codex_threads: item
            .codex_threads
            .into_iter()
            .map(|thread| types::SuggestionCodexThreadRef {
                id: thread.id,
                title: thread.title,
                updated_at: thread.updated_at,
                preview: thread.preview,
            })
            .collect(),
        summary: item.summary,
        reason: item.reason,
        prompt: item.prompt,
        generated_at: item.generated_at,
    }
}

fn map_source_probe(probe: DesktopSuggestionSourceProbe) -> SuggestionSourceProbe {
    SuggestionSourceProbe {
        sources: probe.sources.into_iter().map(map_item_source).collect(),
        local_git: probe
            .local_git
            .map(
                |probe: DesktopSuggestionLocalGitProbe| SuggestionLocalGitProbe {
                    has_recent_commits: probe.has_recent_commits,
                    has_changed_files: probe.has_changed_files,
                },
            ),
    }
}

fn require_application(app: &AppHandle) -> Result<DesktopApplication, String> {
    app.try_state::<DesktopApplication>()
        .map(|state| state.inner().clone())
        .ok_or_else(|| "DesktopApplication unavailable".to_string())
}

#[tauri::command]
pub fn conversation_suggestions_get_settings(app: AppHandle) -> SuggestionSettings {
    require_application(&app)
        .and_then(|application| {
            application
                .conversation_suggestion_settings()
                .map(suggestion_settings_from_shared)
                .map_err(|error| error.to_string())
        })
        .unwrap_or_default()
}

#[tauri::command]
pub fn conversation_suggestions_set_settings(
    app: AppHandle,
    settings: SuggestionSettings,
) -> Result<(), String> {
    let application = require_application(&app)?;
    let normalized = SuggestionSettings {
        enabled: settings.enabled,
        source: settings.source,
    };
    application
        .save_conversation_suggestion_settings(shared_from_suggestion_settings(&normalized))
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn conversation_suggestions_get_sources(
    app: AppHandle,
    _store: State<'_, LiliaStore>,
    project_id: Option<String>,
    _force_refresh: Option<bool>,
) -> Result<SuggestionSourceProbe, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let application = require_application(&app)?;
        application
            .conversation_suggestion_sources(project_id.as_deref())
            .map(map_source_probe)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|err| format!("conversation suggestions sources 任务执行失败：{err}"))?
}

#[tauri::command]
pub async fn conversation_suggestions_get(
    app: AppHandle,
    _store: State<'_, LiliaStore>,
    project_id: Option<String>,
    force_refresh: Option<bool>,
) -> Result<Vec<SuggestionItem>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let application = require_application(&app)?;
        let settings = suggestion_settings_from_shared(
            application
                .conversation_suggestion_settings()
                .map_err(|error| error.to_string())?,
        );
        let models = TauriSuggestionModelPort::new(app.clone(), &settings);
        application
            .conversation_suggestions(
                project_id.as_deref(),
                force_refresh == Some(true),
                &models as &dyn ConversationSuggestionModelPort,
            )
            .map(|items| items.into_iter().map(map_item).collect())
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|err| format!("conversation suggestions 任务执行失败：{err}"))?
}
