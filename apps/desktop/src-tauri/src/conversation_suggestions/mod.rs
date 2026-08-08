mod cache;
#[cfg(test)]
mod command_contract;
mod contract;
mod generation;
mod github_context;
mod local_git;
mod model;
mod scope;
mod types;

pub(crate) use types::{SuggestionItem, SuggestionSettings};

use tauri::{AppHandle, Manager, State};

use crate::settings_store::{load_store_value, save_store_value};
use crate::store::LiliaStore;

use cache::{build_cache_key, cache_scope_key, load_cache_hit, normalize_settings, save_cache};
use generation::{build_generation_prompt, materialize_items, parse_model_suggestions};
use model::{request_model, resolve_model_requests};
use scope::{build_scope, summarize_scope_sources};
use types::{SuggestionSourceProbe, SETTINGS_KEY};

#[tauri::command]
pub fn conversation_suggestions_get_settings(app: AppHandle) -> SuggestionSettings {
    normalize_settings(load_store_value(&app, SETTINGS_KEY))
}

#[tauri::command]
pub async fn conversation_suggestions_get_sources(
    app: AppHandle,
    _store: State<'_, LiliaStore>,
    project_id: Option<String>,
    force_refresh: Option<bool>,
) -> Result<SuggestionSourceProbe, String> {
    tauri::async_runtime::spawn_blocking(move || {
        conversation_suggestions_get_sources_blocking(app, project_id, force_refresh)
    })
    .await
    .map_err(|err| format!("conversation suggestions sources 任务执行失败：{err}"))?
}

fn conversation_suggestions_get_sources_blocking(
    app: AppHandle,
    project_id: Option<String>,
    _force_refresh: Option<bool>,
) -> Result<SuggestionSourceProbe, String> {
    let settings = conversation_suggestions_get_settings(app.clone());
    if !settings.enabled {
        return Ok(SuggestionSourceProbe {
            sources: Vec::new(),
            local_git: None,
        });
    }

    let store = app.state::<LiliaStore>();
    let conn = store.conn()?;
    let Some(scope) = build_scope(&app, &conn, project_id.as_deref())? else {
        return Ok(SuggestionSourceProbe {
            sources: Vec::new(),
            local_git: None,
        });
    };
    Ok(summarize_scope_sources(&scope))
}

#[tauri::command]
pub fn conversation_suggestions_set_settings(
    app: AppHandle,
    settings: SuggestionSettings,
) -> Result<(), String> {
    save_store_value(&app, SETTINGS_KEY, &normalize_settings(Some(settings)))
}

#[tauri::command]
pub async fn conversation_suggestions_get(
    app: AppHandle,
    _store: State<'_, LiliaStore>,
    project_id: Option<String>,
    force_refresh: Option<bool>,
) -> Result<Vec<SuggestionItem>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        conversation_suggestions_get_blocking(app, project_id, force_refresh)
    })
    .await
    .map_err(|err| format!("conversation suggestions 任务执行失败：{err}"))?
}

fn conversation_suggestions_get_blocking(
    app: AppHandle,
    project_id: Option<String>,
    force_refresh: Option<bool>,
) -> Result<Vec<SuggestionItem>, String> {
    let settings = conversation_suggestions_get_settings(app.clone());
    if !settings.enabled {
        return Ok(Vec::new());
    }

    let store = app.state::<LiliaStore>();
    let conn = store.conn()?;
    let Some(scope) = build_scope(&app, &conn, project_id.as_deref())? else {
        return Ok(Vec::new());
    };
    let models = resolve_model_requests(&app, &settings);
    if models.is_empty() {
        return Ok(Vec::new());
    }
    let prompt = build_generation_prompt(&scope);
    let cache_scope = cache_scope_key(project_id.as_deref(), &settings.source);
    for model in models {
        let cache_key = build_cache_key(&scope, &model);
        if force_refresh != Some(true) {
            if let Some(hit) = load_cache_hit(&app, &cache_scope, &cache_key) {
                return Ok(hit.items);
            }
        }
        match request_model(&app, &model, &prompt).and_then(parse_model_suggestions) {
            Ok(items) => {
                let generated = materialize_items(items, &scope);
                save_cache(&app, cache_scope.clone(), cache_key, generated.clone());
                return Ok(generated);
            }
            Err(err) => {
                eprintln!("[conversation-suggestions] generate failed: {err}");
            }
        }
    }
    Ok(Vec::new())
}

#[cfg(test)]
mod tests;
