//! Optional Node JSONL process session registry.
//!
//! Production Native AgentKit turns do not spawn Node children. These helpers
//! only no-op when no process session is registered (legacy leftover sessions
//! or unit tests that start a child).

use std::collections::HashMap;
#[cfg(test)]
use std::io::Write;
use std::process::{Child, ChildStdin};
use std::sync::{Arc, Mutex};

#[cfg(test)]
use serde_json::Value as JsonValue;

#[cfg(test)]
#[derive(Debug)]
pub(crate) enum JsonlProcessPoll {
    Pending,
    Exited,
}

struct JsonlProcessSession {
    child: Child,
    stdin: Option<Arc<Mutex<ChildStdin>>>,
    termination_requested: bool,
    finished: bool,
}

pub(crate) struct JsonlProcessRegistry {
    sessions: Mutex<HashMap<String, JsonlProcessSession>>,
    #[cfg(test)]
    next_id: Mutex<u64>,
}

impl JsonlProcessRegistry {
    pub(crate) fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            #[cfg(test)]
            next_id: Mutex::new(0),
        }
    }

    #[cfg(test)]
    pub(crate) fn start(
        &self,
        mut child: Child,
        initial_payload: &JsonValue,
    ) -> std::io::Result<String> {
        let mut next_id = self.next_id.lock().unwrap();
        *next_id += 1;
        let session_id = format!("jsonl-process-{}", *next_id);
        let stdin = child.stdin.take().map(|stdin| Arc::new(Mutex::new(stdin)));
        let session = JsonlProcessSession {
            child,
            stdin: stdin.clone(),
            termination_requested: false,
            finished: false,
        };
        self.sessions
            .lock()
            .unwrap()
            .insert(session_id.clone(), session);
        if let Some(stdin) = stdin {
            let mut line = serde_json::to_string(initial_payload)?;
            line.push('\n');
            let mut stdin = stdin.lock().unwrap();
            stdin.write_all(line.as_bytes())?;
            stdin.flush()?;
        }
        Ok(session_id)
    }

    pub(crate) fn stdin_handle(&self, session_id: &str) -> Option<Arc<Mutex<ChildStdin>>> {
        self.sessions
            .lock()
            .unwrap()
            .get(session_id)
            .and_then(|session| session.stdin.clone())
    }

    pub(crate) fn terminate(&self, session_id: &str) -> Result<bool, String> {
        let mut sessions = self.sessions.lock().unwrap();
        let Some(session) = sessions.get_mut(session_id) else {
            return Ok(false);
        };
        if session.finished {
            return Ok(false);
        }
        if session.termination_requested {
            return Ok(false);
        }
        if session
            .child
            .try_wait()
            .map_err(|err| err.to_string())?
            .is_some()
        {
            return Ok(false);
        }
        session.child.kill().map_err(|err| err.to_string())?;
        session.termination_requested = true;
        Ok(true)
    }

    pub(crate) fn is_active(&self, session_id: &str) -> bool {
        self.sessions
            .lock()
            .unwrap()
            .get(session_id)
            .is_some_and(|session| !session.finished && !session.termination_requested)
    }

    #[cfg(test)]
    pub(crate) fn remove(&self, session_id: &str) -> Option<()> {
        self.sessions.lock().unwrap().remove(session_id).map(|_| ())
    }

    #[cfg(test)]
    pub(crate) fn poll(&self, session_id: &str) -> Option<JsonlProcessPoll> {
        let mut sessions = self.sessions.lock().unwrap();
        let session = sessions.get_mut(session_id)?;
        if session.finished {
            return Some(JsonlProcessPoll::Pending);
        }
        match session.child.try_wait() {
            Ok(Some(_)) => {
                session.finished = true;
                Some(JsonlProcessPoll::Exited)
            }
            Ok(None) => Some(JsonlProcessPoll::Pending),
            Err(_) => {
                session.finished = true;
                Some(JsonlProcessPoll::Exited)
            }
        }
    }
}

impl Default for JsonlProcessRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{JsonlProcessPoll, JsonlProcessRegistry};
    use serde_json::json;
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};

    fn start_silent_child(registry: &JsonlProcessRegistry) -> String {
        let child = Command::new("node")
            .arg("-e")
            .arg("setInterval(() => {}, 1000)")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        registry.start(child, &json!({ "boot": true })).unwrap()
    }

    fn wait_for_exit(registry: &JsonlProcessRegistry, session_id: &str) {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            match registry.poll(session_id) {
                Some(JsonlProcessPoll::Exited) => break,
                Some(JsonlProcessPoll::Pending) if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(10));
                }
                other => panic!("expected terminated process to exit, got {other:?}"),
            }
        }
    }

    #[test]
    fn terminate_is_idempotent_and_poll_observes_exit() {
        let registry = JsonlProcessRegistry::new();
        let session_id = start_silent_child(&registry);

        assert!(registry.is_active(&session_id));
        assert!(registry.terminate(&session_id).unwrap());
        assert!(!registry.is_active(&session_id));
        assert!(!registry.terminate(&session_id).unwrap());

        wait_for_exit(&registry, &session_id);

        registry.remove(&session_id);
    }
}
