use rusqlite::Connection;

#[cfg(test)]
use super::migrations::SchemaMigration;
use super::migrations::{
    ensure_schema_with_migrations, RESET_BASELINE_SCHEMA_VERSION, SCHEMA_MIGRATIONS,
};

pub(super) fn reset_orphaned_queued_guides(conn: &Connection) -> Result<(), String> {
    conn.execute(
        "UPDATE task_todos SET guide_status = 'pending' WHERE source = 'lilia' AND guide_status = 'queued'",
        [],
    )
    .map(|_| ())
    .map_err(|e| format!("lilia-store: 重置 queued 引导失败：{e}"))
}

pub(super) fn ensure_current_schema(conn: &mut Connection) -> Result<(), String> {
    ensure_schema_with_migrations(conn, current_schema_version(), SCHEMA_MIGRATIONS)
}

fn current_schema_version() -> i64 {
    SCHEMA_MIGRATIONS
        .last()
        .map(|migration| migration.version)
        .unwrap_or(RESET_BASELINE_SCHEMA_VERSION)
}

pub(super) fn reset_development_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = OFF;

        DROP TABLE IF EXISTS agent_usage_records;
        DROP TABLE IF EXISTS agent_timeline_events;
        DROP TABLE IF EXISTS task_runtime_states;
        DROP TABLE IF EXISTS task_runtime_finalizations;
        DROP TABLE IF EXISTS task_pending_turns;
        DROP TABLE IF EXISTS task_agent_sessions;
        DROP TABLE IF EXISTS task_worktrees;
        DROP TABLE IF EXISTS task_handoffs;
        DROP TABLE IF EXISTS project_architecture_changes;
        DROP TABLE IF EXISTS project_architecture_graphs;
        DROP TABLE IF EXISTS task_milestone_links;
        DROP TABLE IF EXISTS milestones;
        DROP TABLE IF EXISTS task_dependencies;
        DROP TABLE IF EXISTS tasks;
        DROP TABLE IF EXISTS projects;
        DROP TABLE IF EXISTS task_todos;
        DROP TABLE IF EXISTS automation_run_nodes;
        DROP TABLE IF EXISTS automation_runs;
        DROP TABLE IF EXISTS automation_workflow_versions;
        DROP TABLE IF EXISTS automation_workflows;
        DROP TABLE IF EXISTS memory_injection_states;
        DROP TABLE IF EXISTS memories;
        DROP TABLE IF EXISTS remote_control_pairing_tickets;
        DROP TABLE IF EXISTS remote_control_trusted_devices;
        DROP TABLE IF EXISTS remote_control_settings;

        PRAGMA foreign_keys = ON;
        "#,
    )
    .map_err(|e| format!("lilia-store: 清空旧开发库失败：{e}"))?;
    create_current_schema(conn)?;
    conn.execute_batch(&format!(
        "PRAGMA user_version = {};",
        current_schema_version()
    ))
    .map_err(|e| format!("lilia-store: 写 user_version 失败：{e}"))
}

fn create_current_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        r#"
        CREATE TABLE task_todos (
          id           TEXT PRIMARY KEY,
          task_id      TEXT NOT NULL,
          text         TEXT NOT NULL,
          done         INTEGER NOT NULL DEFAULT 0,
          "order"      INTEGER NOT NULL,
          source       TEXT NOT NULL CHECK (source IN ('lilia','agent')),
          priority     TEXT NOT NULL DEFAULT 'normal' CHECK (priority IN ('high','normal','low')),
          guide_status TEXT CHECK (guide_status IS NULL OR guide_status IN ('pending','queued','sent')),
          attachments_json TEXT NOT NULL DEFAULT '[]',
          conversation_references_json TEXT NOT NULL DEFAULT '[]',
          created_at   INTEGER NOT NULL,
          updated_at   INTEGER NOT NULL
        );
        CREATE INDEX idx_task_todos_task_id_order
          ON task_todos(task_id, "order");

        CREATE TABLE projects (
          id         TEXT PRIMARY KEY,
          name       TEXT NOT NULL,
          cwd        TEXT,
          created_at INTEGER NOT NULL,
          sort_order INTEGER NOT NULL DEFAULT 0,
          pinned     INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE tasks (
          id          TEXT PRIMARY KEY,
          project_id  TEXT,
          session_id  TEXT NOT NULL,
          title       TEXT NOT NULL,
          title_source TEXT NOT NULL DEFAULT 'auto'
                        CHECK (title_source IN ('auto','manual')),
          status      TEXT NOT NULL DEFAULT 'waiting'
                        CHECK (status IN
                          ('draft','waiting','running','blocked','done','cancelled')),
          created_at  INTEGER NOT NULL,
          parent_id   TEXT,
          archived    INTEGER NOT NULL DEFAULT 0,
          sort_order  INTEGER NOT NULL DEFAULT 0,
          pinned      INTEGER NOT NULL DEFAULT 0,
          FOREIGN KEY (project_id) REFERENCES projects(id)
        );

        CREATE INDEX idx_tasks_project_id
          ON tasks(project_id);
        CREATE INDEX idx_tasks_archived
          ON tasks(archived);
        CREATE INDEX idx_tasks_project_archived_order
          ON tasks(project_id, archived, pinned DESC, sort_order ASC);

        CREATE TABLE task_handoffs (
          handoff_id  TEXT PRIMARY KEY,
          task_id     TEXT NOT NULL UNIQUE,
          payload_json TEXT NOT NULL,
          source_route TEXT NOT NULL,
          created_at  INTEGER NOT NULL,
          FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE
        );
        CREATE TABLE task_dependencies (
          task_id       TEXT NOT NULL,
          depends_on_id TEXT NOT NULL,
          PRIMARY KEY (task_id, depends_on_id),
          FOREIGN KEY (task_id)       REFERENCES tasks(id) ON DELETE CASCADE,
          FOREIGN KEY (depends_on_id) REFERENCES tasks(id) ON DELETE CASCADE
        );

        CREATE TABLE milestones (
          id          TEXT PRIMARY KEY,
          project_id  TEXT NOT NULL,
          title       TEXT NOT NULL,
          description TEXT NOT NULL DEFAULT '',
          status      TEXT NOT NULL DEFAULT 'upcoming'
                      CHECK (status IN ('upcoming','in-progress','done','abandoned')),
          due_date    INTEGER,
          sort_order  INTEGER NOT NULL DEFAULT 0,
          created_at  INTEGER NOT NULL,
          FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
        );

        CREATE INDEX idx_milestones_project_order
          ON milestones(project_id, sort_order ASC, created_at ASC);

        CREATE TABLE task_milestone_links (
          task_id      TEXT NOT NULL,
          milestone_id TEXT NOT NULL,
          PRIMARY KEY (task_id, milestone_id),
          FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE,
          FOREIGN KEY (milestone_id) REFERENCES milestones(id) ON DELETE CASCADE
        );

        CREATE INDEX idx_task_milestone_links_milestone
          ON task_milestone_links(milestone_id);

        CREATE TABLE memories (
          id             TEXT PRIMARY KEY,
          scope          TEXT NOT NULL CHECK (scope IN ('user','project')),
          project_id     TEXT,
          title          TEXT NOT NULL,
          body           TEXT NOT NULL,
          tags_json      TEXT NOT NULL DEFAULT '[]',
          enabled        INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
          source_task_id TEXT,
          created_at     INTEGER NOT NULL,
          updated_at     INTEGER NOT NULL,
          CHECK (
            (scope = 'user' AND project_id IS NULL) OR
            (scope = 'project' AND project_id IS NOT NULL)
          ),
          FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
        );

        CREATE INDEX idx_memories_scope_project
          ON memories(scope, project_id, enabled, updated_at DESC);

        CREATE TABLE memory_injection_states (
          task_id                TEXT PRIMARY KEY,
          enabled                INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
          last_injected_turn_seq INTEGER,
          updated_at             INTEGER NOT NULL,
          FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE
        );

        CREATE TABLE remote_control_settings (
          key        TEXT PRIMARY KEY,
          value      TEXT NOT NULL,
          updated_at INTEGER NOT NULL
        );

        CREATE TABLE remote_control_trusted_devices (
          id                TEXT PRIMARY KEY,
          display_name      TEXT NOT NULL,
          endpoint_id       TEXT NOT NULL UNIQUE,
          protocol_version  INTEGER NOT NULL,
          trusted           INTEGER NOT NULL DEFAULT 1 CHECK (trusted IN (0, 1)),
          first_paired_at   INTEGER NOT NULL,
          last_seen_at      INTEGER,
          revoked_at        INTEGER
        );

        CREATE INDEX idx_remote_control_trusted_devices_endpoint
          ON remote_control_trusted_devices(endpoint_id);

        CREATE TABLE remote_control_pairing_tickets (
          id             TEXT PRIMARY KEY,
          challenge      TEXT NOT NULL,
          pc_name        TEXT NOT NULL,
          endpoint_id    TEXT NOT NULL,
          pairing_uri    TEXT NOT NULL,
          expires_at     INTEGER NOT NULL,
          consumed_at    INTEGER,
          created_at     INTEGER NOT NULL
        );

        CREATE INDEX idx_remote_control_pairing_tickets_active
          ON remote_control_pairing_tickets(expires_at, consumed_at);

        CREATE TABLE project_architecture_graphs (
          project_id TEXT PRIMARY KEY,
          version    INTEGER NOT NULL,
          graph_json TEXT NOT NULL,
          updated_at INTEGER NOT NULL,
          FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
        );

        CREATE TABLE project_architecture_changes (
          id                TEXT PRIMARY KEY,
          project_id        TEXT NOT NULL,
          task_id           TEXT NOT NULL,
          turn_id           TEXT,
          backend           TEXT NOT NULL,
          status            TEXT NOT NULL CHECK (status IN
                            ('proposed','pending','applied','rejected','rolled_back')),
          permission_mode   TEXT NOT NULL,
          summary           TEXT NOT NULL DEFAULT '',
          changes_json      TEXT NOT NULL,
          before_graph_json TEXT,
          after_graph_json  TEXT,
          created_at        INTEGER NOT NULL,
          resolved_at       INTEGER,
          FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
        );

        CREATE INDEX idx_project_architecture_changes_project_created
          ON project_architecture_changes(project_id, created_at DESC);
        CREATE INDEX idx_project_architecture_changes_task
          ON project_architecture_changes(task_id, created_at DESC);

        CREATE TABLE task_agent_sessions (
          task_id         TEXT NOT NULL,
          backend         TEXT NOT NULL,
          session_id      TEXT NOT NULL,
          updated_at      INTEGER NOT NULL,
          PRIMARY KEY (task_id, backend),
          FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE
        );

        CREATE TABLE task_worktrees (
          task_id        TEXT PRIMARY KEY,
          project_id     TEXT,
          base_repo_path TEXT NOT NULL,
          worktree_path  TEXT NOT NULL UNIQUE,
          branch_name    TEXT NOT NULL,
          base_branch    TEXT NOT NULL,
          status         TEXT NOT NULL DEFAULT 'active'
                         CHECK (status IN ('active','merged','removed')),
          created_at     INTEGER NOT NULL,
          updated_at     INTEGER NOT NULL,
          FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE
        );

        CREATE INDEX idx_task_worktrees_project_status
          ON task_worktrees(project_id, status, updated_at DESC);

        CREATE TABLE task_runtime_states (
          task_id         TEXT PRIMARY KEY,
          turn_id         TEXT NOT NULL,
          backend         TEXT NOT NULL,
          phase           TEXT NOT NULL CHECK (phase IN
                            ('running','interrupted_pending_finish','reset_pending_finish')),
          process_session_id TEXT,
          runtime_epoch   TEXT NOT NULL,
          context_json    TEXT,
          updated_at      INTEGER NOT NULL,
          FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE
        );

        CREATE INDEX idx_task_runtime_states_epoch_backend
          ON task_runtime_states(runtime_epoch, backend, updated_at);

        CREATE TABLE task_runtime_finalizations (
          task_id               TEXT PRIMARY KEY,
          pending_reset_cleanup INTEGER NOT NULL DEFAULT 0
                                CHECK (pending_reset_cleanup IN (0, 1)),
          rollback_json         TEXT,
          updated_at            INTEGER NOT NULL,
          FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE
        );

        CREATE TABLE task_pending_turns (
          id              INTEGER PRIMARY KEY AUTOINCREMENT,
          task_id         TEXT NOT NULL,
          content         TEXT NOT NULL,
          composer_json   TEXT NOT NULL,
          project_cwd     TEXT NOT NULL,
          attachments_json TEXT NOT NULL DEFAULT '[]',
          workflow_json   TEXT,
          runtime_command_json TEXT,
          runtime_options_json TEXT,
          message_json    TEXT NOT NULL,
          turn_id         TEXT NOT NULL,
          guide_id        TEXT,
          created_at      INTEGER NOT NULL,
          FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE
        );

        CREATE INDEX idx_task_pending_turns_task_id
          ON task_pending_turns(task_id, id);

        CREATE TABLE agent_timeline_events (
          id                TEXT PRIMARY KEY,
          task_id           TEXT NOT NULL,
          turn_id           TEXT,
          backend           TEXT NOT NULL,
          kind              TEXT NOT NULL,
          status            TEXT NOT NULL,
          title             TEXT NOT NULL,
          summary           TEXT,
          payload           TEXT NOT NULL,
          created_at        INTEGER NOT NULL,
          updated_at        INTEGER NOT NULL,
          turn_seq          INTEGER NOT NULL,
          intra_turn_order  INTEGER NOT NULL
        );

        CREATE INDEX idx_agent_timeline_events_task_id_turn
          ON agent_timeline_events(task_id, turn_seq, intra_turn_order);

        CREATE TABLE agent_usage_records (
          event_id              TEXT PRIMARY KEY,
          task_id               TEXT NOT NULL,
          turn_id               TEXT,
          backend               TEXT NOT NULL,
          session_id            TEXT,
          input_tokens          INTEGER NOT NULL DEFAULT 0,
          output_tokens         INTEGER NOT NULL DEFAULT 0,
          cache_read_tokens     INTEGER NOT NULL DEFAULT 0,
          cache_creation_tokens INTEGER NOT NULL DEFAULT 0,
          total_tokens          INTEGER NOT NULL DEFAULT 0,
          known_cost_usd        REAL,
          raw_usage_json        TEXT NOT NULL,
          created_at            INTEGER NOT NULL,
          updated_at            INTEGER NOT NULL,
          FOREIGN KEY (event_id) REFERENCES agent_timeline_events(id) ON DELETE CASCADE
        );

        CREATE INDEX idx_agent_usage_records_created_at
          ON agent_usage_records(created_at);
        CREATE INDEX idx_agent_usage_records_backend_created
          ON agent_usage_records(backend, created_at);
        CREATE INDEX idx_agent_usage_records_task
          ON agent_usage_records(task_id, created_at);

        CREATE TABLE automation_workflows (
          id                   TEXT PRIMARY KEY,
          name                 TEXT NOT NULL,
          enabled              INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1)),
          scope_json           TEXT NOT NULL DEFAULT '{}',
          draft_json           TEXT NOT NULL DEFAULT '{"nodes":[],"edges":[],"scope":{}}',
          published_version_id TEXT,
          created_at           INTEGER NOT NULL,
          updated_at           INTEGER NOT NULL
        );

        CREATE TABLE automation_workflow_versions (
          id            TEXT PRIMARY KEY,
          workflow_id   TEXT NOT NULL,
          version       INTEGER NOT NULL,
          snapshot_json TEXT NOT NULL,
          created_at    INTEGER NOT NULL,
          FOREIGN KEY (workflow_id) REFERENCES automation_workflows(id) ON DELETE CASCADE
        );

        CREATE UNIQUE INDEX idx_automation_workflow_versions_workflow_version
          ON automation_workflow_versions(workflow_id, version);

        CREATE TABLE automation_runs (
          id                  TEXT PRIMARY KEY,
          workflow_id         TEXT NOT NULL,
          workflow_version_id TEXT NOT NULL,
          status              TEXT NOT NULL CHECK (status IN
                                ('pending','running','succeeded','failed','skipped','cancelled','waiting_user')),
          trigger_json        TEXT NOT NULL,
          scope_json          TEXT NOT NULL,
          started_at          INTEGER NOT NULL,
          finished_at         INTEGER,
          error               TEXT,
          FOREIGN KEY (workflow_id) REFERENCES automation_workflows(id) ON DELETE CASCADE,
          FOREIGN KEY (workflow_version_id) REFERENCES automation_workflow_versions(id)
        );

        CREATE INDEX idx_automation_runs_workflow_started
          ON automation_runs(workflow_id, started_at DESC);
        CREATE INDEX idx_automation_runs_status
          ON automation_runs(status);

        CREATE TABLE automation_run_nodes (
          id          TEXT PRIMARY KEY,
          run_id      TEXT NOT NULL,
          node_id     TEXT NOT NULL,
          status      TEXT NOT NULL CHECK (status IN
                        ('pending','running','succeeded','failed','skipped','cancelled','waiting_user')),
          input_json  TEXT NOT NULL DEFAULT '{}',
          output_json TEXT,
          error       TEXT,
          started_at  INTEGER,
          finished_at INTEGER,
          FOREIGN KEY (run_id) REFERENCES automation_runs(id) ON DELETE CASCADE
        );

        CREATE INDEX idx_automation_run_nodes_run
          ON automation_run_nodes(run_id);

        "#,
    )
    .map_err(|e| format!("lilia-store: 创建当前 schema 失败：{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    #[test]
    fn current_schema_creates_memory_tables() {
        let mut conn = Connection::open_in_memory().unwrap();
        ensure_current_schema(&mut conn).unwrap();

        let count: i64 = conn
            .query_row(
                r#"SELECT COUNT(*) FROM sqlite_master
                   WHERE type = 'table' AND name IN ('memories', 'memory_injection_states')"#,
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn current_schema_creates_task_worktrees() {
        let mut conn = Connection::open_in_memory().unwrap();
        ensure_current_schema(&mut conn).unwrap();

        let count: i64 = conn
            .query_row(
                r#"SELECT COUNT(*) FROM sqlite_master
                   WHERE type = 'table' AND name = 'task_worktrees'"#,
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn todo_conversation_reference_migration_preserves_existing_rows() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE task_todos (
              id TEXT PRIMARY KEY,
              task_id TEXT NOT NULL,
              text TEXT NOT NULL,
              done INTEGER NOT NULL DEFAULT 0,
              "order" INTEGER NOT NULL,
              source TEXT NOT NULL,
              priority TEXT NOT NULL,
              guide_status TEXT,
              attachments_json TEXT NOT NULL DEFAULT '[]',
              created_at INTEGER NOT NULL,
              updated_at INTEGER NOT NULL
            );
            INSERT INTO task_todos
              (id, task_id, text, "order", source, priority, created_at, updated_at)
              VALUES ('guide-1', 'task-1', '保留现有引导', 0, 'lilia', 'normal', 1, 1);
            PRAGMA user_version = 30;
            "#,
        )
        .unwrap();

        ensure_current_schema(&mut conn).unwrap();

        let row: (String, String) = conn
            .query_row(
                "SELECT text, conversation_references_json FROM task_todos WHERE id = 'guide-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(row, ("保留现有引导".to_owned(), "[]".to_owned()));
        assert_eq!(current_schema_version(), 31);
    }

    #[test]
    fn old_development_database_is_rebuilt_from_current_schema() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(&format!(
            r#"
            CREATE TABLE projects (
              id         TEXT PRIMARY KEY,
              name       TEXT NOT NULL,
              cwd        TEXT,
              created_at INTEGER NOT NULL
            );
            INSERT INTO projects (id, name, cwd, created_at)
              VALUES ('old-project', '旧项目', NULL, 1);
            PRAGMA user_version = {};
            "#,
            current_schema_version() + 1
        ))
        .unwrap();

        ensure_current_schema(&mut conn).unwrap();

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, current_schema_version());

        let project_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM projects", [], |row| row.get(0))
            .unwrap();
        assert_eq!(project_count, 0);

        conn.execute(
            "INSERT INTO projects (id, name, cwd, created_at, sort_order, pinned) VALUES (?1, ?2, NULL, ?3, ?4, ?5)",
            params!["new-project", "新项目", 2, 0, 0],
        )
        .unwrap();

        conn.execute(
            r#"INSERT INTO task_todos
               (id, task_id, text, done, "order", source, priority, guide_status, attachments_json, created_at, updated_at)
               VALUES (?1, ?2, ?3, 0, 0, 'lilia', 'normal', 'pending', '[]', ?4, ?5)"#,
            params!["fresh-todo", "task-1", "当前 Todo", 2, 2],
        )
        .unwrap();
    }

    #[test]
    fn architecture_permission_mode_migration_removes_stale_database_enum() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE projects (
              id         TEXT PRIMARY KEY,
              name       TEXT NOT NULL,
              cwd        TEXT,
              created_at INTEGER NOT NULL
            );
            CREATE TABLE tasks (
              id TEXT PRIMARY KEY
            );
            INSERT INTO projects (id, name, cwd, created_at)
              VALUES ('project-1', '项目', NULL, 1);
            INSERT INTO tasks (id) VALUES ('task-1'), ('task-2');

            CREATE TABLE project_architecture_changes (
              id                TEXT PRIMARY KEY,
              project_id        TEXT NOT NULL,
              task_id           TEXT NOT NULL,
              turn_id           TEXT,
              backend           TEXT NOT NULL,
              status            TEXT NOT NULL CHECK (status IN
                                ('proposed','pending','applied','rejected','rolled_back')),
              permission_mode   TEXT NOT NULL CHECK (permission_mode IN ('ask','full','readonly')),
              summary           TEXT NOT NULL DEFAULT '',
              changes_json      TEXT NOT NULL,
              before_graph_json TEXT,
              after_graph_json  TEXT,
              created_at        INTEGER NOT NULL,
              resolved_at       INTEGER,
              FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
            );
            CREATE INDEX idx_project_architecture_changes_project_created
              ON project_architecture_changes(project_id, created_at DESC);
            CREATE INDEX idx_project_architecture_changes_task
              ON project_architecture_changes(task_id, created_at DESC);

            CREATE TABLE task_agent_sessions (
              task_id         TEXT NOT NULL,
              backend         TEXT NOT NULL,
              session_id      TEXT NOT NULL,
              updated_at      INTEGER NOT NULL,
              PRIMARY KEY (task_id, backend)
            );

            CREATE TABLE task_runtime_states (
              task_id         TEXT PRIMARY KEY,
              turn_id         TEXT NOT NULL,
              backend         TEXT NOT NULL,
              phase           TEXT NOT NULL CHECK (phase IN
                                ('running','interrupted_pending_finish','reset_pending_finish')),
              process_session_id TEXT,
              runtime_epoch   TEXT NOT NULL,
              context_json    TEXT,
              updated_at      INTEGER NOT NULL
            );
            CREATE INDEX idx_task_runtime_states_epoch_backend
              ON task_runtime_states(runtime_epoch, backend, updated_at);

            CREATE TABLE agent_timeline_events (
              id                TEXT PRIMARY KEY,
              task_id           TEXT NOT NULL,
              turn_id           TEXT,
              backend           TEXT NOT NULL,
              kind              TEXT NOT NULL,
              status            TEXT NOT NULL,
              title             TEXT NOT NULL,
              summary           TEXT,
              payload           TEXT NOT NULL,
              created_at        INTEGER NOT NULL,
              updated_at        INTEGER NOT NULL,
              turn_seq          INTEGER NOT NULL,
              intra_turn_order  INTEGER NOT NULL
            );
            CREATE INDEX idx_agent_timeline_events_task_id_turn
              ON agent_timeline_events(task_id, turn_seq, intra_turn_order);

            CREATE TABLE agent_usage_records (
              event_id              TEXT PRIMARY KEY,
              task_id               TEXT NOT NULL,
              turn_id               TEXT,
              backend               TEXT NOT NULL,
              session_id            TEXT,
              input_tokens          INTEGER NOT NULL DEFAULT 0,
              output_tokens         INTEGER NOT NULL DEFAULT 0,
              cache_read_tokens     INTEGER NOT NULL DEFAULT 0,
              cache_creation_tokens INTEGER NOT NULL DEFAULT 0,
              total_tokens          INTEGER NOT NULL DEFAULT 0,
              known_cost_usd        REAL,
              raw_usage_json        TEXT NOT NULL,
              created_at            INTEGER NOT NULL,
              updated_at            INTEGER NOT NULL
            );
            CREATE INDEX idx_agent_usage_records_created_at
              ON agent_usage_records(created_at);
            CREATE INDEX idx_agent_usage_records_backend_created
              ON agent_usage_records(backend, created_at);
            CREATE INDEX idx_agent_usage_records_task
              ON agent_usage_records(task_id, created_at);

            INSERT INTO project_architecture_changes
              (id, project_id, task_id, backend, status, permission_mode, summary,
               changes_json, created_at)
              VALUES ('change-1', 'project-1', 'task-1', 'codex', 'applied', 'ask',
                      '旧记录', '[]', 1);
            INSERT INTO task_agent_sessions
              (task_id, backend, session_id, updated_at)
              VALUES ('task-1', 'codex', 'session-1', 1);
            INSERT INTO task_runtime_states
              (task_id, turn_id, backend, phase, runtime_epoch, updated_at)
              VALUES ('task-1', 'turn-1', 'codex', 'running', 'epoch-1', 1);
            INSERT INTO agent_timeline_events
              (id, task_id, backend, kind, status, title, payload, created_at,
               updated_at, turn_seq, intra_turn_order)
              VALUES ('event-1', 'task-1', 'codex', 'message', 'done', '事件',
                      '{}', 1, 1, 0, 0);
            INSERT INTO agent_usage_records
              (event_id, task_id, backend, raw_usage_json, created_at, updated_at)
              VALUES ('event-1', 'task-1', 'codex', '{}', 1, 1);
            PRAGMA user_version = 25;
            "#,
        )
        .unwrap();

        ensure_current_schema(&mut conn).unwrap();

        conn.execute(
            r#"INSERT INTO project_architecture_changes
               (id, project_id, task_id, backend, status, permission_mode, summary,
                changes_json, created_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"#,
            params![
                "change-2",
                "project-1",
                "task-1",
                "custom-backend",
                "applied",
                "free",
                "契约扩展记录",
                "[]",
                2
            ],
        )
        .unwrap();

        conn.execute_batch(
            r#"
            INSERT INTO task_agent_sessions
              (task_id, backend, session_id, updated_at)
              VALUES ('task-1', 'custom-backend', 'session-2', 2);
            INSERT INTO task_runtime_states
              (task_id, turn_id, backend, phase, runtime_epoch, updated_at)
              VALUES ('task-2', 'turn-2', 'custom-backend', 'running', 'epoch-2', 2);
            INSERT INTO agent_timeline_events
              (id, task_id, backend, kind, status, title, payload, created_at,
               updated_at, turn_seq, intra_turn_order)
              VALUES ('event-2', 'task-2', 'custom-backend', 'message', 'done',
                      '事件', '{}', 2, 2, 0, 0);
            INSERT INTO agent_usage_records
              (event_id, task_id, backend, raw_usage_json, created_at, updated_at)
              VALUES ('event-2', 'task-2', 'custom-backend', '{}', 2, 2);
            "#,
        )
        .unwrap();

        let change_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM project_architecture_changes",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(change_count, 2);

        let custom_backend_records: i64 = conn
            .query_row(
                r#"
                SELECT
                  (SELECT COUNT(*) FROM project_architecture_changes WHERE backend = 'custom-backend') +
                  (SELECT COUNT(*) FROM task_agent_sessions WHERE backend = 'custom-backend') +
                  (SELECT COUNT(*) FROM task_runtime_states WHERE backend = 'custom-backend') +
                  (SELECT COUNT(*) FROM agent_timeline_events WHERE backend = 'custom-backend') +
                  (SELECT COUNT(*) FROM agent_usage_records WHERE backend = 'custom-backend')
                "#,
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(custom_backend_records, 5);

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, current_schema_version());
    }

    fn add_future_project_label_migration(conn: &Connection) -> Result<(), String> {
        conn.execute_batch("ALTER TABLE projects ADD COLUMN label TEXT;")
            .map_err(|e| format!("test migration: {e}"))
    }

    #[test]
    fn database_below_break_baseline_is_rebuilt_from_current_schema() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE projects (
              id         TEXT PRIMARY KEY,
              name       TEXT NOT NULL,
              cwd        TEXT,
              created_at INTEGER NOT NULL
            );
            INSERT INTO projects (id, name, cwd, created_at)
              VALUES ('legacy-project', '旧项目', NULL, 1);
            "#,
        )
        .unwrap();
        conn.execute_batch(&format!(
            "PRAGMA user_version = {};",
            RESET_BASELINE_SCHEMA_VERSION - 1
        ))
        .unwrap();

        ensure_current_schema(&mut conn).unwrap();

        let project_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM projects", [], |row| row.get(0))
            .unwrap();
        assert_eq!(project_count, 0);

        conn.execute(
            "INSERT INTO tasks (id, session_id, title, status, created_at) VALUES ('task-1', 'task-1', '任务', 'waiting', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO task_agent_sessions (task_id, backend, session_id, updated_at) VALUES ('task-1', 'codex', 'thread-1', 1)",
            [],
        )
        .unwrap();
    }

    #[test]
    fn reset_orphaned_queued_guides_marks_them_pending() {
        let mut conn = Connection::open_in_memory().unwrap();
        create_current_schema(&conn).unwrap();
        conn.execute_batch(&format!(
            "PRAGMA user_version = {};",
            current_schema_version()
        ))
        .unwrap();
        ensure_current_schema(&mut conn).unwrap();
        conn.execute(
            r#"INSERT INTO task_todos
               (id, task_id, text, done, "order", source, priority, guide_status, created_at, updated_at)
               VALUES (?1, ?2, ?3, ?4, ?5, 'lilia', 'normal', 'queued', ?6, ?7)"#,
            params!["guide-1", "task-1", "排队引导", 0, 0, 1, 1],
        )
        .unwrap();

        reset_orphaned_queued_guides(&conn).unwrap();

        let status: String = conn
            .query_row(
                "SELECT guide_status FROM task_todos WHERE id = 'guide-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "pending");
    }

    #[test]
    fn future_migrations_continue_after_reset_baseline() {
        let mut conn = Connection::open_in_memory().unwrap();
        ensure_current_schema(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO projects (id, name, cwd, created_at, sort_order, pinned) VALUES (?1, ?2, NULL, ?3, ?4, ?5)",
            params!["kept-project", "保留项目", 2, 0, 0],
        )
        .unwrap();

        ensure_schema_with_migrations(
            &mut conn,
            current_schema_version() + 1,
            &[SchemaMigration {
                version: current_schema_version() + 1,
                name: "test_add_project_label",
                apply: add_future_project_label_migration,
            }],
        )
        .unwrap();

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, current_schema_version() + 1);

        let project_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM projects", [], |row| row.get(0))
            .unwrap();
        assert_eq!(project_count, 1);

        conn.execute(
            "UPDATE projects SET label = ?1 WHERE id = ?2",
            params!["future", "kept-project"],
        )
        .unwrap();
    }
}
