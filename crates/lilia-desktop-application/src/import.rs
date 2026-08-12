use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use lilia_service::{StorageWriterGuard, WriterLeaseError, WriterMode};
use lilia_storage::{LiliaDataPaths, PRODUCT_SCHEMA_VERSION};
use rusqlite::backup::Backup;
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::{
    DesktopApplicationConfig, DesktopCredentialAction, DesktopHost, DesktopHostAction,
    DesktopHostContext, DesktopHostResult,
};

const SQLITE_HEADER: &[u8; 16] = b"SQLite format 3\0";

#[derive(Clone)]
pub struct DesktopDataImportService {
    target: DesktopApplicationConfig,
    host: Arc<dyn DesktopHost>,
    host_context: DesktopHostContext,
}

impl DesktopDataImportService {
    pub fn new(target: DesktopApplicationConfig, host: Arc<dyn DesktopHost>) -> Self {
        let host_context = DesktopHostContext::from(&target);
        Self {
            target,
            host,
            host_context,
        }
    }

    pub fn target(&self) -> &DesktopApplicationConfig {
        &self.target
    }

    pub fn plan(
        &self,
        source: &DesktopApplicationConfig,
    ) -> Result<DesktopImportPlan, DesktopImportError> {
        ensure_distinct_homes(source.home(), self.target.home())?;
        let plan_id = next_plan_id();
        let source_home = normalized_home(source.home());
        let target_home = normalized_home(self.target.home());
        let source_paths = LiliaDataPaths::from_home(&source_home);
        let target_paths = LiliaDataPaths::from_home(&target_home);
        let has_source_data = database_kinds().iter().any(|kind| {
            database_component_paths(&source_paths, *kind)
                .iter()
                .any(|(_, path)| path.exists())
        });

        let mut items: Vec<DesktopImportPlanItem> = if has_source_data {
            match acquire_source_import_lock(&source_paths) {
                Ok(_source_guard) => database_kinds()
                    .into_iter()
                    .map(|kind| inspect_database(kind, &source_paths, &target_paths))
                    .collect(),
                Err(error) => database_kinds()
                    .into_iter()
                    .map(|kind| blocked_plan_item(kind, &source_paths, &target_paths, &error))
                    .collect(),
            }
        } else {
            database_kinds()
                .into_iter()
                .map(|kind| missing_plan_item(kind, &source_paths, &target_paths))
                .collect()
        };
        let (credential_keys, credential_item) = match inspect_credential_keys(&source_paths) {
            Ok(keys) => (
                keys,
                DesktopImportPlanItem {
                    kind: DesktopImportItemKind::Credentials,
                    status: DesktopImportPlanItemStatus::RequiresCredentialConfirmation,
                    files: Vec::new(),
                    error: None,
                },
            ),
            Err(error) => (
                Vec::new(),
                DesktopImportPlanItem {
                    kind: DesktopImportItemKind::Credentials,
                    status: DesktopImportPlanItemStatus::InspectionFailed,
                    files: Vec::new(),
                    error: Some(error),
                },
            ),
        };
        items.push(credential_item);

        let status = summarize_plan(&items);
        Ok(DesktopImportPlan {
            id: plan_id,
            source_home,
            source_instance_identity: source.instance_identity().to_owned(),
            target_home,
            target_instance_identity: self.target.instance_identity().to_owned(),
            credential_keys,
            status,
            items,
        })
    }

    pub fn execute(
        &self,
        plan: &DesktopImportPlan,
        options: DesktopImportExecutionOptions,
    ) -> DesktopImportReport {
        if let Err(error) = self.validate_plan(plan) {
            return invalid_plan_report(plan, error);
        }

        let source_paths = LiliaDataPaths::from_home(&plan.source_home);
        let target_paths = LiliaDataPaths::from_home(&plan.target_home);
        let has_ready_database = plan.items.iter().any(|item| {
            matches!(item.kind, DesktopImportItemKind::Database(_))
                && item.status == DesktopImportPlanItemStatus::Ready
        });
        let needs_source_lock = has_ready_database
            || (options.credential_decision == CredentialImportDecision::Confirmed
                && !plan.credential_keys.is_empty());

        let mut source_guard = None;
        let mut target_guard = None;
        let lock_error = if needs_source_lock {
            match acquire_source_import_lock(&source_paths) {
                Ok(guard) => {
                    source_guard = Some(guard);
                    match acquire_target_import_lock(
                        &target_paths,
                        format!(
                            "{}:import-target:{}",
                            self.target.instance_identity(),
                            plan.id
                        ),
                    ) {
                        Ok(guard) => {
                            target_guard = Some(guard);
                            None
                        }
                        Err(error) => Some(target_lock_error(error)),
                    }
                }
                Err(error) => Some(error),
            }
        } else {
            None
        };

        let mut items = Vec::with_capacity(plan.items.len());
        for item in &plan.items {
            let report_item = match item.kind {
                DesktopImportItemKind::Database(kind) => {
                    if let Some(error) = &lock_error {
                        if item.status == DesktopImportPlanItemStatus::Ready {
                            failed_report_item(item.kind.clone(), error.clone())
                        } else {
                            report_from_non_ready_plan_item(item)
                        }
                    } else if item.status == DesktopImportPlanItemStatus::Ready {
                        execute_database_item(item, kind, &source_paths, &target_paths)
                    } else {
                        report_from_non_ready_plan_item(item)
                    }
                }
                DesktopImportItemKind::Credentials => {
                    if item.status == DesktopImportPlanItemStatus::RequiresCredentialConfirmation {
                        self.execute_credentials(
                            plan,
                            options.credential_decision,
                            &source_paths,
                            lock_error.as_ref(),
                        )
                    } else {
                        report_from_non_ready_plan_item(item)
                    }
                }
            };
            items.push(report_item);
        }

        drop(target_guard);
        drop(source_guard);
        DesktopImportReport {
            plan_id: plan.id.clone(),
            source_home: plan.source_home.clone(),
            target_home: plan.target_home.clone(),
            status: summarize_report(&items),
            items,
        }
    }

    fn validate_plan(&self, plan: &DesktopImportPlan) -> Result<(), DesktopImportItemError> {
        ensure_distinct_homes(&plan.source_home, &plan.target_home)
            .map_err(DesktopImportItemError::from)?;
        if plan.id.is_empty()
            || !plan
                .id
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '-')
        {
            return Err(invalid_plan_error(
                "import plan id must contain only ASCII letters, digits, or hyphens",
            ));
        }
        if plan.source_home != normalized_home(&plan.source_home)
            || plan.target_home != normalized_home(&plan.target_home)
        {
            return Err(invalid_plan_error(
                "import plan homes must be normalized absolute paths",
            ));
        }
        let validated_source =
            DesktopApplicationConfig::new(&plan.source_home, &plan.source_instance_identity)
                .map_err(|error| invalid_plan_error(error.to_string()))?;
        if validated_source.instance_identity() != plan.source_instance_identity {
            return Err(invalid_plan_error(
                "source instance identity must already be normalized",
            ));
        }
        if !homes_equal(&plan.target_home, &normalized_home(self.target.home()))
            || plan.target_instance_identity != self.target.instance_identity()
        {
            return Err(invalid_plan_error(
                "import plan belongs to a different target application",
            ));
        }
        if plan.items.len() != database_kinds().len() + 1
            || database_kinds().iter().any(|kind| {
                plan.items
                    .iter()
                    .filter(|item| item.kind == DesktopImportItemKind::Database(*kind))
                    .count()
                    != 1
            })
            || plan
                .items
                .iter()
                .filter(|item| item.kind == DesktopImportItemKind::Credentials)
                .count()
                != 1
        {
            return Err(invalid_plan_error(
                "import plan must contain each supported database and credentials exactly once",
            ));
        }
        let source_paths = LiliaDataPaths::from_home(&plan.source_home);
        let target_paths = LiliaDataPaths::from_home(&plan.target_home);
        for item in &plan.items {
            match item.kind {
                DesktopImportItemKind::Database(kind) => {
                    validate_database_plan_item(item, kind, &source_paths, &target_paths)?;
                }
                DesktopImportItemKind::Credentials => {
                    let valid_status = match item.status {
                        DesktopImportPlanItemStatus::RequiresCredentialConfirmation => {
                            item.error.is_none() && valid_credential_keys(&plan.credential_keys)
                        }
                        DesktopImportPlanItemStatus::InspectionFailed => {
                            item.error.is_some() && plan.credential_keys.is_empty()
                        }
                        _ => false,
                    };
                    if !valid_status || !item.files.is_empty() {
                        return Err(invalid_plan_error(
                            "credential plan item or key manifest is invalid",
                        ));
                    }
                    if item.status == DesktopImportPlanItemStatus::RequiresCredentialConfirmation
                        && inspect_credential_keys(&source_paths)
                            .map_err(|error| invalid_plan_error(error.message))?
                            != plan.credential_keys
                    {
                        return Err(invalid_plan_error(
                            "credential key manifest does not match the source registry",
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    fn execute_credentials(
        &self,
        plan: &DesktopImportPlan,
        decision: CredentialImportDecision,
        source_paths: &LiliaDataPaths,
        lock_error: Option<&DesktopImportItemError>,
    ) -> DesktopImportReportItem {
        let kind = DesktopImportItemKind::Credentials;
        match decision {
            CredentialImportDecision::NotDecided => DesktopImportReportItem {
                kind,
                status: DesktopImportReportItemStatus::AwaitingCredentialConfirmation,
                files: Vec::new(),
                error: None,
            },
            CredentialImportDecision::Denied => DesktopImportReportItem {
                kind,
                status: DesktopImportReportItemStatus::SkippedCredentialDenied,
                files: Vec::new(),
                error: None,
            },
            CredentialImportDecision::Confirmed => {
                if let Some(error) = lock_error.filter(|_| !plan.credential_keys.is_empty()) {
                    return failed_report_item(kind, error.clone());
                }
                let current_keys = match inspect_credential_keys(source_paths) {
                    Ok(keys) => keys,
                    Err(error) => return failed_report_item(kind, error),
                };
                if current_keys != plan.credential_keys {
                    return failed_report_item(
                        kind,
                        DesktopImportItemError::new(
                            DesktopImportErrorCode::SourceChanged,
                            "source credential manifest changed after the import plan was created",
                            true,
                        ),
                    );
                }
                let action =
                    DesktopHostAction::Credential(DesktopCredentialAction::ImportConfirmed {
                        source_instance_identity: plan.source_instance_identity.clone(),
                        keys: plan.credential_keys.clone(),
                    });
                match self.host.execute(&self.host_context, action) {
                    Ok(DesktopHostResult::CredentialImport(result)) => DesktopImportReportItem {
                        kind,
                        status: DesktopImportReportItemStatus::CredentialsImported {
                            imported: result.imported,
                            skipped: result.skipped,
                            failed: result.failed,
                        },
                        files: Vec::new(),
                        error: (result.failed > 0).then(|| {
                            DesktopImportItemError::new(
                                DesktopImportErrorCode::CredentialImportFailed,
                                format!("{} credential entries failed to import", result.failed),
                                true,
                            )
                        }),
                    },
                    Ok(_) => failed_report_item(
                        kind,
                        DesktopImportItemError::new(
                            DesktopImportErrorCode::UnexpectedHostResult,
                            "desktop host returned an unexpected credential import result",
                            false,
                        ),
                    ),
                    Err(error) => failed_report_item(
                        kind,
                        DesktopImportItemError::new(
                            DesktopImportErrorCode::Host,
                            error.to_string(),
                            error.retryable,
                        ),
                    ),
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopImportPlan {
    pub id: String,
    pub source_home: PathBuf,
    pub source_instance_identity: String,
    pub target_home: PathBuf,
    pub target_instance_identity: String,
    #[serde(default)]
    pub credential_keys: Vec<String>,
    pub status: DesktopImportPlanStatus,
    pub items: Vec<DesktopImportPlanItem>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopImportPlanStatus {
    Ready,
    Empty,
    Partial,
    Blocked,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopImportPlanItem {
    pub kind: DesktopImportItemKind,
    pub status: DesktopImportPlanItemStatus,
    pub files: Vec<DesktopImportFile>,
    pub error: Option<DesktopImportItemError>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopImportPlanItemStatus {
    Ready,
    MissingSource,
    Conflict,
    Incompatible,
    SourceBusy,
    InspectionFailed,
    RequiresCredentialConfirmation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "database", rename_all = "snake_case")]
pub enum DesktopImportItemKind {
    Database(DesktopDatabaseKind),
    Credentials,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopDatabaseKind {
    ProductProjections,
    Product,
    AgentRuntime,
    LegacyDesktop,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopImportFile {
    pub role: DesktopImportFileRole,
    pub source: PathBuf,
    pub target: PathBuf,
    pub metadata: DesktopImportFileMetadata,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopImportFileRole {
    Database,
    WriteAheadLog,
    SharedMemory,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopImportFileMetadata {
    pub length: u64,
    pub modified_unix_nanos: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DesktopImportExecutionOptions {
    pub credential_decision: CredentialImportDecision,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CredentialImportDecision {
    #[default]
    NotDecided,
    Denied,
    Confirmed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopImportReport {
    pub plan_id: String,
    pub source_home: PathBuf,
    pub target_home: PathBuf,
    pub status: DesktopImportReportStatus,
    pub items: Vec<DesktopImportReportItem>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopImportReportStatus {
    Completed,
    NothingToImport,
    AwaitingCredentialConfirmation,
    PartialFailure,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopImportReportItem {
    pub kind: DesktopImportItemKind,
    pub status: DesktopImportReportItemStatus,
    pub files: Vec<PathBuf>,
    pub error: Option<DesktopImportItemError>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DesktopImportReportItemStatus {
    Copied,
    MissingSource,
    Conflict,
    AwaitingCredentialConfirmation,
    SkippedCredentialDenied,
    CredentialsImported {
        imported: u32,
        skipped: u32,
        failed: u32,
    },
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopImportItemError {
    pub code: DesktopImportErrorCode,
    pub message: String,
    pub retryable: bool,
    #[serde(default)]
    pub residual_paths: Vec<PathBuf>,
}

impl DesktopImportItemError {
    fn new(code: DesktopImportErrorCode, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code,
            message: message.into(),
            retryable,
            residual_paths: Vec::new(),
        }
    }
}

impl From<DesktopImportError> for DesktopImportItemError {
    fn from(error: DesktopImportError) -> Self {
        match error {
            DesktopImportError::SameHome { .. } => {
                Self::new(DesktopImportErrorCode::SameHome, error.to_string(), false)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopImportErrorCode {
    SameHome,
    InvalidPlan,
    InspectionFailed,
    SourceBusy,
    SourceLockMissing,
    TargetBusy,
    SourceChanged,
    SourceIncomplete,
    TargetConflict,
    IncompatibleSqlite,
    BackupFailed,
    IntegrityCheckFailed,
    CleanupFailed,
    Io,
    Host,
    CredentialImportFailed,
    UnexpectedHostResult,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DesktopImportError {
    #[error("source and target Lilia homes must be different: `{home}`", home = home.display())]
    SameHome { home: PathBuf },
}

fn database_kinds() -> [DesktopDatabaseKind; 4] {
    [
        DesktopDatabaseKind::ProductProjections,
        DesktopDatabaseKind::Product,
        DesktopDatabaseKind::AgentRuntime,
        DesktopDatabaseKind::LegacyDesktop,
    ]
}

fn inspect_credential_keys(paths: &LiliaDataPaths) -> Result<Vec<String>, DesktopImportItemError> {
    let database = paths.agent_runtime_db();
    if !database.exists() {
        return Ok(Vec::new());
    }
    let connection = Connection::open_with_flags(
        &database,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| credential_inspection_error(&database, error))?;
    let has_registry = connection
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'product_credential_registry'",
            [],
            |_| Ok(()),
        )
        .optional()
        .map_err(|error| credential_inspection_error(&database, error))?
        .is_some();
    if !has_registry {
        return Ok(Vec::new());
    }
    let mut statement = connection
        .prepare("SELECT secret_id FROM product_credential_registry ORDER BY secret_id ASC")
        .map_err(|error| credential_inspection_error(&database, error))?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| credential_inspection_error(&database, error))?;
    let mut keys = Vec::new();
    for row in rows {
        let secret_id = row.map_err(|error| credential_inspection_error(&database, error))?;
        let key = crate::provider::credential_secret_key(&secret_id);
        if !valid_credential_key(&key) {
            return Err(DesktopImportItemError::new(
                DesktopImportErrorCode::InspectionFailed,
                "credential registry contains an invalid secret identifier",
                false,
            ));
        }
        keys.push(key);
    }
    keys.sort();
    keys.dedup();
    Ok(keys)
}

fn credential_inspection_error(database: &Path, error: rusqlite::Error) -> DesktopImportItemError {
    DesktopImportItemError::new(
        DesktopImportErrorCode::InspectionFailed,
        format!(
            "failed to inspect credential registry {}: {error}",
            database.display()
        ),
        false,
    )
}

fn valid_credential_keys(keys: &[String]) -> bool {
    keys.windows(2).all(|pair| pair[0] < pair[1])
        && keys.iter().all(|key| valid_credential_key(key))
}

fn valid_credential_key(key: &str) -> bool {
    let Some(secret_id) = key.strip_prefix("agentkit.") else {
        return false;
    };
    !secret_id.is_empty()
        && key.len() <= 256
        && secret_id.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | ':')
        })
}

fn database_path(paths: &LiliaDataPaths, kind: DesktopDatabaseKind) -> PathBuf {
    match kind {
        DesktopDatabaseKind::ProductProjections => paths.product_projections_db(),
        DesktopDatabaseKind::Product => paths.product_db(),
        DesktopDatabaseKind::AgentRuntime => paths.agent_runtime_db(),
        DesktopDatabaseKind::LegacyDesktop => paths.legacy_desktop_db(),
    }
}

fn database_component_paths(
    paths: &LiliaDataPaths,
    kind: DesktopDatabaseKind,
) -> [(DesktopImportFileRole, PathBuf); 3] {
    let database = database_path(paths, kind);
    [
        (DesktopImportFileRole::Database, database.clone()),
        (
            DesktopImportFileRole::WriteAheadLog,
            sidecar_path(&database, "-wal"),
        ),
        (
            DesktopImportFileRole::SharedMemory,
            sidecar_path(&database, "-shm"),
        ),
    ]
}

fn validate_database_plan_item(
    item: &DesktopImportPlanItem,
    kind: DesktopDatabaseKind,
    source_paths: &LiliaDataPaths,
    target_paths: &LiliaDataPaths,
) -> Result<(), DesktopImportItemError> {
    if item.status != DesktopImportPlanItemStatus::Ready {
        if item.files.is_empty() {
            return Ok(());
        }
        return Err(invalid_plan_error(
            "non-ready database plan item must not contain files",
        ));
    }

    let source_components = database_component_paths(source_paths, kind);
    let target_components = database_component_paths(target_paths, kind);
    let mut seen = [false; 3];
    for file in &item.files {
        let index = file_role_index(file.role);
        if seen[index] {
            return Err(invalid_plan_error(format!(
                "database plan item contains duplicate {:?} role",
                file.role
            )));
        }
        seen[index] = true;
        if file.source != source_components[index].1 || file.target != target_components[index].1 {
            return Err(invalid_plan_error(format!(
                "database plan item {:?} path is outside the canonical import whitelist",
                file.role
            )));
        }
    }

    if !seen[file_role_index(DesktopImportFileRole::Database)] {
        return Err(invalid_plan_error(
            "ready database plan item must contain its database file",
        ));
    }
    for (index, (role, source)) in source_components.iter().enumerate() {
        if *role == DesktopImportFileRole::SharedMemory {
            if seen[index] {
                return Err(invalid_plan_error(
                    "ready database plan item must not include SQLite shared memory",
                ));
            }
            continue;
        }
        if source.exists() != seen[index] {
            return Err(invalid_plan_error(format!(
                "database plan item does not exactly match source component `{}`",
                source.display()
            )));
        }
    }
    Ok(())
}

fn file_role_index(role: DesktopImportFileRole) -> usize {
    match role {
        DesktopImportFileRole::Database => 0,
        DesktopImportFileRole::WriteAheadLog => 1,
        DesktopImportFileRole::SharedMemory => 2,
    }
}

fn sidecar_path(database: &Path, suffix: &str) -> PathBuf {
    let mut name = database.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

fn inspect_database(
    kind: DesktopDatabaseKind,
    source_paths: &LiliaDataPaths,
    target_paths: &LiliaDataPaths,
) -> DesktopImportPlanItem {
    let source_components = database_component_paths(source_paths, kind);
    let target_components = database_component_paths(target_paths, kind);
    let source_database = &source_components[0].1;
    let source_sidecar_exists = source_components[1..].iter().any(|(_, path)| path.exists());

    if !source_database.exists() {
        if source_sidecar_exists {
            return error_plan_item(
                kind,
                DesktopImportPlanItemStatus::InspectionFailed,
                DesktopImportItemError::new(
                    DesktopImportErrorCode::SourceIncomplete,
                    format!(
                        "SQLite sidecar exists without database `{}`",
                        source_database.display()
                    ),
                    true,
                ),
            );
        }
        return missing_plan_item(kind, source_paths, target_paths);
    }

    if target_components.iter().any(|(_, path)| path.exists()) {
        return error_plan_item(
            kind,
            DesktopImportPlanItemStatus::Conflict,
            DesktopImportItemError::new(
                DesktopImportErrorCode::TargetConflict,
                format!(
                    "target already contains `{}` or one of its SQLite sidecars",
                    target_components[0].1.display()
                ),
                false,
            ),
        );
    }

    if let Err(error) = validate_sqlite_compatibility(source_database, kind) {
        return error_plan_item(kind, DesktopImportPlanItemStatus::Incompatible, error);
    }

    let mut files = Vec::new();
    for ((role, source), (_, target)) in source_components.into_iter().zip(target_components) {
        if role == DesktopImportFileRole::SharedMemory {
            continue;
        }
        if !source.exists() {
            continue;
        }
        match read_file_metadata(&source) {
            Ok(metadata) => files.push(DesktopImportFile {
                role,
                source,
                target,
                metadata,
            }),
            Err(error) => {
                return error_plan_item(kind, DesktopImportPlanItemStatus::InspectionFailed, error);
            }
        }
    }

    DesktopImportPlanItem {
        kind: DesktopImportItemKind::Database(kind),
        status: DesktopImportPlanItemStatus::Ready,
        files,
        error: None,
    }
}

fn missing_plan_item(
    kind: DesktopDatabaseKind,
    _source_paths: &LiliaDataPaths,
    _target_paths: &LiliaDataPaths,
) -> DesktopImportPlanItem {
    DesktopImportPlanItem {
        kind: DesktopImportItemKind::Database(kind),
        status: DesktopImportPlanItemStatus::MissingSource,
        files: Vec::new(),
        error: None,
    }
}

fn blocked_plan_item(
    kind: DesktopDatabaseKind,
    _source_paths: &LiliaDataPaths,
    _target_paths: &LiliaDataPaths,
    error: &DesktopImportItemError,
) -> DesktopImportPlanItem {
    DesktopImportPlanItem {
        kind: DesktopImportItemKind::Database(kind),
        status: DesktopImportPlanItemStatus::SourceBusy,
        files: Vec::new(),
        error: Some(error.clone()),
    }
}

fn error_plan_item(
    kind: DesktopDatabaseKind,
    status: DesktopImportPlanItemStatus,
    error: DesktopImportItemError,
) -> DesktopImportPlanItem {
    DesktopImportPlanItem {
        kind: DesktopImportItemKind::Database(kind),
        status,
        files: Vec::new(),
        error: Some(error),
    }
}

struct SourceImportGuard {
    _file: File,
}

fn acquire_source_import_lock(
    paths: &LiliaDataPaths,
) -> Result<SourceImportGuard, DesktopImportItemError> {
    let lock_path = paths.db_dir().join("writer.lock");
    let file = open_existing_source_lock(&lock_path).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            DesktopImportItemError::new(
                DesktopImportErrorCode::SourceLockMissing,
                format!(
                    "source writer lock `{}` is missing; source shutdown cannot be proven",
                    lock_path.display()
                ),
                true,
            )
        } else if error.kind() == io::ErrorKind::WouldBlock
            || error.kind() == io::ErrorKind::PermissionDenied
            || matches!(error.raw_os_error(), Some(16 | 32 | 33))
        {
            DesktopImportItemError::new(
                DesktopImportErrorCode::SourceBusy,
                format!("source writer lock `{}` is busy", lock_path.display()),
                true,
            )
        } else {
            io_item_error("acquire source writer lock", &lock_path, error)
        }
    })?;
    Ok(SourceImportGuard { _file: file })
}

#[cfg(windows)]
fn open_existing_source_lock(path: &Path) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;

    OpenOptions::new().read(true).share_mode(0).open(path)
}

#[cfg(unix)]
fn open_existing_source_lock(path: &Path) -> io::Result<File> {
    use std::os::unix::io::AsRawFd;

    let file = OpenOptions::new().read(true).open(path)?;
    unsafe extern "C" {
        fn flock(fd: i32, operation: i32) -> i32;
    }
    const LOCK_EX: i32 = 2;
    const LOCK_NB: i32 = 4;
    if unsafe { flock(file.as_raw_fd(), LOCK_EX | LOCK_NB) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(file)
}

#[cfg(not(any(unix, windows)))]
fn open_existing_source_lock(_path: &Path) -> io::Result<File> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "read-only source writer lock unsupported on this platform",
    ))
}

fn acquire_target_import_lock(
    paths: &LiliaDataPaths,
    owner: String,
) -> Result<StorageWriterGuard, WriterLeaseError> {
    StorageWriterGuard::try_acquire_with_file_lock(
        paths.product_projections_db().display().to_string(),
        owner,
        WriterMode::Embedded,
        paths.db_dir().join("writer.lock"),
    )
}

fn validate_sqlite_header(path: &Path) -> Result<(), DesktopImportItemError> {
    let mut file = File::open(path).map_err(|error| io_item_error("open", path, error))?;
    let mut header = [0_u8; 16];
    file.read_exact(&mut header).map_err(|error| {
        DesktopImportItemError::new(
            DesktopImportErrorCode::IncompatibleSqlite,
            format!(
                "SQLite database `{}` has no supported header: {error}",
                path.display()
            ),
            false,
        )
    })?;
    if &header != SQLITE_HEADER {
        return Err(DesktopImportItemError::new(
            DesktopImportErrorCode::IncompatibleSqlite,
            format!(
                "SQLite database `{}` has an unsupported file header",
                path.display()
            ),
            false,
        ));
    }
    Ok(())
}

fn validate_sqlite_compatibility(
    path: &Path,
    kind: DesktopDatabaseKind,
) -> Result<(), DesktopImportItemError> {
    validate_sqlite_header(path)?;
    let connection = open_source_database(path)?;
    let version = match kind {
        DesktopDatabaseKind::Product | DesktopDatabaseKind::ProductProjections => connection
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| incompatible_database_error(path, error))?,
        DesktopDatabaseKind::AgentRuntime | DesktopDatabaseKind::LegacyDesktop => connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .map_err(|error| incompatible_database_error(path, error))?,
    };
    let maximum = match kind {
        DesktopDatabaseKind::Product => PRODUCT_SCHEMA_VERSION,
        DesktopDatabaseKind::ProductProjections => 1,
        DesktopDatabaseKind::AgentRuntime => 1,
        DesktopDatabaseKind::LegacyDesktop => 30,
    };
    if version < 0 || version > maximum {
        return Err(DesktopImportItemError::new(
            DesktopImportErrorCode::IncompatibleSqlite,
            format!(
                "SQLite database `{}` schema version {version} exceeds supported range 0..={maximum}",
                path.display()
            ),
            false,
        ));
    }
    let required_table = match kind {
        DesktopDatabaseKind::Product | DesktopDatabaseKind::ProductProjections => {
            "schema_migrations"
        }
        DesktopDatabaseKind::AgentRuntime => "agent_runtime_sessions",
        DesktopDatabaseKind::LegacyDesktop => "tasks",
    };
    if !sqlite_table_exists(&connection, required_table)
        .map_err(|error| incompatible_database_error(path, error))?
    {
        return Err(DesktopImportItemError::new(
            DesktopImportErrorCode::IncompatibleSqlite,
            format!(
                "SQLite database `{}` is missing required table `{required_table}`",
                path.display()
            ),
            false,
        ));
    }
    Ok(())
}

fn open_source_database(path: &Path) -> Result<Connection, DesktopImportItemError> {
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| incompatible_database_error(path, error))
}

fn sqlite_table_exists(connection: &Connection, table: &str) -> rusqlite::Result<bool> {
    connection
        .query_row(
            "SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = ?1",
            [table],
            |_| Ok(true),
        )
        .optional()
        .map(Option::unwrap_or_default)
}

fn incompatible_database_error(
    path: &Path,
    error: impl std::fmt::Display,
) -> DesktopImportItemError {
    DesktopImportItemError::new(
        DesktopImportErrorCode::IncompatibleSqlite,
        format!(
            "SQLite database `{}` is incompatible: {error}",
            path.display()
        ),
        false,
    )
}

fn read_file_metadata(path: &Path) -> Result<DesktopImportFileMetadata, DesktopImportItemError> {
    let metadata = fs::metadata(path).map_err(|error| io_item_error("inspect", path, error))?;
    if !metadata.is_file() {
        return Err(DesktopImportItemError::new(
            DesktopImportErrorCode::SourceIncomplete,
            format!("import source `{}` is not a regular file", path.display()),
            false,
        ));
    }
    let modified_unix_nanos = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .and_then(|duration| u64::try_from(duration.as_nanos()).ok());
    Ok(DesktopImportFileMetadata {
        length: metadata.len(),
        modified_unix_nanos,
    })
}

fn execute_database_item(
    item: &DesktopImportPlanItem,
    kind: DesktopDatabaseKind,
    source_paths: &LiliaDataPaths,
    target_paths: &LiliaDataPaths,
) -> DesktopImportReportItem {
    if let Err(error) = revalidate_item(item, kind, source_paths, target_paths) {
        return failed_report_item(item.kind.clone(), error);
    }
    if let Err(error) = fs::create_dir_all(target_paths.db_dir()) {
        return failed_report_item(
            item.kind.clone(),
            io_item_error(
                "create target database directory",
                &target_paths.db_dir(),
                error,
            ),
        );
    }

    match copy_database_group(item) {
        Ok(files) => DesktopImportReportItem {
            kind: item.kind.clone(),
            status: DesktopImportReportItemStatus::Copied,
            files,
            error: None,
        },
        Err(error) => failed_report_item(item.kind.clone(), error),
    }
}

fn revalidate_item(
    item: &DesktopImportPlanItem,
    kind: DesktopDatabaseKind,
    source_paths: &LiliaDataPaths,
    target_paths: &LiliaDataPaths,
) -> Result<(), DesktopImportItemError> {
    if database_component_paths(target_paths, kind)
        .iter()
        .any(|(_, path)| path.exists())
    {
        return Err(DesktopImportItemError::new(
            DesktopImportErrorCode::TargetConflict,
            "target database or SQLite sidecar appeared after planning",
            false,
        ));
    }

    let current_paths: Vec<(DesktopImportFileRole, PathBuf)> =
        database_component_paths(source_paths, kind)
            .into_iter()
            .filter(|(role, path)| *role != DesktopImportFileRole::SharedMemory && path.exists())
            .collect();
    if current_paths.len() != item.files.len() {
        return Err(DesktopImportItemError::new(
            DesktopImportErrorCode::SourceChanged,
            "source SQLite file set changed after planning",
            true,
        ));
    }
    for ((role, path), planned) in current_paths.into_iter().zip(&item.files) {
        let expected_target = database_component_paths(target_paths, kind)
            .into_iter()
            .find_map(|(candidate_role, candidate_path)| {
                (candidate_role == role).then_some(candidate_path)
            });
        if role != planned.role
            || path != planned.source
            || expected_target.as_ref() != Some(&planned.target)
        {
            return Err(DesktopImportItemError::new(
                DesktopImportErrorCode::SourceChanged,
                "source SQLite file set changed after planning",
                true,
            ));
        }
        let metadata = read_file_metadata(&path)?;
        if metadata != planned.metadata {
            return Err(DesktopImportItemError::new(
                DesktopImportErrorCode::SourceChanged,
                format!("source metadata changed for `{}`", path.display()),
                true,
            ));
        }
    }
    validate_sqlite_compatibility(&database_path(source_paths, kind), kind)
}

fn copy_database_group(
    item: &DesktopImportPlanItem,
) -> Result<Vec<PathBuf>, DesktopImportItemError> {
    let database = item
        .files
        .iter()
        .find(|file| file.role == DesktopImportFileRole::Database)
        .ok_or_else(|| invalid_plan_error("ready database item has no main database file"))?;
    let (staging, reservation) = create_staging_file(&database.target)?;
    drop(reservation);

    let backup_result = (|| {
        let source = open_source_database(&database.source)?;
        let mut destination = Connection::open(&staging)
            .map_err(|error| sqlite_backup_error("open staging database", &staging, error))?;
        {
            let backup = Backup::new(&source, &mut destination)
                .map_err(|error| sqlite_backup_error("initialize backup", &staging, error))?;
            backup
                .run_to_completion(128, Duration::from_millis(1), None)
                .map_err(|error| sqlite_backup_error("run backup", &staging, error))?;
        }
        let journal_mode: String = destination
            .query_row("PRAGMA journal_mode = DELETE", [], |row| row.get(0))
            .map_err(|error| {
                sqlite_backup_error("finalize rollback journal mode", &staging, error)
            })?;
        if !journal_mode.eq_ignore_ascii_case("delete") {
            return Err(DesktopImportItemError::new(
                DesktopImportErrorCode::BackupFailed,
                format!(
                    "staging database `{}` retained unsupported journal mode `{journal_mode}`",
                    staging.display()
                ),
                true,
            ));
        }
        let integrity: String = destination
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .map_err(|error| sqlite_backup_error("integrity check", &staging, error))?;
        if integrity != "ok" {
            return Err(DesktopImportItemError::new(
                DesktopImportErrorCode::IntegrityCheckFailed,
                format!(
                    "staging database `{}` failed integrity_check: {integrity}",
                    staging.display()
                ),
                true,
            ));
        }
        drop(destination);
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(&staging)
            .and_then(|file| file.sync_all())
            .map_err(|error| io_item_error("sync staging database", &staging, error))?;
        Ok(())
    })();
    if let Err(error) = backup_result {
        return Err(remove_owned_staging(&staging, error));
    }
    let staging_auxiliary = staging_auxiliary_paths(&staging);
    if let Some(path) = staging_auxiliary.iter().find(|path| path.exists()) {
        return Err(remove_owned_staging(
            &staging,
            DesktopImportItemError::new(
                DesktopImportErrorCode::BackupFailed,
                format!(
                    "staging database left an auxiliary file `{}` after finalization",
                    path.display()
                ),
                true,
            ),
        ));
    }
    let target_conflict = std::iter::once(database.target.clone())
        .chain(staging_auxiliary_paths(&database.target))
        .find(|path| path.exists());
    if let Some(path) = target_conflict {
        return Err(remove_owned_staging(
            &staging,
            DesktopImportItemError::new(
                DesktopImportErrorCode::TargetConflict,
                format!(
                    "target SQLite component `{}` appeared during import",
                    path.display()
                ),
                false,
            ),
        ));
    }
    if let Err(error) = fs::rename(&staging, &database.target) {
        return Err(remove_owned_staging(
            &staging,
            io_item_error("commit staging database", &database.target, error),
        ));
    }
    Ok(vec![database.target.clone()])
}

fn create_staging_file(target: &Path) -> Result<(PathBuf, File), DesktopImportItemError> {
    let Some(file_name) = target.file_name() else {
        return Err(invalid_plan_error(format!(
            "target `{}` has no file name",
            target.display()
        )));
    };
    for _ in 0..16 {
        let unique = next_internal_staging_id();
        let mut staging_name = std::ffi::OsString::from(".lilia-import-");
        staging_name.push(format!("{}-{unique}-", std::process::id()));
        staging_name.push(file_name);
        let staging = target.with_file_name(staging_name);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staging)
        {
            Ok(file) => {
                if staging_auxiliary_paths(&staging)
                    .iter()
                    .any(|path| path.exists())
                {
                    drop(file);
                    fs::remove_file(&staging).map_err(|error| {
                        io_item_error("remove staging collision", &staging, error)
                    })?;
                    continue;
                }
                return Ok((staging, file));
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(io_item_error("create staging file", &staging, error)),
        }
    }
    Err(DesktopImportItemError::new(
        DesktopImportErrorCode::Io,
        format!(
            "could not allocate a unique staging file beside `{}`",
            target.display()
        ),
        true,
    ))
}

fn remove_owned_staging(
    staging: &Path,
    mut error: DesktopImportItemError,
) -> DesktopImportItemError {
    let mut cleanup_failures = Vec::new();
    for path in staging_auxiliary_paths(staging)
        .into_iter()
        .chain(std::iter::once(staging.to_path_buf()))
    {
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(cleanup_error) if cleanup_error.kind() == io::ErrorKind::NotFound => {}
            Err(cleanup_error) => {
                error.residual_paths.push(path.clone());
                cleanup_failures.push(format!("`{}`: {cleanup_error}", path.display()));
            }
        }
    }
    if !cleanup_failures.is_empty() {
        error.code = DesktopImportErrorCode::CleanupFailed;
        error.message = format!(
            "{}; cleanup failed for residual staging files: {}",
            error.message,
            cleanup_failures.join(", ")
        );
        error.retryable = false;
    }
    error
}

fn staging_auxiliary_paths(database: &Path) -> [PathBuf; 3] {
    [
        sidecar_path(database, "-journal"),
        sidecar_path(database, "-wal"),
        sidecar_path(database, "-shm"),
    ]
}

fn report_from_non_ready_plan_item(item: &DesktopImportPlanItem) -> DesktopImportReportItem {
    let status = match item.status {
        DesktopImportPlanItemStatus::MissingSource => DesktopImportReportItemStatus::MissingSource,
        DesktopImportPlanItemStatus::Conflict => DesktopImportReportItemStatus::Conflict,
        DesktopImportPlanItemStatus::RequiresCredentialConfirmation => {
            DesktopImportReportItemStatus::AwaitingCredentialConfirmation
        }
        DesktopImportPlanItemStatus::Ready => DesktopImportReportItemStatus::Failed,
        DesktopImportPlanItemStatus::Incompatible
        | DesktopImportPlanItemStatus::SourceBusy
        | DesktopImportPlanItemStatus::InspectionFailed => DesktopImportReportItemStatus::Failed,
    };
    DesktopImportReportItem {
        kind: item.kind.clone(),
        status,
        files: Vec::new(),
        error: item.error.clone(),
    }
}

fn failed_report_item(
    kind: DesktopImportItemKind,
    error: DesktopImportItemError,
) -> DesktopImportReportItem {
    let files = error.residual_paths.clone();
    DesktopImportReportItem {
        kind,
        status: DesktopImportReportItemStatus::Failed,
        files,
        error: Some(error),
    }
}

fn summarize_plan(items: &[DesktopImportPlanItem]) -> DesktopImportPlanStatus {
    let database_items = items
        .iter()
        .filter(|item| matches!(item.kind, DesktopImportItemKind::Database(_)));
    let statuses: Vec<_> = database_items.map(|item| item.status).collect();
    let ready = statuses
        .iter()
        .filter(|status| **status == DesktopImportPlanItemStatus::Ready)
        .count();
    let missing = statuses
        .iter()
        .filter(|status| **status == DesktopImportPlanItemStatus::MissingSource)
        .count();
    if missing == statuses.len() {
        DesktopImportPlanStatus::Empty
    } else if ready == statuses.len() - missing {
        DesktopImportPlanStatus::Ready
    } else if ready > 0 {
        DesktopImportPlanStatus::Partial
    } else {
        DesktopImportPlanStatus::Blocked
    }
}

fn summarize_report(items: &[DesktopImportReportItem]) -> DesktopImportReportStatus {
    let copied = items.iter().any(|item| {
        matches!(
            item.status,
            DesktopImportReportItemStatus::Copied
                | DesktopImportReportItemStatus::CredentialsImported { failed: 0, .. }
        )
    });
    let failed = items.iter().any(|item| {
        matches!(
            item.status,
            DesktopImportReportItemStatus::Failed | DesktopImportReportItemStatus::Conflict
        ) || matches!(
            item.status,
            DesktopImportReportItemStatus::CredentialsImported { failed: 1.., .. }
        )
    });
    let awaiting = items
        .iter()
        .any(|item| item.status == DesktopImportReportItemStatus::AwaitingCredentialConfirmation);
    if failed && copied {
        DesktopImportReportStatus::PartialFailure
    } else if failed {
        DesktopImportReportStatus::Failed
    } else if awaiting {
        DesktopImportReportStatus::AwaitingCredentialConfirmation
    } else if copied {
        DesktopImportReportStatus::Completed
    } else {
        DesktopImportReportStatus::NothingToImport
    }
}

fn invalid_plan_report(
    plan: &DesktopImportPlan,
    error: DesktopImportItemError,
) -> DesktopImportReport {
    DesktopImportReport {
        plan_id: plan.id.clone(),
        source_home: plan.source_home.clone(),
        target_home: plan.target_home.clone(),
        status: DesktopImportReportStatus::Failed,
        items: plan
            .items
            .iter()
            .map(|item| failed_report_item(item.kind.clone(), error.clone()))
            .collect(),
    }
}

fn ensure_distinct_homes(source: &Path, target: &Path) -> Result<(), DesktopImportError> {
    let source = normalized_home(source);
    let target = normalized_home(target);
    if homes_equal(&source, &target) {
        return Err(DesktopImportError::SameHome { home: source });
    }
    Ok(())
}

fn normalized_home(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|current| current.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    };
    let normalized = lexical_normalize(&absolute);
    let mut existing = normalized.clone();
    let mut missing = Vec::new();
    while !existing.exists() {
        let Some(name) = existing.file_name().map(ToOwned::to_owned) else {
            return normalized;
        };
        missing.push(name);
        if !existing.pop() {
            return normalized;
        }
    }
    let mut resolved = fs::canonicalize(&existing).unwrap_or(existing);
    for component in missing.into_iter().rev() {
        resolved.push(component);
    }
    resolved
}

fn lexical_normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            component => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

#[cfg(windows)]
fn homes_equal(left: &Path, right: &Path) -> bool {
    left.to_string_lossy()
        .eq_ignore_ascii_case(&right.to_string_lossy())
}

#[cfg(not(windows))]
fn homes_equal(left: &Path, right: &Path) -> bool {
    left == right
}

fn target_lock_error(error: WriterLeaseError) -> DesktopImportItemError {
    DesktopImportItemError::new(DesktopImportErrorCode::TargetBusy, error.to_string(), true)
}

fn io_item_error(operation: &str, path: &Path, error: io::Error) -> DesktopImportItemError {
    DesktopImportItemError::new(
        DesktopImportErrorCode::Io,
        format!("{operation} `{}`: {error}", path.display()),
        true,
    )
}

fn sqlite_backup_error(
    operation: &str,
    path: &Path,
    error: impl std::fmt::Display,
) -> DesktopImportItemError {
    DesktopImportItemError::new(
        DesktopImportErrorCode::BackupFailed,
        format!("{operation} `{}`: {error}", path.display()),
        true,
    )
}

fn invalid_plan_error(message: impl Into<String>) -> DesktopImportItemError {
    DesktopImportItemError::new(DesktopImportErrorCode::InvalidPlan, message, false)
}

fn next_plan_id() -> String {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    let sequence = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    format!("{timestamp}-{sequence}")
}

fn next_internal_staging_id() -> u64 {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    use crate::{DesktopHostError, HostCredentialImportResult};

    use super::*;

    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

    struct TestDirectory {
        path: PathBuf,
    }

    impl TestDirectory {
        fn new(label: &str) -> Self {
            let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "lilia-desktop-import-{label}-{}-{id}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn child(&self, name: &str) -> PathBuf {
            self.path.join(name)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let is_generated_test_directory = self
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("lilia-desktop-import-"));
            if is_generated_test_directory {
                let _ = fs::remove_dir_all(&self.path);
            }
        }
    }

    struct TestHost {
        actions: Mutex<Vec<DesktopHostAction>>,
        credential_result: HostCredentialImportResult,
    }

    impl Default for TestHost {
        fn default() -> Self {
            Self {
                actions: Mutex::new(Vec::new()),
                credential_result: HostCredentialImportResult {
                    imported: 0,
                    skipped: 0,
                    failed: 0,
                },
            }
        }
    }

    impl DesktopHost for TestHost {
        fn execute(
            &self,
            _context: &DesktopHostContext,
            action: DesktopHostAction,
        ) -> Result<DesktopHostResult, DesktopHostError> {
            self.actions.lock().unwrap().push(action.clone());
            match action {
                DesktopHostAction::Credential(DesktopCredentialAction::ImportConfirmed {
                    ..
                }) => Ok(DesktopHostResult::CredentialImport(
                    self.credential_result.clone(),
                )),
                _ => Ok(DesktopHostResult::Completed),
            }
        }
    }

    fn config(home: PathBuf, identity: &str) -> DesktopApplicationConfig {
        DesktopApplicationConfig::new(home, identity).unwrap()
    }

    fn service(
        root: &TestDirectory,
        host: Arc<TestHost>,
    ) -> (DesktopDataImportService, DesktopApplicationConfig) {
        let source = config(root.child("source"), "liliacode");
        let target = config(root.child("target"), "liliacode.native-preview");
        (DesktopDataImportService::new(target, host), source)
    }

    fn write_database(home: &Path, kind: DesktopDatabaseKind, marker: u8) -> Vec<u8> {
        let paths = LiliaDataPaths::from_home(home);
        fs::create_dir_all(paths.db_dir()).unwrap();
        let writer_lock = paths.db_dir().join("writer.lock");
        if !writer_lock.exists() {
            fs::write(writer_lock, b"stable-source-writer-lock").unwrap();
        }
        let path = database_path(&paths, kind);
        let connection = Connection::open(&path).unwrap();
        initialize_database(&connection, kind, marker);
        drop(connection);
        fs::read(path).unwrap()
    }

    fn initialize_database(connection: &Connection, kind: DesktopDatabaseKind, marker: u8) {
        match kind {
            DesktopDatabaseKind::Product => connection
                .execute_batch(
                    "CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY);\
                     INSERT INTO schema_migrations(version) VALUES (5);",
                )
                .unwrap(),
            DesktopDatabaseKind::ProductProjections => connection
                .execute_batch(
                    "CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY);\
                     INSERT INTO schema_migrations(version) VALUES (1);",
                )
                .unwrap(),
            DesktopDatabaseKind::AgentRuntime => connection
                .execute_batch(
                    "PRAGMA user_version = 1;\
                     CREATE TABLE agent_runtime_sessions(\
                       session_id TEXT PRIMARY KEY, payload TEXT NOT NULL\
                     );",
                )
                .unwrap(),
            DesktopDatabaseKind::LegacyDesktop => connection
                .execute_batch(
                    "PRAGMA user_version = 30;\
                     CREATE TABLE tasks(id TEXT PRIMARY KEY);",
                )
                .unwrap(),
        }
        connection
            .execute_batch("CREATE TABLE import_marker(value INTEGER NOT NULL);")
            .unwrap();
        connection
            .execute("INSERT INTO import_marker(value) VALUES (?1)", [marker])
            .unwrap();
    }

    fn read_marker(path: &Path) -> u8 {
        Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap()
            .query_row("SELECT value FROM import_marker", [], |row| row.get(0))
            .unwrap()
    }

    fn write_credential_registry(source: &DesktopApplicationConfig, secret_ids: &[&str]) {
        write_database(source.home(), DesktopDatabaseKind::AgentRuntime, 7);
        let connection = Connection::open(source.data_paths().agent_runtime_db()).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE product_credential_registry (
                   credential_id TEXT PRIMARY KEY,
                   secret_id TEXT NOT NULL,
                   descriptor_json TEXT NOT NULL,
                   revocation_intent_json TEXT
                 );",
            )
            .unwrap();
        for (index, secret_id) in secret_ids.iter().enumerate() {
            connection
                .execute(
                    "INSERT INTO product_credential_registry
                       (credential_id, secret_id, descriptor_json)
                     VALUES (?1, ?2, '{}')",
                    (format!("credential-{index}"), *secret_id),
                )
                .unwrap();
        }
    }

    fn database_item(
        plan: &DesktopImportPlan,
        kind: DesktopDatabaseKind,
    ) -> &DesktopImportPlanItem {
        plan.items
            .iter()
            .find(|item| item.kind == DesktopImportItemKind::Database(kind))
            .unwrap()
    }

    fn database_item_mut(
        plan: &mut DesktopImportPlan,
        kind: DesktopDatabaseKind,
    ) -> &mut DesktopImportPlanItem {
        plan.items
            .iter_mut()
            .find(|item| item.kind == DesktopImportItemKind::Database(kind))
            .unwrap()
    }

    fn assert_invalid_plan(service: &DesktopDataImportService, plan: &DesktopImportPlan) {
        let report = service.execute(plan, denied_options());
        assert_eq!(report.status, DesktopImportReportStatus::Failed);
        assert!(report.items.iter().all(|item| {
            item.error.as_ref().map(|error| error.code) == Some(DesktopImportErrorCode::InvalidPlan)
        }));
        assert!(!service.target().data_paths().db_dir().exists());
    }

    fn directory_file_contents(directory: &Path) -> BTreeMap<String, Vec<u8>> {
        fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap())
            .filter(|entry| entry.file_type().unwrap().is_file())
            .map(|entry| {
                (
                    entry.file_name().to_string_lossy().into_owned(),
                    fs::read(entry.path()).unwrap(),
                )
            })
            .collect()
    }

    fn denied_options() -> DesktopImportExecutionOptions {
        DesktopImportExecutionOptions {
            credential_decision: CredentialImportDecision::Denied,
        }
    }

    #[test]
    fn empty_source_is_a_successful_no_op() {
        let root = TestDirectory::new("empty");
        let host = Arc::new(TestHost::default());
        let (service, source) = service(&root, host.clone());

        let plan = service.plan(&source).unwrap();
        assert_eq!(plan.status, DesktopImportPlanStatus::Empty);
        assert!(
            plan.items
                .iter()
                .filter(|item| {
                    matches!(item.kind, DesktopImportItemKind::Database(_))
                        && item.status == DesktopImportPlanItemStatus::MissingSource
                })
                .count()
                == 4
        );

        let report = service.execute(&plan, denied_options());
        assert_eq!(report.status, DesktopImportReportStatus::NothingToImport);
        assert!(host.actions.lock().unwrap().is_empty());
        assert!(!service.target().data_paths().product_db().exists());
    }

    #[test]
    fn live_wal_database_is_backed_up_into_one_target_database() {
        let root = TestDirectory::new("wal-group");
        let host = Arc::new(TestHost::default());
        let (service, source) = service(&root, host);
        let source_paths = source.data_paths();
        fs::create_dir_all(source_paths.db_dir()).unwrap();
        fs::write(
            source_paths.db_dir().join("writer.lock"),
            b"stable-source-writer-lock",
        )
        .unwrap();
        let source_database = source_paths.product_db();
        let source_connection = Connection::open(&source_database).unwrap();
        source_connection
            .execute_batch("PRAGMA journal_mode = WAL; PRAGMA wal_autocheckpoint = 0;")
            .unwrap();
        initialize_database(&source_connection, DesktopDatabaseKind::Product, 7);
        let source_wal = sidecar_path(&source_database, "-wal");
        let source_shm = sidecar_path(&source_database, "-shm");
        assert!(source_wal.exists());

        let plan = service.plan(&source).unwrap();
        let item = database_item(&plan, DesktopDatabaseKind::Product);
        assert_eq!(item.status, DesktopImportPlanItemStatus::Ready);
        assert!(item.files.len() >= 2);

        let report = service.execute(&plan, denied_options());
        assert_eq!(
            report.status,
            DesktopImportReportStatus::Completed,
            "{report:#?}"
        );
        let target = service.target().data_paths().product_db();
        assert_eq!(read_marker(&target), 7);
        assert!(!sidecar_path(&target, "-wal").exists());
        assert!(!sidecar_path(&target, "-shm").exists());
        assert!(source_wal.exists());
        assert!(source_shm.exists());
        assert!(item
            .files
            .iter()
            .all(|file| file.role != DesktopImportFileRole::SharedMemory));
        drop(source_connection);
    }

    #[test]
    fn json_plan_roundtrip_can_be_revalidated_and_executed() {
        let root = TestDirectory::new("plan-json-roundtrip");
        let host = Arc::new(TestHost::default());
        let (service, source) = service(&root, host);
        write_database(source.home(), DesktopDatabaseKind::Product, 8);
        fs::write(
            source.data_paths().product_projections_db(),
            b"unsupported sqlite version",
        )
        .unwrap();
        let plan = service.plan(&source).unwrap();
        assert_eq!(plan.status, DesktopImportPlanStatus::Partial);

        let plan_file = root.child("import-plan.json");
        fs::write(&plan_file, serde_json::to_vec_pretty(&plan).unwrap()).unwrap();
        let restored: DesktopImportPlan =
            serde_json::from_slice(&fs::read(plan_file).unwrap()).unwrap();
        assert_eq!(restored, plan);

        let report = service.execute(&restored, denied_options());
        assert_eq!(report.status, DesktopImportReportStatus::PartialFailure);
        assert_eq!(read_marker(&service.target().data_paths().product_db()), 8);
    }

    #[test]
    fn tampered_plan_file_specs_are_rejected_before_any_external_io() {
        let root = TestDirectory::new("tampered-plan-files");
        let host = Arc::new(TestHost::default());
        let (service, source) = service(&root, host);
        write_database(source.home(), DesktopDatabaseKind::Product, 9);
        let plan = service.plan(&source).unwrap();
        let external_source = root.child("external-source.txt");
        let external_target = root.child("external-target.txt");
        fs::write(&external_source, b"external source sentinel").unwrap();
        fs::write(&external_target, b"external target sentinel").unwrap();

        let mut source_tampered = plan.clone();
        database_item_mut(&mut source_tampered, DesktopDatabaseKind::Product).files[0].source =
            external_source.clone();
        assert_invalid_plan(&service, &source_tampered);

        let mut target_tampered = plan.clone();
        database_item_mut(&mut target_tampered, DesktopDatabaseKind::Product).files[0].target =
            external_target.clone();
        assert_invalid_plan(&service, &target_tampered);

        let mut role_tampered = plan.clone();
        database_item_mut(&mut role_tampered, DesktopDatabaseKind::Product).files[0].role =
            DesktopImportFileRole::WriteAheadLog;
        assert_invalid_plan(&service, &role_tampered);

        let mut duplicated = plan.clone();
        let product = database_item_mut(&mut duplicated, DesktopDatabaseKind::Product);
        product.files.push(product.files[0].clone());
        assert_invalid_plan(&service, &duplicated);

        let mut missing_database = plan;
        database_item_mut(&mut missing_database, DesktopDatabaseKind::Product)
            .files
            .clear();
        assert_invalid_plan(&service, &missing_database);

        assert_eq!(
            fs::read(external_source).unwrap(),
            b"external source sentinel"
        );
        assert_eq!(
            fs::read(external_target).unwrap(),
            b"external target sentinel"
        );
    }

    #[test]
    fn source_identity_must_pass_application_config_validation() {
        let root = TestDirectory::new("tampered-source-identity");
        let host = Arc::new(TestHost::default());
        let (service, source) = service(&root, host.clone());
        let mut plan = service.plan(&source).unwrap();
        plan.source_instance_identity = "liliacode\0forged".into();

        assert_invalid_plan(&service, &plan);
        assert!(host.actions.lock().unwrap().is_empty());
    }

    #[test]
    fn source_lock_and_database_files_are_unchanged_by_plan_and_execute() {
        let root = TestDirectory::new("source-read-only");
        let host = Arc::new(TestHost::default());
        let (service, source) = service(&root, host);
        write_database(source.home(), DesktopDatabaseKind::Product, 6);
        let source_db_dir = source.data_paths().db_dir();
        let before = directory_file_contents(&source_db_dir);

        let plan = service.plan(&source).unwrap();
        assert_eq!(directory_file_contents(&source_db_dir), before);
        let report = service.execute(&plan, denied_options());
        assert_eq!(
            report.status,
            DesktopImportReportStatus::Completed,
            "{report:#?}"
        );
        assert_eq!(directory_file_contents(&source_db_dir), before);
    }

    #[test]
    fn missing_source_writer_lock_blocks_without_creating_it() {
        let root = TestDirectory::new("source-lock-missing");
        let host = Arc::new(TestHost::default());
        let (service, source) = service(&root, host);
        let contents = write_database(source.home(), DesktopDatabaseKind::Product, 7);
        let lock_path = source.data_paths().db_dir().join("writer.lock");
        fs::remove_file(&lock_path).unwrap();

        let plan = service.plan(&source).unwrap();
        assert_eq!(plan.status, DesktopImportPlanStatus::Blocked);
        assert!(
            plan.items
                .iter()
                .filter(|item| {
                    matches!(item.kind, DesktopImportItemKind::Database(_))
                        && item.error.as_ref().map(|error| error.code)
                            == Some(DesktopImportErrorCode::SourceLockMissing)
                })
                .count()
                == 4
        );
        assert!(!lock_path.exists());
        assert_eq!(
            fs::read(source.data_paths().product_db()).unwrap(),
            contents
        );
    }

    #[test]
    fn preexisting_legacy_staging_name_is_never_removed() {
        let root = TestDirectory::new("staging-sentinel");
        let host = Arc::new(TestHost::default());
        let (service, source) = service(&root, host);
        write_database(source.home(), DesktopDatabaseKind::Product, 4);
        let plan = service.plan(&source).unwrap();
        let target = service.target().data_paths().product_db();
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        let legacy_staging = target.with_file_name(format!("product.db.{}.importing", plan.id));
        fs::write(&legacy_staging, b"preexisting staging sentinel").unwrap();

        let report = service.execute(&plan, denied_options());
        assert_eq!(
            report.status,
            DesktopImportReportStatus::Completed,
            "{report:#?}"
        );
        assert_eq!(
            fs::read(legacy_staging).unwrap(),
            b"preexisting staging sentinel"
        );
        assert!(target.exists());
    }

    #[test]
    fn relative_source_config_produces_only_absolute_plan_paths() {
        let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let relative_root = PathBuf::from("target").join(format!(
            "lilia-desktop-import-relative-{}-{id}",
            std::process::id()
        ));
        fs::create_dir_all(&relative_root).unwrap();
        let root = TestDirectory {
            path: normalized_home(&relative_root),
        };
        let source_relative = relative_root.join("source");
        let source = config(source_relative.clone(), "liliacode");
        let target = config(root.child("target"), "liliacode.native-preview");
        write_database(&source_relative, DesktopDatabaseKind::Product, 3);
        let service = DesktopDataImportService::new(target, Arc::new(TestHost::default()));

        let plan = service.plan(&source).unwrap();
        assert!(plan.source_home.is_absolute());
        assert!(plan.target_home.is_absolute());
        assert!(plan
            .items
            .iter()
            .flat_map(|item| &item.files)
            .all(|file| { file.source.is_absolute() && file.target.is_absolute() }));

        drop(plan);
        drop(service);
        drop(source);
        fs::remove_dir_all(&root.path).unwrap();
    }

    #[test]
    fn repeated_import_reports_conflict_and_never_overwrites_target() {
        let root = TestDirectory::new("repeat");
        let host = Arc::new(TestHost::default());
        let (service, source) = service(&root, host);
        write_database(source.home(), DesktopDatabaseKind::Product, 4);
        let first = service.plan(&source).unwrap();
        let first_report = service.execute(&first, denied_options());
        assert_eq!(
            first_report.status,
            DesktopImportReportStatus::Completed,
            "{first_report:#?}"
        );
        let target = service.target().data_paths().product_db();
        let original = fs::read(&target).unwrap();

        let repeated = service.plan(&source).unwrap();
        assert_eq!(
            database_item(&repeated, DesktopDatabaseKind::Product).status,
            DesktopImportPlanItemStatus::Conflict
        );
        let report = service.execute(&repeated, denied_options());
        assert_eq!(report.status, DesktopImportReportStatus::Failed);
        assert_eq!(fs::read(target).unwrap(), original);
    }

    #[test]
    fn compatible_items_can_succeed_when_another_database_is_incompatible() {
        let root = TestDirectory::new("partial");
        let host = Arc::new(TestHost::default());
        let (service, source) = service(&root, host);
        write_database(source.home(), DesktopDatabaseKind::Product, 2);
        let invalid = source.data_paths().product_projections_db();
        fs::write(&invalid, b"not sqlite").unwrap();

        let plan = service.plan(&source).unwrap();
        assert_eq!(plan.status, DesktopImportPlanStatus::Partial);
        assert_eq!(
            database_item(&plan, DesktopDatabaseKind::ProductProjections).status,
            DesktopImportPlanItemStatus::Incompatible
        );
        let report = service.execute(&plan, denied_options());
        assert_eq!(
            report.status,
            DesktopImportReportStatus::PartialFailure,
            "{report:#?}"
        );
        assert!(service.target().data_paths().product_db().exists());
        assert!(!service
            .target()
            .data_paths()
            .product_projections_db()
            .exists());
    }

    #[test]
    fn future_database_schemas_are_blocked_during_planning() {
        let root = TestDirectory::new("future-schema");
        let host = Arc::new(TestHost::default());
        let (service, source) = service(&root, host);
        for kind in database_kinds() {
            write_database(source.home(), kind, 1);
            let connection = Connection::open(database_path(&source.data_paths(), kind)).unwrap();
            match kind {
                DesktopDatabaseKind::Product => {
                    connection
                        .execute(
                            "INSERT INTO schema_migrations(version) VALUES (?1)",
                            [PRODUCT_SCHEMA_VERSION + 1],
                        )
                        .unwrap();
                }
                DesktopDatabaseKind::ProductProjections => {
                    connection
                        .execute("INSERT INTO schema_migrations(version) VALUES (2)", [])
                        .unwrap();
                }
                DesktopDatabaseKind::AgentRuntime => {
                    connection
                        .execute_batch("PRAGMA user_version = 2;")
                        .unwrap();
                }
                DesktopDatabaseKind::LegacyDesktop => {
                    connection
                        .execute_batch("PRAGMA user_version = 31;")
                        .unwrap();
                }
            }
        }

        let plan = service.plan(&source).unwrap();
        assert_eq!(plan.status, DesktopImportPlanStatus::Blocked);
        assert!(database_kinds().into_iter().all(|kind| {
            let item = database_item(&plan, kind);
            item.status == DesktopImportPlanItemStatus::Incompatible
                && item.error.as_ref().map(|error| error.code)
                    == Some(DesktopImportErrorCode::IncompatibleSqlite)
        }));
    }

    #[test]
    fn database_failing_integrity_never_reaches_the_target() {
        let root = TestDirectory::new("integrity-failure");
        let host = Arc::new(TestHost::default());
        let (service, source) = service(&root, host);
        write_database(source.home(), DesktopDatabaseKind::Product, 1);
        let source_database = source.data_paths().product_db();
        let connection = Connection::open(&source_database).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE integrity_probe(value INTEGER NOT NULL);
                 CREATE INDEX integrity_probe_index ON integrity_probe(value);
                 INSERT INTO integrity_probe(value) VALUES (1), (2), (3);
                 PRAGMA writable_schema = ON;
                 UPDATE sqlite_schema
                    SET rootpage = (SELECT rootpage FROM sqlite_schema WHERE name = 'integrity_probe')
                  WHERE name = 'integrity_probe_index';
                 PRAGMA writable_schema = OFF;
                 PRAGMA schema_version = 1000;",
            )
            .unwrap();
        drop(connection);

        let plan = service.plan(&source).unwrap();
        let item = database_item(&plan, DesktopDatabaseKind::Product);
        assert_eq!(item.status, DesktopImportPlanItemStatus::Ready, "{item:#?}");
        let report = service.execute(&plan, denied_options());
        let item = report
            .items
            .iter()
            .find(|item| item.kind == DesktopImportItemKind::Database(DesktopDatabaseKind::Product))
            .unwrap();
        assert_eq!(item.status, DesktopImportReportItemStatus::Failed);
        assert_eq!(
            item.error.as_ref().map(|error| error.code),
            Some(DesktopImportErrorCode::IntegrityCheckFailed),
            "{item:#?}"
        );
        assert!(!service.target().data_paths().product_db().exists());
    }

    #[test]
    fn active_source_writer_blocks_planning_without_copying() {
        let root = TestDirectory::new("busy");
        let host = Arc::new(TestHost::default());
        let (service, source) = service(&root, host);
        write_database(source.home(), DesktopDatabaseKind::Product, 3);
        let source_paths = source.data_paths();
        let guard = acquire_target_import_lock(&source_paths, "running-source".into()).unwrap();

        let plan = service.plan(&source).unwrap();
        assert_eq!(plan.status, DesktopImportPlanStatus::Blocked);
        assert!(
            plan.items
                .iter()
                .filter(|item| {
                    matches!(item.kind, DesktopImportItemKind::Database(_))
                        && item.status == DesktopImportPlanItemStatus::SourceBusy
                })
                .count()
                == 4
        );
        drop(guard);
        assert!(!service.target().data_paths().product_db().exists());
    }

    #[test]
    fn source_metadata_is_rechecked_before_copy() {
        let root = TestDirectory::new("metadata");
        let host = Arc::new(TestHost::default());
        let (service, source) = service(&root, host);
        write_database(source.home(), DesktopDatabaseKind::Product, 5);
        let plan = service.plan(&source).unwrap();
        let source_database = source.data_paths().product_db();
        let mut changed = fs::read(&source_database).unwrap();
        changed.push(99);
        fs::write(&source_database, changed).unwrap();

        let report = service.execute(&plan, denied_options());
        let item = report
            .items
            .iter()
            .find(|item| item.kind == DesktopImportItemKind::Database(DesktopDatabaseKind::Product))
            .unwrap();
        assert_eq!(item.status, DesktopImportReportItemStatus::Failed);
        assert_eq!(
            item.error.as_ref().unwrap().code,
            DesktopImportErrorCode::SourceChanged
        );
        assert!(!service.target().data_paths().product_db().exists());
    }

    #[test]
    fn same_home_is_rejected_before_inspection() {
        let root = TestDirectory::new("same-home");
        let home = root.child("same");
        let target = config(home.clone(), "preview");
        let service = DesktopDataImportService::new(target, Arc::new(TestHost::default()));
        let source = config(home, "stable");

        assert!(matches!(
            service.plan(&source),
            Err(DesktopImportError::SameHome { .. })
        ));
    }

    #[test]
    fn denied_credentials_never_call_the_host() {
        let root = TestDirectory::new("credential-denied");
        let host = Arc::new(TestHost::default());
        let (service, source) = service(&root, host.clone());
        let plan = service.plan(&source).unwrap();
        let credential_item = plan
            .items
            .iter()
            .find(|item| item.kind == DesktopImportItemKind::Credentials)
            .unwrap();
        assert_eq!(
            credential_item.status,
            DesktopImportPlanItemStatus::RequiresCredentialConfirmation
        );

        let report = service.execute(&plan, denied_options());
        assert!(host.actions.lock().unwrap().is_empty());
        assert!(report.items.iter().any(|item| {
            item.kind == DesktopImportItemKind::Credentials
                && item.status == DesktopImportReportItemStatus::SkippedCredentialDenied
        }));
    }

    #[test]
    fn confirmed_credentials_use_only_the_explicit_host_action() {
        let root = TestDirectory::new("credential-confirmed");
        let host = Arc::new(TestHost {
            actions: Mutex::new(Vec::new()),
            credential_result: HostCredentialImportResult {
                imported: 2,
                skipped: 1,
                failed: 0,
            },
        });
        let (service, source) = service(&root, host.clone());
        let plan = service.plan(&source).unwrap();

        let report = service.execute(
            &plan,
            DesktopImportExecutionOptions {
                credential_decision: CredentialImportDecision::Confirmed,
            },
        );
        assert_eq!(report.status, DesktopImportReportStatus::Completed);
        assert_eq!(
            host.actions.lock().unwrap().as_slice(),
            &[DesktopHostAction::Credential(
                DesktopCredentialAction::ImportConfirmed {
                    source_instance_identity: "liliacode".into(),
                    keys: Vec::new(),
                }
            )]
        );
        assert!(!service.target().data_paths().db_dir().exists());
    }

    #[test]
    fn credential_manifest_is_registry_derived_and_tamper_checked_before_copy() {
        let root = TestDirectory::new("credential-manifest");
        let host = Arc::new(TestHost::default());
        let (service, source) = service(&root, host.clone());
        write_credential_registry(&source, &["secret-z", "secret-a", "secret-a"]);

        let plan = service.plan(&source).unwrap();
        assert_eq!(
            plan.credential_keys,
            vec!["agentkit.secret-a", "agentkit.secret-z"]
        );
        let mut tampered = plan.clone();
        tampered.credential_keys[0] = "agentkit.another-secret".into();
        assert_invalid_plan(&service, &tampered);
        assert!(host.actions.lock().unwrap().is_empty());

        let report = service.execute(
            &plan,
            DesktopImportExecutionOptions {
                credential_decision: CredentialImportDecision::Confirmed,
            },
        );
        assert_eq!(report.status, DesktopImportReportStatus::Completed);
        assert_eq!(
            host.actions.lock().unwrap().as_slice(),
            &[DesktopHostAction::Credential(
                DesktopCredentialAction::ImportConfirmed {
                    source_instance_identity: "liliacode".into(),
                    keys: vec!["agentkit.secret-a".into(), "agentkit.secret-z".into()],
                }
            )]
        );
    }
}
