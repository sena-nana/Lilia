use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};

use rusqlite::Connection;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

use crate::{
    app_events_contract,
    projects_tasks::{display_project_path, ensure_project_row_for_cwd},
    store::LiliaStore,
    task_handoff, MAIN_WINDOW_LABEL,
};

const STORE_READY_TIMEOUT: Duration = Duration::from_secs(5);
const STORE_READY_POLL: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CliProjectOpenPayload {
    project_id: String,
    cwd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    handoff_id: Option<String>,
}

#[derive(Debug)]
enum CliOpenRequest {
    Project(String),
    TaskHandoff(PathBuf),
}

#[derive(Default)]
pub(crate) struct CliProjectOpenState {
    pending: Mutex<Option<CliProjectOpenPayload>>,
}

#[tauri::command]
pub(crate) fn cli_project_open_consume_pending(
    state: State<'_, CliProjectOpenState>,
) -> Option<CliProjectOpenPayload> {
    state.pending.lock().unwrap().take()
}

pub(crate) fn handle_initial_args<R: Runtime>(app: &AppHandle<R>) {
    let argv = env::args().collect::<Vec<_>>();
    if argv.len() <= 1 {
        return;
    }
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    handle_cli_project_args(app, argv, cwd, false);
}

pub(crate) fn handle_second_instance<R: Runtime>(
    app: &AppHandle<R>,
    argv: Vec<String>,
    cwd: String,
) {
    handle_cli_project_args(app, argv, PathBuf::from(cwd), true);
    focus_main_window(app);
}

fn handle_cli_project_args<R: Runtime>(
    app: &AppHandle<R>,
    argv: Vec<String>,
    cwd: PathBuf,
    emit_immediately: bool,
) {
    let request = match parse_cli_open_request(&argv) {
        Ok(Some(value)) => value,
        Ok(None) => return,
        Err(err) => {
            eprintln!("[liliacode] {err}");
            return;
        }
    };

    let result = match request {
        CliOpenRequest::Project(project_path_arg) => {
            resolve_cli_project_open(app, &project_path_arg, &cwd)
        }
        CliOpenRequest::TaskHandoff(path) => task_handoff::resolve_task_handoff(app, &path, &cwd)
            .map(|payload| CliProjectOpenPayload {
                project_id: payload.project_id,
                cwd: payload.cwd,
                task_id: Some(payload.task_id),
                handoff_id: Some(payload.handoff_id),
            }),
    };
    match result {
        Ok(payload) => publish_cli_project_open(app, payload, emit_immediately),
        Err(err) => eprintln!("[liliacode] {err}"),
    }
}

fn parse_cli_open_request(argv: &[String]) -> Result<Option<CliOpenRequest>, String> {
    let args = argv
        .iter()
        .skip(1)
        .filter_map(|arg| {
            let trimmed = arg.trim();
            let unquoted = trim_surrounding_quotes(trimmed).trim();
            (!unquoted.is_empty()).then(|| unquoted.to_string())
        })
        .collect::<Vec<_>>();
    match args.as_slice() {
        [] => Ok(None),
        [path] => Ok(Some(CliOpenRequest::Project(path.clone()))),
        [flag, path] if flag == "--task-handoff" => {
            Ok(Some(CliOpenRequest::TaskHandoff(PathBuf::from(path))))
        }
        _ => Err("用法：liliacode <path> | liliacode --task-handoff <handoff.json>".to_string()),
    }
}

fn publish_cli_project_open<R: Runtime>(
    app: &AppHandle<R>,
    payload: CliProjectOpenPayload,
    emit_immediately: bool,
) {
    if emit_immediately {
        if let Err(err) = app.emit(
            app_events_contract::cli_project_open_event_name(),
            payload.clone(),
        ) {
            eprintln!("[liliacode] emit project open failed: {err}");
        }
        return;
    }

    if let Some(state) = app.try_state::<CliProjectOpenState>() {
        *state.pending.lock().unwrap() = Some(payload);
    }
}

fn resolve_cli_project_open<R: Runtime>(
    app: &AppHandle<R>,
    project_path_arg: &str,
    cwd: &Path,
) -> Result<CliProjectOpenPayload, String> {
    let path = resolve_project_path(project_path_arg, cwd)?;
    let cwd = display_path(&path);
    let started = Instant::now();
    loop {
        if let Some(store) = app.try_state::<LiliaStore>() {
            let conn = store.conn()?;
            return ensure_project_for_cwd(&conn, &cwd);
        }
        if started.elapsed() >= STORE_READY_TIMEOUT {
            return Err("项目存储尚未初始化".to_string());
        }
        thread::sleep(STORE_READY_POLL);
    }
}

fn trim_surrounding_quotes(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(value)
}

pub(crate) fn resolve_project_path(project_path_arg: &str, cwd: &Path) -> Result<PathBuf, String> {
    let raw = PathBuf::from(project_path_arg);
    let path = if raw.is_absolute() {
        raw
    } else {
        cwd.join(raw)
    };
    if !path.exists() {
        return Err(format!("路径不存在：{}", path.display()));
    }
    if !path.is_dir() {
        return Err(format!("路径不是目录：{}", path.display()));
    }
    fs::canonicalize(&path).map_err(|err| format!("解析路径失败：{}：{err}", path.display()))
}

fn ensure_project_for_cwd(conn: &Connection, cwd: &str) -> Result<CliProjectOpenPayload, String> {
    let project = ensure_project_row_for_cwd(conn, cwd, "cli_project_open")?;
    Ok(CliProjectOpenPayload {
        project_id: project.id,
        cwd: cwd.to_string(),
        task_id: None,
        handoff_id: None,
    })
}

fn display_path(path: &Path) -> String {
    display_project_path(path)
}

fn focus_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn setup_projects_schema(conn: &Connection) {
        conn.execute_batch(
            r#"
            CREATE TABLE projects (
              id TEXT PRIMARY KEY,
              name TEXT NOT NULL,
              cwd TEXT,
              created_at INTEGER NOT NULL,
              sort_order INTEGER NOT NULL DEFAULT 0,
              pinned INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE tasks (
              id TEXT PRIMARY KEY,
              project_id TEXT,
              archived INTEGER NOT NULL DEFAULT 0
            );
            "#,
        )
        .unwrap();
    }

    #[test]
    fn parse_cli_open_request_preserves_project_path_behavior() {
        let request = parse_cli_open_request(&[
            "LiliaCode.exe".to_string(),
            r#""C:\work\Lilia""#.to_string(),
        ])
        .unwrap()
        .unwrap();

        assert!(matches!(
            request,
            CliOpenRequest::Project(path) if path == "C:\\work\\Lilia"
        ));
    }

    #[test]
    fn parse_cli_open_request_accepts_versioned_task_handoff_file() {
        let request = parse_cli_open_request(&[
            "LiliaCode.exe".to_string(),
            "--task-handoff".to_string(),
            "C:\\temp\\handoff.json".to_string(),
        ])
        .unwrap()
        .unwrap();
        assert!(matches!(
            request,
            CliOpenRequest::TaskHandoff(path) if path == PathBuf::from("C:\\temp\\handoff.json")
        ));
    }

    #[test]
    fn parse_cli_open_request_rejects_ambiguous_arguments() {
        assert!(parse_cli_open_request(&[
            "LiliaCode.exe".to_string(),
            "C:\\one".to_string(),
            "C:\\two".to_string(),
        ])
        .is_err());
    }

    #[test]
    fn normalize_windows_extended_paths_for_display() {
        assert_eq!(
            display_project_path(Path::new(r"\\?\C:\work\Lilia")),
            r"C:\work\Lilia",
        );
        assert_eq!(
            display_project_path(Path::new(r"\\?\UNC\server\share")),
            r"\\server\share",
        );
    }

    #[test]
    fn resolve_project_path_uses_terminal_cwd_for_relative_paths() {
        let root = env::temp_dir().join(format!("lilia-cli-{}", Uuid::new_v4()));
        let project = root.join("Project");
        fs::create_dir_all(&project).unwrap();

        let resolved = resolve_project_path("Project", &root).unwrap();
        assert_eq!(resolved, fs::canonicalize(&project).unwrap());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ensure_project_for_cwd_reuses_existing_project_by_normalized_cwd() {
        let conn = Connection::open_in_memory().unwrap();
        setup_projects_schema(&conn);
        conn.execute(
            "INSERT INTO projects (id, name, cwd, created_at, sort_order) VALUES ('p1', 'Lilia', 'C:\\work\\Lilia\\', 1, 0)",
            [],
        )
        .unwrap();

        let payload = ensure_project_for_cwd(&conn, "C:/work/Lilia").unwrap();
        assert_eq!(payload.project_id, "p1");
        assert_eq!(payload.cwd, "C:/work/Lilia");
    }

    #[test]
    fn ensure_project_for_cwd_creates_project_when_missing() {
        let conn = Connection::open_in_memory().unwrap();
        setup_projects_schema(&conn);

        let payload = ensure_project_for_cwd(&conn, "C:\\work\\NewProject").unwrap();
        let row: (String, String, String, i64) = conn
            .query_row(
                "SELECT id, name, cwd, sort_order FROM projects",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();

        assert_eq!(row.0, payload.project_id);
        assert_eq!(row.1, "NewProject");
        assert_eq!(row.2, "C:\\work\\NewProject");
        assert_eq!(row.3, 0);
    }
}
