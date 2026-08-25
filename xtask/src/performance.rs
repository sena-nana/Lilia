use std::fs;
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;

use crate::agent_debug::{require_ok, Session};
use crate::{output, Result, XtaskError};

const SAMPLES: usize = 30;
const COLD_START_SAMPLES: usize = 5;
const EXPECTED_TIMELINE_EVENTS: u64 = 1_000;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    samples: usize,
    cold_start_ms: Vec<f64>,
    cold_start_p95_ms: f64,
    composer_frame_p95_ms: f64,
    panel_resize_frame_p95_ms: f64,
    thousand_timeline_ready_ms: f64,
    idle_cpu_percent: f64,
    idle_rss_bytes: u64,
    gates: Gates,
    passed: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Gates {
    cold_start_p95_ms: f64,
    frame_p95_ms: f64,
    thousand_timeline_ms: f64,
    idle_cpu_percent: f64,
    idle_rss_bytes: u64,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProcessSample {
    cpu_seconds: f64,
    working_set_bytes: u64,
}

pub fn run() -> Result {
    if !cfg!(target_os = "windows") {
        return Err(XtaskError::blocker(
            "windows_required",
            "native performance requires Windows with a real WGPU desktop",
        ));
    }
    let gates = Gates {
        cold_start_p95_ms: env_f64("LILIA_PERFORMANCE_COLD_START_P95_MS", 15_000.0),
        frame_p95_ms: env_f64("LILIA_PERFORMANCE_FRAME_P95_MS", 100.0),
        thousand_timeline_ms: env_f64("LILIA_PERFORMANCE_TIMELINE_MS", 30_000.0),
        idle_cpu_percent: env_f64("LILIA_PERFORMANCE_IDLE_CPU_PERCENT", 10.0),
        idle_rss_bytes: env_u64("LILIA_PERFORMANCE_IDLE_RSS_BYTES", 1_073_741_824),
    };

    let session = Session::start("performance")?;
    let timeline_started = Instant::now();
    act(
        &session,
        serde_json::json!({
            "command": "click",
            "targetId": "lilia.project.equivalence-performance-v1-project"
        }),
    )?;
    act(
        &session,
        serde_json::json!({
            "command": "click",
            "targetId": "lilia.task.equivalence-performance-v1-task"
        }),
    )?;
    let mut observation = observe(&session)?;
    while observation
        .pointer("/observation/timelineHasMoreBefore")
        .and_then(Value::as_bool)
        == Some(true)
    {
        observation = act(
            &session,
            serde_json::json!({
                "command": "click",
                "targetId": "lilia.task-session.timeline.load-earlier"
            }),
        )?;
    }
    let timeline_count = observation
        .pointer("/observation/timelineEventCount")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    if timeline_count != EXPECTED_TIMELINE_EVENTS {
        return Err(XtaskError::failure(
            "performance_corpus_incomplete",
            format!("loaded {timeline_count}/{EXPECTED_TIMELINE_EVENTS} timeline events"),
        ));
    }
    let thousand_timeline_ready_ms = timeline_started.elapsed().as_secs_f64() * 1_000.0;

    let mut composer = Vec::with_capacity(SAMPLES);
    for index in 0..SAMPLES {
        let response = act(
            &session,
            serde_json::json!({
                "command": "input-frame",
                "targetId": "lilia.task-session.composer.input",
                "text": format!("固定性能输入 {index}")
            }),
        )?;
        composer.push(duration_ms(&response, "composer input-frame")?);
    }
    if !visible(&observation, "lilia.task-session.inspector") {
        observation = act(
            &session,
            serde_json::json!({
                "command": "click",
                "targetId": "lilia.task-session.inspector.toggle"
            }),
        )?;
    }
    if !visible(&observation, "lilia.task-session.inspector") {
        return Err(XtaskError::failure(
            "performance_inspector_missing",
            "inspector did not become visible before resize measurement",
        ));
    }
    let mut resize = Vec::with_capacity(SAMPLES);
    for index in 0..SAMPLES {
        let response = act(
            &session,
            serde_json::json!({
                "command": "resize-panel-frame",
                // 调试协议的字段一律是字符串，数字会在边界解析处被拒。
                "extent": if index % 2 == 0 { "368" } else { "352" }
            }),
        )?;
        resize.push(duration_ms(&response, "panel resize-frame")?);
    }

    thread::sleep(Duration::from_secs(5));
    let before = process_sample(session.pid())?;
    thread::sleep(Duration::from_secs(1));
    let after = process_sample(session.pid())?;
    let processors = std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(1) as f64;
    let idle_cpu_percent = ((after.cpu_seconds - before.cpu_seconds).max(0.0) * 100.0) / processors;
    let idle_rss_bytes = after.working_set_bytes;

    let mut cold_start_ms = vec![session.startup_ms];
    for index in 1..COLD_START_SAMPLES {
        let cold = Session::start(&format!("performance-cold-{index}"))?;
        cold_start_ms.push(cold.startup_ms);
    }
    let cold_start_p95_ms = percentile(&mut cold_start_ms.clone(), 0.95)?;
    let composer_frame_p95_ms = percentile(&mut composer, 0.95)?;
    let panel_resize_frame_p95_ms = percentile(&mut resize, 0.95)?;
    let passed = cold_start_p95_ms <= gates.cold_start_p95_ms
        && composer_frame_p95_ms <= gates.frame_p95_ms
        && panel_resize_frame_p95_ms <= gates.frame_p95_ms
        && thousand_timeline_ready_ms <= gates.thousand_timeline_ms
        && idle_cpu_percent <= gates.idle_cpu_percent
        && idle_rss_bytes <= gates.idle_rss_bytes;
    let report = Report {
        samples: SAMPLES,
        cold_start_ms,
        cold_start_p95_ms,
        composer_frame_p95_ms,
        panel_resize_frame_p95_ms,
        thousand_timeline_ready_ms,
        idle_cpu_percent,
        idle_rss_bytes,
        gates,
        passed,
    };
    let path = session.run_dir.join("performance.json");
    fs::write(&path, serde_json::to_vec_pretty(&report).unwrap()).map_err(|error| {
        XtaskError::io(
            "performance_artifact_failed",
            &path.display().to_string(),
            error,
        )
    })?;
    if !passed {
        return Err(XtaskError::failure(
            "performance_gate_failed",
            format!(
                "one or more native absolute gates failed; artifact: {}",
                path.display()
            ),
        ));
    }
    println!("performance: ok ({})", path.display());
    Ok(())
}

fn observe(session: &Session) -> Result<Value> {
    act(session, serde_json::json!({ "command": "observe" }))
}

fn act(session: &Session, request: Value) -> Result<Value> {
    let command = request
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let response = session.request(&request)?;
    require_ok(&response, command)?;
    Ok(response)
}

fn visible(response: &Value, target: &str) -> bool {
    response
        .pointer("/observation/visibleTargetIds")
        .and_then(Value::as_array)
        .is_some_and(|values| values.iter().any(|value| value.as_str() == Some(target)))
}

fn duration_ms(response: &Value, label: &str) -> Result<f64> {
    response
        .get("durationMs")
        .and_then(Value::as_f64)
        .ok_or_else(|| XtaskError::failure("performance_duration_missing", label))
}

fn process_sample(pid: u32) -> Result<ProcessSample> {
    let script = format!(
        "$p=Get-Process -Id {pid}; @{{cpuSeconds=[double]$p.CPU;workingSetBytes=[uint64]$p.WorkingSet64}} | ConvertTo-Json -Compress"
    );
    let value = output(
        crate::command("powershell.exe").args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &script,
        ]),
        "sample native desktop process",
    )?;
    serde_json::from_str(value.trim()).map_err(|error| {
        XtaskError::failure("performance_process_sample_invalid", error.to_string())
    })
}

fn env_f64(name: &str, default: f64) -> f64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn percentile(samples: &mut [f64], quantile: f64) -> Result<f64> {
    if samples.is_empty() || !(0.0..=1.0).contains(&quantile) {
        return Err(XtaskError::failure(
            "performance_samples_invalid",
            "percentile requires samples and a quantile between zero and one",
        ));
    }
    samples.sort_by(f64::total_cmp);
    let index = ((samples.len() - 1) as f64 * quantile).ceil() as usize;
    Ok(samples[index])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_uses_nearest_rank_without_hiding_tail_latency() {
        let mut samples = (1..=20).map(f64::from).collect::<Vec<_>>();
        assert_eq!(percentile(&mut samples, 0.95).unwrap(), 20.0);
    }
}
