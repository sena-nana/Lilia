use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use fs2::FileExt;
use crate::application::{
    DesktopDatabaseKind, DesktopImportItemKind, DesktopImportReport, DesktopImportReportItemStatus,
    DesktopImportReportStatus,
};
use lilia_storage::LiliaDataPaths;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const INTERNAL_HELPER_ARGUMENT: &str = "--complete-pending-data-import";
const MANIFEST_VERSION: u32 = 1;
const WAIT_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PendingImportManifest {
    version: u32,
    plan_id: String,
    staging_home: PathBuf,
    databases: Vec<PendingDatabase>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PendingDatabase {
    kind: DesktopDatabaseKind,
    length: u64,
    sha256: String,
}

pub fn is_helper_request(arguments: &[std::ffi::OsString]) -> bool {
    arguments.len() == 1 && arguments[0] == INTERNAL_HELPER_ARGUMENT
}

pub fn has_pending(base_home: &Path) -> bool {
    manifest_path(base_home).is_file()
}

pub fn activate_on_startup(base_home: &Path) -> Result<bool, String> {
    if !has_pending(base_home) {
        return Ok(false);
    }
    let writer_lock = LiliaDataPaths::from_home(base_home)
        .db_dir()
        .join("writer.lock");
    let guard = wait_for_writer(&writer_lock)?;
    let result = activate(base_home);
    let _ = FileExt::unlock(&guard);
    drop(guard);
    result.map(|_| true)
}

pub fn create_staging_home(base_home: &Path) -> Result<PathBuf, String> {
    let home = base_home
        .join("import-staging")
        .join(uuid::Uuid::new_v4().to_string());
    fs::create_dir_all(home.join("db"))
        .map_err(|error| format!("cannot create import staging directory: {error}"))?;
    Ok(home)
}

pub fn target_is_empty(base_home: &Path) -> Result<bool, String> {
    let paths = LiliaDataPaths::from_home(base_home);
    for database in database_paths(&paths) {
        if database.exists() && database_contains_user_data(&database)? {
            return Ok(false);
        }
    }
    Ok(true)
}

pub fn schedule(base_home: &Path, report: &DesktopImportReport) -> Result<bool, String> {
    if report.status != DesktopImportReportStatus::Completed {
        return Ok(false);
    }
    let staging_home = validate_staging_home(base_home, &report.target_home)?;
    let staging_paths = LiliaDataPaths::from_home(&staging_home);
    let copied_legacy = report.items.iter().any(|item| {
        item.kind == DesktopImportItemKind::Database(DesktopDatabaseKind::LegacyDesktop)
            && item.status == DesktopImportReportItemStatus::Copied
    });
    if copied_legacy {
        merge_legacy_into_product(&staging_paths)?;
    }
    let mut databases = Vec::new();
    for item in &report.items {
        let DesktopImportItemKind::Database(kind) = item.kind else {
            continue;
        };
        if item.status != DesktopImportReportItemStatus::Copied {
            continue;
        }
        if kind == DesktopDatabaseKind::LegacyDesktop {
            continue;
        }
        let path = database_path(&staging_paths, kind);
        let metadata = fs::metadata(&path)
            .map_err(|error| format!("cannot read staged database metadata: {error}"))?;
        databases.push(PendingDatabase {
            kind,
            length: metadata.len(),
            sha256: sha256_file(&path)?,
        });
    }
    if copied_legacy
        && !databases
            .iter()
            .any(|database| database.kind == DesktopDatabaseKind::Product)
    {
        let path = staging_paths.product_db();
        let metadata = fs::metadata(&path)
            .map_err(|error| format!("cannot read merged product database metadata: {error}"))?;
        databases.push(PendingDatabase {
            kind: DesktopDatabaseKind::Product,
            length: metadata.len(),
            sha256: sha256_file(&path)?,
        });
    }
    if databases.is_empty() {
        return Ok(false);
    }
    if !valid_plan_id(&report.plan_id) {
        return Err("the import plan identifier is invalid".to_owned());
    }
    let manifest = PendingImportManifest {
        version: MANIFEST_VERSION,
        plan_id: report.plan_id.clone(),
        staging_home,
        databases,
    };
    write_manifest(base_home, &manifest)?;
    Ok(true)
}

fn merge_legacy_into_product(paths: &LiliaDataPaths) -> Result<(), String> {
    let legacy = paths.legacy_desktop_db();
    if !legacy.is_file() {
        return Ok(());
    }
    let product = paths.product_db();
    let connection = Connection::open(&product)
        .map_err(|error| format!("cannot open staged product database: {error}"))?;
    connection
        .execute(
            "ATTACH DATABASE ?1 AS imported_legacy",
            [legacy.to_string_lossy().as_ref()],
        )
        .map_err(|error| format!("cannot attach imported legacy database: {error}"))?;
    let tables = {
        let mut statement = connection
            .prepare(
                "SELECT name, sql FROM imported_legacy.sqlite_master \
                 WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND sql IS NOT NULL \
                 ORDER BY name",
            )
            .map_err(|error| format!("cannot inspect imported legacy database: {error}"))?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|error| format!("cannot list imported legacy tables: {error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("cannot read imported legacy table: {error}"))?;
        rows
    };
    connection
        .execute_batch("BEGIN IMMEDIATE")
        .map_err(|error| format!("cannot begin imported legacy merge: {error}"))?;
    let merge = (|| {
        for (name, create_sql) in tables {
            let quoted = format!("\"{}\"", name.replace('"', "\"\""));
            let exists: bool = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM main.sqlite_master WHERE type='table' AND name=?1)",
                    [&name],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())?;
            if !exists {
                connection
                    .execute_batch(&create_sql)
                    .map_err(|error| error.to_string())?;
            }
            connection
                .execute_batch(&format!(
                    "INSERT OR IGNORE INTO main.{quoted} SELECT * FROM imported_legacy.{quoted};"
                ))
                .map_err(|error| format!("merge imported table {name}: {error}"))?;
        }
        connection
            .execute_batch("COMMIT")
            .map_err(|error| error.to_string())
    })();
    if let Err(error) = merge {
        let _ = connection.execute_batch("ROLLBACK");
        return Err(format!("cannot merge imported legacy database: {error}"));
    }
    connection
        .execute_batch("DETACH DATABASE imported_legacy")
        .map_err(|error| format!("cannot detach imported legacy database: {error}"))?;
    drop(connection);
    for component in sqlite_components(&legacy) {
        if component.exists() {
            fs::remove_file(&component)
                .map_err(|error| format!("cannot remove staged legacy database: {error}"))?;
        }
    }
    Ok(())
}

pub fn discard_staging(base_home: &Path, staging_home: &Path) -> Result<(), String> {
    if !staging_home.exists() {
        let staging_root = base_home.join("import-staging");
        if staging_home
            .parent()
            .is_some_and(|parent| paths_equal(parent, &staging_root))
            && staging_home.file_name().is_some()
        {
            return Ok(());
        }
        return Err("staged import is outside the LiliaCode import directory".to_owned());
    }
    let staging_home = validate_staging_home(base_home, staging_home)?;
    if staging_home.exists() {
        fs::remove_dir_all(&staging_home)
            .map_err(|error| format!("cannot remove import staging directory: {error}"))?;
    }
    Ok(())
}

pub fn launch_helper() -> Result<(), String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("cannot locate LiliaCode executable: {error}"))?;
    let mut command = Command::new(executable);
    command.arg(INTERNAL_HELPER_ARGUMENT);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("cannot start import activation helper: {error}"))
}

pub fn run_helper(base_home: &Path) -> Result<(), String> {
    let writer_lock = LiliaDataPaths::from_home(base_home)
        .db_dir()
        .join("writer.lock");
    let guard = wait_for_writer(&writer_lock)?;
    let activation = activate(base_home);
    let _ = FileExt::unlock(&guard);
    drop(guard);

    let executable = std::env::current_exe()
        .map_err(|error| format!("cannot locate LiliaCode executable: {error}"))?;
    let mut command = Command::new(executable);
    if activation.is_err() {
        command.env("LILIA_IMPORT_ACTIVATION_FAILED", "1");
    }
    command
        .spawn()
        .map_err(|error| format!("cannot restart LiliaCode: {error}"))?;
    activation
}

fn activate(base_home: &Path) -> Result<(), String> {
    let manifest = read_manifest(base_home)?;
    if manifest.version != MANIFEST_VERSION {
        return Err("unsupported pending import manifest version".to_owned());
    }
    if !valid_plan_id(&manifest.plan_id) {
        return Err("the pending import plan identifier is invalid".to_owned());
    }
    let staging_home = validate_staging_home(base_home, &manifest.staging_home)?;
    if !target_is_empty(base_home)? {
        return Err("LiliaCode already contains data; pending import was not activated".to_owned());
    }
    let staging_paths = LiliaDataPaths::from_home(&staging_home);
    for database in &manifest.databases {
        let path = database_path(&staging_paths, database.kind);
        let metadata = fs::metadata(&path)
            .map_err(|error| format!("cannot read staged database metadata: {error}"))?;
        if metadata.len() != database.length || sha256_file(&path)? != database.sha256 {
            return Err("a staged database changed before activation".to_owned());
        }
        validate_sqlite(&path)?;
    }

    let backup_home = base_home.join("import-backups").join(&manifest.plan_id);
    if backup_home.exists() {
        return Err("the import backup directory already exists".to_owned());
    }
    let backup_db = backup_home.join("db");
    fs::create_dir_all(&backup_db)
        .map_err(|error| format!("cannot create import backup directory: {error}"))?;
    let target_paths = LiliaDataPaths::from_home(base_home);
    let mut activated = Vec::new();
    let result = (|| {
        for database in &manifest.databases {
            let target = database_path(&target_paths, database.kind);
            let staged = database_path(&staging_paths, database.kind);
            for component in sqlite_components(&target) {
                if component.exists() {
                    let file_name = component
                        .file_name()
                        .ok_or_else(|| "target database component has no file name".to_owned())?;
                    fs::rename(&component, backup_db.join(file_name)).map_err(|error| {
                        format!("cannot move current database into the import backup: {error}")
                    })?;
                }
            }
            fs::rename(&staged, &target)
                .map_err(|error| format!("cannot activate staged database: {error}"))?;
            activated.push((staged, target));
        }
        Ok(())
    })();
    if let Err(error) = result {
        rollback_activation(&activated, &backup_db, &target_paths.db_dir())?;
        return Err(error);
    }

    fs::remove_file(manifest_path(base_home))
        .map_err(|error| format!("cannot clear pending import manifest: {error}"))?;
    if staging_home.exists() {
        fs::remove_dir_all(&staging_home)
            .map_err(|error| format!("cannot clean activated import staging directory: {error}"))?;
    }
    Ok(())
}

fn rollback_activation(
    activated: &[(PathBuf, PathBuf)],
    backup_db: &Path,
    target_db: &Path,
) -> Result<(), String> {
    let mut failures = Vec::new();
    for (staged, target) in activated.iter().rev() {
        if target.exists() {
            if let Some(parent) = staged.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Err(error) = fs::rename(target, staged) {
                failures.push(format!("restore staged database: {error}"));
            }
        }
    }
    if backup_db.exists() {
        let entries = fs::read_dir(backup_db)
            .map_err(|error| format!("cannot inspect import backup during rollback: {error}"))?;
        for entry in entries {
            let entry = entry.map_err(|error| format!("cannot read import backup: {error}"))?;
            if let Err(error) = fs::rename(entry.path(), target_db.join(entry.file_name())) {
                failures.push(format!("restore current database: {error}"));
            }
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "import activation rollback failed: {}",
            failures.join("; ")
        ))
    }
}

fn wait_for_writer(path: &Path) -> Result<File, String> {
    let started = Instant::now();
    loop {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(false)
            .open(path)
            .map_err(|error| format!("cannot open LiliaCode writer lock: {error}"))?;
        match file.try_lock_exclusive() {
            Ok(()) => return Ok(file),
            Err(_) if started.elapsed() < WAIT_TIMEOUT => {
                drop(file);
                thread::sleep(Duration::from_millis(100));
            }
            Err(error) => {
                return Err(format!(
                    "LiliaCode did not release its writer lock in time: {error}"
                ));
            }
        }
    }
}

fn database_contains_user_data(path: &Path) -> Result<bool, String> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| format!("cannot inspect LiliaCode data: {error}"))?;
    let mut statement = connection
        .prepare(
            "SELECT name FROM sqlite_master \
             WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )
        .map_err(|error| format!("cannot inspect LiliaCode schema: {error}"))?;
    let tables = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| format!("cannot inspect LiliaCode schema: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("cannot inspect LiliaCode schema: {error}"))?;
    drop(statement);
    for table in tables {
        if matches!(
            table.as_str(),
            "schema_migrations" | "migration_runs" | "remote_control_settings"
        ) {
            continue;
        }
        let quoted = table.replace('"', "\"\"");
        let count: i64 = connection
            .query_row(&format!("SELECT COUNT(*) FROM \"{quoted}\""), [], |row| {
                row.get(0)
            })
            .map_err(|error| format!("cannot inspect LiliaCode data: {error}"))?;
        if count > 0 {
            return Ok(true);
        }
    }
    Ok(false)
}

fn validate_sqlite(path: &Path) -> Result<(), String> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| format!("cannot open staged database: {error}"))?;
    let integrity: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(|error| format!("cannot validate staged database: {error}"))?;
    if integrity == "ok" {
        Ok(())
    } else {
        Err("a staged database failed its integrity check".to_owned())
    }
}

fn write_manifest(base_home: &Path, manifest: &PendingImportManifest) -> Result<(), String> {
    let path = manifest_path(base_home);
    if path.exists() {
        return Err("another data import is already waiting for restart".to_owned());
    }
    let directory = path
        .parent()
        .ok_or_else(|| "pending import manifest has no parent directory".to_owned())?;
    fs::create_dir_all(directory)
        .map_err(|error| format!("cannot create pending import directory: {error}"))?;
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    let encoded = serde_json::to_vec_pretty(manifest)
        .map_err(|error| format!("cannot encode pending import manifest: {error}"))?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| format!("cannot create pending import manifest: {error}"))?;
    file.write_all(&encoded)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("cannot write pending import manifest: {error}"))?;
    fs::rename(&temporary, &path)
        .map_err(|error| format!("cannot publish pending import manifest: {error}"))
}

fn read_manifest(base_home: &Path) -> Result<PendingImportManifest, String> {
    let encoded = fs::read(manifest_path(base_home))
        .map_err(|error| format!("cannot read pending import manifest: {error}"))?;
    serde_json::from_slice(&encoded)
        .map_err(|error| format!("cannot decode pending import manifest: {error}"))
}

fn manifest_path(base_home: &Path) -> PathBuf {
    base_home.join("import").join("pending.json")
}

fn validate_staging_home(base_home: &Path, staging_home: &Path) -> Result<PathBuf, String> {
    let staging_root = base_home.join("import-staging");
    let canonical_root = fs::canonicalize(&staging_root)
        .map_err(|error| format!("cannot resolve import staging directory: {error}"))?;
    let canonical_home = fs::canonicalize(staging_home)
        .map_err(|error| format!("cannot resolve staged import: {error}"))?;
    if canonical_home == canonical_root || !canonical_home.starts_with(&canonical_root) {
        return Err("staged import is outside the LiliaCode import directory".to_owned());
    }
    Ok(canonical_home)
}

fn valid_plan_id(plan_id: &str) -> bool {
    !plan_id.is_empty()
        && plan_id.len() <= 128
        && plan_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

#[cfg(windows)]
fn paths_equal(left: &Path, right: &Path) -> bool {
    fn normalize(path: &Path) -> String {
        let value = path.to_string_lossy().replace('/', "\\");
        value.strip_prefix(r"\\?\").unwrap_or(&value).to_lowercase()
    }
    normalize(left) == normalize(right)
}

#[cfg(not(windows))]
fn paths_equal(left: &Path, right: &Path) -> bool {
    left == right
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path)
        .map_err(|error| format!("cannot open staged database for hashing: {error}"))?;
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("cannot hash staged database: {error}"))?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn database_paths(paths: &LiliaDataPaths) -> [PathBuf; 4] {
    [
        paths.product_projections_db(),
        paths.product_db(),
        paths.agent_runtime_db(),
        paths.legacy_desktop_db(),
    ]
}

fn database_path(paths: &LiliaDataPaths, kind: DesktopDatabaseKind) -> PathBuf {
    match kind {
        DesktopDatabaseKind::ProductProjections => paths.product_projections_db(),
        DesktopDatabaseKind::Product => paths.product_db(),
        DesktopDatabaseKind::AgentRuntime => paths.agent_runtime_db(),
        DesktopDatabaseKind::LegacyDesktop => paths.legacy_desktop_db(),
    }
}

fn sqlite_components(database: &Path) -> [PathBuf; 3] {
    [
        database.to_path_buf(),
        sidecar_path(database, "-wal"),
        sidecar_path(database, "-shm"),
    ]
}

fn sidecar_path(database: &Path, suffix: &str) -> PathBuf {
    let mut value = database.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use crate::application::{
        CredentialImportDecision, DesktopDataImportService, DesktopImportExecutionOptions,
        DesktopImportReportItem, DesktopProjectCreate,
    };
    use crate::application::{DesktopApplication, DesktopApplicationConfig};

    use crate::host::NativeDesktopHost;

    fn temporary_home(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "lilia-native-pending-import-{label}-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ))
    }

    #[test]
    fn schema_only_databases_are_empty_but_user_rows_are_not() {
        let home = temporary_home("empty");
        let paths = LiliaDataPaths::from_home(&home);
        fs::create_dir_all(paths.db_dir()).unwrap();
        let connection = Connection::open(paths.product_db()).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE schema_migrations (version INTEGER);\
                 INSERT INTO schema_migrations VALUES (1);\
                 CREATE TABLE projects (id TEXT);",
            )
            .unwrap();
        assert!(target_is_empty(&home).unwrap());
        connection
            .execute("INSERT INTO projects VALUES ('project-1')", [])
            .unwrap();
        assert!(!target_is_empty(&home).unwrap());
        drop(connection);
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn freshly_bootstrapped_native_application_remains_importable() {
        let home = temporary_home("bootstrap");
        let config = DesktopApplicationConfig::new(&home, "native-import-test").unwrap();
        let application =
            DesktopApplication::bootstrap(config, Arc::new(NativeDesktopHost)).unwrap();
        assert!(target_is_empty(&home).unwrap());
        drop(application);
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn created_staging_home_accepts_a_real_nonempty_database_import() {
        let root = temporary_home("service");
        let source_home = root.join("source");
        let target_home = root.join("target");
        let source_config = DesktopApplicationConfig::new(&source_home, "liliacode").unwrap();
        let source =
            DesktopApplication::bootstrap(source_config.clone(), Arc::new(NativeDesktopHost))
                .unwrap();
        source
            .create_project(DesktopProjectCreate::new("Imported project"))
            .unwrap();
        drop(source);
        let staging_home = create_staging_home(&target_home).unwrap();
        let target_config =
            DesktopApplicationConfig::new(&staging_home, "native-import-test").unwrap();
        let service = DesktopDataImportService::new(target_config, Arc::new(NativeDesktopHost));
        let plan = service.plan(&source_config).unwrap();
        let report = service.execute(
            &plan,
            DesktopImportExecutionOptions {
                credential_decision: CredentialImportDecision::Denied,
            },
        );
        assert!(
            report.items.iter().any(|item| {
                item.kind == DesktopImportItemKind::Database(DesktopDatabaseKind::Product)
                    && item.status == DesktopImportReportItemStatus::Copied
            }),
            "plan: {plan:#?}\nreport: {report:#?}"
        );
        assert!(LiliaDataPaths::from_home(&staging_home)
            .product_db()
            .is_file());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn staging_validation_rejects_sibling_directories() {
        let home = temporary_home("boundary");
        let staging = home.join("import-staging").join("valid");
        let sibling = home.join("import-staging-other").join("invalid");
        fs::create_dir_all(&staging).unwrap();
        fs::create_dir_all(&sibling).unwrap();
        assert!(validate_staging_home(&home, &staging).is_ok());
        assert!(validate_staging_home(&home, &sibling).is_err());
        fs::remove_dir_all(home).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn absent_extended_length_staging_path_can_be_safely_discarded() {
        let home = temporary_home("extended-empty");
        fs::create_dir_all(&home).unwrap();
        let staging = home.join("import-staging").join("not-created");
        let extended = PathBuf::from(format!(r"\\?\{}", staging.display()));
        discard_staging(&home, &extended).unwrap();
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn activation_refuses_to_replace_user_data() {
        let home = temporary_home("conflict");
        let staging = home.join("import-staging").join("stage");
        let target_paths = LiliaDataPaths::from_home(&home);
        let staging_paths = LiliaDataPaths::from_home(&staging);
        fs::create_dir_all(target_paths.db_dir()).unwrap();
        fs::create_dir_all(staging_paths.db_dir()).unwrap();
        let target = Connection::open(target_paths.product_db()).unwrap();
        target
            .execute_batch("CREATE TABLE projects (id TEXT); INSERT INTO projects VALUES ('p');")
            .unwrap();
        drop(target);
        let source = Connection::open(staging_paths.product_db()).unwrap();
        source
            .execute_batch("CREATE TABLE projects (id TEXT); INSERT INTO projects VALUES ('old');")
            .unwrap();
        drop(source);
        let staged_path = staging_paths.product_db();
        let metadata = fs::metadata(&staged_path).unwrap();
        let manifest = PendingImportManifest {
            version: MANIFEST_VERSION,
            plan_id: "plan-conflict".to_owned(),
            staging_home: staging,
            databases: vec![PendingDatabase {
                kind: DesktopDatabaseKind::Product,
                length: metadata.len(),
                sha256: sha256_file(&staged_path).unwrap(),
            }],
        };
        write_manifest(&home, &manifest).unwrap();
        assert!(activate(&home).is_err());
        let target = Connection::open(target_paths.product_db()).unwrap();
        let id: String = target
            .query_row("SELECT id FROM projects", [], |row| row.get(0))
            .unwrap();
        assert_eq!(id, "p");
        drop(target);
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn completed_import_activates_after_backing_up_the_empty_database() {
        let home = temporary_home("activate");
        let staging = home.join("import-staging").join("stage");
        let target_paths = LiliaDataPaths::from_home(&home);
        let staging_paths = LiliaDataPaths::from_home(&staging);
        fs::create_dir_all(target_paths.db_dir()).unwrap();
        fs::create_dir_all(staging_paths.db_dir()).unwrap();
        let target = Connection::open(target_paths.product_db()).unwrap();
        target
            .execute_batch(
                "CREATE TABLE schema_migrations (version INTEGER);\
                 INSERT INTO schema_migrations VALUES (1);\
                 CREATE TABLE projects (id TEXT);",
            )
            .unwrap();
        drop(target);
        let source = Connection::open(staging_paths.product_db()).unwrap();
        source
            .execute_batch(
                "CREATE TABLE projects (id TEXT);\
                 INSERT INTO projects VALUES ('imported');",
            )
            .unwrap();
        drop(source);
        let report = DesktopImportReport {
            plan_id: "plan-activate".to_owned(),
            source_home: home.join("legacy"),
            target_home: staging.clone(),
            status: DesktopImportReportStatus::Completed,
            items: vec![DesktopImportReportItem {
                kind: DesktopImportItemKind::Database(DesktopDatabaseKind::Product),
                status: DesktopImportReportItemStatus::Copied,
                files: vec![staging_paths.product_db()],
                error: None,
            }],
        };

        assert!(schedule(&home, &report).unwrap());
        fs::write(target_paths.db_dir().join("writer.lock"), b"startup-lock").unwrap();
        assert!(activate_on_startup(&home).unwrap());
        assert!(!has_pending(&home));

        let activated = Connection::open(target_paths.product_db()).unwrap();
        let id: String = activated
            .query_row("SELECT id FROM projects", [], |row| row.get(0))
            .unwrap();
        assert_eq!(id, "imported");
        drop(activated);
        assert!(home
            .join("import-backups")
            .join("plan-activate")
            .join("db")
            .join("product.db")
            .is_file());
        assert!(!manifest_path(&home).exists());
        assert!(!staging.exists());
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn scheduling_import_merges_legacy_tables_into_product_without_creating_lilia_db() {
        let home = temporary_home("merge-legacy");
        let staging = create_staging_home(&home).unwrap();
        let paths = LiliaDataPaths::from_home(&staging);
        let product = Connection::open(paths.product_db()).unwrap();
        product
            .execute_batch(
                "CREATE TABLE projects (id TEXT PRIMARY KEY); INSERT INTO projects VALUES ('p');",
            )
            .unwrap();
        drop(product);
        let legacy = Connection::open(paths.legacy_desktop_db()).unwrap();
        legacy
            .execute_batch("CREATE TABLE composer_drafts (task_id TEXT PRIMARY KEY, body TEXT); INSERT INTO composer_drafts VALUES ('t', 'draft');")
            .unwrap();
        drop(legacy);
        let report = DesktopImportReport {
            plan_id: "plan-merge-legacy".to_owned(),
            source_home: home.join("preview"),
            target_home: staging,
            status: DesktopImportReportStatus::Completed,
            items: vec![
                DesktopImportReportItem {
                    kind: DesktopImportItemKind::Database(DesktopDatabaseKind::Product),
                    status: DesktopImportReportItemStatus::Copied,
                    files: vec![paths.product_db()],
                    error: None,
                },
                DesktopImportReportItem {
                    kind: DesktopImportItemKind::Database(DesktopDatabaseKind::LegacyDesktop),
                    status: DesktopImportReportItemStatus::Copied,
                    files: vec![paths.legacy_desktop_db()],
                    error: None,
                },
            ],
        };

        assert!(schedule(&home, &report).unwrap());
        assert!(!paths.legacy_desktop_db().exists());
        let manifest = read_manifest(&home).unwrap();
        assert_eq!(manifest.databases.len(), 1);
        assert_eq!(manifest.databases[0].kind, DesktopDatabaseKind::Product);
        let merged = Connection::open(paths.product_db()).unwrap();
        let draft: String = merged
            .query_row(
                "SELECT body FROM composer_drafts WHERE task_id='t'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(draft, "draft");
        drop(merged);
        fs::remove_dir_all(home).unwrap();
    }
}
