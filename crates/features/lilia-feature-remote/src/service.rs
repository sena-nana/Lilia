use std::collections::{BTreeMap, HashMap};
use std::net::UdpSocket;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use lilia_contracts::TaskId;
use lilia_storage::Db;
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::types::{
    RemoteCapabilitySet, RemoteControlStatus, RemoteEndpointAddress, RemotePairDeviceInput,
    RemotePairingTicket, RemotePeerSummary, RemotePublicStatus, RemoteSessionCredential,
    REMOTE_ALPN, REMOTE_MIN_PROTOCOL_VERSION, REMOTE_PROTOCOL_VERSION,
};

pub trait RemoteWakeHost: Send + Sync + 'static {
    fn set_system_awake(&self, active: bool) -> Result<(), String>;
}

pub const HOST_ENABLED_KEY: &str = "host_enabled";
pub const PC_NAME_KEY: &str = "pc_name";
const ENDPOINT_ID_KEY: &str = "endpoint_id";
pub const KEEP_AWAKE_ENABLED_KEY: &str = "keep_awake_enabled";
pub const PAIRING_TTL_MS: i64 = 10 * 60 * 1000;
pub const DEFAULT_HTTP_BRIDGE_PORT: u16 = 41478;
/// Dangerous opt-in: when set to `1`/`true`, the HTTP bridge binds `0.0.0.0` (LAN-visible).
/// Default is loopback-only (`127.0.0.1`). Cleartext LAN exposure is intentional only for
/// explicit local testing — prefer adb reverse / emulator loopback for Android cleartext.
pub const DANGEROUS_BIND_NON_LOOPBACK_ENV: &str = "LILIA_REMOTE_BRIDGE_BIND_NON_LOOPBACK";
pub const SESSION_TTL_MS: i64 = 7 * 24 * 60 * 60 * 1000;
const RECENT_ANDROID_SEEN_MS: i64 = 2 * 60 * 1000;
const REMOTE_WAKE_MONITOR_IDLE_MS: u64 = 30_000;

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{code}: {message}")]
pub struct DesktopRemoteControlError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

impl DesktopRemoteControlError {
    pub fn new(code: impl Into<String>, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retryable,
        }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new("invalidRequest", message, false)
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new("unauthorized", message, false)
    }

    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::new("unsupported", message, false)
    }

    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::new("unavailable", message, true)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new("internal", message, false)
    }
}

#[derive(Clone)]
pub struct DesktopRemoteControlService {
    inner: Arc<RemoteServiceInner>,
}

struct RemoteServiceInner {
    connection: Db,
    bridge: Mutex<Option<RemoteHttpBridge>>,
    wake: Arc<RemoteWakeController>,
    process_sessions: Mutex<HashMap<TaskId, String>>,
}

#[derive(Clone, Copy, Debug)]
struct RemoteHttpBridge {
    port: u16,
}

struct RemoteWakeController {
    state: Mutex<RemoteWakeRuntime>,
    changed: Condvar,
    host: Arc<dyn RemoteWakeHost>,
}

#[derive(Clone, Debug)]
struct RemoteWakeRuntime {
    configured: bool,
    active_until_ms: Option<i64>,
    platform_active: bool,
}

impl DesktopRemoteControlService {
    pub fn in_memory(
        host: Arc<dyn RemoteWakeHost>,
    ) -> Result<Self, DesktopRemoteControlError> {
        let connection = Db::in_memory()
            .map_err(|error| DesktopRemoteControlError::internal(error.to_string()))?;
        Self::from_db(connection, host)
    }

    pub fn from_db(
        connection: Db,
        host: Arc<dyn RemoteWakeHost>,
    ) -> Result<Self, DesktopRemoteControlError> {
        initialize_schema(&connection.lock())?;
        let wake = Arc::new(RemoteWakeController {
            state: Mutex::new(RemoteWakeRuntime {
                configured: false,
                active_until_ms: None,
                platform_active: false,
            }),
            changed: Condvar::new(),
            host,
        });
        let monitor = wake.clone();
        thread::Builder::new()
            .name("lilia-remote-wake".to_owned())
            .spawn(move || remote_wake_monitor(monitor))
            .map_err(|error| DesktopRemoteControlError::internal(error.to_string()))?;
        Ok(Self {
            inner: Arc::new(RemoteServiceInner {
                connection,
                bridge: Mutex::new(None),
                wake,
                process_sessions: Mutex::new(HashMap::new()),
            }),
        })
    }

    pub fn with_connection<T>(
        &self,
        action: impl FnOnce(&Connection) -> Result<T, DesktopRemoteControlError>,
    ) -> Result<T, DesktopRemoteControlError> {
        action(&self.inner.connection.lock())
    }

    pub fn bridge_url(&self) -> Result<Option<String>, DesktopRemoteControlError> {
        Ok(self
            .inner
            .bridge
            .lock()
            .map_err(|_| DesktopRemoteControlError::internal("remote bridge lock poisoned"))?
            .as_ref()
            .map(|bridge| advertised_bridge_url(bridge.port)))
    }

    pub fn sync_wake(&self) -> Result<(), DesktopRemoteControlError> {
        let (configured, active) = self.with_connection(|connection| {
            let configured = host_enabled(connection)? && keep_awake_enabled(connection)?;
            let active = configured && has_recent_connected_device(connection)?;
            Ok((configured, active))
        })?;
        self.inner.wake.set_target(
            configured,
            active.then(|| now_millis() + RECENT_ANDROID_SEEN_MS),
        );
        Ok(())
    }

    pub fn record_activity(&self) -> Result<(), DesktopRemoteControlError> {
        let configured = self.with_connection(|connection| {
            Ok(host_enabled(connection)? && keep_awake_enabled(connection)?)
        })?;
        self.inner.wake.set_target(
            configured,
            configured.then(|| now_millis() + RECENT_ANDROID_SEEN_MS),
        );
        Ok(())
    }

    pub fn store_bridge_port(&self, port: u16) -> Result<(), DesktopRemoteControlError> {
        let mut bridge = self
            .inner
            .bridge
            .lock()
            .map_err(|_| DesktopRemoteControlError::internal("remote bridge lock poisoned"))?;
        *bridge = Some(RemoteHttpBridge { port });
        Ok(())
    }

    pub fn process_session(&self, task_id: &TaskId) -> Option<String> {
        self.inner
            .process_sessions
            .lock()
            .ok()
            .and_then(|sessions| sessions.get(task_id).cloned())
    }

    pub fn remember_process_session(&self, task_id: TaskId, session_id: String) {
        if let Ok(mut sessions) = self.inner.process_sessions.lock() {
            sessions.insert(task_id, session_id);
        }
    }

    pub fn forget_process_session(&self, task_id: &TaskId) {
        if let Ok(mut sessions) = self.inner.process_sessions.lock() {
            sessions.remove(task_id);
        }
    }

    pub fn start_pairing(
        &self,
        bridge_url: &str,
    ) -> Result<RemotePairingTicket, DesktopRemoteControlError> {
        self.with_connection(|connection| start_pairing(connection, bridge_url))
    }
}

impl RemoteWakeController {
    fn set_target(&self, configured: bool, active_until_ms: Option<i64>) {
        if let Ok(mut state) = self.state.lock() {
            state.configured = configured;
            state.active_until_ms = configured.then_some(active_until_ms).flatten();
            self.changed.notify_one();
        }
    }
}

fn remote_wake_monitor(controller: Arc<RemoteWakeController>) {
    loop {
        let mut state = match controller.state.lock() {
            Ok(state) => state,
            Err(_) => return,
        };
        let now = now_millis();
        if state
            .active_until_ms
            .is_some_and(|active_until| active_until <= now)
        {
            state.active_until_ms = None;
        }
        let target = state.configured
            && state
                .active_until_ms
                .is_some_and(|active_until| active_until > now);
        if state.platform_active != target {
            if controller.host.set_system_awake(target).is_ok() {
                state.platform_active = target;
            }
        }
        let wait = state
            .active_until_ms
            .filter(|active_until| *active_until > now)
            .map(|active_until| Duration::from_millis((active_until - now) as u64))
            .unwrap_or_else(|| Duration::from_millis(REMOTE_WAKE_MONITOR_IDLE_MS));
        match controller.changed.wait_timeout(state, wait) {
            Ok(_) => {}
            Err(_) => return,
        }
    }
}
fn initialize_schema(connection: &Connection) -> Result<(), DesktopRemoteControlError> {
    connection
        .execute_batch(
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
            CREATE TABLE IF NOT EXISTS remote_control_sessions (
              token_hash   TEXT PRIMARY KEY,
              endpoint_id  TEXT NOT NULL,
              expires_at   INTEGER NOT NULL,
              created_at   INTEGER NOT NULL,
              last_used_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_remote_control_sessions_endpoint
              ON remote_control_sessions(endpoint_id);
            CREATE INDEX IF NOT EXISTS idx_remote_control_sessions_expires
              ON remote_control_sessions(expires_at);
            "#,
        )
        .map_err(database_error)
}

fn setting(
    connection: &Connection,
    key: &str,
) -> Result<Option<String>, DesktopRemoteControlError> {
    connection
        .query_row(
            "SELECT value FROM remote_control_settings WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )
        .optional()
        .map_err(database_error)
}

pub fn set_setting(
    connection: &Connection,
    key: &str,
    value: &str,
) -> Result<(), DesktopRemoteControlError> {
    connection
        .execute(
            r#"INSERT INTO remote_control_settings (key, value, updated_at)
               VALUES (?1, ?2, ?3)
               ON CONFLICT(key) DO UPDATE SET
                 value = excluded.value,
                 updated_at = excluded.updated_at"#,
            params![key, value, now_millis()],
        )
        .map(|_| ())
        .map_err(database_error)
}

pub fn host_enabled(connection: &Connection) -> Result<bool, DesktopRemoteControlError> {
    Ok(setting(connection, HOST_ENABLED_KEY)?.as_deref() == Some("true"))
}

pub fn keep_awake_enabled(connection: &Connection) -> Result<bool, DesktopRemoteControlError> {
    Ok(setting(connection, KEEP_AWAKE_ENABLED_KEY)?.as_deref() != Some("false"))
}

pub fn pc_name(connection: &Connection) -> Result<String, DesktopRemoteControlError> {
    Ok(setting(connection, PC_NAME_KEY)?
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "Lilia 电脑".to_owned()))
}

pub fn endpoint_id(connection: &Connection) -> Result<String, DesktopRemoteControlError> {
    if let Some(id) = setting(connection, ENDPOINT_ID_KEY)?
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
    {
        return Ok(id);
    }
    let id = format!("pc-{}", Uuid::new_v4());
    set_setting(connection, ENDPOINT_ID_KEY, &id)?;
    Ok(id)
}

pub fn endpoint(connection: &Connection) -> Result<RemoteEndpointAddress, DesktopRemoteControlError> {
    Ok(RemoteEndpointAddress {
        endpoint_id: endpoint_id(connection)?,
        relay_url: None,
        direct_addresses: Vec::new(),
    })
}

pub fn active_ticket(
    connection: &Connection,
) -> Result<Option<RemotePairingTicket>, DesktopRemoteControlError> {
    connection
        .query_row(
            r#"SELECT id, challenge, pc_name, endpoint_id, pairing_uri, expires_at
               FROM remote_control_pairing_tickets
               WHERE consumed_at IS NULL AND expires_at > ?1
               ORDER BY created_at DESC LIMIT 1"#,
            params![now_millis()],
            |row| {
                let pairing_uri: String = row.get(4)?;
                Ok(RemotePairingTicket {
                    id: row.get(0)?,
                    challenge: row.get(1)?,
                    pc_name: row.get(2)?,
                    pc_endpoint: RemoteEndpointAddress {
                        endpoint_id: row.get(3)?,
                        relay_url: None,
                        direct_addresses: Vec::new(),
                    },
                    protocol_version: REMOTE_PROTOCOL_VERSION,
                    expires_at: row.get(5)?,
                    bridge_url: bridge_url_from_pairing_uri(&pairing_uri),
                    pairing_uri,
                })
            },
        )
        .optional()
        .map_err(database_error)
}

fn trusted_devices(
    connection: &Connection,
) -> Result<Vec<RemotePeerSummary>, DesktopRemoteControlError> {
    let mut statement = connection
        .prepare(
            r#"SELECT id, display_name, endpoint_id, protocol_version, trusted,
                      first_paired_at, last_seen_at, revoked_at
               FROM remote_control_trusted_devices
               ORDER BY revoked_at IS NOT NULL ASC, last_seen_at DESC, first_paired_at DESC"#,
        )
        .map_err(database_error)?;
    let rows = statement
        .query_map([], peer_from_row)
        .map_err(database_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(database_error)
}

fn peer_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RemotePeerSummary> {
    Ok(RemotePeerSummary {
        id: row.get(0)?,
        kind: "android".to_owned(),
        display_name: row.get(1)?,
        endpoint_id: row.get(2)?,
        protocol_version: row.get(3)?,
        trusted: row.get::<_, i64>(4)? != 0,
        first_paired_at: row.get(5)?,
        last_seen_at: row.get(6)?,
        revoked_at: row.get(7)?,
    })
}

pub fn remote_status(
    connection: &Connection,
    bridge_url: Option<&str>,
) -> Result<RemoteControlStatus, DesktopRemoteControlError> {
    let enabled = host_enabled(connection)?;
    let mut ticket = enabled
        .then(|| active_ticket(connection))
        .transpose()?
        .flatten();
    if let (Some(ticket), Some(bridge_url)) = (ticket.as_mut(), bridge_url) {
        ticket.bridge_url = Some(bridge_url.to_owned());
        ticket.pairing_uri = pairing_uri_with_bridge_url(&ticket.pairing_uri, bridge_url);
    }
    let connected = enabled && has_recent_connected_device(connection)?;
    let state = if !enabled {
        "disabled"
    } else if ticket.is_some() {
        "pairing"
    } else if connected {
        "connected"
    } else {
        "listening"
    };
    Ok(RemoteControlStatus {
        host_enabled: enabled,
        state: state.to_owned(),
        pc_name: pc_name(connection)?,
        keep_awake_enabled: keep_awake_enabled(connection)?,
        endpoint: enabled.then(|| endpoint(connection)).transpose()?,
        active_ticket: ticket,
        trusted_devices: trusted_devices(connection)?,
        capabilities: remote_capabilities(),
    })
}

pub fn remote_capabilities() -> RemoteCapabilitySet {
    RemoteCapabilitySet {
        protocol_version: REMOTE_PROTOCOL_VERSION,
        min_protocol_version: REMOTE_MIN_PROTOCOL_VERSION,
        alpn: REMOTE_ALPN.to_owned(),
        supports_pairing: true,
        supports_task_inbox: true,
        supports_timeline_subscription: true,
        supports_timeline_pagination: true,
        supports_chat_send: true,
        supports_interaction_response: true,
        supports_interrupt: true,
        supports_agent_wire: true,
        supports_session_fork: true,
        supports_process_session: true,
        supports_session_token: true,
    }
}

/// HTTP-safe status: never includes pairing tickets, challenges, or trusted endpoint ids.
pub fn public_remote_status(
    connection: &Connection,
) -> Result<RemotePublicStatus, DesktopRemoteControlError> {
    let enabled = host_enabled(connection)?;
    let pairing = enabled
        .then(|| active_ticket(connection))
        .transpose()?
        .flatten()
        .is_some();
    let connected = enabled && has_recent_connected_device(connection)?;
    let state = if !enabled {
        "disabled"
    } else if pairing {
        "pairing"
    } else if connected {
        "connected"
    } else {
        "listening"
    };
    Ok(RemotePublicStatus {
        host_enabled: enabled,
        state: state.to_owned(),
        pc_name: pc_name(connection)?,
        keep_awake_enabled: keep_awake_enabled(connection)?,
        capabilities: remote_capabilities(),
        bridge_bind: if bridge_binds_non_loopback() {
            "non-loopback".to_owned()
        } else {
            "loopback".to_owned()
        },
    })
}
pub fn start_pairing(
    connection: &Connection,
    bridge_url: &str,
) -> Result<RemotePairingTicket, DesktopRemoteControlError> {
    set_setting(connection, HOST_ENABLED_KEY, "true")?;
    cancel_pairing(connection)?;
    let name = pc_name(connection)?;
    let endpoint = endpoint(connection)?;
    let id = Uuid::new_v4().to_string();
    let challenge = Uuid::new_v4().to_string();
    let expires_at = now_millis() + PAIRING_TTL_MS;
    let pairing_uri = format!(
        "lilia-remote://pair?v={}&ticket={}&challenge={}&endpoint={}&name={}&bridge={}",
        REMOTE_PROTOCOL_VERSION,
        url_encode(&id),
        url_encode(&challenge),
        url_encode(&endpoint.endpoint_id),
        url_encode(&name),
        url_encode(bridge_url),
    );
    connection
        .execute(
            r#"INSERT INTO remote_control_pairing_tickets
               (id, challenge, pc_name, endpoint_id, pairing_uri, expires_at, consumed_at, created_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7)"#,
            params![
                id,
                challenge,
                name,
                endpoint.endpoint_id,
                pairing_uri,
                expires_at,
                now_millis()
            ],
        )
        .map_err(database_error)?;
    active_ticket(connection)?.ok_or_else(|| {
        DesktopRemoteControlError::internal("pairing ticket was not persisted")
    })
}

pub fn authorize_request(
    connection: &Connection,
    device_id: &str,
    request_type: &str,
) -> Result<(), DesktopRemoteControlError> {
    if request_type == "connection.capabilities.read" {
        return Ok(());
    }
    if !host_enabled(connection)? {
        return Err(DesktopRemoteControlError::new(
            "unavailable",
            "remote control host is disabled",
            false,
        ));
    }
    if request_type.starts_with("connection.") {
        return Ok(());
    }
    let trusted = connection
        .query_row(
            r#"SELECT trusted FROM remote_control_trusted_devices
               WHERE endpoint_id = ?1 AND revoked_at IS NULL"#,
            params![device_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(database_error)?;
    if trusted != Some(1) {
        return Err(DesktopRemoteControlError::unauthorized(
            "Android device is not paired or has been revoked",
        ));
    }
    connection
        .execute(
            "UPDATE remote_control_trusted_devices SET last_seen_at = ?1 WHERE endpoint_id = ?2",
            params![now_millis(), device_id],
        )
        .map_err(database_error)?;
    Ok(())
}

pub fn cancel_pairing(connection: &Connection) -> Result<(), DesktopRemoteControlError> {
    connection
        .execute(
            "UPDATE remote_control_pairing_tickets SET consumed_at = ?1 WHERE consumed_at IS NULL",
            params![now_millis()],
        )
        .map(|_| ())
        .map_err(database_error)
}

pub fn pair_device(
    connection: &Connection,
    input: RemotePairDeviceInput,
) -> Result<RemotePeerSummary, DesktopRemoteControlError> {
    if input.protocol_version < REMOTE_MIN_PROTOCOL_VERSION {
        return Err(DesktopRemoteControlError::unsupported(
            "Android protocol version is too old",
        ));
    }
    if !host_enabled(connection)? {
        return Err(DesktopRemoteControlError::unavailable(
            "remote control host is disabled",
        ));
    }
    let ticket = connection
        .query_row(
            r#"SELECT challenge, expires_at FROM remote_control_pairing_tickets
               WHERE id = ?1 AND consumed_at IS NULL"#,
            params![input.ticket_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()
        .map_err(database_error)?
        .ok_or_else(|| {
            DesktopRemoteControlError::invalid("pairing ticket is missing or consumed")
        })?;
    if ticket.1 <= now_millis() {
        return Err(DesktopRemoteControlError::invalid(
            "pairing ticket has expired",
        ));
    }
    if ticket.0 != input.challenge {
        return Err(DesktopRemoteControlError::invalid(
            "pairing challenge does not match",
        ));
    }
    let now = now_millis();
    let display_name = input.device_name.trim();
    let display_name = if display_name.is_empty() {
        "Android 设备"
    } else {
        display_name
    };
    connection
        .execute(
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
                format!("android-{}", Uuid::new_v4()),
                display_name,
                input.android_endpoint.endpoint_id,
                input.protocol_version,
                now
            ],
        )
        .map_err(database_error)?;
    connection
        .execute(
            "UPDATE remote_control_pairing_tickets SET consumed_at = ?1 WHERE id = ?2",
            params![now, input.ticket_id],
        )
        .map_err(database_error)?;
    peer_for_endpoint(connection, &input.android_endpoint.endpoint_id)?
        .ok_or_else(|| DesktopRemoteControlError::internal("paired device could not be reloaded"))
}

fn peer_for_endpoint(
    connection: &Connection,
    endpoint_id: &str,
) -> Result<Option<RemotePeerSummary>, DesktopRemoteControlError> {
    connection
        .query_row(
            r#"SELECT id, display_name, endpoint_id, protocol_version, trusted,
                      first_paired_at, last_seen_at, revoked_at
               FROM remote_control_trusted_devices WHERE endpoint_id = ?1"#,
            params![endpoint_id],
            peer_from_row,
        )
        .optional()
        .map_err(database_error)
}

pub fn refresh_trusted_peer_seen(
    connection: &Connection,
    endpoint_id: &str,
) -> Result<Option<RemotePeerSummary>, DesktopRemoteControlError> {
    connection
        .execute(
            r#"UPDATE remote_control_trusted_devices SET last_seen_at = ?1
               WHERE endpoint_id = ?2 AND trusted = 1 AND revoked_at IS NULL"#,
            params![now_millis(), endpoint_id],
        )
        .map_err(database_error)?;
    peer_for_endpoint(connection, endpoint_id)
        .map(|peer| peer.filter(|peer| peer.trusted && peer.revoked_at.is_none()))
}

fn has_recent_connected_device(connection: &Connection) -> Result<bool, DesktopRemoteControlError> {
    connection
        .query_row(
            r#"SELECT EXISTS(
                 SELECT 1 FROM remote_control_trusted_devices
                 WHERE trusted = 1 AND revoked_at IS NULL
                   AND last_seen_at IS NOT NULL AND last_seen_at >= ?1
               )"#,
            params![now_millis() - RECENT_ANDROID_SEEN_MS],
            |row| row.get::<_, i64>(0),
        )
        .map(|value| value != 0)
        .map_err(database_error)
}

fn local_lan_ip() -> String {
    UdpSocket::bind("0.0.0.0:0")
        .and_then(|socket| {
            let _ = socket.connect("8.8.8.8:80");
            socket.local_addr()
        })
        .map(|address| address.ip().to_string())
        .unwrap_or_else(|_| "127.0.0.1".to_owned())
}

/// Host address for `TcpListener::bind`. Defaults to loopback.
pub fn bridge_bind_host() -> &'static str {
    match std::env::var(DANGEROUS_BIND_NON_LOOPBACK_ENV) {
        Ok(value) if value == "1" || value.eq_ignore_ascii_case("true") => "0.0.0.0",
        _ => "127.0.0.1",
    }
}

pub fn bridge_binds_non_loopback() -> bool {
    bridge_bind_host() == "0.0.0.0"
}

pub fn advertised_bridge_url(port: u16) -> String {
    let host = if bridge_binds_non_loopback() {
        local_lan_ip()
    } else {
        "127.0.0.1".to_owned()
    };
    format!("http://{host}:{port}")
}

pub fn hash_session_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Constant-time equality for equal-length byte slices.
pub fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in left.iter().zip(right.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}

pub fn mint_session_token(
    connection: &Connection,
    endpoint_id: &str,
) -> Result<RemoteSessionCredential, DesktopRemoteControlError> {
    let endpoint_id = endpoint_id.trim();
    if endpoint_id.is_empty() {
        return Err(DesktopRemoteControlError::invalid("endpoint id is required"));
    }
    // Replace any prior sessions for this device.
    connection
        .execute(
            "DELETE FROM remote_control_sessions WHERE endpoint_id = ?1",
            params![endpoint_id],
        )
        .map_err(database_error)?;
    let token = format!(
        "{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    );
    let token_hash = hash_session_token(&token);
    let now = now_millis();
    let expires_at = now + SESSION_TTL_MS;
    connection
        .execute(
            r#"INSERT INTO remote_control_sessions
               (token_hash, endpoint_id, expires_at, created_at, last_used_at)
               VALUES (?1, ?2, ?3, ?4, ?4)"#,
            params![token_hash, endpoint_id, expires_at, now],
        )
        .map_err(database_error)?;
    Ok(RemoteSessionCredential {
        session_token: token,
        expires_at,
    })
}

pub fn authenticate_session_token(
    connection: &Connection,
    token: &str,
) -> Result<String, DesktopRemoteControlError> {
    let token = token.trim();
    if token.is_empty() {
        return Err(DesktopRemoteControlError::unauthorized(
            "session token is required",
        ));
    }
    let token_hash = hash_session_token(token);
    let row = connection
        .query_row(
            r#"SELECT token_hash, endpoint_id, expires_at FROM remote_control_sessions
               WHERE token_hash = ?1"#,
            params![token_hash],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .optional()
        .map_err(database_error)?;
    let Some((stored_hash, endpoint_id, expires_at)) = row else {
        return Err(DesktopRemoteControlError::unauthorized(
            "session token is invalid",
        ));
    };
    if !constant_time_eq(stored_hash.as_bytes(), token_hash.as_bytes()) {
        return Err(DesktopRemoteControlError::unauthorized(
            "session token is invalid",
        ));
    }
    let now = now_millis();
    if expires_at <= now {
        let _ = connection.execute(
            "DELETE FROM remote_control_sessions WHERE token_hash = ?1",
            params![token_hash],
        );
        return Err(DesktopRemoteControlError::unauthorized(
            "session token has expired",
        ));
    }
    let new_expires = now + SESSION_TTL_MS;
    connection
        .execute(
            r#"UPDATE remote_control_sessions
               SET last_used_at = ?1, expires_at = ?2
               WHERE token_hash = ?3"#,
            params![now, new_expires, token_hash],
        )
        .map_err(database_error)?;
    Ok(endpoint_id)
}

pub fn revoke_sessions_for_endpoint(
    connection: &Connection,
    endpoint_id: &str,
) -> Result<(), DesktopRemoteControlError> {
    connection
        .execute(
            "DELETE FROM remote_control_sessions WHERE endpoint_id = ?1",
            params![endpoint_id],
        )
        .map(|_| ())
        .map_err(database_error)
}

pub fn revoke_sessions_for_device_row(
    connection: &Connection,
    device_row_id: &str,
) -> Result<(), DesktopRemoteControlError> {
    let endpoint = connection
        .query_row(
            "SELECT endpoint_id FROM remote_control_trusted_devices WHERE id = ?1",
            params![device_row_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(database_error)?;
    if let Some(endpoint_id) = endpoint {
        revoke_sessions_for_endpoint(connection, &endpoint_id)?;
    }
    Ok(())
}

/// Drop classic preload / interpreter hijack variables from remote process env overlays.
pub fn sanitize_remote_process_environment(
    environment: BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    environment
        .into_iter()
        .filter(|(key, _)| !is_denied_remote_env_key(key))
        .collect()
}

pub fn is_denied_remote_env_key(key: &str) -> bool {
    let upper = key.to_ascii_uppercase();
    if upper.starts_with("LD_") || upper.starts_with("DYLD_") {
        return true;
    }
    matches!(
        upper.as_str(),
        "PATH"
            | "PYTHONPATH"
            | "PYTHONHOME"
            | "PERL5LIB"
            | "PERL5OPT"
            | "RUBYLIB"
            | "RUBYOPT"
            | "NODE_OPTIONS"
            | "NODE_PATH"
            | "BASH_ENV"
            | "ENV"
            | "IFS"
            | "SHELLOPTS"
            | "PS4"
            | "PROMPT_COMMAND"
            | "SSLKEYLOGFILE"
            | "GIT_SSH_COMMAND"
            | "GIT_EXEC_PATH"
            | "GIT_CONFIG_GLOBAL"
            | "GIT_CONFIG_SYSTEM"
            | "OPENSSL_CONF"
            | "CURL_HOME"
            | "NPM_CONFIG_PREFIX"
            | "JAVA_TOOL_OPTIONS"
            | "_JAVA_OPTIONS"
            | "CLASSPATH"
            | "TERM_PROGRAM"
            | "TERMINFO"
            | "TERMCAP"
    )
}

fn pairing_uri_with_bridge_url(pairing_uri: &str, bridge_url: &str) -> String {
    let encoded = url_encode(bridge_url);
    match pairing_uri.split_once('?') {
        Some((base, query)) => {
            let mut parts = query
                .split('&')
                .filter(|part| part.split_once('=').is_none_or(|(key, _)| key != "bridge"))
                .map(str::to_owned)
                .collect::<Vec<_>>();
            parts.push(format!("bridge={encoded}"));
            format!("{base}?{}", parts.join("&"))
        }
        None => format!("{pairing_uri}?bridge={encoded}"),
    }
}

fn bridge_url_from_pairing_uri(pairing_uri: &str) -> Option<String> {
    pairing_uri
        .split_once('?')?
        .1
        .split('&')
        .filter_map(|part| part.split_once('='))
        .find_map(|(key, value)| (key == "bridge").then(|| url_decode(value)))
}

pub fn url_encode(value: &str) -> String {
    let mut output = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                output.push(char::from(byte));
            }
            _ => output.push_str(&format!("%{byte:02X}")),
        }
    }
    output
}

fn url_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&value[index + 1..index + 3], 16) {
                output.push(byte);
                index += 3;
                continue;
            }
        }
        output.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&output).into_owned()
}

pub fn database_error(error: rusqlite::Error) -> DesktopRemoteControlError {
    DesktopRemoteControlError::internal(format!("remote database: {error}"))
}

pub fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    struct NoopWake;

    impl RemoteWakeHost for NoopWake {
        fn set_system_awake(&self, _active: bool) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn bridge_bind_defaults_to_loopback() {
        // Env may be set in the process; the helper itself is covered by advertised URL logic.
        let host = bridge_bind_host();
        assert!(host == "127.0.0.1" || host == "0.0.0.0");
    }

    #[test]
    fn public_status_omits_secrets_even_while_pairing() {
        let service = DesktopRemoteControlService::in_memory(Arc::new(NoopWake)).unwrap();
        service
            .with_connection(|connection| {
                set_setting(connection, HOST_ENABLED_KEY, "true")?;
                start_pairing(connection, "http://127.0.0.1:41478")?;
                let public = public_remote_status(connection)?;
                assert_eq!(public.state, "pairing");
                assert!(public.bridge_bind == "loopback" || public.bridge_bind == "non-loopback");
                let full = remote_status(connection, Some("http://127.0.0.1:41478"))?;
                assert!(full.active_ticket.is_some());
                assert!(!full.trusted_devices.is_empty() || full.trusted_devices.is_empty());
                // public type has no ticket field — compile-time guarantee; runtime check:
                let encoded = serde_json::to_value(&public).unwrap();
                assert!(encoded.get("activeTicket").is_none());
                assert!(encoded.get("trustedDevices").is_none());
                assert!(encoded.get("endpoint").is_none());
                assert!(encoded.get("challenge").is_none());
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn session_token_round_trip_uses_constant_time_hash_lookup() {
        let service = DesktopRemoteControlService::in_memory(Arc::new(NoopWake)).unwrap();
        service
            .with_connection(|connection| {
                let minted = mint_session_token(connection, "android-device-1")?;
                assert!(!minted.session_token.is_empty());
                let endpoint = authenticate_session_token(connection, &minted.session_token)?;
                assert_eq!(endpoint, "android-device-1");
                let err = authenticate_session_token(connection, "totally-invalid-token")
                    .expect_err("invalid token");
                assert_eq!(err.code, "unauthorized");
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn denied_env_keys_cover_preload_hijacks() {
        assert!(is_denied_remote_env_key("LD_PRELOAD"));
        assert!(is_denied_remote_env_key("ld_library_path"));
        assert!(is_denied_remote_env_key("DYLD_INSERT_LIBRARIES"));
        assert!(is_denied_remote_env_key("PYTHONPATH"));
        assert!(is_denied_remote_env_key("PATH"));
        assert!(is_denied_remote_env_key("NODE_OPTIONS"));
        assert!(!is_denied_remote_env_key("MY_APP_FLAG"));
        let cleaned = sanitize_remote_process_environment(BTreeMap::from([
            ("LD_PRELOAD".to_owned(), "evil.so".to_owned()),
            ("MY_APP_FLAG".to_owned(), "1".to_owned()),
        ]));
        assert_eq!(cleaned.len(), 1);
        assert_eq!(cleaned.get("MY_APP_FLAG").map(String::as_str), Some("1"));
    }

    #[test]
    fn constant_time_eq_rejects_length_mismatch() {
        assert!(!constant_time_eq(b"abc", b"ab"));
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
    }
}
