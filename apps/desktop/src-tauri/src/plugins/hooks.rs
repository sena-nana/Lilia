use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map as JsonMap, Value as JsonValue};
use tauri::{AppHandle, Runtime};
use toml_edit::{DocumentMut, Item};

use super::paths::{
    claude_root_for, ensure_dir, home_dir, list_subdirs, CLAUDE_DIR, PLUGINS_SUBDIR,
    PLUGIN_MANIFEST, SCOPE_PROJECT, SCOPE_USER,
};
use super::types::{
    HookDocumentUpdateInput, HookDocumentView, HookHandlerUpdateInput, HookHandlerView,
    HookSourceSummary, HookTrustState, HooksOverview,
};
use crate::{BACKEND_CLAUDE, BACKEND_CODEX};

pub(super) const FORMAT_CLAUDE_SETTINGS_JSON: &str = "claude_settings_json";
pub(super) const FORMAT_CODEX_HOOKS_JSON: &str = "codex_hooks_json";
const FORMAT_CODEX_CONFIG_TOML: &str = "codex_config_toml";
const FORMAT_MANAGED_SETTINGS: &str = "managed_settings";
const FORMAT_REQUIREMENTS_TOML: &str = "requirements_toml";
const FORMAT_PLUGIN_MANIFEST: &str = "plugin_manifest";
const SCOPE_LOCAL: &str = "local";
const SCOPE_MANAGED: &str = "managed";
const SCOPE_PLUGIN: &str = "plugin";
const SCOPE_SYSTEM: &str = "system";
const CLAUDE_SETTINGS_FILE: &str = "settings.json";
const CLAUDE_LOCAL_SETTINGS_FILE: &str = "settings.local.json";
const CLAUDE_MANAGED_SETTINGS_FILE: &str = "managed-settings.json";
const CLAUDE_MANAGED_SETTINGS_D_DIR: &str = "managed-settings.d";
const CODEX_CONFIG_FILE: &str = "config.toml";
const CODEX_HOOKS_FILE: &str = "hooks.json";
const CODEX_REQUIREMENTS_FILE: &str = "requirements.toml";
const WINDOWS_CLAUDE_MANAGED_DIR: &str = r"C:\Program Files\ClaudeCode";

#[derive(Debug, Default)]
struct PluginManifestInfo {
    disabled: bool,
    hooks: Option<JsonValue>,
    description: Option<String>,
}

struct HookParseResult {
    handlers: Vec<HookHandlerView>,
    warnings: Vec<String>,
}

pub fn hooks_overview<R: Runtime>(app: &AppHandle<R>, project_cwd: Option<&str>) -> HooksOverview {
    let mut warnings = Vec::new();
    let mut sources = Vec::new();
    sources.extend(claude_hook_sources(app, project_cwd, &mut warnings));
    sources.extend(codex_hook_sources(app, project_cwd, &mut warnings));
    HooksOverview { sources, warnings }
}

pub fn read_hook_source<R: Runtime>(
    _app: &AppHandle<R>,
    source: &HookSourceSummary,
) -> Result<HookDocumentView, String> {
    match (source.backend.as_str(), source.format.as_str()) {
        (BACKEND_CLAUDE, FORMAT_CLAUDE_SETTINGS_JSON) => read_json_source(
            source.clone(),
            JsonRootKind::HooksField,
            ProviderFlavor::Claude,
        ),
        (BACKEND_CLAUDE, FORMAT_MANAGED_SETTINGS) => read_json_source(
            source.clone(),
            JsonRootKind::HooksField,
            ProviderFlavor::Claude,
        ),
        (BACKEND_CLAUDE, FORMAT_PLUGIN_MANIFEST) => read_json_source(
            source.clone(),
            JsonRootKind::HooksField,
            ProviderFlavor::Claude,
        ),
        (BACKEND_CODEX, FORMAT_CODEX_HOOKS_JSON) => read_json_source(
            source.clone(),
            JsonRootKind::HooksField,
            ProviderFlavor::Codex,
        ),
        (BACKEND_CODEX, FORMAT_CODEX_CONFIG_TOML) => read_codex_toml_source(source.clone()),
        (BACKEND_CODEX, FORMAT_REQUIREMENTS_TOML) => read_codex_requirements_source(source.clone()),
        _ => Err(format!(
            "未知 hooks source：backend={} format={}",
            source.backend, source.format
        )),
    }
}

pub fn create_hook_source<R: Runtime>(
    app: &AppHandle<R>,
    backend: &str,
    scope: &str,
    project_cwd: Option<&str>,
) -> Result<HookSourceSummary, String> {
    let source = editable_source_template(app, backend, scope, project_cwd)?;
    if let Some(parent) = Path::new(&source.path).parent() {
        ensure_dir(parent)?;
    }
    if source.format == FORMAT_CODEX_HOOKS_JSON || source.format == FORMAT_CLAUDE_SETTINGS_JSON {
        let path = PathBuf::from(&source.path);
        if !path.exists() {
            fs::write(&path, b"{\n  \"hooks\": {}\n}\n")
                .map_err(|e| format!("初始化 {} 失败：{e}", path.display()))?;
        }
    }
    Ok(read_hook_source(app, &source)?.source)
}

pub fn update_hook_source<R: Runtime>(
    app: &AppHandle<R>,
    source: &HookSourceSummary,
    input: HookDocumentUpdateInput,
) -> Result<HookDocumentView, String> {
    if !source.editable {
        return Err("当前 hooks source 为只读，不能保存".to_string());
    }
    match (source.backend.as_str(), source.format.as_str()) {
        (BACKEND_CLAUDE, FORMAT_CLAUDE_SETTINGS_JSON) => update_json_source(
            source,
            input,
            JsonRootKind::HooksField,
            ProviderFlavor::Claude,
        ),
        (BACKEND_CODEX, FORMAT_CODEX_HOOKS_JSON) => update_json_source(
            source,
            input,
            JsonRootKind::HooksField,
            ProviderFlavor::Codex,
        ),
        _ => Err("当前 hooks source 不支持结构化保存".to_string()),
    }?;
    read_hook_source(app, source)
}

pub fn delete_hook_source<R: Runtime>(
    _app: &AppHandle<R>,
    source: &HookSourceSummary,
) -> Result<(), String> {
    if !source.editable {
        return Err("当前 hooks source 为只读，不能删除".to_string());
    }
    match (source.backend.as_str(), source.format.as_str()) {
        (BACKEND_CLAUDE, FORMAT_CLAUDE_SETTINGS_JSON) => {
            let path = PathBuf::from(&source.path);
            delete_json_hooks_field(&path, true)
        }
        (BACKEND_CODEX, FORMAT_CODEX_HOOKS_JSON) => {
            let path = PathBuf::from(&source.path);
            if path.exists() {
                fs::remove_file(&path).map_err(|e| format!("删除 {} 失败：{e}", path.display()))?;
            }
            Ok(())
        }
        _ => Err("当前 hooks source 不支持删除".to_string()),
    }
}

pub fn set_hook_source_enabled<R: Runtime>(
    app: &AppHandle<R>,
    source: &HookSourceSummary,
    enabled: bool,
) -> Result<HookSourceSummary, String> {
    if enabled {
        create_hook_source(
            app,
            &source.backend,
            &source.scope,
            project_cwd_from_source(source).as_deref(),
        )
    } else {
        delete_hook_source(app, source)?;
        Ok(read_hook_source(app, source)?.source)
    }
}

#[cfg(feature = "legacy-runner")]
pub fn runtime_claude_hooks<R: Runtime>(
    app: &AppHandle<R>,
    project_cwd: Option<&str>,
) -> (Option<JsonValue>, Vec<String>) {
    let mut warnings = Vec::new();
    let mut documents = Vec::new();
    for source in claude_hook_sources(app, project_cwd, &mut warnings) {
        if !source.enabled {
            continue;
        }
        if let Ok(document) = read_hook_source(app, &source) {
            if !document.handlers.is_empty() {
                documents.push(document);
            }
        }
    }
    let merged = merge_hook_documents(documents.into_iter().map(|doc| doc.handlers).collect());
    (merged, warnings)
}

fn project_cwd_from_source(source: &HookSourceSummary) -> Option<String> {
    if source.scope != SCOPE_PROJECT && source.scope != SCOPE_LOCAL {
        return None;
    }
    let path = Path::new(&source.path);
    let parent = path.parent()?;
    if parent
        .file_name()?
        .to_string_lossy()
        .eq_ignore_ascii_case(CLAUDE_DIR)
    {
        return parent
            .parent()
            .map(|value| value.to_string_lossy().to_string());
    }
    if parent
        .file_name()?
        .to_string_lossy()
        .eq_ignore_ascii_case(".codex")
    {
        return parent
            .parent()
            .map(|value| value.to_string_lossy().to_string());
    }
    None
}

enum JsonRootKind {
    HooksField,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ProviderFlavor {
    Claude,
    Codex,
}

fn editable_source_template<R: Runtime>(
    app: &AppHandle<R>,
    backend: &str,
    scope: &str,
    project_cwd: Option<&str>,
) -> Result<HookSourceSummary, String> {
    match (backend, scope) {
        (BACKEND_CLAUDE, SCOPE_USER) => Ok(build_source_summary(
            backend,
            scope,
            FORMAT_CLAUDE_SETTINGS_JSON,
            "Claude User Hooks",
            claude_user_settings_path(app)?
                .to_string_lossy()
                .to_string(),
            false,
            true,
            Vec::new(),
            Vec::new(),
            HookTrustState::Unknown,
            Some("~/.claude/settings.json 中的 hooks".to_string()),
        )),
        (BACKEND_CLAUDE, SCOPE_PROJECT) => Ok(build_source_summary(
            backend,
            scope,
            FORMAT_CLAUDE_SETTINGS_JSON,
            "Claude Project Hooks",
            claude_project_settings_path(project_cwd)?
                .to_string_lossy()
                .to_string(),
            false,
            true,
            Vec::new(),
            Vec::new(),
            HookTrustState::Unknown,
            Some(".claude/settings.json 中的 hooks".to_string()),
        )),
        (BACKEND_CLAUDE, SCOPE_LOCAL) => Ok(build_source_summary(
            backend,
            scope,
            FORMAT_CLAUDE_SETTINGS_JSON,
            "Claude Local Hooks",
            claude_local_settings_path(project_cwd)?
                .to_string_lossy()
                .to_string(),
            false,
            true,
            Vec::new(),
            Vec::new(),
            HookTrustState::Unknown,
            Some(".claude/settings.local.json 中的 hooks".to_string()),
        )),
        (BACKEND_CODEX, SCOPE_USER) => Ok(build_source_summary(
            backend,
            scope,
            FORMAT_CODEX_HOOKS_JSON,
            "Codex User Hooks",
            codex_user_hooks_path(app)?.to_string_lossy().to_string(),
            false,
            true,
            Vec::new(),
            Vec::new(),
            HookTrustState::Required,
            Some("~/.codex/hooks.json".to_string()),
        )),
        (BACKEND_CODEX, SCOPE_PROJECT) => Ok(build_source_summary(
            backend,
            scope,
            FORMAT_CODEX_HOOKS_JSON,
            "Codex Project Hooks",
            codex_project_hooks_path(project_cwd)?
                .to_string_lossy()
                .to_string(),
            false,
            true,
            Vec::new(),
            Vec::new(),
            HookTrustState::Required,
            Some("<repo>/.codex/hooks.json".to_string()),
        )),
        _ => Err("当前 scope 不支持创建 hooks source".to_string()),
    }
}

fn claude_hook_sources<R: Runtime>(
    app: &AppHandle<R>,
    project_cwd: Option<&str>,
    warnings: &mut Vec<String>,
) -> Vec<HookSourceSummary> {
    let mut sources = Vec::new();
    if let Ok(document) = read_json_source(
        build_source_summary(
            BACKEND_CLAUDE,
            SCOPE_USER,
            FORMAT_CLAUDE_SETTINGS_JSON,
            "Claude User Hooks",
            claude_user_settings_path(app)
                .map(|path| path.to_string_lossy().to_string())
                .unwrap_or_default(),
            false,
            true,
            Vec::new(),
            Vec::new(),
            HookTrustState::Unknown,
            Some("~/.claude/settings.json 中的 hooks".to_string()),
        ),
        JsonRootKind::HooksField,
        ProviderFlavor::Claude,
    ) {
        warnings.extend(document.warnings.clone());
        sources.push(document.source);
    }
    if let Some(project_cwd) = project_cwd {
        if let Ok(document) = read_json_source(
            build_source_summary(
                BACKEND_CLAUDE,
                SCOPE_PROJECT,
                FORMAT_CLAUDE_SETTINGS_JSON,
                "Claude Project Hooks",
                claude_project_settings_path(Some(project_cwd))
                    .map(|path| path.to_string_lossy().to_string())
                    .unwrap_or_default(),
                false,
                true,
                Vec::new(),
                Vec::new(),
                HookTrustState::Unknown,
                Some(".claude/settings.json 中的 hooks".to_string()),
            ),
            JsonRootKind::HooksField,
            ProviderFlavor::Claude,
        ) {
            warnings.extend(document.warnings.clone());
            sources.push(document.source);
        }
        if let Ok(document) = read_json_source(
            build_source_summary(
                BACKEND_CLAUDE,
                SCOPE_LOCAL,
                FORMAT_CLAUDE_SETTINGS_JSON,
                "Claude Local Hooks",
                claude_local_settings_path(Some(project_cwd))
                    .map(|path| path.to_string_lossy().to_string())
                    .unwrap_or_default(),
                false,
                true,
                Vec::new(),
                Vec::new(),
                HookTrustState::Unknown,
                Some(".claude/settings.local.json 中的 hooks".to_string()),
            ),
            JsonRootKind::HooksField,
            ProviderFlavor::Claude,
        ) {
            warnings.extend(document.warnings.clone());
            sources.push(document.source);
        }
    }
    for managed_path in claude_managed_settings_paths() {
        let source = build_source_summary(
            BACKEND_CLAUDE,
            SCOPE_MANAGED,
            FORMAT_MANAGED_SETTINGS,
            managed_path
                .file_name()
                .map(|value| value.to_string_lossy().to_string())
                .unwrap_or_else(|| "Claude Managed Hooks".to_string()),
            managed_path.to_string_lossy().to_string(),
            true,
            false,
            Vec::new(),
            Vec::new(),
            HookTrustState::Managed,
            Some("Claude managed settings".to_string()),
        );
        if let Ok(document) =
            read_json_source(source, JsonRootKind::HooksField, ProviderFlavor::Claude)
        {
            warnings.extend(document.warnings.clone());
            if document.source.exists {
                sources.push(document.source);
            }
        }
    }
    let (plugin_sources, plugin_warnings) = claude_plugin_hook_sources(app);
    warnings.extend(plugin_warnings);
    sources.extend(plugin_sources);
    sources
}

fn codex_hook_sources<R: Runtime>(
    app: &AppHandle<R>,
    project_cwd: Option<&str>,
    warnings: &mut Vec<String>,
) -> Vec<HookSourceSummary> {
    let mut sources = Vec::new();
    if let Ok(document) = read_json_source(
        build_source_summary(
            BACKEND_CODEX,
            SCOPE_USER,
            FORMAT_CODEX_HOOKS_JSON,
            "Codex User Hooks",
            codex_user_hooks_path(app)
                .map(|path| path.to_string_lossy().to_string())
                .unwrap_or_default(),
            false,
            true,
            Vec::new(),
            Vec::new(),
            HookTrustState::Required,
            Some("~/.codex/hooks.json".to_string()),
        ),
        JsonRootKind::HooksField,
        ProviderFlavor::Codex,
    ) {
        warnings.extend(document.warnings.clone());
        sources.push(document.source);
    }
    if let Ok(document) = read_codex_toml_source(build_source_summary(
        BACKEND_CODEX,
        SCOPE_USER,
        FORMAT_CODEX_CONFIG_TOML,
        "Codex User Config Hooks",
        codex_user_config_path(app)
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_default(),
        false,
        false,
        Vec::new(),
        Vec::new(),
        HookTrustState::Required,
        Some("~/.codex/config.toml 中的 inline [hooks]".to_string()),
    )) {
        warnings.extend(document.warnings.clone());
        sources.push(document.source);
    }
    if let Ok(document) = read_codex_requirements_source(build_source_summary(
        BACKEND_CODEX,
        SCOPE_SYSTEM,
        FORMAT_REQUIREMENTS_TOML,
        "Codex User Requirements",
        codex_user_requirements_path(app)
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_default(),
        true,
        false,
        Vec::new(),
        Vec::new(),
        HookTrustState::Managed,
        Some("~/.codex/requirements.toml".to_string()),
    )) {
        warnings.extend(document.warnings.clone());
        if document.source.exists {
            sources.push(document.source);
        }
    }
    if let Some(project_cwd) = project_cwd {
        if let Ok(document) = read_json_source(
            build_source_summary(
                BACKEND_CODEX,
                SCOPE_PROJECT,
                FORMAT_CODEX_HOOKS_JSON,
                "Codex Project Hooks",
                codex_project_hooks_path(Some(project_cwd))
                    .map(|path| path.to_string_lossy().to_string())
                    .unwrap_or_default(),
                false,
                true,
                Vec::new(),
                Vec::new(),
                HookTrustState::Required,
                Some("<repo>/.codex/hooks.json".to_string()),
            ),
            JsonRootKind::HooksField,
            ProviderFlavor::Codex,
        ) {
            warnings.extend(document.warnings.clone());
            sources.push(document.source);
        }
        if let Ok(document) = read_codex_toml_source(build_source_summary(
            BACKEND_CODEX,
            SCOPE_PROJECT,
            FORMAT_CODEX_CONFIG_TOML,
            "Codex Project Config Hooks",
            codex_project_config_path(Some(project_cwd))
                .map(|path| path.to_string_lossy().to_string())
                .unwrap_or_default(),
            false,
            false,
            Vec::new(),
            Vec::new(),
            HookTrustState::Required,
            Some("<repo>/.codex/config.toml 中的 inline [hooks]".to_string()),
        )) {
            warnings.extend(document.warnings.clone());
            sources.push(document.source);
        }
        if let Ok(document) = read_codex_requirements_source(build_source_summary(
            BACKEND_CODEX,
            SCOPE_MANAGED,
            FORMAT_REQUIREMENTS_TOML,
            "Codex Project Requirements",
            codex_project_requirements_path(Some(project_cwd))
                .map(|path| path.to_string_lossy().to_string())
                .unwrap_or_default(),
            true,
            false,
            Vec::new(),
            Vec::new(),
            HookTrustState::Managed,
            Some("<repo>/.codex/requirements.toml".to_string()),
        )) {
            warnings.extend(document.warnings.clone());
            if document.source.exists {
                sources.push(document.source);
            }
        }
    }
    annotate_codex_mixed_source_warnings(&mut sources);
    sources
}

fn annotate_codex_mixed_source_warnings(sources: &mut [HookSourceSummary]) {
    let mut active_layers = BTreeSet::new();
    for source in sources.iter() {
        if source.backend != BACKEND_CODEX || !source.exists || !source.enabled {
            continue;
        }
        if source.scope == SCOPE_USER || source.scope == SCOPE_PROJECT {
            active_layers.insert((source.scope.clone(), source.handler_count > 0));
        }
    }
    for scope in [SCOPE_USER, SCOPE_PROJECT] {
        let has_hooks_json = sources.iter().any(|source| {
            source.backend == BACKEND_CODEX
                && source.scope == scope
                && source.format == FORMAT_CODEX_HOOKS_JSON
                && source.exists
                && source.enabled
        });
        let has_config_toml = sources.iter().any(|source| {
            source.backend == BACKEND_CODEX
                && source.scope == scope
                && source.format == FORMAT_CODEX_CONFIG_TOML
                && source.exists
                && source.enabled
        });
        if !has_hooks_json || !has_config_toml {
            continue;
        }
        for source in sources.iter_mut().filter(|source| {
            source.backend == BACKEND_CODEX
                && source.scope == scope
                && (source.format == FORMAT_CODEX_HOOKS_JSON
                    || source.format == FORMAT_CODEX_CONFIG_TOML)
        }) {
            source.warnings.push(
                "同一层同时存在 hooks.json 与 inline [hooks]；Codex 会同时加载两者。".to_string(),
            );
        }
    }
}

fn claude_plugin_hook_sources<R: Runtime>(
    app: &AppHandle<R>,
) -> (Vec<HookSourceSummary>, Vec<String>) {
    let root = match claude_root_for(app, SCOPE_USER, None, PLUGINS_SUBDIR) {
        Ok(path) => path,
        Err(err) => return (Vec::new(), vec![err]),
    };
    if !root.exists() {
        return (Vec::new(), Vec::new());
    }
    let mut warnings = Vec::new();
    let mut sources = Vec::new();
    for (name, path) in list_subdirs(&root) {
        let manifest_path = path.join(PLUGIN_MANIFEST);
        if !manifest_path.exists() {
            continue;
        }
        let text = match fs::read_to_string(&manifest_path) {
            Ok(text) => text,
            Err(err) => {
                warnings.push(format!("读取 {} 失败：{err}", manifest_path.display()));
                continue;
            }
        };
        let parsed = match parse_plugin_manifest(&text) {
            Ok(parsed) => parsed,
            Err(err) => {
                warnings.push(format!("解析 {} 失败：{err}", manifest_path.display()));
                continue;
            }
        };
        let Some(hooks) = parsed.hooks.as_ref() else {
            continue;
        };
        let parse = parse_hook_handlers_from_hook_object(hooks, ProviderFlavor::Claude);
        let limitations = if parsed.disabled {
            vec!["插件当前已停用，Claude 不会加载该 hooks 来源。".to_string()]
        } else {
            Vec::new()
        };
        sources.push(build_source_summary(
            BACKEND_CLAUDE,
            SCOPE_PLUGIN,
            FORMAT_PLUGIN_MANIFEST,
            format!("Claude Plugin · {name}"),
            manifest_path.to_string_lossy().to_string(),
            false,
            false,
            parse.warnings,
            limitations,
            HookTrustState::Unknown,
            parsed.description,
        ));
        if let Some(last) = sources.last_mut() {
            last.exists = true;
            last.enabled = !parsed.disabled && !parse.handlers.is_empty();
            last.handler_count = parse.handlers.len();
        }
    }
    (sources, warnings)
}

fn parse_plugin_manifest(text: &str) -> Result<PluginManifestInfo, String> {
    let value: JsonValue = serde_json::from_str(text).map_err(|e| format!("不是合法 JSON：{e}"))?;
    let Some(object) = value.as_object() else {
        return Err("plugin.json 顶层必须是对象".to_string());
    };
    Ok(PluginManifestInfo {
        disabled: object
            .get("disabled")
            .and_then(JsonValue::as_bool)
            .unwrap_or(false),
        hooks: object.get("hooks").cloned(),
        description: object
            .get("description")
            .and_then(JsonValue::as_str)
            .map(|value| value.to_string()),
    })
}

fn read_json_source(
    mut source: HookSourceSummary,
    root_kind: JsonRootKind,
    flavor: ProviderFlavor,
) -> Result<HookDocumentView, String> {
    let path = PathBuf::from(&source.path);
    let (root, raw_text, mut warnings) = read_json_value(&path)?;
    let mut limitations = source.limitations.clone();
    let handlers = if let Some(root) = root.as_ref() {
        let hook_value = match root_kind {
            JsonRootKind::HooksField => root.get("hooks").cloned().unwrap_or(JsonValue::Null),
        };
        let parsed = parse_hook_handlers_from_hook_object(&hook_value, flavor);
        warnings.extend(parsed.warnings.clone());
        parsed.handlers
    } else {
        Vec::new()
    };
    source.exists = path.exists();
    source.enabled = !handlers.is_empty();
    source.handler_count = handlers.len();
    source.warnings.extend(warnings.clone());
    if !source.editable {
        limitations.extend(readonly_limitations_for_source(&source));
    }
    Ok(HookDocumentView {
        source,
        handlers,
        raw_document: raw_text,
        raw_format: "json".to_string(),
        warnings,
        limitations,
    })
}

fn read_codex_toml_source(mut source: HookSourceSummary) -> Result<HookDocumentView, String> {
    let path = PathBuf::from(&source.path);
    let (doc, raw_text, warnings) = read_toml_document(&path)?;
    let hooks_item = doc
        .as_ref()
        .and_then(|doc| doc.as_table().get("hooks"))
        .cloned()
        .unwrap_or(Item::None);
    let parse = parse_toml_hook_item(&hooks_item, ProviderFlavor::Codex);
    source.exists = path.exists();
    source.enabled = !parse.handlers.is_empty();
    source.handler_count = parse.handlers.len();
    source.warnings.extend(warnings.clone());
    source.warnings.extend(parse.warnings.clone());
    Ok(HookDocumentView {
        source,
        handlers: parse.handlers,
        raw_document: raw_text,
        raw_format: "toml".to_string(),
        warnings: [warnings, parse.warnings].concat(),
        limitations: vec![
            "当前来源为只读 inline [hooks]，Lilia 不会自动重写 config.toml。".to_string(),
            "如需编辑，请改用 hooks.json 或直接手工维护 TOML。".to_string(),
        ],
    })
}

fn read_codex_requirements_source(
    mut source: HookSourceSummary,
) -> Result<HookDocumentView, String> {
    let path = PathBuf::from(&source.path);
    let (doc, raw_text, warnings) = read_toml_document(&path)?;
    let hooks_item = doc
        .as_ref()
        .and_then(|doc| doc.as_table().get("hooks"))
        .cloned()
        .unwrap_or(Item::None);
    let parse = parse_toml_hook_item(&hooks_item, ProviderFlavor::Codex);
    let mut limitations =
        vec!["当前来源来自 requirements.toml，仅用于展示约束与托管 hooks。".to_string()];
    if let Some(doc) = doc.as_ref() {
        if doc
            .as_table()
            .get("allow_managed_hooks_only")
            .and_then(Item::as_bool)
            .unwrap_or(false)
        {
            limitations.push("allow_managed_hooks_only = true：仅允许托管 hooks。".to_string());
        }
        let features_hooks = doc
            .as_table()
            .get("features")
            .and_then(Item::as_table)
            .and_then(|table| table.get("hooks"))
            .and_then(Item::as_bool);
        if let Some(enabled) = features_hooks {
            limitations.push(format!(
                "requirements.toml 约束 features.hooks = {}",
                if enabled { "true" } else { "false" }
            ));
        }
    }
    source.exists = path.exists();
    source.enabled = !parse.handlers.is_empty();
    source.handler_count = parse.handlers.len();
    source.warnings.extend(warnings.clone());
    source.warnings.extend(parse.warnings.clone());
    Ok(HookDocumentView {
        source,
        handlers: parse.handlers,
        raw_document: raw_text,
        raw_format: "toml".to_string(),
        warnings: [warnings, parse.warnings].concat(),
        limitations,
    })
}

fn readonly_limitations_for_source(source: &HookSourceSummary) -> Vec<String> {
    match source.format.as_str() {
        FORMAT_PLUGIN_MANIFEST => vec!["Claude plugin 提供的 hooks 只读展示。".to_string()],
        FORMAT_MANAGED_SETTINGS => vec!["托管 settings 来源不可在 Lilia 中修改。".to_string()],
        _ => Vec::new(),
    }
}

fn read_json_value(
    path: &Path,
) -> Result<(Option<JsonValue>, Option<String>, Vec<String>), String> {
    if !path.exists() {
        return Ok((None, None, Vec::new()));
    }
    let text =
        fs::read_to_string(path).map_err(|e| format!("读取 {} 失败：{e}", path.display()))?;
    if text.trim().is_empty() {
        return Ok((
            Some(JsonValue::Object(JsonMap::new())),
            Some(text),
            Vec::new(),
        ));
    }
    match serde_json::from_str::<JsonValue>(&text) {
        Ok(value) => Ok((Some(value), Some(text), Vec::new())),
        Err(err) => Ok((
            Some(JsonValue::Object(JsonMap::new())),
            Some(text),
            vec![format!("{} 不是合法 JSON：{err}", path.display())],
        )),
    }
}

fn read_toml_document(
    path: &Path,
) -> Result<(Option<DocumentMut>, Option<String>, Vec<String>), String> {
    if !path.exists() {
        return Ok((None, None, Vec::new()));
    }
    let text =
        fs::read_to_string(path).map_err(|e| format!("读取 {} 失败：{e}", path.display()))?;
    if text.trim().is_empty() {
        return Ok((Some(DocumentMut::new()), Some(text), Vec::new()));
    }
    match text.parse::<DocumentMut>() {
        Ok(value) => Ok((Some(value), Some(text), Vec::new())),
        Err(err) => Ok((
            Some(DocumentMut::new()),
            Some(text),
            vec![format!("{} 不是合法 TOML：{err}", path.display())],
        )),
    }
}

fn update_json_source(
    source: &HookSourceSummary,
    input: HookDocumentUpdateInput,
    root_kind: JsonRootKind,
    flavor: ProviderFlavor,
) -> Result<(), String> {
    let path = PathBuf::from(&source.path);
    let (root, _, warnings) = read_json_value(&path)?;
    if !warnings.is_empty() {
        return Err(warnings.join("\n"));
    }
    let mut root = match root {
        Some(JsonValue::Object(map)) => JsonValue::Object(map),
        Some(_) => return Err(format!("{} 顶层必须是 JSON 对象", path.display())),
        None => JsonValue::Object(JsonMap::new()),
    };
    let hooks = build_hook_object_from_handlers(&input.handlers, flavor)?;
    match root_kind {
        JsonRootKind::HooksField => {
            if let Some(object) = root.as_object_mut() {
                object.insert("hooks".to_string(), hooks);
            }
        }
    }
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    let mut text = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?;
    text.push('\n');
    fs::write(&path, text).map_err(|e| format!("写入 {} 失败：{e}", path.display()))
}

fn delete_json_hooks_field(path: &Path, remove_file_if_empty: bool) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let (root, _, warnings) = read_json_value(path)?;
    if !warnings.is_empty() {
        return Err(warnings.join("\n"));
    }
    let Some(JsonValue::Object(mut object)) = root else {
        return Ok(());
    };
    object.remove("hooks");
    if object.is_empty() && remove_file_if_empty {
        fs::remove_file(path).map_err(|e| format!("删除 {} 失败：{e}", path.display()))?;
        return Ok(());
    }
    let mut text =
        serde_json::to_string_pretty(&JsonValue::Object(object)).map_err(|e| e.to_string())?;
    text.push('\n');
    fs::write(path, text).map_err(|e| format!("写入 {} 失败：{e}", path.display()))
}

fn build_hook_object_from_handlers(
    handlers: &[HookHandlerUpdateInput],
    flavor: ProviderFlavor,
) -> Result<JsonValue, String> {
    let mut events: BTreeMap<
        String,
        Vec<(Option<String>, Option<String>, JsonMap<String, JsonValue>)>,
    > = BTreeMap::new();
    for handler in handlers {
        let event = handler.event.trim();
        if event.is_empty() {
            continue;
        }
        let matcher = handler
            .matcher
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let group_advanced =
            parse_json_map(handler.group_advanced_json.as_deref(), "groupAdvancedJson")?;
        let advanced = parse_json_map(handler.advanced_json.as_deref(), "advancedJson")?;
        let mut handler_object = JsonMap::new();
        handler_object.insert(
            "type".to_string(),
            JsonValue::String(handler.handler_type.trim().to_string()),
        );
        if let Some(command) = handler
            .command
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            handler_object.insert(
                "command".to_string(),
                JsonValue::String(command.to_string()),
            );
        }
        if let Some(command_windows) = handler
            .command_windows
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            handler_object.insert(
                "commandWindows".to_string(),
                JsonValue::String(command_windows.to_string()),
            );
        }
        if let Some(timeout) = handler.timeout_seconds {
            handler_object.insert("timeout".to_string(), JsonValue::Number(timeout.into()));
        }
        if let Some(status_message) = handler
            .status_message
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            handler_object.insert(
                "statusMessage".to_string(),
                JsonValue::String(status_message.to_string()),
            );
        }
        merge_json_maps(&mut handler_object, advanced);
        let group_marker = matcher.clone();
        let event_groups = events.entry(event.to_string()).or_default();
        if let Some(index) = event_groups
            .iter()
            .position(|(existing_matcher, _, _)| *existing_matcher == group_marker)
        {
            if let Some(group) = event_groups.get_mut(index) {
                let entry = group
                    .2
                    .entry("__hooks".to_string())
                    .or_insert_with(|| JsonValue::Array(Vec::new()));
                if let Some(items) = entry.as_array_mut() {
                    items.push(JsonValue::Object(handler_object));
                }
                if group.1.is_none() && handler.group_advanced_json.is_some() {
                    group.1 = handler.group_advanced_json.clone();
                }
            }
        } else {
            let mut group = group_advanced;
            group.insert(
                "__hooks".to_string(),
                JsonValue::Array(vec![JsonValue::Object(handler_object)]),
            );
            event_groups.push((matcher, handler.group_advanced_json.clone(), group));
        }
    }
    let mut root = JsonMap::new();
    for (event, groups) in events {
        let mut event_groups = Vec::new();
        for (matcher, group_advanced_json, mut group) in groups {
            let hooks = group
                .remove("__hooks")
                .unwrap_or_else(|| JsonValue::Array(Vec::new()));
            let mut group_object = group;
            if let Some(matcher) = matcher {
                group_object.insert("matcher".to_string(), JsonValue::String(matcher));
            }
            group_object.insert("hooks".to_string(), hooks);
            let _ = group_advanced_json;
            event_groups.push(JsonValue::Object(group_object));
        }
        root.insert(event, JsonValue::Array(event_groups));
    }
    let hooks = JsonValue::Object(root);
    match flavor {
        ProviderFlavor::Claude | ProviderFlavor::Codex => Ok(hooks),
    }
}

fn parse_json_map(value: Option<&str>, label: &str) -> Result<JsonMap<String, JsonValue>, String> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(JsonMap::new());
    };
    let parsed: JsonValue =
        serde_json::from_str(value).map_err(|e| format!("{label} 不是合法 JSON：{e}"))?;
    let Some(object) = parsed.as_object() else {
        return Err(format!("{label} 必须是 JSON 对象"));
    };
    Ok(object.clone())
}

fn merge_json_maps(target: &mut JsonMap<String, JsonValue>, source: JsonMap<String, JsonValue>) {
    for (key, value) in source {
        if !target.contains_key(&key) {
            target.insert(key, value);
        }
    }
}

fn parse_hook_handlers_from_hook_object(
    value: &JsonValue,
    flavor: ProviderFlavor,
) -> HookParseResult {
    let Some(object) = value.as_object() else {
        return HookParseResult {
            handlers: Vec::new(),
            warnings: if value.is_null() {
                Vec::new()
            } else {
                vec!["hooks 不是对象，已跳过".to_string()]
            },
        };
    };
    let mut warnings = Vec::new();
    let mut handlers = Vec::new();
    for (event, groups) in object {
        let Some(groups) = groups.as_array() else {
            warnings.push(format!("hooks.{event} 不是数组"));
            continue;
        };
        for (group_index, group) in groups.iter().enumerate() {
            let Some(group_obj) = group.as_object() else {
                warnings.push(format!("hooks.{event}[{group_index}] 不是对象"));
                continue;
            };
            let matcher = group_obj
                .get("matcher")
                .and_then(JsonValue::as_str)
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());
            let group_advanced = advanced_json_string(group_obj, &["matcher", "hooks"]);
            let Some(group_handlers) = group_obj.get("hooks").and_then(JsonValue::as_array) else {
                warnings.push(format!("hooks.{event}[{group_index}].hooks 不是数组"));
                continue;
            };
            for (handler_index, handler) in group_handlers.iter().enumerate() {
                let Some(handler_obj) = handler.as_object() else {
                    warnings.push(format!(
                        "hooks.{event}[{group_index}].hooks[{handler_index}] 不是对象"
                    ));
                    continue;
                };
                let handler_type = handler_obj
                    .get("type")
                    .and_then(JsonValue::as_str)
                    .unwrap_or("command")
                    .trim()
                    .to_string();
                let command_windows = handler_obj
                    .get("commandWindows")
                    .or_else(|| handler_obj.get("command_windows"))
                    .and_then(JsonValue::as_str)
                    .map(|value| value.to_string());
                let timeout_seconds = handler_obj.get("timeout").and_then(|value| {
                    value
                        .as_u64()
                        .or_else(|| value.as_i64().and_then(|num| u64::try_from(num).ok()))
                });
                let supported = match flavor {
                    ProviderFlavor::Codex => handler_type == "command",
                    ProviderFlavor::Claude => true,
                };
                let executable = match flavor {
                    ProviderFlavor::Codex => {
                        supported
                            && handler_obj.get("async").and_then(JsonValue::as_bool) != Some(true)
                    }
                    ProviderFlavor::Claude => true,
                };
                let mut row_warnings = Vec::new();
                if flavor == ProviderFlavor::Codex && handler_type != "command" {
                    row_warnings.push("Codex 当前仅执行 type = command 的 handler。".to_string());
                }
                if flavor == ProviderFlavor::Codex
                    && handler_obj.get("async").and_then(JsonValue::as_bool) == Some(true)
                {
                    row_warnings
                        .push("Codex 当前会跳过 async = true 的 command hook。".to_string());
                }
                handlers.push(HookHandlerView {
                    id: format!("{event}:{group_index}:{handler_index}"),
                    event: event.to_string(),
                    matcher: matcher.clone(),
                    r#type: handler_type,
                    command: handler_obj
                        .get("command")
                        .and_then(JsonValue::as_str)
                        .map(|value| value.to_string()),
                    command_windows,
                    timeout_seconds,
                    status_message: handler_obj
                        .get("statusMessage")
                        .and_then(JsonValue::as_str)
                        .map(|value| value.to_string()),
                    supported,
                    executable,
                    group_advanced_json: group_advanced.clone(),
                    advanced_json: advanced_json_string(
                        handler_obj,
                        &[
                            "type",
                            "command",
                            "commandWindows",
                            "command_windows",
                            "timeout",
                            "statusMessage",
                        ],
                    ),
                    warnings: row_warnings,
                });
            }
        }
    }
    HookParseResult { handlers, warnings }
}

fn parse_toml_hook_item(value: &Item, flavor: ProviderFlavor) -> HookParseResult {
    if value.is_none() {
        return HookParseResult {
            handlers: Vec::new(),
            warnings: Vec::new(),
        };
    }
    let Some(table) = value.as_table() else {
        return HookParseResult {
            handlers: Vec::new(),
            warnings: vec!["[hooks] 不是 table，已跳过".to_string()],
        };
    };
    let mut warnings = Vec::new();
    let mut handlers = Vec::new();
    for (event, groups) in table.iter() {
        let Some(groups) = groups.as_array_of_tables() else {
            continue;
        };
        for (group_index, group) in groups.iter().enumerate() {
            let matcher = group
                .get("matcher")
                .and_then(Item::as_str)
                .map(|value| value.to_string());
            let Some(group_handlers) = group.get("hooks").and_then(Item::as_array_of_tables) else {
                warnings.push(format!("hooks.{event}[{group_index}].hooks 不是数组表"));
                continue;
            };
            let group_advanced_json = toml_table_advanced_json(group, &["matcher", "hooks"]);
            for (handler_index, handler) in group_handlers.iter().enumerate() {
                let handler_type = handler
                    .get("type")
                    .and_then(Item::as_str)
                    .unwrap_or("command");
                let async_enabled = handler.get("async").and_then(Item::as_bool) == Some(true);
                let mut row_warnings = Vec::new();
                if flavor == ProviderFlavor::Codex && handler_type != "command" {
                    row_warnings.push("Codex 当前仅执行 type = command 的 handler。".to_string());
                }
                if flavor == ProviderFlavor::Codex && async_enabled {
                    row_warnings
                        .push("Codex 当前会跳过 async = true 的 command hook。".to_string());
                }
                handlers.push(HookHandlerView {
                    id: format!("{event}:{group_index}:{handler_index}"),
                    event: event.to_string(),
                    matcher: matcher.clone(),
                    r#type: handler_type.to_string(),
                    command: handler
                        .get("command")
                        .and_then(Item::as_str)
                        .map(|value| value.to_string()),
                    command_windows: handler
                        .get("command_windows")
                        .or_else(|| handler.get("commandWindows"))
                        .and_then(Item::as_str)
                        .map(|value| value.to_string()),
                    timeout_seconds: handler
                        .get("timeout")
                        .and_then(Item::as_integer)
                        .and_then(|value| u64::try_from(value).ok()),
                    status_message: handler
                        .get("statusMessage")
                        .or_else(|| handler.get("status_message"))
                        .and_then(Item::as_str)
                        .map(|value| value.to_string()),
                    supported: flavor != ProviderFlavor::Codex || handler_type == "command",
                    executable: flavor != ProviderFlavor::Codex
                        || (handler_type == "command" && !async_enabled),
                    group_advanced_json: group_advanced_json.clone(),
                    advanced_json: toml_table_advanced_json(
                        handler,
                        &[
                            "type",
                            "command",
                            "command_windows",
                            "commandWindows",
                            "timeout",
                            "statusMessage",
                            "status_message",
                        ],
                    ),
                    warnings: row_warnings,
                });
            }
        }
    }
    HookParseResult { handlers, warnings }
}

fn advanced_json_string(
    object: &JsonMap<String, JsonValue>,
    ignored_keys: &[&str],
) -> Option<String> {
    let mut filtered = JsonMap::new();
    for (key, value) in object {
        if ignored_keys.iter().any(|ignored| ignored == key) {
            continue;
        }
        filtered.insert(key.clone(), value.clone());
    }
    if filtered.is_empty() {
        None
    } else {
        serde_json::to_string_pretty(&JsonValue::Object(filtered)).ok()
    }
}

fn toml_table_advanced_json(
    table: &dyn toml_edit::TableLike,
    ignored_keys: &[&str],
) -> Option<String> {
    let mut filtered = JsonMap::new();
    for (key, value) in table.iter() {
        if ignored_keys.iter().any(|ignored| *ignored == key) {
            continue;
        }
        if let Some(json_value) = toml_item_to_json(value) {
            filtered.insert(key.to_string(), json_value);
        }
    }
    if filtered.is_empty() {
        None
    } else {
        serde_json::to_string_pretty(&JsonValue::Object(filtered)).ok()
    }
}

fn toml_item_to_json(item: &Item) -> Option<JsonValue> {
    if let Some(value) = item.as_value() {
        if let Some(text) = value.as_str() {
            return Some(JsonValue::String(text.to_string()));
        }
        if let Some(integer) = value.as_integer() {
            return Some(JsonValue::Number(integer.into()));
        }
        if let Some(boolean) = value.as_bool() {
            return Some(JsonValue::Bool(boolean));
        }
    }
    if let Some(array) = item.as_array() {
        let values = array
            .iter()
            .filter_map(|value| {
                if let Some(text) = value.as_str() {
                    Some(JsonValue::String(text.to_string()))
                } else if let Some(integer) = value.as_integer() {
                    Some(JsonValue::Number(integer.into()))
                } else {
                    value.as_bool().map(JsonValue::Bool)
                }
            })
            .collect::<Vec<_>>();
        return Some(JsonValue::Array(values));
    }
    if let Some(table) = item.as_table_like() {
        let mut object = JsonMap::new();
        for (key, value) in table.iter() {
            if let Some(json_value) = toml_item_to_json(value) {
                object.insert(key.to_string(), json_value);
            }
        }
        return Some(JsonValue::Object(object));
    }
    None
}

#[cfg(feature = "legacy-runner")]
fn merge_hook_documents(docs: Vec<Vec<HookHandlerView>>) -> Option<JsonValue> {
    let mut handlers = Vec::new();
    for doc in docs {
        for handler in doc {
            handlers.push(HookHandlerUpdateInput {
                id: Some(handler.id),
                event: handler.event,
                matcher: handler.matcher,
                handler_type: handler.r#type,
                command: handler.command,
                command_windows: handler.command_windows,
                timeout_seconds: handler.timeout_seconds,
                status_message: handler.status_message,
                group_advanced_json: handler.group_advanced_json,
                advanced_json: handler.advanced_json,
            });
        }
    }
    if handlers.is_empty() {
        None
    } else {
        build_hook_object_from_handlers(&handlers, ProviderFlavor::Claude).ok()
    }
}

fn build_source_summary(
    backend: &str,
    scope: &str,
    format: &str,
    name: impl Into<String>,
    path: String,
    managed: bool,
    editable: bool,
    warnings: Vec<String>,
    limitations: Vec<String>,
    trust_state: HookTrustState,
    description: Option<String>,
) -> HookSourceSummary {
    HookSourceSummary {
        id: format!("{backend}|{scope}|{format}|{path}"),
        backend: backend.to_string(),
        scope: scope.to_string(),
        format: format.to_string(),
        name: name.into(),
        path,
        exists: false,
        editable,
        managed,
        enabled: false,
        handler_count: 0,
        warnings,
        limitations,
        trust_state,
        description,
    }
}

fn claude_user_settings_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    Ok(home_dir(app)?.join(CLAUDE_DIR).join(CLAUDE_SETTINGS_FILE))
}

fn claude_project_settings_path(project_cwd: Option<&str>) -> Result<PathBuf, String> {
    let cwd = project_cwd
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "项目 hooks 需要 projectCwd".to_string())?;
    Ok(PathBuf::from(cwd)
        .join(CLAUDE_DIR)
        .join(CLAUDE_SETTINGS_FILE))
}

fn claude_local_settings_path(project_cwd: Option<&str>) -> Result<PathBuf, String> {
    let cwd = project_cwd
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "本地 hooks 需要 projectCwd".to_string())?;
    Ok(PathBuf::from(cwd)
        .join(CLAUDE_DIR)
        .join(CLAUDE_LOCAL_SETTINGS_FILE))
}

fn claude_managed_settings_paths() -> Vec<PathBuf> {
    let root = PathBuf::from(WINDOWS_CLAUDE_MANAGED_DIR);
    let mut out = Vec::new();
    let managed = root.join(CLAUDE_MANAGED_SETTINGS_FILE);
    if managed.exists() {
        out.push(managed);
    }
    let dropin = root.join(CLAUDE_MANAGED_SETTINGS_D_DIR);
    if dropin.exists() {
        let mut files = fs::read_dir(dropin)
            .ok()
            .into_iter()
            .flat_map(|items| items.flatten())
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|ext| ext.to_string_lossy().eq_ignore_ascii_case("json"))
            })
            .collect::<Vec<_>>();
        files.sort();
        out.extend(files);
    }
    out
}

fn codex_user_root<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    Ok(home_dir(app)?.join(".codex"))
}

fn codex_user_hooks_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    Ok(codex_user_root(app)?.join(CODEX_HOOKS_FILE))
}

fn codex_user_config_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    Ok(codex_user_root(app)?.join(CODEX_CONFIG_FILE))
}

fn codex_user_requirements_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    Ok(codex_user_root(app)?.join(CODEX_REQUIREMENTS_FILE))
}

fn codex_project_hooks_path(project_cwd: Option<&str>) -> Result<PathBuf, String> {
    let cwd = project_cwd
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "项目 hooks 需要 projectCwd".to_string())?;
    Ok(PathBuf::from(cwd).join(".codex").join(CODEX_HOOKS_FILE))
}

fn codex_project_config_path(project_cwd: Option<&str>) -> Result<PathBuf, String> {
    let cwd = project_cwd
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "项目 hooks 需要 projectCwd".to_string())?;
    Ok(PathBuf::from(cwd).join(".codex").join(CODEX_CONFIG_FILE))
}

fn codex_project_requirements_path(project_cwd: Option<&str>) -> Result<PathBuf, String> {
    let cwd = project_cwd
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "项目 hooks 需要 projectCwd".to_string())?;
    Ok(PathBuf::from(cwd)
        .join(".codex")
        .join(CODEX_REQUIREMENTS_FILE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_codex_hook_handlers_and_marks_unsupported_types() {
        let hooks = serde_json::json!({
            "PreToolUse": [
                {
                    "matcher": "Bash",
                    "hooks": [
                        { "type": "command", "command": "python hook.py", "timeout": 30 },
                        { "type": "prompt", "statusMessage": "review" }
                    ]
                }
            ]
        });
        let parsed = parse_hook_handlers_from_hook_object(&hooks, ProviderFlavor::Codex);
        assert_eq!(parsed.handlers.len(), 2);
        assert!(parsed.handlers[0].supported);
        assert!(!parsed.handlers[1].supported);
        assert!(!parsed.handlers[1].warnings.is_empty());
    }

    #[test]
    fn build_hook_object_groups_rows_by_event_and_matcher() {
        let hooks = build_hook_object_from_handlers(
            &[
                HookHandlerUpdateInput {
                    event: "PreToolUse".to_string(),
                    matcher: Some("Bash".to_string()),
                    handler_type: "command".to_string(),
                    command: Some("python hook.py".to_string()),
                    timeout_seconds: Some(30),
                    ..HookHandlerUpdateInput::default()
                },
                HookHandlerUpdateInput {
                    event: "PreToolUse".to_string(),
                    matcher: Some("Bash".to_string()),
                    handler_type: "command".to_string(),
                    command: Some("python hook2.py".to_string()),
                    ..HookHandlerUpdateInput::default()
                },
            ],
            ProviderFlavor::Codex,
        )
        .unwrap();
        let groups = hooks
            .get("PreToolUse")
            .and_then(JsonValue::as_array)
            .unwrap();
        assert_eq!(groups.len(), 1);
        let handlers = groups[0]
            .get("hooks")
            .and_then(JsonValue::as_array)
            .unwrap();
        assert_eq!(handlers.len(), 2);
    }
}
