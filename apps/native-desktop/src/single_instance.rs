use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock, Weak};
use std::time::{Duration, Instant};

use fs2::FileExt;
use lilia_desktop_application::{DesktopCliRequest, DesktopCliResult};
use serde::{Deserialize, Serialize};

const PROTOCOL: &str = "liliacode-native-single-instance";
const PROTOCOL_VERSION: u32 = 1;
#[cfg(not(test))]
const DESCRIPTOR_WAIT: Duration = Duration::from_secs(15);
#[cfg(test)]
const DESCRIPTOR_WAIT: Duration = Duration::from_millis(500);
const HANDLER_WAIT: Duration = Duration::from_secs(15);
const IO_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_MESSAGE_BYTES: usize = 256 * 1024;

type RequestHandler = dyn Fn(DesktopCliRequest) -> DesktopCliResult + Send + Sync;

pub enum InstanceDisposition {
    Primary(Arc<NativeInstanceCoordinator>),
    Forwarded(DesktopCliResult),
}

pub struct NativeInstanceCoordinator {
    state: Arc<CoordinatorState>,
    lock_file: File,
    descriptor_path: PathBuf,
    descriptor: InstanceDescriptor,
    listener: Mutex<Option<TcpListener>>,
    token: String,
}

struct CoordinatorState {
    running: AtomicBool,
    handler: Mutex<Option<Arc<RequestHandler>>>,
    handler_ready: Condvar,
    initial_request: Mutex<Option<DesktopCliRequest>>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstanceDescriptor {
    protocol: String,
    version: u32,
    instance_identity: String,
    address: String,
    token: String,
    process_id: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IpcRequest {
    protocol: String,
    version: u32,
    token: String,
    request: DesktopCliRequest,
}

static CURRENT: OnceLock<Mutex<Weak<NativeInstanceCoordinator>>> = OnceLock::new();

pub fn acquire(
    home: &Path,
    instance_identity: &str,
    request: DesktopCliRequest,
) -> Result<InstanceDisposition, String> {
    let runtime_dir = home.join("run");
    fs::create_dir_all(&runtime_dir).map_err(|error| {
        format!(
            "failed to create Native instance directory {}: {error}",
            runtime_dir.display()
        )
    })?;
    let lock_path = runtime_dir.join("instance.lock");
    let descriptor_path = runtime_dir.join("instance.json");
    let lock_file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|error| format!("failed to open {}: {error}", lock_path.display()))?;
    match lock_file.try_lock_exclusive() {
        Ok(()) => start_primary(
            lock_file,
            descriptor_path,
            instance_identity.to_owned(),
            request,
        ),
        Err(lock_error) => match forward_to_primary(
            &descriptor_path,
            instance_identity,
            request.clone(),
            lock_error,
        ) {
            Ok(result) => Ok(InstanceDisposition::Forwarded(result)),
            Err(first_error) => match lock_file.try_lock_exclusive() {
                Ok(()) => start_primary(
                    lock_file,
                    descriptor_path,
                    instance_identity.to_owned(),
                    request,
                ),
                Err(recovery_lock_error) => forward_to_primary(
                    &descriptor_path,
                    instance_identity,
                    request,
                    recovery_lock_error,
                )
                .map(InstanceDisposition::Forwarded)
                .map_err(|retry_error| {
                    format!(
                        "Native Preview primary changed while forwarding the CLI request: {first_error}; retry failed: {retry_error}"
                    )
                }),
            },
        },
    }
}

pub fn install_handler(
    handler: impl Fn(DesktopCliRequest) -> DesktopCliResult + Send + Sync + 'static,
) -> Result<(), String> {
    let coordinator = CURRENT
        .get()
        .and_then(|current| current.lock().ok())
        .and_then(|current| current.upgrade())
        .ok_or_else(|| "Native single-instance coordinator is unavailable".to_owned())?;
    coordinator.install_handler(Arc::new(handler))
}

fn start_primary(
    lock_file: File,
    descriptor_path: PathBuf,
    instance_identity: String,
    request: DesktopCliRequest,
) -> Result<InstanceDisposition, String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|error| format!("failed to bind Native instance IPC: {error}"))?;
    let token = uuid::Uuid::new_v4().to_string();
    let descriptor = InstanceDescriptor {
        protocol: PROTOCOL.to_owned(),
        version: PROTOCOL_VERSION,
        instance_identity,
        address: listener
            .local_addr()
            .map_err(|error| format!("failed to read Native instance address: {error}"))?
            .to_string(),
        token: token.clone(),
        process_id: std::process::id(),
    };
    let coordinator = Arc::new(NativeInstanceCoordinator {
        state: Arc::new(CoordinatorState {
            running: AtomicBool::new(true),
            handler: Mutex::new(None),
            handler_ready: Condvar::new(),
            initial_request: Mutex::new(Some(request)),
        }),
        lock_file,
        descriptor_path,
        descriptor,
        listener: Mutex::new(Some(listener)),
        token,
    });
    let current = CURRENT.get_or_init(|| Mutex::new(Weak::new()));
    *current
        .lock()
        .map_err(|_| "Native instance registry is unavailable".to_owned())? =
        Arc::downgrade(&coordinator);
    Ok(InstanceDisposition::Primary(coordinator))
}

impl NativeInstanceCoordinator {
    fn install_handler(&self, handler: Arc<RequestHandler>) -> Result<(), String> {
        self.publish()?;
        if let Ok(mut slot) = self.state.handler.lock() {
            *slot = Some(handler.clone());
            self.state.handler_ready.notify_all();
        }
        if let Some(request) = self
            .state
            .initial_request
            .lock()
            .ok()
            .and_then(|mut request| request.take())
        {
            let _ = handler(request);
        }
        Ok(())
    }

    fn publish(&self) -> Result<(), String> {
        let mut listener = self
            .listener
            .lock()
            .map_err(|_| "Native instance listener is unavailable".to_owned())?;
        let Some(pending) = listener.as_ref() else {
            return Ok(());
        };
        let server = pending
            .try_clone()
            .map_err(|error| format!("failed to clone Native instance IPC: {error}"))?;
        write_descriptor(&self.descriptor_path, &self.descriptor)?;
        let weak = Arc::downgrade(&self.state);
        if let Err(error) = std::thread::Builder::new()
            .name("lilia-native-instance-ipc".to_owned())
            .spawn(move || listen(server, weak))
        {
            let _ = fs::remove_file(&self.descriptor_path);
            return Err(format!("failed to start Native instance IPC: {error}"));
        }
        listener.take();
        Ok(())
    }
}

impl Drop for NativeInstanceCoordinator {
    fn drop(&mut self) {
        self.state.running.store(false, Ordering::Release);
        self.state.handler_ready.notify_all();
        let _ = TcpStream::connect(&self.descriptor.address);
        let owns_descriptor = fs::read(&self.descriptor_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<InstanceDescriptor>(&bytes).ok())
            .is_some_and(|descriptor| descriptor.token == self.token);
        if owns_descriptor {
            let _ = fs::remove_file(&self.descriptor_path);
        }
        let _ = FileExt::unlock(&self.lock_file);
    }
}

fn listen(listener: TcpListener, state: Weak<CoordinatorState>) {
    loop {
        let Some(state) = state.upgrade() else {
            return;
        };
        if !state.running.load(Ordering::Acquire) {
            return;
        }
        match listener.accept() {
            Ok((stream, _)) => {
                let state = Arc::clone(&state);
                let _ = std::thread::Builder::new()
                    .name("lilia-native-instance-request".to_owned())
                    .spawn(move || handle_stream(stream, &state));
            }
            Err(_) => return,
        }
    }
}

fn handle_stream(mut stream: TcpStream, state: &CoordinatorState) {
    let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
    let result = read_json_line::<IpcRequest>(&mut stream)
        .and_then(validate_ipc_request)
        .and_then(|request| wait_for_handler(state).map(|handler| handler(request)))
        .unwrap_or_else(cli_failure);
    let _ = write_json_line(&mut stream, &result);
}

fn validate_ipc_request(request: IpcRequest) -> Result<DesktopCliRequest, String> {
    if request.protocol != PROTOCOL || request.version != PROTOCOL_VERSION {
        return Err("Native single-instance protocol is incompatible".to_owned());
    }
    let current_token = CURRENT
        .get()
        .and_then(|current| current.lock().ok())
        .and_then(|current| current.upgrade())
        .map(|current| current.token.clone())
        .ok_or_else(|| "Native single-instance coordinator is unavailable".to_owned())?;
    if request.token != current_token {
        return Err("Native single-instance authentication failed".to_owned());
    }
    Ok(request.request)
}

fn wait_for_handler(state: &CoordinatorState) -> Result<Arc<RequestHandler>, String> {
    let deadline = Instant::now() + HANDLER_WAIT;
    let mut handler = state
        .handler
        .lock()
        .map_err(|_| "Native instance handler is unavailable".to_owned())?;
    loop {
        if let Some(handler) = handler.as_ref() {
            return Ok(Arc::clone(handler));
        }
        let now = Instant::now();
        if now >= deadline || !state.running.load(Ordering::Acquire) {
            return Err("Native Preview did not become ready for the CLI request".to_owned());
        }
        let wait = deadline.saturating_duration_since(now);
        let (next, result) = state
            .handler_ready
            .wait_timeout(handler, wait)
            .map_err(|_| "Native instance handler is unavailable".to_owned())?;
        handler = next;
        if result.timed_out() && handler.is_none() {
            return Err("Native Preview did not become ready for the CLI request".to_owned());
        }
    }
}

fn forward_to_primary(
    descriptor_path: &Path,
    instance_identity: &str,
    request: DesktopCliRequest,
    lock_error: std::io::Error,
) -> Result<DesktopCliResult, String> {
    let deadline = Instant::now() + DESCRIPTOR_WAIT;
    let descriptor = loop {
        if let Ok(bytes) = fs::read(descriptor_path) {
            if let Ok(descriptor) = serde_json::from_slice::<InstanceDescriptor>(&bytes) {
                if descriptor.protocol == PROTOCOL
                    && descriptor.version == PROTOCOL_VERSION
                    && descriptor.instance_identity == instance_identity
                {
                    break descriptor;
                }
            }
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "another Native Preview instance holds the lock, but its IPC endpoint is unavailable: {lock_error}"
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let mut stream = TcpStream::connect(&descriptor.address)
        .map_err(|error| format!("failed to connect to running Native Preview: {error}"))?;
    stream
        .set_read_timeout(Some(IO_TIMEOUT))
        .map_err(|error| format!("failed to configure Native IPC read timeout: {error}"))?;
    stream
        .set_write_timeout(Some(IO_TIMEOUT))
        .map_err(|error| format!("failed to configure Native IPC write timeout: {error}"))?;
    write_json_line(
        &mut stream,
        &IpcRequest {
            protocol: PROTOCOL.to_owned(),
            version: PROTOCOL_VERSION,
            token: descriptor.token,
            request,
        },
    )?;
    read_json_line(&mut stream)
}

fn write_descriptor(path: &Path, descriptor: &InstanceDescriptor) -> Result<(), String> {
    let bytes = serde_json::to_vec(descriptor)
        .map_err(|error| format!("failed to encode Native instance descriptor: {error}"))?;
    let mut file = File::create(path)
        .map_err(|error| format!("failed to create {}: {error}", path.display()))?;
    file.write_all(&bytes)
        .and_then(|_| file.write_all(b"\n"))
        .map_err(|error| format!("failed to write {}: {error}", path.display()))
}

fn write_json_line(stream: &mut TcpStream, value: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| format!("failed to encode Native IPC message: {error}"))?;
    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err("Native IPC message is too large".to_owned());
    }
    stream
        .write_all(&bytes)
        .and_then(|_| stream.write_all(b"\n"))
        .and_then(|_| stream.flush())
        .map_err(|error| format!("failed to write Native IPC message: {error}"))
}

fn read_json_line<T: for<'de> Deserialize<'de>>(stream: &mut TcpStream) -> Result<T, String> {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 1024];
    loop {
        let count = stream
            .read(&mut buffer)
            .map_err(|error| format!("failed to read Native IPC message: {error}"))?;
        if count == 0 {
            return Err("Native IPC peer closed before sending a message".to_owned());
        }
        if let Some(newline) = buffer[..count].iter().position(|byte| *byte == b'\n') {
            bytes.extend_from_slice(&buffer[..newline]);
            if bytes.len() > MAX_MESSAGE_BYTES {
                return Err("Native IPC message is too large".to_owned());
            }
            break;
        }
        bytes.extend_from_slice(&buffer[..count]);
        if bytes.len() > MAX_MESSAGE_BYTES {
            return Err("Native IPC message is too large".to_owned());
        }
    }
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("Native IPC message is invalid: {error}"))
}

fn cli_failure(message: impl Into<String>) -> DesktopCliResult {
    DesktopCliResult {
        accepted: false,
        exit_code: Some(2),
        message: Some(message.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn request_takes_over_when_the_primary_dies_before_ipc_is_available() {
        let _guard = TEST_LOCK.lock().unwrap();
        let home = std::env::temp_dir().join(format!(
            "lilia-native-single-instance-recovery-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let runtime = home.join("run");
        fs::create_dir_all(&runtime).unwrap();
        let blocker = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(runtime.join("instance.lock"))
            .unwrap();
        blocker.try_lock_exclusive().unwrap();

        let recovery_home = home.clone();
        let recovery = std::thread::spawn(move || {
            acquire(
                &recovery_home,
                "liliacode.native.recovery-test",
                DesktopCliRequest {
                    request_id: "recover".to_owned(),
                    arguments: vec!["C:/work/recovered".to_owned()],
                    working_directory: None,
                },
            )
        });
        std::thread::sleep(Duration::from_millis(100));
        assert!(!recovery.is_finished());
        FileExt::unlock(&blocker).unwrap();

        let primary = match recovery.join().unwrap().unwrap() {
            InstanceDisposition::Primary(primary) => primary,
            InstanceDisposition::Forwarded(_) => panic!("recovery request was not promoted"),
        };
        install_handler(|_| DesktopCliResult {
            accepted: true,
            exit_code: Some(0),
            message: None,
        })
        .unwrap();
        assert!(runtime.join("instance.json").exists());
        drop(primary);
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn second_instance_forwards_authenticated_cli_request_to_primary() {
        let _guard = TEST_LOCK.lock().unwrap();
        let home = std::env::temp_dir().join(format!(
            "lilia-native-single-instance-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let initial = DesktopCliRequest {
            request_id: "initial".to_owned(),
            arguments: Vec::new(),
            working_directory: None,
        };
        let primary = match acquire(&home, "liliacode.native.instance-test", initial).unwrap() {
            InstanceDisposition::Primary(primary) => primary,
            InstanceDisposition::Forwarded(_) => panic!("first instance was not primary"),
        };
        install_handler(|request| DesktopCliResult {
            accepted: true,
            exit_code: Some(0),
            message: request.arguments.first().cloned(),
        })
        .unwrap();

        let forwarded = acquire(
            &home,
            "liliacode.native.instance-test",
            DesktopCliRequest {
                request_id: "second".to_owned(),
                arguments: vec!["C:/work/Lilia".to_owned()],
                working_directory: Some(PathBuf::from("C:/work")),
            },
        )
        .unwrap();
        let result = match forwarded {
            InstanceDisposition::Forwarded(result) => result,
            InstanceDisposition::Primary(_) => panic!("second instance acquired the primary lock"),
        };

        assert!(result.accepted);
        assert_eq!(result.message.as_deref(), Some("C:/work/Lilia"));
        drop(primary);
        if home.exists() {
            fs::remove_dir_all(home).unwrap();
        }
    }
}
