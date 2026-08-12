#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("native_performance_probe requires Windows");
    std::process::exit(2);
}

#[cfg(target_os = "windows")]
fn main() {
    if let Err(error) = windows_probe::run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(target_os = "windows")]
mod windows_probe {
    use std::collections::BTreeSet;
    use std::io::{self, BufRead, Write};
    use std::mem::size_of;
    use std::thread;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    use serde::{Deserialize, Serialize};
    use windows::core::BOOL;
    use windows::Win32::Foundation::{CloseHandle, FILETIME, HWND, LPARAM, RECT};
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
    };
    use windows::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowRect, GetWindowThreadProcessId, IsWindowVisible,
    };

    pub fn run() -> Result<(), String> {
        let mut arguments = std::env::args().skip(1);
        let command = arguments.next().ok_or("probe command is required")?;
        if command == "server" {
            ensure_no_arguments(arguments)?;
            return serve();
        }
        let result = match command.as_str() {
            "wait-window" => {
                let pid = parse_u32(arguments.next(), "pid")?;
                let timeout_ms = parse_u64(arguments.next(), "timeout-ms")?;
                ensure_no_arguments(arguments)?;
                serde_json::to_value(wait_window(pid, Duration::from_millis(timeout_ms))?)
            }
            "sample-tree" => {
                let pid = parse_u32(arguments.next(), "pid")?;
                ensure_no_arguments(arguments)?;
                serde_json::to_value(sample_tree(pid)?)
            }
            _ => return Err(format!("unsupported probe command `{command}`")),
        }
        .map_err(|error| error.to_string())?;
        println!("{result}");
        Ok(())
    }

    fn parse_u32(value: Option<String>, label: &str) -> Result<u32, String> {
        value
            .ok_or_else(|| format!("{label} is required"))?
            .parse()
            .map_err(|_| format!("{label} must be an unsigned integer"))
    }

    fn parse_u64(value: Option<String>, label: &str) -> Result<u64, String> {
        value
            .ok_or_else(|| format!("{label} is required"))?
            .parse()
            .map_err(|_| format!("{label} must be an unsigned integer"))
    }

    fn ensure_no_arguments(mut arguments: impl Iterator<Item = String>) -> Result<(), String> {
        arguments.next().map_or(Ok(()), |argument| {
            Err(format!("unexpected argument `{argument}`"))
        })
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct WindowReady {
        pid: u32,
        elapsed_ms: f64,
        width: i32,
        height: i32,
    }

    struct WindowSearch {
        pid: u32,
        rect: Option<RECT>,
    }

    fn wait_window(pid: u32, timeout: Duration) -> Result<WindowReady, String> {
        wait_window_since(pid, timeout, Instant::now())
    }

    fn wait_window_since(
        pid: u32,
        timeout: Duration,
        started: Instant,
    ) -> Result<WindowReady, String> {
        loop {
            let mut search = WindowSearch { pid, rect: None };
            let enumerated = unsafe {
                EnumWindows(
                    Some(find_window),
                    LPARAM((&mut search as *mut WindowSearch) as isize),
                )
            };
            if let Some(rect) = search.rect {
                return Ok(WindowReady {
                    pid,
                    elapsed_ms: started.elapsed().as_secs_f64() * 1_000.0,
                    width: rect.right - rect.left,
                    height: rect.bottom - rect.top,
                });
            }
            enumerated.map_err(|error| format!("EnumWindows failed: {error}"))?;
            if started.elapsed() >= timeout {
                return Err(format!(
                    "process {pid} did not expose a visible window within {timeout:?}"
                ));
            }
            thread::sleep(Duration::from_millis(5));
        }
    }

    unsafe extern "system" fn find_window(window: HWND, parameter: LPARAM) -> BOOL {
        let search = unsafe { &mut *(parameter.0 as *mut WindowSearch) };
        if !unsafe { IsWindowVisible(window).as_bool() } {
            return true.into();
        }
        let mut window_pid = 0;
        unsafe { GetWindowThreadProcessId(window, Some(&mut window_pid)) };
        if window_pid != search.pid {
            return true.into();
        }
        let mut rect = RECT::default();
        if unsafe { GetWindowRect(window, &mut rect) }.is_ok()
            && rect.right > rect.left
            && rect.bottom > rect.top
        {
            search.rect = Some(rect);
            return false.into();
        }
        true.into()
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct ProcessTreeSample {
        root_pid: u32,
        captured_at_ms: u64,
        cpu_ms: f64,
        working_set_bytes: u64,
        private_bytes: u64,
        process_count: usize,
        discovered_process_count: usize,
        inaccessible_pids: Vec<u32>,
    }

    fn sample_tree(root_pid: u32) -> Result<ProcessTreeSample, String> {
        let processes = process_snapshot()?;
        let mut pids = BTreeSet::from([root_pid]);
        loop {
            let before = pids.len();
            for (pid, parent_pid) in &processes {
                if pids.contains(parent_pid) {
                    pids.insert(*pid);
                }
            }
            if pids.len() == before {
                break;
            }
        }

        let mut cpu_100ns = 0_u64;
        let mut working_set_bytes = 0_u64;
        let mut private_bytes = 0_u64;
        let mut process_count = 0_usize;
        let mut inaccessible_pids = Vec::new();
        for pid in &pids {
            match sample_process(*pid) {
                Ok(sample) => {
                    cpu_100ns = cpu_100ns.saturating_add(sample.cpu_100ns);
                    working_set_bytes = working_set_bytes.saturating_add(sample.working_set_bytes);
                    private_bytes = private_bytes.saturating_add(sample.private_bytes);
                    process_count += 1;
                }
                Err(_) => inaccessible_pids.push(*pid),
            }
        }
        Ok(ProcessTreeSample {
            root_pid,
            captured_at_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|error| error.to_string())?
                .as_millis()
                .try_into()
                .map_err(|_| "system timestamp does not fit u64".to_owned())?,
            cpu_ms: cpu_100ns as f64 / 10_000.0,
            working_set_bytes,
            private_bytes,
            process_count,
            discovered_process_count: pids.len(),
            inaccessible_pids,
        })
    }

    fn process_snapshot() -> Result<Vec<(u32, u32)>, String> {
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }
            .map_err(|error| format!("CreateToolhelp32Snapshot failed: {error}"))?;
        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..PROCESSENTRY32W::default()
        };
        let mut processes = Vec::new();
        let first = unsafe { Process32FirstW(snapshot, &mut entry) };
        if first.is_ok() {
            loop {
                processes.push((entry.th32ProcessID, entry.th32ParentProcessID));
                if unsafe { Process32NextW(snapshot, &mut entry) }.is_err() {
                    break;
                }
            }
        }
        unsafe { CloseHandle(snapshot) }.map_err(|error| format!("CloseHandle failed: {error}"))?;
        first.map_err(|error| format!("Process32FirstW failed: {error}"))?;
        Ok(processes)
    }

    struct ProcessSample {
        cpu_100ns: u64,
        working_set_bytes: u64,
        private_bytes: u64,
    }

    fn sample_process(pid: u32) -> Result<ProcessSample, String> {
        let process =
            unsafe { OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid) }
                .map_err(|error| format!("OpenProcess({pid}) failed: {error}"))?;
        let result = (|| {
            let mut creation = FILETIME::default();
            let mut exit = FILETIME::default();
            let mut kernel = FILETIME::default();
            let mut user = FILETIME::default();
            unsafe { GetProcessTimes(process, &mut creation, &mut exit, &mut kernel, &mut user) }
                .map_err(|error| format!("GetProcessTimes({pid}) failed: {error}"))?;
            let mut memory = PROCESS_MEMORY_COUNTERS_EX {
                cb: size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
                ..PROCESS_MEMORY_COUNTERS_EX::default()
            };
            unsafe {
                GetProcessMemoryInfo(
                    process,
                    (&mut memory as *mut PROCESS_MEMORY_COUNTERS_EX)
                        .cast::<PROCESS_MEMORY_COUNTERS>(),
                    size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
                )
            }
            .map_err(|error| format!("GetProcessMemoryInfo({pid}) failed: {error}"))?;
            Ok(ProcessSample {
                cpu_100ns: filetime(kernel).saturating_add(filetime(user)),
                working_set_bytes: memory.WorkingSetSize as u64,
                private_bytes: memory.PrivateUsage as u64,
            })
        })();
        unsafe { CloseHandle(process) }
            .map_err(|error| format!("CloseHandle({pid}) failed: {error}"))?;
        result
    }

    fn filetime(value: FILETIME) -> u64 {
        (u64::from(value.dwHighDateTime) << 32) | u64::from(value.dwLowDateTime)
    }

    #[derive(Deserialize)]
    #[serde(tag = "command", rename_all = "kebab-case")]
    enum ServerRequest {
        WaitWindow { pid: u32, timeout_ms: u64 },
        SampleTree { pid: u32 },
        Exit,
    }

    fn serve() -> Result<(), String> {
        let stdin = io::stdin();
        let mut stdout = io::BufWriter::new(io::stdout().lock());
        write_response(&mut stdout, serde_json::json!({ "ready": true }))?;
        for line in stdin.lock().lines() {
            let line = line.map_err(|error| format!("cannot read probe request: {error}"))?;
            let request = match serde_json::from_str::<ServerRequest>(&line) {
                Ok(request) => request,
                Err(error) => {
                    write_response(
                        &mut stdout,
                        serde_json::json!({ "ok": false, "error": error.to_string() }),
                    )?;
                    continue;
                }
            };
            let result = match request {
                ServerRequest::WaitWindow { pid, timeout_ms } => {
                    wait_window(pid, Duration::from_millis(timeout_ms)).and_then(|result| {
                        serde_json::to_value(result).map_err(|error| error.to_string())
                    })
                }
                ServerRequest::SampleTree { pid } => sample_tree(pid).and_then(|result| {
                    serde_json::to_value(result).map_err(|error| error.to_string())
                }),
                ServerRequest::Exit => return Ok(()),
            };
            match result {
                Ok(result) => write_response(
                    &mut stdout,
                    serde_json::json!({ "ok": true, "result": result }),
                )?,
                Err(error) => write_response(
                    &mut stdout,
                    serde_json::json!({ "ok": false, "error": error }),
                )?,
            }
        }
        Ok(())
    }

    fn write_response(output: &mut impl Write, response: serde_json::Value) -> Result<(), String> {
        serde_json::to_writer(&mut *output, &response)
            .map_err(|error| format!("cannot encode probe response: {error}"))?;
        output
            .write_all(b"\n")
            .and_then(|_| output.flush())
            .map_err(|error| format!("cannot write probe response: {error}"))
    }
}
