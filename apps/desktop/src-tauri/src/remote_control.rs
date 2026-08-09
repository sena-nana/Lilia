use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

use crate::agent_timeline;
use crate::chat;
use crate::native_agent::BACKEND_NATIVE_AGENTKIT;
use crate::projects_tasks::{SidebarConversationSummaryRow, TaskRow};
use crate::store::LiliaStore;
use crate::util::now_millis;

const PROTOCOL_VERSION: i64 = 1;
const MIN_PROTOCOL_VERSION: i64 = 1;
const REMOTE_ALPN: &str = "lilia.remote-control.v1";
const HOST_ENABLED_KEY: &str = "host_enabled";
const PC_NAME_KEY: &str = "pc_name";
const ENDPOINT_ID_KEY: &str = "endpoint_id";
const KEEP_AWAKE_ENABLED_KEY: &str = "keep_awake_enabled";
const PAIRING_TTL_MS: i64 = 10 * 60 * 1000;
const RECENT_ANDROID_SEEN_MS: i64 = 2 * 60 * 1000;
const DEFAULT_HTTP_BRIDGE_PORT: u16 = 41478;
const REMOTE_WAKE_MONITOR_IDLE_MS: u64 = 30_000;

static HTTP_BRIDGE: OnceLock<Mutex<Option<RemoteHttpBridge>>> = OnceLock::new();
static REMOTE_WAKE: OnceLock<Arc<RemoteWakeController>> = OnceLock::new();

#[derive(Debug, Clone)]
struct RemoteHttpBridge {
    port: u16,
}

#[derive(Debug)]
struct RemoteWakeController {
    state: Mutex<RemoteWakeRuntime>,
    changed: Condvar,
}

#[derive(Debug, Clone)]
struct RemoteWakeRuntime {
    configured: bool,
    active_until_ms: Option<i64>,
    platform_active: bool,
}

impl RemoteWakeRuntime {
    fn new(configured: bool) -> Self {
        Self {
            configured,
            active_until_ms: None,
            platform_active: false,
        }
    }

    fn set_target(&mut self, configured: bool, active_until_ms: Option<i64>) {
        self.configured = configured;
        self.active_until_ms = configured.then_some(active_until_ms).flatten();
    }

    fn expire(&mut self, now: i64) {
        if self
            .active_until_ms
            .is_some_and(|active_until| active_until <= now)
        {
            self.active_until_ms = None;
        }
    }

    fn should_keep_awake(&self, now: i64) -> bool {
        self.configured
            && self
                .active_until_ms
                .is_some_and(|active_until| active_until > now)
    }

    fn next_wait(&self, now: i64) -> Duration {
        self.active_until_ms
            .filter(|active_until| *active_until > now)
            .map(|active_until| Duration::from_millis((active_until - now) as u64))
            .unwrap_or_else(|| Duration::from_millis(REMOTE_WAKE_MONITOR_IDLE_MS))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteEndpointAddress {
    pub endpoint_id: String,
    #[serde(default)]
    pub relay_url: Option<String>,
    #[serde(default)]
    pub direct_addresses: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteCapabilitySet {
    pub protocol_version: i64,
    pub min_protocol_version: i64,
    pub alpn: String,
    pub supports_pairing: bool,
    pub supports_task_inbox: bool,
    pub supports_timeline_subscription: bool,
    pub supports_timeline_pagination: bool,
    pub supports_chat_send: bool,
    pub supports_interaction_response: bool,
    pub supports_interrupt: bool,
    pub supports_agent_wire: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemotePeerSummary {
    pub id: String,
    pub kind: String,
    pub display_name: String,
    pub endpoint_id: String,
    pub protocol_version: i64,
    pub trusted: bool,
    pub first_paired_at: i64,
    pub last_seen_at: Option<i64>,
    pub revoked_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemotePairingTicket {
    pub id: String,
    pub pc_name: String,
    pub pc_endpoint: RemoteEndpointAddress,
    pub protocol_version: i64,
    pub challenge: String,
    pub expires_at: i64,
    pub pairing_uri: String,
    #[serde(default)]
    pub bridge_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteControlStatus {
    pub host_enabled: bool,
    pub state: String,
    pub pc_name: String,
    pub keep_awake_enabled: bool,
    pub endpoint: Option<RemoteEndpointAddress>,
    pub active_ticket: Option<RemotePairingTicket>,
    pub trusted_devices: Vec<RemotePeerSummary>,
    pub capabilities: RemoteCapabilitySet,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteEndpointInput {
    pub endpoint_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemotePairDeviceInput {
    pub ticket_id: String,
    pub challenge: String,
    pub device_name: String,
    pub android_endpoint: RemoteEndpointInput,
    pub protocol_version: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteRequestEnvelope {
    pub id: String,
    pub protocol_version: i64,
    pub device_id: String,
    pub request: JsonValue,
}

fn capabilities() -> RemoteCapabilitySet {
    RemoteCapabilitySet {
        protocol_version: PROTOCOL_VERSION,
        min_protocol_version: MIN_PROTOCOL_VERSION,
        alpn: REMOTE_ALPN.to_string(),
        supports_pairing: true,
        supports_task_inbox: true,
        supports_timeline_subscription: true,
        supports_timeline_pagination: true,
        supports_chat_send: true,
        supports_interaction_response: true,
        supports_interrupt: true,
        supports_agent_wire: true,
    }
}

fn remote_wake_controller() -> &'static Arc<RemoteWakeController> {
    REMOTE_WAKE.get_or_init(|| {
        let controller = Arc::new(RemoteWakeController {
            state: Mutex::new(RemoteWakeRuntime::new(false)),
            changed: Condvar::new(),
        });
        let monitor = controller.clone();
        thread::spawn(move || remote_wake_monitor(monitor));
        controller
    })
}

fn remote_wake_monitor(controller: Arc<RemoteWakeController>) {
    loop {
        let mut state = match controller.state.lock() {
            Ok(state) => state,
            Err(_) => return,
        };
        let now = now_millis();
        state.expire(now);
        let should_keep_awake = state.should_keep_awake(now);
        if state.platform_active != should_keep_awake {
            match crate::system_wake::set_system_awake(should_keep_awake) {
                Ok(()) => state.platform_active = should_keep_awake,
                Err(err) => eprintln!("[remote-control] update system wake state failed: {err}"),
            }
        }
        let wait = state.next_wait(now);
        match controller.changed.wait_timeout(state, wait) {
            Ok((_state, _timeout)) => {}
            Err(_) => return,
        }
    }
}

fn set_remote_wake(configured: bool, active_until_ms: Option<i64>) {
    let controller = remote_wake_controller();
    if let Ok(mut state) = controller.state.lock() {
        state.set_target(configured, active_until_ms);
        controller.changed.notify_one();
    }
}

fn setting(conn: &Connection, key: &str) -> Result<Option<String>, String> {
    conn.query_row(
        "SELECT value FROM remote_control_settings WHERE key = ?1",
        params![key],
        |row| row.get(0),
    )
    .optional()
    .map_err(|e| format!("remote_control: 读取设置失败：{e}"))
}

fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    conn.execute(
        r#"INSERT INTO remote_control_settings (key, value, updated_at)
           VALUES (?1, ?2, ?3)
           ON CONFLICT(key) DO UPDATE SET
             value = excluded.value,
             updated_at = excluded.updated_at"#,
        params![key, value, now_millis()],
    )
    .map(|_| ())
    .map_err(|e| format!("remote_control: 写设置失败：{e}"))
}

fn host_enabled(conn: &Connection) -> Result<bool, String> {
    Ok(setting(conn, HOST_ENABLED_KEY)?.as_deref() == Some("true"))
}

fn keep_awake_enabled(conn: &Connection) -> Result<bool, String> {
    Ok(setting(conn, KEEP_AWAKE_ENABLED_KEY)?.as_deref() != Some("false"))
}

fn remote_wake_configured(conn: &Connection) -> Result<bool, String> {
    Ok(host_enabled(conn)? && keep_awake_enabled(conn)?)
}

fn sync_remote_wake_from_db(conn: &Connection) -> Result<(), String> {
    let configured = remote_wake_configured(conn)?;
    let active_until_ms = if configured && has_recent_connected_device(conn)? {
        Some(now_millis() + RECENT_ANDROID_SEEN_MS)
    } else {
        None
    };
    set_remote_wake(configured, active_until_ms);
    Ok(())
}

fn record_remote_activity_from_db(conn: &Connection) -> Result<(), String> {
    let configured = remote_wake_configured(conn)?;
    let active_until_ms = if configured {
        Some(now_millis() + RECENT_ANDROID_SEEN_MS)
    } else {
        None
    };
    set_remote_wake(configured, active_until_ms);
    Ok(())
}

fn pc_name(conn: &Connection) -> Result<String, String> {
    Ok(setting(conn, PC_NAME_KEY)?
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "Lilia 电脑".to_string()))
}

fn endpoint_id(conn: &Connection) -> Result<String, String> {
    if let Some(value) = setting(conn, ENDPOINT_ID_KEY)?
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        return Ok(value);
    }
    let id = format!("pc-{}", Uuid::new_v4());
    set_setting(conn, ENDPOINT_ID_KEY, &id)?;
    Ok(id)
}

fn endpoint(conn: &Connection) -> Result<RemoteEndpointAddress, String> {
    Ok(RemoteEndpointAddress {
        endpoint_id: endpoint_id(conn)?,
        relay_url: None,
        direct_addresses: Vec::new(),
    })
}

fn ticket_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RemotePairingTicket> {
    let id: String = row.get(0)?;
    let challenge: String = row.get(1)?;
    let pc_name: String = row.get(2)?;
    let endpoint_id: String = row.get(3)?;
    let pairing_uri: String = row.get(4)?;
    let expires_at: i64 = row.get(5)?;
    Ok(RemotePairingTicket {
        id,
        pc_name,
        pc_endpoint: RemoteEndpointAddress {
            endpoint_id,
            relay_url: None,
            direct_addresses: Vec::new(),
        },
        protocol_version: PROTOCOL_VERSION,
        challenge,
        expires_at,
        bridge_url: bridge_url_from_pairing_uri(&pairing_uri),
        pairing_uri,
    })
}

fn active_ticket(conn: &Connection) -> Result<Option<RemotePairingTicket>, String> {
    conn.query_row(
        r#"SELECT id, challenge, pc_name, endpoint_id, pairing_uri, expires_at
           FROM remote_control_pairing_tickets
           WHERE consumed_at IS NULL AND expires_at > ?1
           ORDER BY created_at DESC
           LIMIT 1"#,
        params![now_millis()],
        ticket_from_row,
    )
    .optional()
    .map_err(|e| format!("remote_control: 查询配对票据失败：{e}"))
}

fn trusted_devices(conn: &Connection) -> Result<Vec<RemotePeerSummary>, String> {
    let mut stmt = conn
        .prepare(
            r#"SELECT id, display_name, endpoint_id, protocol_version, trusted,
                      first_paired_at, last_seen_at, revoked_at
               FROM remote_control_trusted_devices
               ORDER BY revoked_at IS NOT NULL ASC, last_seen_at DESC, first_paired_at DESC"#,
        )
        .map_err(|e| format!("remote_control: trusted devices prepare 失败：{e}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok(RemotePeerSummary {
                id: row.get(0)?,
                kind: "android".to_string(),
                display_name: row.get(1)?,
                endpoint_id: row.get(2)?,
                protocol_version: row.get(3)?,
                trusted: row.get::<_, i64>(4)? != 0,
                first_paired_at: row.get(5)?,
                last_seen_at: row.get(6)?,
                revoked_at: row.get(7)?,
            })
        })
        .map_err(|e| format!("remote_control: trusted devices query 失败：{e}"))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("remote_control: trusted devices row 失败：{e}"))?);
    }
    Ok(out)
}

fn has_recent_connected_device(conn: &Connection) -> Result<bool, String> {
    let cutoff = now_millis() - RECENT_ANDROID_SEEN_MS;
    conn.query_row(
        r#"SELECT EXISTS(
             SELECT 1 FROM remote_control_trusted_devices
             WHERE trusted = 1
               AND revoked_at IS NULL
               AND last_seen_at IS NOT NULL
               AND last_seen_at >= ?1
           )"#,
        params![cutoff],
        |row| row.get::<_, i64>(0),
    )
    .map(|value| value != 0)
    .map_err(|e| format!("remote_control: 查询在线设备失败：{e}"))
}

fn remote_status(conn: &Connection) -> Result<RemoteControlStatus, String> {
    remote_status_with_bridge(conn, None)
}

fn remote_status_with_bridge(
    conn: &Connection,
    bridge_url: Option<&str>,
) -> Result<RemoteControlStatus, String> {
    let enabled = host_enabled(conn)?;
    let active_ticket = if enabled {
        active_ticket(conn)?.map(|ticket| match bridge_url {
            Some(bridge_url) => ticket_with_bridge_url(ticket, bridge_url),
            None => ticket,
        })
    } else {
        None
    };
    let endpoint = enabled.then(|| endpoint(conn)).transpose()?;
    let state = if !enabled {
        "disabled"
    } else if active_ticket.is_some() {
        "pairing"
    } else if has_recent_connected_device(conn)? {
        "connected"
    } else {
        "listening"
    };
    Ok(RemoteControlStatus {
        host_enabled: enabled,
        state: state.to_string(),
        pc_name: pc_name(conn)?,
        keep_awake_enabled: keep_awake_enabled(conn)?,
        endpoint,
        active_ticket,
        trusted_devices: trusted_devices(conn)?,
        capabilities: capabilities(),
    })
}

#[tauri::command]
pub fn remote_control_status(
    app: AppHandle,
    store: State<'_, LiliaStore>,
) -> Result<RemoteControlStatus, String> {
    let conn = store.conn()?;
    let bridge_url = if host_enabled(&conn)? {
        Some(ensure_http_bridge(app)?)
    } else {
        None
    };
    remote_status_with_bridge(&conn, bridge_url.as_deref())
}

fn ticket_with_bridge_url(
    mut ticket: RemotePairingTicket,
    bridge_url: &str,
) -> RemotePairingTicket {
    ticket.bridge_url = Some(bridge_url.to_string());
    ticket.pairing_uri = pairing_uri_with_bridge_url(&ticket.pairing_uri, bridge_url);
    ticket
}

fn pairing_uri_with_bridge_url(pairing_uri: &str, bridge_url: &str) -> String {
    let encoded_bridge = url_encode(bridge_url);
    match pairing_uri.split_once('?') {
        Some((base, query)) => {
            let mut parts = query
                .split('&')
                .filter(|part| {
                    !part.is_empty()
                        && part
                            .split_once('=')
                            .map(|(key, _)| key != "bridge")
                            .unwrap_or(*part != "bridge")
                })
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            parts.push(format!("bridge={encoded_bridge}"));
            format!("{base}?{}", parts.join("&"))
        }
        None => format!("{pairing_uri}?bridge={encoded_bridge}"),
    }
}

#[tauri::command]
pub fn remote_control_set_host_enabled(
    app: AppHandle,
    enabled: bool,
    store: State<'_, LiliaStore>,
) -> Result<RemoteControlStatus, String> {
    let conn = store.conn()?;
    if enabled {
        let bridge_url = ensure_http_bridge(app)?;
        set_setting(&conn, HOST_ENABLED_KEY, "true")?;
        let _ = endpoint_id(&conn)?;
        sync_remote_wake_from_db(&conn)?;
        remote_status_with_bridge(&conn, Some(&bridge_url))
    } else {
        disable_host(&conn)?;
        let _ = endpoint_id(&conn)?;
        sync_remote_wake_from_db(&conn)?;
        remote_status_with_bridge(&conn, None)
    }
}

fn disable_host(conn: &Connection) -> Result<(), String> {
    set_setting(conn, HOST_ENABLED_KEY, "false")?;
    cancel_active_pairing_tickets(conn)
}

#[tauri::command]
pub fn remote_control_set_pc_name(
    name: String,
    store: State<'_, LiliaStore>,
) -> Result<RemoteControlStatus, String> {
    let conn = store.conn()?;
    let normalized = name.trim();
    set_setting(
        &conn,
        PC_NAME_KEY,
        if normalized.is_empty() {
            "Lilia 电脑"
        } else {
            normalized
        },
    )?;
    remote_status(&conn)
}

#[tauri::command]
pub fn remote_control_set_keep_awake_enabled(
    enabled: bool,
    store: State<'_, LiliaStore>,
) -> Result<RemoteControlStatus, String> {
    let conn = store.conn()?;
    set_setting(
        &conn,
        KEEP_AWAKE_ENABLED_KEY,
        if enabled { "true" } else { "false" },
    )?;
    sync_remote_wake_from_db(&conn)?;
    remote_status(&conn)
}

#[tauri::command]
pub fn remote_control_start_pairing(
    app: AppHandle,
    store: State<'_, LiliaStore>,
) -> Result<RemotePairingTicket, String> {
    let conn = store.conn()?;
    let bridge_url = ensure_http_bridge(app)?;
    set_setting(&conn, HOST_ENABLED_KEY, "true")?;
    let pc_name = pc_name(&conn)?;
    let endpoint = endpoint(&conn)?;
    let id = Uuid::new_v4().to_string();
    let challenge = Uuid::new_v4().to_string();
    let expires_at = now_millis() + PAIRING_TTL_MS;
    let pairing_uri = format!(
        "lilia-remote://pair?v={}&ticket={}&challenge={}&endpoint={}&name={}&bridge={}",
        PROTOCOL_VERSION,
        url_encode(&id),
        url_encode(&challenge),
        url_encode(&endpoint.endpoint_id),
        url_encode(&pc_name),
        url_encode(&bridge_url),
    );
    conn.execute(
        r#"INSERT INTO remote_control_pairing_tickets
           (id, challenge, pc_name, endpoint_id, pairing_uri, expires_at, consumed_at, created_at)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7)"#,
        params![
            id,
            challenge,
            pc_name,
            endpoint.endpoint_id,
            pairing_uri,
            expires_at,
            now_millis()
        ],
    )
    .map_err(|e| format!("remote_control: 创建配对票据失败：{e}"))?;
    let mut ticket = active_ticket(&conn)?
        .ok_or_else(|| "remote_control: 创建配对票据后读取失败".to_string())?;
    ticket.bridge_url = Some(bridge_url);
    Ok(ticket)
}

#[tauri::command]
pub fn remote_control_cancel_pairing(store: State<'_, LiliaStore>) -> Result<(), String> {
    let conn = store.conn()?;
    cancel_active_pairing_tickets(&conn)
}

fn cancel_active_pairing_tickets(conn: &Connection) -> Result<(), String> {
    conn.execute(
        "UPDATE remote_control_pairing_tickets SET consumed_at = ?1 WHERE consumed_at IS NULL",
        params![now_millis()],
    )
    .map(|_| ())
    .map_err(|e| format!("remote_control: 取消配对失败：{e}"))
}

#[tauri::command]
pub fn remote_control_pair_device(
    input: RemotePairDeviceInput,
    store: State<'_, LiliaStore>,
) -> Result<RemotePeerSummary, String> {
    let conn = store.conn()?;
    pair_device(&conn, input)
}

fn pair_device(
    conn: &Connection,
    input: RemotePairDeviceInput,
) -> Result<RemotePeerSummary, String> {
    if input.protocol_version < MIN_PROTOCOL_VERSION {
        return Err("remote_control: Android 协议版本过旧".to_string());
    }
    if !host_enabled(conn)? {
        return Err("remote_control: 远控主机未启用".to_string());
    }
    let ticket = conn
        .query_row(
            r#"SELECT id, challenge, pc_name, endpoint_id, pairing_uri, expires_at
               FROM remote_control_pairing_tickets
               WHERE id = ?1 AND consumed_at IS NULL"#,
            params![input.ticket_id.as_str()],
            ticket_from_row,
        )
        .optional()
        .map_err(|e| format!("remote_control: 查询配对票据失败：{e}"))?
        .ok_or_else(|| "remote_control: 配对票据不存在或已使用".to_string())?;
    if ticket.expires_at <= now_millis() {
        return Err("remote_control: 配对票据已过期".to_string());
    }
    if ticket.challenge != input.challenge {
        return Err("remote_control: 配对 challenge 不匹配".to_string());
    }
    let now = now_millis();
    let device_id = format!("android-{}", Uuid::new_v4());
    let display_name = input.device_name.trim();
    let display_name = if display_name.is_empty() {
        "Android 设备"
    } else {
        display_name
    };
    conn.execute(
        r#"INSERT INTO remote_control_trusted_devices
           (id, display_name, endpoint_id, protocol_version, trusted, first_paired_at, last_seen_at, revoked_at)
           VALUES (?1, ?2, ?3, ?4, 1, ?5, ?5, NULL)
           ON CONFLICT(endpoint_id) DO UPDATE SET
             display_name = excluded.display_name,
             protocol_version = excluded.protocol_version,
             trusted = 1,
             last_seen_at = excluded.last_seen_at,
             revoked_at = NULL"#,
        params![
            device_id,
            display_name,
            input.android_endpoint.endpoint_id,
            input.protocol_version,
            now
        ],
    )
    .map_err(|e| format!("remote_control: 保存 trusted device 失败：{e}"))?;
    conn.execute(
        "UPDATE remote_control_pairing_tickets SET consumed_at = ?1 WHERE id = ?2",
        params![now, input.ticket_id],
    )
    .map_err(|e| format!("remote_control: 标记配对票据失败：{e}"))?;
    let peer = peer_for_endpoint(&conn, &input.android_endpoint.endpoint_id)?
        .ok_or_else(|| "remote_control: 配对后读取设备失败".to_string())?;
    record_remote_activity_from_db(conn)?;
    Ok(peer)
}

#[tauri::command]
pub fn remote_control_revoke_device(
    device_id: String,
    store: State<'_, LiliaStore>,
) -> Result<RemoteControlStatus, String> {
    let conn = store.conn()?;
    conn.execute(
        r#"UPDATE remote_control_trusted_devices
           SET trusted = 0, revoked_at = ?1
           WHERE id = ?2"#,
        params![now_millis(), device_id],
    )
    .map_err(|e| format!("remote_control: 撤销设备失败：{e}"))?;
    sync_remote_wake_from_db(&conn)?;
    remote_status(&conn)
}

#[tauri::command]
pub fn remote_control_dispatch_request(
    app: AppHandle,
    envelope: RemoteRequestEnvelope,
) -> JsonValue {
    match dispatch_request(app, envelope.clone()) {
        Ok(payload) => response_ok(&envelope, payload),
        Err(err) => response_err(&envelope, err),
    }
}

fn dispatch_request(
    app: AppHandle,
    envelope: RemoteRequestEnvelope,
) -> Result<JsonValue, RemoteDispatchError> {
    if envelope.protocol_version < MIN_PROTOCOL_VERSION {
        return Err(RemoteDispatchError::unsupported("远控协议版本过旧"));
    }
    let Some(store) = app.try_state::<LiliaStore>() else {
        return Err(RemoteDispatchError::unavailable("Lilia store 未初始化"));
    };
    let conn = store
        .conn()
        .map_err(|err| RemoteDispatchError::unavailable(err))?;
    let request_type = envelope
        .request
        .get("type")
        .and_then(JsonValue::as_str)
        .ok_or_else(|| RemoteDispatchError::invalid("request.type 缺失"))?;
    ensure_host_accepts_request(&conn, request_type)?;
    if authorize_envelope(&conn, &envelope)? {
        record_remote_activity_from_db(&conn).map_err(RemoteDispatchError::internal)?;
    }
    match request_type {
        "connection.capabilities.read" => Ok(json!({
            "type": "connection.capabilities",
            "capabilities": capabilities(),
        })),
        "connection.resume" => {
            let peer = refresh_trusted_peer_seen(&conn, &envelope.device_id)
                .map_err(RemoteDispatchError::internal)?;
            if peer.is_some() {
                record_remote_activity_from_db(&conn).map_err(RemoteDispatchError::internal)?;
            } else {
                sync_remote_wake_from_db(&conn).map_err(RemoteDispatchError::internal)?;
            }
            Ok(json!({
                "type": "connection.resume",
                "accepted": peer.is_some(),
                "peer": peer,
            }))
        }
        "tasks.list" => Ok(json!({
            "type": "tasks.list",
            "tasks": list_sidebar_conversations(&conn, request_limit(&envelope.request, 80))?,
        })),
        "tasks.get" => {
            let task_id = string_field(&envelope.request, "taskId")?;
            let task = load_task(&conn, &task_id)?;
            let chat_store = app.state::<chat::state::ChatStore>();
            let runtime = chat::state::chat_runtime_snapshot_with_persisted(
                Some(&conn),
                &chat_store,
                &task_id,
            );
            Ok(json!({
                "type": "tasks.get",
                "task": task,
                "runtime": runtime,
            }))
        }
        "timeline.snapshot" => {
            let task_id = string_field(&envelope.request, "taskId")?;
            let direction = envelope
                .request
                .get("direction")
                .and_then(JsonValue::as_str)
                .unwrap_or("all");
            let limit = envelope
                .request
                .get("limit")
                .and_then(JsonValue::as_i64)
                .filter(|value| *value > 0)
                .map(|value| value.min(500) as usize);
            if let Some(limit) = limit {
                let page = match direction {
                    "before" => {
                        let cursor = envelope
                            .request
                            .get("cursor")
                            .and_then(JsonValue::as_str)
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .ok_or_else(|| RemoteDispatchError::invalid("timeline cursor 缺失"))?;
                        crate::native_agent::product_timeline_page_before(&task_id, cursor, limit)
                            .map_err(RemoteDispatchError::internal)?
                            .map(Ok)
                            .unwrap_or_else(|| {
                                agent_timeline::list_before_page(&conn, &task_id, cursor, limit)
                            })
                    }
                    _ => crate::native_agent::latest_product_timeline_page(&task_id, limit)
                        .map_err(RemoteDispatchError::internal)?
                        .map(Ok)
                        .unwrap_or_else(|| {
                            agent_timeline::list_latest_page(&conn, &task_id, limit)
                        }),
                }
                .map_err(RemoteDispatchError::internal)?;
                return Ok(json!({
                    "type": request_type,
                    "taskId": task_id,
                    "events": page.events,
                    "page": {
                        "beforeCursor": page.before_cursor,
                        "afterCursor": page.after_cursor,
                        "hasMoreBefore": page.has_more_before,
                        "hasMoreAfter": page.has_more_after,
                    },
                }));
            }
            let events = match crate::native_agent::list_default_timeline_for_task(&task_id)
                .map_err(RemoteDispatchError::internal)?
            {
                Some(events) => events,
                None => {
                    agent_timeline::list(&conn, &task_id).map_err(RemoteDispatchError::internal)?
                }
            };
            Ok(json!({
                "type": request_type,
                "taskId": task_id,
                "events": events,
            }))
        }
        "timeline.subscribe" => {
            let task_id = string_field(&envelope.request, "taskId")?;
            let limit = envelope
                .request
                .get("limit")
                .and_then(JsonValue::as_i64)
                .filter(|value| *value > 0)
                .map(|value| value.min(500) as usize);
            let events = if let Some(after_cursor) = envelope
                .request
                .get("afterCursor")
                .and_then(JsonValue::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                match crate::native_agent::product_timeline_after(&task_id, after_cursor, limit)
                    .map_err(RemoteDispatchError::internal)?
                {
                    Some(events) => events,
                    None => agent_timeline::list_after_cursor(&conn, &task_id, after_cursor, limit)
                        .map_err(RemoteDispatchError::internal)?,
                }
            } else if let Some(after_event_id) = envelope
                .request
                .get("afterEventId")
                .and_then(JsonValue::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                match crate::native_agent::product_timeline_after(&task_id, after_event_id, limit)
                    .map_err(RemoteDispatchError::internal)?
                {
                    Some(events) => events,
                    None => {
                        agent_timeline::list_after_event(&conn, &task_id, after_event_id, limit)
                            .map_err(RemoteDispatchError::internal)?
                    }
                }
            } else {
                match crate::native_agent::list_default_timeline_for_task(&task_id)
                    .map_err(RemoteDispatchError::internal)?
                {
                    Some(events) => events,
                    None => agent_timeline::list(&conn, &task_id)
                        .map_err(RemoteDispatchError::internal)?,
                }
            };
            Ok(json!({
                "type": request_type,
                "taskId": task_id,
                "events": events,
            }))
        }
        "agent.session.open" => {
            let task_id = string_field(&envelope.request, "taskId")?;
            let session = crate::native_agent::open_product_agent_wire_session(&app, &task_id)
                .map_err(RemoteDispatchError::unavailable)?;
            let project_cwd = load_task_project_cwd(&conn, &task_id).unwrap_or_default();
            let generation = u64::try_from(now_millis()).unwrap_or_default();
            let workspace = json!({
                "workspace_id": format!("lilia.task:{task_id}"),
                "folders": if project_cwd.is_empty() {
                    Vec::<String>::new()
                } else {
                    vec![project_cwd]
                },
                "metadata": {
                    "productTaskId": task_id,
                    "source": "lilia-android-remote",
                },
            });
            Ok(json!({
                "type": "agent.session.open",
                "taskId": task_id,
                "session": session,
                "context": {
                    "workspace": workspace,
                    "editorContext": {
                        "snapshot_id": format!("lilia:{task_id}:remote:{generation}"),
                        "workspace": workspace,
                        "generation": generation,
                        "active_document": null,
                        "documents": [],
                        "supports_workspace_edit_preview": false,
                        "supports_workspace_edit_apply": false,
                    },
                },
            }))
        }
        "agent.wire" => {
            let request = envelope
                .request
                .get("envelope")
                .cloned()
                .ok_or_else(|| RemoteDispatchError::invalid("agent wire envelope 缺失"))?;
            let request = serde_json::from_value(request)
                .map_err(|error| RemoteDispatchError::invalid(error.to_string()))?;
            let response = crate::native_agent::dispatch_agent_wire(request).map_err(|error| {
                RemoteDispatchError {
                    code: "agentWire",
                    message: format!("{}: {}", error.code, error.message),
                    retryable: error.retryable,
                }
            })?;
            Ok(json!({
                "type": "agent.wire",
                "envelope": response,
            }))
        }
        "chat.send" => {
            let task_id = string_field(&envelope.request, "taskId")?;
            let content = string_field(&envelope.request, "content")?;
            let composer: chat::types::ChatComposerState =
                parse_optional_field(&envelope.request, "composer")?
                    .unwrap_or_else(|| chat::state::default_composer(&task_id));
            let project_cwd = envelope
                .request
                .get("projectCwd")
                .and_then(JsonValue::as_str)
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| load_task_project_cwd(&conn, &task_id).unwrap_or_default());
            let attachments =
                parse_optional_field(&envelope.request, "attachments")?.unwrap_or_default();
            let conversation_references =
                parse_optional_field(&envelope.request, "conversationReferences")?
                    .unwrap_or_default();
            let workflow = parse_optional_field(&envelope.request, "workflow")?;
            let runtime_command = parse_optional_field(&envelope.request, "runtimeCommand")?;
            let runtime_options = parse_optional_field(&envelope.request, "runtimeOptions")?;
            let guide_id = envelope
                .request
                .get("guideId")
                .and_then(JsonValue::as_str)
                .map(ToString::to_string);
            let chat_store = app.state::<chat::state::ChatStore>();
            if let Some(command) = runtime_command.clone() {
                if is_process_session_control_command(&command) {
                    chat::commands::chat_send_process_session_command(task_id, command, chat_store)
                        .map_err(RemoteDispatchError::unavailable)?;
                    return Ok(json!({
                        "type": "chat.send",
                        "result": { "accepted": true },
                    }));
                }
            }
            let result = chat::commands::chat_send_message(
                app.clone(),
                task_id,
                content,
                composer,
                project_cwd,
                attachments,
                conversation_references,
                guide_id,
                workflow,
                runtime_command,
                runtime_options,
                chat_store,
            )
            .map_err(RemoteDispatchError::unavailable)?;
            Ok(json!({ "type": "chat.send", "result": result }))
        }
        "chat.interrupt" => {
            let task_id = string_field(&envelope.request, "taskId")?;
            let chat_store = app.state::<chat::state::ChatStore>();
            let result = chat::commands::chat_interrupt_turn(task_id, app.clone(), chat_store)
                .map_err(RemoteDispatchError::unavailable)?;
            Ok(json!({ "type": "chat.interrupt", "result": result }))
        }
        "chat.retry" => {
            let task_id = string_field(&envelope.request, "taskId")?;
            let events =
                agent_timeline::list(&conn, &task_id).map_err(RemoteDispatchError::internal)?;
            let event_id = envelope
                .request
                .get("eventId")
                .and_then(JsonValue::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty());
            let retry_context = match event_id {
                Some(event_id) => retry_context_for_timeline_event_id(&events, event_id),
                None => latest_retry_context(&events),
            }
            .ok_or_else(|| RemoteDispatchError::conflict("没有可重试的远控消息"))?;
            let project_cwd = load_task_project_cwd(&conn, &task_id).unwrap_or_default();
            let chat_store = app.state::<chat::state::ChatStore>();
            let composer = {
                let composers = chat_store.composers.lock().unwrap();
                composers
                    .get(&task_id)
                    .cloned()
                    .unwrap_or_else(|| chat::state::default_composer(&task_id))
            };
            let result = chat::commands::chat_send_message(
                app.clone(),
                task_id,
                retry_context.content,
                composer,
                project_cwd,
                retry_context.attachments,
                retry_context.conversation_references,
                None,
                None,
                None,
                None,
                chat_store,
            )
            .map_err(RemoteDispatchError::unavailable)?;
            Ok(json!({ "type": "chat.retry", "result": result }))
        }
        "interaction.pending.read" => {
            let task_id = envelope.request.get("taskId").and_then(JsonValue::as_str);
            let mut interactions = Vec::new();
            if let Some(task_id) = task_id {
                let native = crate::native_agent::list_product_pending_for_task(task_id)
                    .map_err(RemoteDispatchError::internal)?;
                interactions.extend(native.into_iter().filter_map(|pending| {
                    if pending.status != lilia_contracts::PendingProjectionStatus::Open {
                        return None;
                    }
                    let provider_context = pending
                        .payload
                        .get("providerContext")
                        .cloned()
                        .unwrap_or(JsonValue::Null);
                    let tool = pending
                        .payload
                        .get("tool")
                        .cloned()
                        .unwrap_or(JsonValue::Null);
                    let side_effect = pending
                        .payload
                        .get("sideEffect")
                        .cloned()
                        .unwrap_or(JsonValue::Null);
                    Some(json!({
                        "taskId": pending.task_id.as_str(),
                        "turnId": pending.turn_id,
                        "backend": BACKEND_NATIVE_AGENTKIT,
                        "requestId": pending.request_id,
                        "kind": pending.kind,
                        "payload": {
                            "title": "Native 审批",
                            "body": pending.prompt,
                            "toolName": tool,
                            "requestedAccess": {
                                "tool": tool,
                                "sideEffect": side_effect,
                            },
                            "scopeSuggestion": "turn",
                            "providerContext": provider_context,
                        },
                    }))
                }));
            }
            Ok(json!({
                "type": "interaction.pending",
                "interactions": interactions,
            }))
        }
        "interaction.respond" => {
            let response = envelope
                .request
                .get("response")
                .cloned()
                .ok_or_else(|| RemoteDispatchError::invalid("interaction response 缺失"))?;
            let task_id = string_field(&response, "taskId")?;
            let request_id = string_field(&response, "requestId")?;
            let kind = string_field(&response, "kind")?;
            let result = response
                .get("result")
                .cloned()
                .ok_or_else(|| RemoteDispatchError::invalid("interaction result 缺失"))?;
            if kind == "permission_approval" {
                let pending = crate::native_agent::list_product_pending_for_task(&task_id)
                    .map_err(RemoteDispatchError::internal)?
                    .into_iter()
                    .find(|pending| {
                        pending.request_id == request_id
                            && pending.status == lilia_contracts::PendingProjectionStatus::Open
                    });
                if let Some(pending) = pending {
                    let approved = result
                        .get("action")
                        .and_then(JsonValue::as_str)
                        .is_some_and(|action| action == "approve");
                    let turn_id = pending.turn_id.ok_or_else(|| {
                        RemoteDispatchError::invalid("native approval turnId 缺失")
                    })?;
                    let version = pending.action_revision.ok_or_else(|| {
                        RemoteDispatchError::invalid("native approval version 缺失")
                    })?;
                    crate::native_agent::respond_product_agent_approval(
                        lilia_contracts::ProductApprovalDecision {
                            session_id: pending.agent_session.as_str().to_string(),
                            turn_id,
                            action_id: pending.request_id,
                            version,
                            approved,
                        },
                    )
                    .map_err(RemoteDispatchError::unavailable)?;
                    return Ok(json!({
                        "type": "interaction.respond",
                        "accepted": true,
                        "backend": BACKEND_NATIVE_AGENTKIT,
                    }));
                }
            }
            let chat_store = app.state::<chat::state::ChatStore>();
            chat::commands::chat_respond_agent_interaction(
                task_id,
                request_id.clone(),
                kind,
                result,
                app.clone(),
                chat_store,
            )
            .map_err(RemoteDispatchError::unavailable)?;
            Ok(json!({ "type": "interaction.respond", "accepted": true }))
        }
        "provider.status.read" => {
            let backend = crate::provider::load_active_backend(&app);
            Ok(json!({
                "type": "provider.status",
                "backend": backend,
                "ready": crate::provider::validate_backend_ready_for_send(&backend).is_ok(),
            }))
        }
        _ => Err(RemoteDispatchError::unsupported(format!(
            "不支持的远控请求：{request_type}"
        ))),
    }
}

fn is_process_session_control_command(command: &chat::types::ChatRuntimeCommand) -> bool {
    matches!(
        command,
        chat::types::ChatRuntimeCommand::ProcessSession { action, .. } if action != "spawn"
    )
}

fn authorize_envelope(
    conn: &Connection,
    envelope: &RemoteRequestEnvelope,
) -> Result<bool, RemoteDispatchError> {
    if envelope
        .request
        .get("type")
        .and_then(JsonValue::as_str)
        .is_some_and(|ty| ty.starts_with("connection."))
    {
        return Ok(false);
    }
    let trusted = conn
        .query_row(
            r#"SELECT trusted FROM remote_control_trusted_devices
               WHERE endpoint_id = ?1 AND revoked_at IS NULL"#,
            params![envelope.device_id.as_str()],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|e| RemoteDispatchError::internal(format!("读取 trusted device 失败：{e}")))?;
    if trusted == Some(1) {
        conn.execute(
            "UPDATE remote_control_trusted_devices SET last_seen_at = ?1 WHERE endpoint_id = ?2",
            params![now_millis(), envelope.device_id.as_str()],
        )
        .map_err(|e| RemoteDispatchError::internal(format!("更新设备时间失败：{e}")))?;
        Ok(true)
    } else {
        Err(RemoteDispatchError {
            code: "unauthorized",
            message: "Android 设备未配对或已撤销".to_string(),
            retryable: false,
        })
    }
}

fn peer_for_endpoint(
    conn: &Connection,
    endpoint_id: &str,
) -> Result<Option<RemotePeerSummary>, String> {
    conn.query_row(
        r#"SELECT id, display_name, endpoint_id, protocol_version, trusted,
                  first_paired_at, last_seen_at, revoked_at
           FROM remote_control_trusted_devices WHERE endpoint_id = ?1"#,
        params![endpoint_id],
        |row| {
            Ok(RemotePeerSummary {
                id: row.get(0)?,
                kind: "android".to_string(),
                display_name: row.get(1)?,
                endpoint_id: row.get(2)?,
                protocol_version: row.get(3)?,
                trusted: row.get::<_, i64>(4)? != 0,
                first_paired_at: row.get(5)?,
                last_seen_at: row.get(6)?,
                revoked_at: row.get(7)?,
            })
        },
    )
    .optional()
    .map_err(|e| format!("remote_control: 查询 peer 失败：{e}"))
}

fn refresh_trusted_peer_seen(
    conn: &Connection,
    endpoint_id: &str,
) -> Result<Option<RemotePeerSummary>, String> {
    conn.execute(
        r#"UPDATE remote_control_trusted_devices
           SET last_seen_at = ?1
           WHERE endpoint_id = ?2 AND trusted = 1 AND revoked_at IS NULL"#,
        params![now_millis(), endpoint_id],
    )
    .map_err(|e| format!("remote_control: 更新设备时间失败：{e}"))?;
    conn.query_row(
        r#"SELECT id, display_name, endpoint_id, protocol_version, trusted,
                  first_paired_at, last_seen_at, revoked_at
           FROM remote_control_trusted_devices
           WHERE endpoint_id = ?1 AND trusted = 1 AND revoked_at IS NULL"#,
        params![endpoint_id],
        |row| {
            Ok(RemotePeerSummary {
                id: row.get(0)?,
                kind: "android".to_string(),
                display_name: row.get(1)?,
                endpoint_id: row.get(2)?,
                protocol_version: row.get(3)?,
                trusted: row.get::<_, i64>(4)? != 0,
                first_paired_at: row.get(5)?,
                last_seen_at: row.get(6)?,
                revoked_at: row.get(7)?,
            })
        },
    )
    .optional()
    .map_err(|e| format!("remote_control: 查询 trusted peer 失败：{e}"))
}

fn list_sidebar_conversations(
    conn: &Connection,
    limit: i64,
) -> Result<Vec<SidebarConversationSummaryRow>, RemoteDispatchError> {
    let deps = load_task_deps(conn, None)?;
    let mut stmt = conn
        .prepare(
            r#"SELECT
                   t.id,
                   t.project_id,
                   p.name,
                   t.title,
                   t.status,
                   t.created_at,
                   t.pinned
               FROM tasks t
               LEFT JOIN projects p ON p.id = t.project_id
               WHERE t.archived = 0
               ORDER BY
                 CASE t.status
                   WHEN 'running' THEN 0
                   WHEN 'blocked' THEN 1
                   WHEN 'waiting' THEN 2
                   ELSE 3
                 END,
                 t.pinned DESC,
                 t.created_at DESC
               LIMIT ?1"#,
        )
        .map_err(|e| RemoteDispatchError::internal(format!("读取任务列表失败：{e}")))?;
    let rows = stmt
        .query_map(params![limit], |row| {
            let task_id = row.get::<_, String>(0)?;
            let project_id = row.get::<_, Option<String>>(1)?;
            let project_name = row.get::<_, Option<String>>(2)?;
            let status = row.get::<_, String>(4)?;
            let depends_on = deps.get(&task_id).cloned().unwrap_or_default();
            let route = match project_id.as_deref() {
                Some(project_id) => format!("/projects/{project_id}/tasks/{task_id}"),
                None => format!("/chats/{task_id}"),
            };
            Ok(SidebarConversationSummaryRow {
                task_id,
                project_id,
                project_name,
                title: row.get(3)?,
                status,
                depends_on,
                created_at: row.get(5)?,
                pinned: row.get::<_, i64>(6)? != 0,
                route,
            })
        })
        .map_err(|e| RemoteDispatchError::internal(format!("读取任务列表失败：{e}")))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| RemoteDispatchError::internal(format!("读取任务行失败：{e}")))?);
    }
    Ok(out)
}

fn load_task(conn: &Connection, id: &str) -> Result<Option<TaskRow>, RemoteDispatchError> {
    let deps = load_task_deps(conn, Some(id))?;
    conn.query_row(
        r#"SELECT id, project_id, session_id, title, title_source, status, created_at,
                  parent_id, sort_order, pinned
           FROM tasks WHERE id = ?1 AND archived = 0"#,
        params![id],
        |row| {
            Ok(TaskRow {
                id: row.get(0)?,
                project_id: row.get(1)?,
                session_id: row.get(2)?,
                title: row.get(3)?,
                title_source: row.get(4)?,
                status: row.get(5)?,
                created_at: row.get(6)?,
                parent_id: row.get(7)?,
                depends_on: deps.get(id).cloned().unwrap_or_default(),
                sort_order: row.get(8)?,
                pinned: row.get::<_, i64>(9)? != 0,
            })
        },
    )
    .optional()
    .map_err(|e| RemoteDispatchError::internal(format!("读取任务失败：{e}")))
}

fn load_task_deps(
    conn: &Connection,
    task_id: Option<&str>,
) -> Result<HashMap<String, Vec<String>>, RemoteDispatchError> {
    let mut stmt = conn
        .prepare(
            r#"SELECT d.task_id, d.depends_on_id
               FROM task_dependencies d
               INNER JOIN tasks t ON t.id = d.task_id
               WHERE t.archived = 0
                 AND (?1 IS NULL OR d.task_id = ?1)"#,
        )
        .map_err(|e| RemoteDispatchError::internal(format!("读取任务依赖失败：{e}")))?;
    let rows = stmt
        .query_map(params![task_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| RemoteDispatchError::internal(format!("读取任务依赖失败：{e}")))?;
    collect_task_deps(rows)
}

fn collect_task_deps<I>(rows: I) -> Result<HashMap<String, Vec<String>>, RemoteDispatchError>
where
    I: IntoIterator<Item = rusqlite::Result<(String, String)>>,
{
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    for row in rows {
        let (task_id, depends_on_id) =
            row.map_err(|e| RemoteDispatchError::internal(format!("读取任务依赖失败：{e}")))?;
        out.entry(task_id).or_default().push(depends_on_id);
    }
    Ok(out)
}

fn load_task_project_cwd(conn: &Connection, task_id: &str) -> Result<String, RemoteDispatchError> {
    conn.query_row(
        r#"SELECT COALESCE(p.cwd, '')
           FROM tasks t
           LEFT JOIN projects p ON p.id = t.project_id
           WHERE t.id = ?1"#,
        params![task_id],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map_err(|e| RemoteDispatchError::internal(format!("读取任务项目路径失败：{e}")))
    .map(|value| value.unwrap_or_default())
}

fn request_limit(request: &JsonValue, default_limit: i64) -> i64 {
    request
        .get("limit")
        .and_then(JsonValue::as_i64)
        .filter(|limit| *limit > 0)
        .map(|limit| limit.min(200))
        .unwrap_or(default_limit)
}

fn string_field(value: &JsonValue, key: &str) -> Result<String, RemoteDispatchError> {
    value
        .get(key)
        .and_then(JsonValue::as_str)
        .map(ToString::to_string)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| RemoteDispatchError::invalid(format!("{key} 缺失")))
}

fn parse_optional_field<T: for<'de> Deserialize<'de>>(
    value: &JsonValue,
    key: &str,
) -> Result<Option<T>, RemoteDispatchError> {
    match value.get(key) {
        None | Some(JsonValue::Null) => Ok(None),
        Some(raw) => serde_json::from_value(raw.clone())
            .map(Some)
            .map_err(|e| RemoteDispatchError::invalid(format!("{key} 格式错误：{e}"))),
    }
}

#[derive(Debug, Clone)]
struct RemoteTimelineRetryContext {
    content: String,
    attachments: Vec<chat::types::ChatAttachment>,
    conversation_references: Vec<chat::types::ChatConversationReference>,
}

fn latest_retry_context(
    events: &[agent_timeline::AgentTimelineEvent],
) -> Option<RemoteTimelineRetryContext> {
    events
        .iter()
        .rev()
        .find_map(|event| retry_context_for_timeline_event(event, events))
}

fn retry_context_for_timeline_event_id(
    events: &[agent_timeline::AgentTimelineEvent],
    event_id: &str,
) -> Option<RemoteTimelineRetryContext> {
    let event = events.iter().find(|event| event.id == event_id)?;
    retry_context_for_timeline_event(event, events)
}

fn retry_context_for_timeline_event(
    event: &agent_timeline::AgentTimelineEvent,
    events: &[agent_timeline::AgentTimelineEvent],
) -> Option<RemoteTimelineRetryContext> {
    if event.kind != "error" {
        return None;
    }
    if let Some(embedded) = read_timeline_retry_context(event.payload.get("retryContext")) {
        return Some(embedded);
    }
    let turn_id = event.turn_id.as_deref()?;
    let source = events.iter().find(|candidate| {
        candidate.kind == "message"
            && candidate.turn_id.as_deref() == Some(turn_id)
            && candidate.payload.get("role").and_then(JsonValue::as_str) == Some("user")
    })?;
    let content = source
        .payload
        .get("content")
        .and_then(JsonValue::as_str)
        .or(source.summary.as_deref())
        .unwrap_or_default();
    read_timeline_retry_context_from_parts(
        content,
        source.payload.get("attachments"),
        source.payload.get("conversationReferences"),
    )
}

fn read_timeline_retry_context(value: Option<&JsonValue>) -> Option<RemoteTimelineRetryContext> {
    let payload = value?.as_object()?;
    let content = payload
        .get("content")
        .and_then(JsonValue::as_str)
        .unwrap_or_default();
    read_timeline_retry_context_from_parts(
        content,
        payload.get("attachments"),
        payload.get("conversationReferences"),
    )
}

fn read_timeline_retry_context_from_parts(
    content: &str,
    attachments: Option<&JsonValue>,
    conversation_references: Option<&JsonValue>,
) -> Option<RemoteTimelineRetryContext> {
    let attachments = parse_retry_context_array(attachments);
    let conversation_references = parse_retry_context_array(conversation_references);
    if content.trim().is_empty() && attachments.is_empty() && conversation_references.is_empty() {
        return None;
    }
    Some(RemoteTimelineRetryContext {
        content: content.to_string(),
        attachments,
        conversation_references,
    })
}

fn parse_retry_context_array<T: for<'de> Deserialize<'de>>(value: Option<&JsonValue>) -> Vec<T> {
    value
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

#[derive(Debug)]
struct RemoteDispatchError {
    code: &'static str,
    message: String,
    retryable: bool,
}

impl RemoteDispatchError {
    fn invalid(message: impl Into<String>) -> Self {
        Self {
            code: "invalidRequest",
            message: message.into(),
            retryable: false,
        }
    }

    fn unsupported(message: impl Into<String>) -> Self {
        Self {
            code: "unsupported",
            message: message.into(),
            retryable: false,
        }
    }

    fn conflict(message: impl Into<String>) -> Self {
        Self {
            code: "conflict",
            message: message.into(),
            retryable: false,
        }
    }

    fn unavailable(message: impl Into<String>) -> Self {
        Self {
            code: "unavailable",
            message: message.into(),
            retryable: true,
        }
    }

    fn disabled(message: impl Into<String>) -> Self {
        Self {
            code: "unavailable",
            message: message.into(),
            retryable: false,
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            code: "internal",
            message: message.into(),
            retryable: false,
        }
    }
}

fn ensure_host_accepts_request(
    conn: &Connection,
    request_type: &str,
) -> Result<(), RemoteDispatchError> {
    if request_type == "connection.capabilities.read" {
        return Ok(());
    }
    if host_enabled(conn).map_err(RemoteDispatchError::internal)? {
        Ok(())
    } else {
        Err(RemoteDispatchError::disabled("远控主机未启用"))
    }
}

fn response_ok(envelope: &RemoteRequestEnvelope, payload: JsonValue) -> JsonValue {
    json!({
        "id": format!("remote-response-{}", Uuid::new_v4()),
        "requestId": envelope.id,
        "protocolVersion": PROTOCOL_VERSION,
        "sentAt": now_millis(),
        "ok": true,
        "payload": payload,
    })
}

fn response_err(envelope: &RemoteRequestEnvelope, err: RemoteDispatchError) -> JsonValue {
    json!({
        "id": format!("remote-response-{}", Uuid::new_v4()),
        "requestId": envelope.id,
        "protocolVersion": PROTOCOL_VERSION,
        "sentAt": now_millis(),
        "ok": false,
        "error": {
            "code": err.code,
            "message": err.message,
            "retryable": err.retryable,
        },
    })
}

pub(crate) fn restore_http_bridge_if_enabled(app: &AppHandle) {
    let Some(store) = app.try_state::<LiliaStore>() else {
        return;
    };
    let Ok(conn) = store.conn() else {
        return;
    };
    match host_enabled(&conn) {
        Ok(true) => {
            if let Err(err) = ensure_http_bridge(app.clone()) {
                eprintln!("[remote-control] restore HTTP bridge failed: {err}");
            }
            if let Err(err) = sync_remote_wake_from_db(&conn) {
                eprintln!("[remote-control] restore wake state failed: {err}");
            }
        }
        Ok(false) => set_remote_wake(false, None),
        Err(err) => eprintln!("[remote-control] read host enabled failed: {err}"),
    }
}

fn ensure_http_bridge(app: AppHandle) -> Result<String, String> {
    let lock = HTTP_BRIDGE.get_or_init(|| Mutex::new(None));
    let mut bridge = lock
        .lock()
        .map_err(|_| "remote_control: HTTP bridge lock poisoned".to_string())?;
    if let Some(existing) = bridge.as_ref() {
        return Ok(advertised_bridge_url(existing.port));
    }

    let listener = TcpListener::bind(("0.0.0.0", DEFAULT_HTTP_BRIDGE_PORT))
        .or_else(|_| TcpListener::bind("0.0.0.0:0"))
        .map_err(|e| format!("remote_control: 启动 HTTP bridge 失败：{e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("remote_control: 读取 HTTP bridge 地址失败：{e}"))?
        .port();
    let url = advertised_bridge_url(port);
    let thread_app = app.clone();
    thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let app = thread_app.clone();
                    thread::spawn(move || handle_http_stream(app, stream));
                }
                Err(err) => {
                    eprintln!("[remote-control] HTTP bridge accept failed: {err}");
                    break;
                }
            }
        }
    });
    *bridge = Some(RemoteHttpBridge { port });
    Ok(url)
}

fn handle_http_stream(app: AppHandle, mut stream: TcpStream) {
    let response = match read_http_request(&mut stream) {
        Ok(request) => handle_http_request(app, request),
        Err(err) => http_json_response(400, http_error_payload("invalidRequest", err, false)),
    };
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

struct HttpRequest {
    method: String,
    path: String,
    body: Vec<u8>,
}

fn read_http_request(stream: &mut TcpStream) -> Result<HttpRequest, String> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    let header_end;
    loop {
        let read = stream
            .read(&mut chunk)
            .map_err(|e| format!("读取请求失败：{e}"))?;
        if read == 0 {
            return Err("连接已关闭".to_string());
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(index) = find_header_end(&buffer) {
            header_end = index;
            break;
        }
        if buffer.len() > 64 * 1024 {
            return Err("请求头过大".to_string());
        }
    }
    let header_text = String::from_utf8_lossy(&buffer[..header_end]);
    let mut lines = header_text.lines();
    let request_line = lines.next().ok_or_else(|| "请求行缺失".to_string())?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();
    if method.is_empty() || path.is_empty() {
        return Err("请求行无效".to_string());
    }
    let content_length = lines
        .filter_map(|line| line.split_once(':'))
        .find_map(|(name, value)| {
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    let body_start = header_end + 4;
    while buffer.len() < body_start + content_length {
        let read = stream
            .read(&mut chunk)
            .map_err(|e| format!("读取请求体失败：{e}"))?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
    let body = buffer
        .get(body_start..body_start.saturating_add(content_length))
        .unwrap_or_default()
        .to_vec();
    Ok(HttpRequest { method, path, body })
}

fn handle_http_request(app: AppHandle, request: HttpRequest) -> String {
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/status") => {
            let Some(store) = app.try_state::<LiliaStore>() else {
                return http_json_response(
                    503,
                    http_error_payload("unavailable", "存储不可用", true),
                );
            };
            match store.conn().and_then(|conn| remote_status(&conn)) {
                Ok(status) => http_json_response(200, json!({ "ok": true, "status": status })),
                Err(err) => http_json_response(500, http_error_payload("internal", err, false)),
            }
        }
        ("POST", "/pair") => {
            let input = match serde_json::from_slice::<RemotePairDeviceInput>(&request.body) {
                Ok(input) => input,
                Err(err) => {
                    return http_json_response(
                        400,
                        http_error_payload("invalidRequest", err.to_string(), false),
                    )
                }
            };
            let Some(store) = app.try_state::<LiliaStore>() else {
                return http_json_response(
                    503,
                    http_error_payload("unavailable", "存储不可用", true),
                );
            };
            match store.conn().and_then(|conn| pair_device(&conn, input)) {
                Ok(peer) => http_json_response(200, json!({ "ok": true, "peer": peer })),
                Err(err) => {
                    http_json_response(403, http_error_payload("invalidRequest", err, false))
                }
            }
        }
        ("POST", "/dispatch") => {
            let envelope = match serde_json::from_slice::<RemoteRequestEnvelope>(&request.body) {
                Ok(envelope) => envelope,
                Err(err) => {
                    return http_json_response(
                        400,
                        http_error_payload("invalidRequest", err.to_string(), false),
                    )
                }
            };
            let response = match dispatch_request(app, envelope.clone()) {
                Ok(payload) => response_ok(&envelope, payload),
                Err(err) => response_err(&envelope, err),
            };
            http_json_response(200, response)
        }
        _ => http_json_response(404, http_error_payload("unsupported", "未知路由", false)),
    }
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn http_json_response(status: u16, payload: JsonValue) -> String {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "OK",
    };
    let body = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string());
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nAccess-Control-Allow-Origin: *\r\n\r\n{body}",
        body.as_bytes().len(),
    )
}

fn http_error_payload(
    code: &'static str,
    message: impl Into<String>,
    retryable: bool,
) -> JsonValue {
    json!({
        "ok": false,
        "error": {
            "code": code,
            "message": message.into(),
            "retryable": retryable,
        },
    })
}

fn local_lan_ip() -> String {
    UdpSocket::bind("0.0.0.0:0")
        .and_then(|socket| {
            let _ = socket.connect("8.8.8.8:80");
            socket.local_addr()
        })
        .map(|addr| addr.ip().to_string())
        .unwrap_or_else(|_| "127.0.0.1".to_string())
}

fn advertised_bridge_url(port: u16) -> String {
    bridge_url_for_host(&local_lan_ip(), port)
}

fn bridge_url_for_host(host: &str, port: u16) -> String {
    format!("http://{host}:{port}")
}

fn url_encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn bridge_url_from_pairing_uri(pairing_uri: &str) -> Option<String> {
    let query = pairing_uri.split_once('?')?.1;
    for part in query.split('&') {
        let (key, value) = part.split_once('=')?;
        if key == "bridge" {
            return Some(url_decode(value));
        }
    }
    None
}

fn url_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = &value[index + 1..index + 3];
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
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
            "#,
        )
        .unwrap();
        conn
    }

    fn insert_ticket(conn: &Connection) {
        conn.execute(
            r#"INSERT INTO remote_control_pairing_tickets
               (id, challenge, pc_name, endpoint_id, pairing_uri, expires_at, consumed_at, created_at)
               VALUES ('ticket-1', 'challenge-1', 'PC', 'pc-endpoint',
                       'lilia-remote://pair?v=1&ticket=ticket-1&bridge=http%3A%2F%2F192.168.1.12%3A41478', ?1, NULL, ?2)"#,
            params![now_millis() + 60_000, now_millis()],
        )
        .unwrap();
    }

    fn enable_host(conn: &Connection) {
        set_setting(conn, HOST_ENABLED_KEY, "true").unwrap();
    }

    fn insert_trusted_device(conn: &Connection, endpoint_id: &str, trusted: bool, revoked: bool) {
        insert_trusted_device_with_seen_at(conn, endpoint_id, trusted, revoked, None);
    }

    fn insert_trusted_device_with_seen_at(
        conn: &Connection,
        endpoint_id: &str,
        trusted: bool,
        revoked: bool,
        last_seen_at: Option<i64>,
    ) {
        conn.execute(
            r#"INSERT INTO remote_control_trusted_devices
               (id, display_name, endpoint_id, protocol_version, trusted, first_paired_at, last_seen_at, revoked_at)
               VALUES (?1, 'Pixel', ?2, 1, ?3, ?4, ?5, ?6)"#,
            params![
                format!("device-{endpoint_id}"),
                endpoint_id,
                if trusted { 1 } else { 0 },
                now_millis(),
                last_seen_at,
                revoked.then(now_millis)
            ],
        )
        .unwrap();
    }

    #[test]
    fn keep_awake_enabled_defaults_to_true_and_persists_false() {
        let conn = conn();

        assert!(keep_awake_enabled(&conn).unwrap());

        set_setting(&conn, KEEP_AWAKE_ENABLED_KEY, "false").unwrap();
        assert!(!keep_awake_enabled(&conn).unwrap());

        set_setting(&conn, KEEP_AWAKE_ENABLED_KEY, "true").unwrap();
        assert!(keep_awake_enabled(&conn).unwrap());
    }

    #[test]
    fn remote_wake_config_requires_host_and_keep_awake_setting() {
        let conn = conn();

        assert!(!remote_wake_configured(&conn).unwrap());

        enable_host(&conn);
        assert!(remote_wake_configured(&conn).unwrap());

        set_setting(&conn, KEEP_AWAKE_ENABLED_KEY, "false").unwrap();
        assert!(!remote_wake_configured(&conn).unwrap());
    }

    #[test]
    fn remote_wake_runtime_activity_enters_active_window() {
        let mut runtime = RemoteWakeRuntime::new(true);

        runtime.set_target(true, Some(1_000 + RECENT_ANDROID_SEEN_MS));

        assert!(runtime.should_keep_awake(1_000));
        assert!(runtime.should_keep_awake(1_000 + RECENT_ANDROID_SEEN_MS - 1));
        assert!(!runtime.should_keep_awake(1_000 + RECENT_ANDROID_SEEN_MS));
    }

    #[test]
    fn remote_wake_runtime_repeated_activity_extends_window() {
        let mut runtime = RemoteWakeRuntime::new(true);

        runtime.set_target(true, Some(1_000 + RECENT_ANDROID_SEEN_MS));
        runtime.set_target(true, Some(5_000 + RECENT_ANDROID_SEEN_MS));

        assert!(runtime.should_keep_awake(5_000 + RECENT_ANDROID_SEEN_MS - 1));
        assert!(!runtime.should_keep_awake(5_000 + RECENT_ANDROID_SEEN_MS));
    }

    #[test]
    fn remote_wake_runtime_expire_and_disable_release_activity() {
        let mut runtime = RemoteWakeRuntime::new(true);
        runtime.set_target(true, Some(1_000 + RECENT_ANDROID_SEEN_MS));

        runtime.expire(1_000 + RECENT_ANDROID_SEEN_MS);

        assert!(runtime.active_until_ms.is_none());
        assert!(!runtime.should_keep_awake(1_000 + RECENT_ANDROID_SEEN_MS));

        runtime.set_target(true, Some(5_000 + RECENT_ANDROID_SEEN_MS));
        runtime.set_target(false, Some(5_000 + RECENT_ANDROID_SEEN_MS));

        assert!(runtime.active_until_ms.is_none());
        assert!(!runtime.should_keep_awake(5_001));
    }

    #[test]
    fn http_error_payload_is_structured_for_android() {
        let payload = http_error_payload("invalidRequest", "bad JSON", false);

        assert_eq!(payload["ok"], false);
        assert_eq!(payload["error"]["code"], "invalidRequest");
        assert_eq!(payload["error"]["message"], "bad JSON");
        assert_eq!(payload["error"]["retryable"], false);
    }

    #[test]
    fn dispatch_response_envelopes_match_android_contract() {
        let envelope = RemoteRequestEnvelope {
            id: "request-1".to_string(),
            protocol_version: 1,
            device_id: "android-endpoint".to_string(),
            request: json!({ "type": "connection.resume" }),
        };

        let success = response_ok(
            &envelope,
            json!({ "type": "connection.resume", "accepted": true }),
        );
        assert_eq!(success["requestId"], "request-1");
        assert_eq!(success["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(success["ok"], true);
        assert_eq!(success["payload"]["type"], "connection.resume");
        assert_eq!(success["payload"]["accepted"], true);
        assert!(success.get("request_id").is_none());

        let failure = response_err(
            &envelope,
            RemoteDispatchError::unsupported("unsupported request"),
        );
        assert_eq!(failure["requestId"], "request-1");
        assert_eq!(failure["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(failure["ok"], false);
        assert_eq!(failure["error"]["code"], "unsupported");
        assert_eq!(failure["error"]["message"], "unsupported request");
        assert_eq!(failure["error"]["retryable"], false);
    }

    #[test]
    fn pair_device_consumes_ticket_and_trusts_endpoint() {
        let conn = conn();
        enable_host(&conn);
        insert_ticket(&conn);

        let peer = pair_device(
            &conn,
            RemotePairDeviceInput {
                ticket_id: "ticket-1".to_string(),
                challenge: "challenge-1".to_string(),
                device_name: "Pixel".to_string(),
                android_endpoint: RemoteEndpointInput {
                    endpoint_id: "android-endpoint".to_string(),
                },
                protocol_version: 1,
            },
        )
        .unwrap();

        assert_eq!(peer.display_name, "Pixel");
        assert_eq!(peer.endpoint_id, "android-endpoint");
        assert!(peer.trusted);
        assert_eq!(
            remote_status_with_bridge(&conn, Some("http://192.168.1.12:41478"))
                .unwrap()
                .state,
            "connected"
        );
        let consumed: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM remote_control_pairing_tickets WHERE id = 'ticket-1' AND consumed_at IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(consumed, 1);
    }

    #[test]
    fn pair_device_rejects_wrong_challenge() {
        let conn = conn();
        enable_host(&conn);
        insert_ticket(&conn);

        let err = pair_device(
            &conn,
            RemotePairDeviceInput {
                ticket_id: "ticket-1".to_string(),
                challenge: "wrong".to_string(),
                device_name: "Pixel".to_string(),
                android_endpoint: RemoteEndpointInput {
                    endpoint_id: "android-endpoint".to_string(),
                },
                protocol_version: 1,
            },
        )
        .unwrap_err();

        assert!(err.contains("challenge"));
    }

    #[test]
    fn pair_device_rejects_when_host_disabled() {
        let conn = conn();
        insert_ticket(&conn);

        let err = pair_device(
            &conn,
            RemotePairDeviceInput {
                ticket_id: "ticket-1".to_string(),
                challenge: "challenge-1".to_string(),
                device_name: "Pixel".to_string(),
                android_endpoint: RemoteEndpointInput {
                    endpoint_id: "android-endpoint".to_string(),
                },
                protocol_version: 1,
            },
        )
        .unwrap_err();

        assert!(err.contains("未启用"));
    }

    #[test]
    fn remote_status_returns_connected_for_recent_trusted_device() {
        let conn = conn();
        enable_host(&conn);
        insert_trusted_device_with_seen_at(
            &conn,
            "android-recent",
            true,
            false,
            Some(now_millis()),
        );

        let status = remote_status_with_bridge(&conn, Some("http://192.168.1.12:41478")).unwrap();

        assert_eq!(status.state, "connected");
    }

    #[test]
    fn remote_status_returns_listening_for_stale_trusted_device() {
        let conn = conn();
        enable_host(&conn);
        insert_trusted_device_with_seen_at(
            &conn,
            "android-stale",
            true,
            false,
            Some(now_millis() - RECENT_ANDROID_SEEN_MS - 1_000),
        );

        let status = remote_status_with_bridge(&conn, Some("http://192.168.1.12:41478")).unwrap();

        assert_eq!(status.state, "listening");
    }

    #[test]
    fn remote_status_ignores_revoked_and_untrusted_recent_devices() {
        let conn = conn();
        enable_host(&conn);
        insert_trusted_device_with_seen_at(
            &conn,
            "android-revoked",
            true,
            true,
            Some(now_millis()),
        );
        insert_trusted_device_with_seen_at(
            &conn,
            "android-untrusted",
            false,
            false,
            Some(now_millis()),
        );

        let status = remote_status_with_bridge(&conn, Some("http://192.168.1.12:41478")).unwrap();

        assert_eq!(status.state, "listening");
    }

    #[test]
    fn disable_host_consumes_active_pairing_tickets() {
        let conn = conn();
        enable_host(&conn);
        insert_ticket(&conn);

        disable_host(&conn).unwrap();

        let status = remote_status_with_bridge(&conn, Some("http://192.168.1.12:41478")).unwrap();
        assert_eq!(status.state, "disabled");
        assert!(status.active_ticket.is_none());
        let unconsumed: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM remote_control_pairing_tickets WHERE consumed_at IS NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(unconsumed, 0);
    }

    #[test]
    fn disabled_status_hides_legacy_active_pairing_ticket() {
        let conn = conn();
        insert_ticket(&conn);

        let status = remote_status_with_bridge(&conn, Some("http://192.168.1.12:41478")).unwrap();

        assert_eq!(status.state, "disabled");
        assert!(status.active_ticket.is_none());
    }

    #[test]
    fn authorize_envelope_requires_trusted_endpoint_for_non_connection_requests() {
        let conn = conn();
        let envelope = RemoteRequestEnvelope {
            id: "request-1".to_string(),
            protocol_version: 1,
            device_id: "unknown-endpoint".to_string(),
            request: json!({ "type": "tasks.list" }),
        };

        let err = authorize_envelope(&conn, &envelope).unwrap_err();
        assert_eq!(err.code, "unauthorized");

        conn.execute(
            r#"INSERT INTO remote_control_trusted_devices
               (id, display_name, endpoint_id, protocol_version, trusted, first_paired_at, last_seen_at, revoked_at)
               VALUES ('device-1', 'Pixel', 'unknown-endpoint', 1, 1, ?1, NULL, NULL)"#,
            params![now_millis()],
        )
        .unwrap();
        assert!(authorize_envelope(&conn, &envelope).unwrap());
        assert!(!authorize_envelope(
            &conn,
            &RemoteRequestEnvelope {
                id: "request-2".to_string(),
                protocol_version: 1,
                device_id: "unknown-endpoint".to_string(),
                request: json!({ "type": "connection.resume" }),
            },
        )
        .unwrap());
    }

    #[test]
    fn refresh_trusted_peer_seen_accepts_only_active_trusted_devices() {
        let conn = conn();
        insert_trusted_device(&conn, "android-trusted", true, false);
        insert_trusted_device(&conn, "android-revoked", true, true);
        insert_trusted_device(&conn, "android-untrusted", false, false);

        let peer = refresh_trusted_peer_seen(&conn, "android-trusted")
            .unwrap()
            .unwrap();
        assert_eq!(peer.endpoint_id, "android-trusted");
        assert_eq!(peer.trusted, true);
        assert!(peer.revoked_at.is_none());
        assert!(peer.last_seen_at.is_some());

        assert!(refresh_trusted_peer_seen(&conn, "android-revoked")
            .unwrap()
            .is_none());
        assert!(refresh_trusted_peer_seen(&conn, "android-untrusted")
            .unwrap()
            .is_none());
        assert!(refresh_trusted_peer_seen(&conn, "android-unknown")
            .unwrap()
            .is_none());
    }

    #[test]
    fn host_gate_rejects_control_requests_when_disabled() {
        let conn = conn();

        let err = ensure_host_accepts_request(&conn, "tasks.list").unwrap_err();
        assert_eq!(err.code, "unavailable");
        assert!(!err.retryable);

        ensure_host_accepts_request(&conn, "connection.capabilities.read").unwrap();
        enable_host(&conn);
        ensure_host_accepts_request(&conn, "tasks.list").unwrap();
    }

    #[test]
    fn remote_process_session_controls_reuse_chat_send_runtime_command() {
        let write_stdin: chat::types::ChatRuntimeCommand = serde_json::from_value(json!({
            "type": "process_session",
            "action": "write_stdin",
            "stdin": "q"
        }))
        .unwrap();
        let kill: chat::types::ChatRuntimeCommand = serde_json::from_value(json!({
            "type": "process_session",
            "action": "kill"
        }))
        .unwrap();
        let spawn: chat::types::ChatRuntimeCommand = serde_json::from_value(json!({
            "type": "process_session",
            "action": "spawn",
            "command": "npm test"
        }))
        .unwrap();

        assert!(is_process_session_control_command(&write_stdin));
        assert!(is_process_session_control_command(&kill));
        assert!(!is_process_session_control_command(&spawn));
    }

    #[test]
    fn remote_status_serializes_android_bridge_contract_as_camel_case() {
        let conn = conn();
        enable_host(&conn);

        let status = remote_status_with_bridge(&conn, Some("http://192.168.1.12:41478")).unwrap();
        let wire = serde_json::to_value(status).unwrap();

        assert_eq!(wire["hostEnabled"], true);
        assert_eq!(wire["state"], "listening");
        assert_eq!(wire["pcName"], "Lilia 电脑");
        assert_eq!(wire["keepAwakeEnabled"], true);
        assert!(wire["endpoint"]["endpointId"]
            .as_str()
            .unwrap()
            .starts_with("pc-"));
        assert_eq!(wire["capabilities"]["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(
            wire["capabilities"]["minProtocolVersion"],
            MIN_PROTOCOL_VERSION
        );
        assert_eq!(wire["capabilities"]["supportsPairing"], true);
        assert_eq!(wire["capabilities"]["supportsTaskInbox"], true);
        assert_eq!(wire["capabilities"]["supportsTimelinePagination"], true);
        assert_eq!(wire["capabilities"]["supportsChatSend"], true);
        assert_eq!(wire["capabilities"]["supportsInteractionResponse"], true);
        assert_eq!(wire["capabilities"]["supportsInterrupt"], true);
        assert!(wire.get("host_enabled").is_none());
        assert!(wire.get("keep_awake_enabled").is_none());
        assert!(wire["capabilities"].get("supports_task_inbox").is_none());
    }

    #[test]
    fn latest_retry_context_prefers_embedded_error_context() {
        let events = vec![timeline_event(
            "error-1",
            "error",
            None,
            None,
            json!({
                "retryContext": {
                    "content": "Retry this",
                    "attachments": [{
                        "id": "att-1",
                        "name": "file.rs",
                        "path": "C:/repo/file.rs",
                        "kind": "file",
                        "size": null
                    }],
                    "conversationReferences": []
                }
            }),
        )];

        let context = latest_retry_context(&events).unwrap();

        assert_eq!(context.content, "Retry this");
        assert_eq!(context.attachments.len(), 1);
        assert_eq!(context.attachments[0].path, "C:/repo/file.rs");
    }

    #[test]
    fn latest_retry_context_falls_back_to_same_turn_user_message() {
        let events = vec![
            timeline_event(
                "message-1",
                "message",
                Some("turn-1"),
                Some("Original prompt"),
                json!({
                    "role": "user",
                    "content": "Original prompt",
                    "attachments": [],
                    "conversationReferences": [{
                        "taskId": "task-other",
                        "title": "Other task",
                        "route": "/chats/task-other"
                    }]
                }),
            ),
            timeline_event(
                "error-1",
                "error",
                Some("turn-1"),
                Some("Failed"),
                json!({ "message": "Failed" }),
            ),
        ];

        let context = latest_retry_context(&events).unwrap();

        assert_eq!(context.content, "Original prompt");
        assert_eq!(context.conversation_references.len(), 1);
        assert_eq!(context.conversation_references[0].task_id, "task-other");
    }

    #[test]
    fn latest_retry_context_ignores_unlocatable_errors() {
        let events = vec![timeline_event(
            "error-1",
            "error",
            None,
            Some("Failed"),
            json!({ "message": "Failed" }),
        )];

        assert!(latest_retry_context(&events).is_none());
    }

    #[test]
    fn retry_context_for_timeline_event_id_selects_requested_error_without_fallback() {
        let events = vec![
            timeline_event(
                "message-1",
                "message",
                Some("turn-1"),
                Some("Original prompt"),
                json!({ "role": "user", "content": "Original prompt" }),
            ),
            timeline_event(
                "error-1",
                "error",
                Some("turn-1"),
                Some("Old failure"),
                json!({ "message": "Old failure" }),
            ),
            timeline_event(
                "error-2",
                "error",
                None,
                Some("New failure"),
                json!({ "retryContext": { "content": "Retry new" } }),
            ),
        ];

        let selected = retry_context_for_timeline_event_id(&events, "error-1").unwrap();

        assert_eq!(selected.content, "Original prompt");
        assert_eq!(latest_retry_context(&events).unwrap().content, "Retry new");
        assert!(retry_context_for_timeline_event_id(&events, "missing").is_none());
        assert!(retry_context_for_timeline_event_id(&events, "message-1").is_none());
    }

    #[test]
    fn active_ticket_restores_bridge_url_from_pairing_uri() {
        let conn = conn();
        insert_ticket(&conn);

        let ticket = active_ticket(&conn).unwrap().unwrap();

        assert_eq!(
            ticket.bridge_url.as_deref(),
            Some("http://192.168.1.12:41478")
        );
    }

    fn timeline_event(
        id: &str,
        kind: &str,
        turn_id: Option<&str>,
        summary: Option<&str>,
        payload: JsonValue,
    ) -> agent_timeline::AgentTimelineEvent {
        agent_timeline::AgentTimelineEvent {
            id: id.to_string(),
            task_id: "task-1".to_string(),
            turn_id: turn_id.map(ToString::to_string),
            backend: "codex".to_string(),
            kind: kind.to_string(),
            status: if kind == "error" { "error" } else { "success" }.to_string(),
            title: kind.to_string(),
            summary: summary.map(ToString::to_string),
            payload,
            created_at: now_millis(),
            updated_at: now_millis(),
            turn_seq: 0,
            intra_turn_order: 0,
        }
    }

    #[test]
    fn remote_status_refreshes_active_ticket_bridge_url() {
        let conn = conn();
        enable_host(&conn);
        insert_ticket(&conn);

        let status = remote_status_with_bridge(&conn, Some("http://192.168.1.99:41478")).unwrap();
        let ticket = status.active_ticket.unwrap();

        assert_eq!(
            ticket.bridge_url.as_deref(),
            Some("http://192.168.1.99:41478")
        );
        assert_eq!(
            bridge_url_from_pairing_uri(&ticket.pairing_uri).as_deref(),
            Some("http://192.168.1.99:41478")
        );
    }

    #[test]
    fn list_sidebar_conversations_includes_task_status() {
        let conn = conn();
        conn.execute_batch(
            r#"
            CREATE TABLE projects (
              id   TEXT PRIMARY KEY,
              name TEXT NOT NULL
            );
            CREATE TABLE tasks (
              id         TEXT PRIMARY KEY,
              project_id TEXT,
              title      TEXT NOT NULL,
              status     TEXT NOT NULL,
              created_at INTEGER NOT NULL,
              pinned     INTEGER NOT NULL,
              archived   INTEGER NOT NULL
            );
            CREATE TABLE task_dependencies (
              task_id       TEXT NOT NULL,
              depends_on_id TEXT NOT NULL
            );
            INSERT INTO projects (id, name) VALUES ('project-1', 'Lilia');
            INSERT INTO tasks (id, project_id, title, status, created_at, pinned, archived)
            VALUES ('task-1', 'project-1', 'Android remote', 'running', 1710000000000, 0, 0);
            INSERT INTO task_dependencies (task_id, depends_on_id)
            VALUES ('task-1', 'task-0');
            "#,
        )
        .unwrap();

        let rows = list_sidebar_conversations(&conn, 80).unwrap();

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].task_id, "task-1");
        assert_eq!(rows[0].project_name.as_deref(), Some("Lilia"));
        assert_eq!(rows[0].status, "running");
        assert_eq!(rows[0].depends_on, vec!["task-0".to_string()]);

        let wire = serde_json::to_value(&rows[0]).unwrap();
        assert_eq!(wire["taskId"], "task-1");
        assert_eq!(wire["projectName"], "Lilia");
        assert_eq!(wire["dependsOn"][0], "task-0");
        assert_eq!(wire["createdAt"], 1710000000000i64);
        assert!(wire.get("task_id").is_none());
        assert!(wire.get("project_name").is_none());
        assert!(wire.get("depends_on").is_none());
    }

    #[test]
    fn task_row_serializes_remote_detail_contract_as_camel_case() {
        let task = TaskRow {
            id: "task-1".to_string(),
            project_id: Some("project-1".to_string()),
            session_id: "session-1".to_string(),
            title: "Android remote".to_string(),
            title_source: "manual".to_string(),
            status: "running".to_string(),
            created_at: 1710000000000,
            parent_id: None,
            depends_on: vec!["task-0".to_string()],
            sort_order: 7,
            pinned: true,
        };

        let wire = serde_json::to_value(&task).unwrap();

        assert_eq!(wire["id"], "task-1");
        assert_eq!(wire["projectId"], "project-1");
        assert_eq!(wire["sessionId"], "session-1");
        assert_eq!(wire["createdAt"], 1710000000000i64);
        assert_eq!(wire["parentId"], JsonValue::Null);
        assert_eq!(wire["dependsOn"][0], "task-0");
        assert!(wire.get("project_id").is_none());
        assert!(wire.get("created_at").is_none());
        assert!(wire.get("depends_on").is_none());
    }

    #[test]
    fn load_task_includes_dependency_ids() {
        let conn = conn();
        conn.execute_batch(
            r#"
            CREATE TABLE tasks (
              id           TEXT PRIMARY KEY,
              project_id   TEXT,
              session_id   TEXT NOT NULL,
              title        TEXT NOT NULL,
              title_source TEXT NOT NULL DEFAULT 'manual',
              status       TEXT NOT NULL,
              created_at   INTEGER NOT NULL,
              parent_id    TEXT,
              sort_order   INTEGER NOT NULL,
              pinned       INTEGER NOT NULL,
              archived     INTEGER NOT NULL
            );
            CREATE TABLE task_dependencies (
              task_id       TEXT NOT NULL,
              depends_on_id TEXT NOT NULL
            );
            INSERT INTO tasks (
              id, project_id, session_id, title, title_source, status,
              created_at, parent_id, sort_order, pinned, archived
            )
            VALUES (
              'task-1', 'project-1', 'session-1', 'Android remote', 'manual', 'blocked',
              1710000000000, NULL, 7, 1, 0
            );
            INSERT INTO task_dependencies (task_id, depends_on_id)
            VALUES ('task-1', 'dep-1');
            "#,
        )
        .unwrap();

        let task = load_task(&conn, "task-1").unwrap().unwrap();

        assert_eq!(task.depends_on, vec!["dep-1".to_string()]);
    }

    #[test]
    fn url_encode_and_decode_round_trip_bridge_url() {
        let url = "http://192.168.1.12:41478";

        let encoded = url_encode(url);

        assert_eq!(encoded, "http%3A%2F%2F192.168.1.12%3A41478");
        assert_eq!(url_decode(&encoded), url);
    }

    #[test]
    fn pairing_uri_with_bridge_url_replaces_existing_bridge() {
        let uri = pairing_uri_with_bridge_url(
            "lilia-remote://pair?v=1&ticket=ticket-1&bridge=http%3A%2F%2F192.168.1.12%3A41478",
            "http://192.168.1.99:41478",
        );

        assert_eq!(
            uri,
            "lilia-remote://pair?v=1&ticket=ticket-1&bridge=http%3A%2F%2F192.168.1.99%3A41478"
        );
    }

    #[test]
    fn bridge_url_for_host_uses_advertised_port() {
        assert_eq!(
            bridge_url_for_host("192.168.1.12", 41478),
            "http://192.168.1.12:41478"
        );
    }
}
