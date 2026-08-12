use rusqlite::Connection;

pub(super) const RESET_BASELINE_SCHEMA_VERSION: i64 = 24;

pub(super) struct SchemaMigration {
    pub version: i64,
    pub name: &'static str,
    pub apply: fn(&Connection) -> Result<(), String>,
}

pub(super) const SCHEMA_MIGRATIONS: &[SchemaMigration] = &[
    SchemaMigration {
        version: 25,
        name: "memory_layer1",
        apply: create_memory_tables,
    },
    SchemaMigration {
        version: 26,
        name: "project_architecture_permission_contract",
        apply: relax_project_architecture_permission_mode,
    },
    SchemaMigration {
        version: 27,
        name: "chat_backend_contract",
        apply: relax_chat_backend_enums,
    },
    SchemaMigration {
        version: 28,
        name: "task_worktrees",
        apply: create_task_worktree_table,
    },
    SchemaMigration {
        version: 29,
        name: "remote_control",
        apply: create_remote_control_tables,
    },
    SchemaMigration {
        version: 30,
        name: "task_handoffs",
        apply: create_task_handoff_table,
    },
    SchemaMigration {
        version: 31,
        name: "todo_conversation_references",
        apply: add_todo_conversation_references_column,
    },
];

fn add_todo_conversation_references_column(conn: &Connection) -> Result<(), String> {
    let exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('task_todos') WHERE name = 'conversation_references_json'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| format!("lilia-store: 检查 Todo 对话引用字段失败：{error}"))?;
    if exists == 0 {
        conn.execute(
            "ALTER TABLE task_todos ADD COLUMN conversation_references_json TEXT NOT NULL DEFAULT '[]'",
            [],
        )
        .map_err(|error| format!("lilia-store: 添加 Todo 对话引用字段失败：{error}"))?;
    }
    Ok(())
}

fn create_task_handoff_table(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS task_handoffs (
          handoff_id  TEXT PRIMARY KEY,
          task_id     TEXT NOT NULL UNIQUE,
          payload_json TEXT NOT NULL,
          source_route TEXT NOT NULL,
          created_at  INTEGER NOT NULL,
          FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE
        );
        "#,
    )
    .map_err(|e| format!("lilia-store: 创建 task handoff schema 失败：{e}"))
}

fn create_memory_tables(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS memories (
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
        CREATE INDEX IF NOT EXISTS idx_memories_scope_project
          ON memories(scope, project_id, enabled, updated_at DESC);

        CREATE TABLE IF NOT EXISTS memory_injection_states (
          task_id                TEXT PRIMARY KEY,
          enabled                INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
          last_injected_turn_seq INTEGER,
          updated_at             INTEGER NOT NULL,
          FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE
        );
        "#,
    )
    .map_err(|e| format!("lilia-store: 创建 Memory schema 失败：{e}"))
}

fn relax_project_architecture_permission_mode(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = OFF;

        ALTER TABLE project_architecture_changes
          RENAME TO project_architecture_changes_old;

        CREATE TABLE project_architecture_changes (
          id                TEXT PRIMARY KEY,
          project_id        TEXT NOT NULL,
          task_id           TEXT NOT NULL,
          turn_id           TEXT,
          backend           TEXT NOT NULL CHECK (backend IN ('claude','codex')),
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

        INSERT INTO project_architecture_changes (
          id, project_id, task_id, turn_id, backend, status, permission_mode,
          summary, changes_json, before_graph_json, after_graph_json, created_at, resolved_at
        )
        SELECT
          id, project_id, task_id, turn_id, backend, status, permission_mode,
          summary, changes_json, before_graph_json, after_graph_json, created_at, resolved_at
        FROM project_architecture_changes_old;

        DROP TABLE project_architecture_changes_old;

        CREATE INDEX idx_project_architecture_changes_project_created
          ON project_architecture_changes(project_id, created_at DESC);
        CREATE INDEX idx_project_architecture_changes_task
          ON project_architecture_changes(task_id, created_at DESC);

        PRAGMA foreign_keys = ON;
        "#,
    )
    .map_err(|e| format!("lilia-store: 迁移架构权限 schema 失败：{e}"))
}

fn relax_chat_backend_enums(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = OFF;

        ALTER TABLE project_architecture_changes
          RENAME TO project_architecture_changes_old;
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
        INSERT INTO project_architecture_changes (
          id, project_id, task_id, turn_id, backend, status, permission_mode,
          summary, changes_json, before_graph_json, after_graph_json, created_at, resolved_at
        )
        SELECT
          id, project_id, task_id, turn_id, backend, status, permission_mode,
          summary, changes_json, before_graph_json, after_graph_json, created_at, resolved_at
        FROM project_architecture_changes_old;
        DROP TABLE project_architecture_changes_old;
        CREATE INDEX idx_project_architecture_changes_project_created
          ON project_architecture_changes(project_id, created_at DESC);
        CREATE INDEX idx_project_architecture_changes_task
          ON project_architecture_changes(task_id, created_at DESC);

        ALTER TABLE task_agent_sessions
          RENAME TO task_agent_sessions_old;
        CREATE TABLE task_agent_sessions (
          task_id         TEXT NOT NULL,
          backend         TEXT NOT NULL,
          session_id      TEXT NOT NULL,
          updated_at      INTEGER NOT NULL,
          PRIMARY KEY (task_id, backend),
          FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE
        );
        INSERT INTO task_agent_sessions (task_id, backend, session_id, updated_at)
        SELECT task_id, backend, session_id, updated_at
        FROM task_agent_sessions_old;
        DROP TABLE task_agent_sessions_old;

        ALTER TABLE task_runtime_states
          RENAME TO task_runtime_states_old;
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
        INSERT INTO task_runtime_states (
          task_id, turn_id, backend, phase, process_session_id, runtime_epoch,
          context_json, updated_at
        )
        SELECT
          task_id, turn_id, backend, phase, process_session_id, runtime_epoch,
          context_json, updated_at
        FROM task_runtime_states_old;
        DROP TABLE task_runtime_states_old;
        CREATE INDEX idx_task_runtime_states_epoch_backend
          ON task_runtime_states(runtime_epoch, backend, updated_at);

        ALTER TABLE agent_timeline_events
          RENAME TO agent_timeline_events_old;
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
        INSERT INTO agent_timeline_events (
          id, task_id, turn_id, backend, kind, status, title, summary, payload,
          created_at, updated_at, turn_seq, intra_turn_order
        )
        SELECT
          id, task_id, turn_id, backend, kind, status, title, summary, payload,
          created_at, updated_at, turn_seq, intra_turn_order
        FROM agent_timeline_events_old;
        DROP TABLE agent_timeline_events_old;
        CREATE INDEX idx_agent_timeline_events_task_id_turn
          ON agent_timeline_events(task_id, turn_seq, intra_turn_order);

        ALTER TABLE agent_usage_records
          RENAME TO agent_usage_records_old;
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
        INSERT INTO agent_usage_records (
          event_id, task_id, turn_id, backend, session_id, input_tokens,
          output_tokens, cache_read_tokens, cache_creation_tokens, total_tokens,
          known_cost_usd, raw_usage_json, created_at, updated_at
        )
        SELECT
          event_id, task_id, turn_id, backend, session_id, input_tokens,
          output_tokens, cache_read_tokens, cache_creation_tokens, total_tokens,
          known_cost_usd, raw_usage_json, created_at, updated_at
        FROM agent_usage_records_old;
        DROP TABLE agent_usage_records_old;
        CREATE INDEX idx_agent_usage_records_created_at
          ON agent_usage_records(created_at);
        CREATE INDEX idx_agent_usage_records_backend_created
          ON agent_usage_records(backend, created_at);
        CREATE INDEX idx_agent_usage_records_task
          ON agent_usage_records(task_id, created_at);

        PRAGMA foreign_keys = ON;
        "#,
    )
    .map_err(|e| format!("lilia-store: 迁移后端 schema 失败：{e}"))
}

fn create_task_worktree_table(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS task_worktrees (
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
        CREATE INDEX IF NOT EXISTS idx_task_worktrees_project_status
          ON task_worktrees(project_id, status, updated_at DESC);
        "#,
    )
    .map_err(|e| format!("lilia-store: 创建 task worktree schema 失败：{e}"))
}

fn create_remote_control_tables(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS remote_control_settings (
          key        TEXT PRIMARY KEY,
          value      TEXT NOT NULL,
          updated_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS remote_control_trusted_devices (
          id                TEXT PRIMARY KEY,
          display_name      TEXT NOT NULL,
          endpoint_id       TEXT NOT NULL UNIQUE,
          protocol_version  INTEGER NOT NULL,
          trusted           INTEGER NOT NULL DEFAULT 1 CHECK (trusted IN (0, 1)),
          first_paired_at   INTEGER NOT NULL,
          last_seen_at      INTEGER,
          revoked_at        INTEGER
        );

        CREATE INDEX IF NOT EXISTS idx_remote_control_trusted_devices_endpoint
          ON remote_control_trusted_devices(endpoint_id);

        CREATE TABLE IF NOT EXISTS remote_control_pairing_tickets (
          id             TEXT PRIMARY KEY,
          challenge      TEXT NOT NULL,
          pc_name        TEXT NOT NULL,
          endpoint_id    TEXT NOT NULL,
          pairing_uri    TEXT NOT NULL,
          expires_at     INTEGER NOT NULL,
          consumed_at    INTEGER,
          created_at     INTEGER NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_remote_control_pairing_tickets_active
          ON remote_control_pairing_tickets(expires_at, consumed_at);
        "#,
    )
    .map_err(|e| format!("lilia-store: 创建 Remote Control schema 失败：{e}"))
}

pub(super) fn ensure_schema_with_migrations(
    conn: &mut Connection,
    target_version: i64,
    migrations: &[SchemaMigration],
) -> Result<(), String> {
    let current: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|e| format!("lilia-store: 读取 user_version 失败：{e}"))?;

    if current < RESET_BASELINE_SCHEMA_VERSION || current > target_version {
        super::schema::reset_development_schema(conn)?;
    }

    let current: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|e| format!("lilia-store: 读取重置后 user_version 失败：{e}"))?;

    for migration in migrations {
        if migration.version <= current || migration.version > target_version {
            continue;
        }
        (migration.apply)(conn)
            .map_err(|e| format!("lilia-store: migration {} failed: {e}", migration.name))?;
        conn.execute_batch(&format!("PRAGMA user_version = {};", migration.version))
            .map_err(|e| format!("lilia-store: 写 migration version 失败：{e}"))?;
    }

    Ok(())
}
