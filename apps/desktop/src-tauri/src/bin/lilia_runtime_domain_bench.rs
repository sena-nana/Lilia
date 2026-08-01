use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use lilia_lib::runtime_domains::{
    LiliaRuntimeDomainReference, LiliaRuntimeTopology, LiliaWorkload,
};
use mutsuki_runtime_contracts::{DomainTaskHandle, TaskOutcome, TaskStatus};
use serde::Serialize;
use serde_json::{json, Value};

#[derive(Clone, Debug)]
struct Options {
    samples: usize,
    min_background_ms: u64,
    workspace: PathBuf,
    output: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, Serialize)]
struct CalibratedWork {
    agent_iterations: usize,
    workspace_iterations: usize,
}

#[derive(Clone, Debug, Serialize)]
struct Distribution {
    samples: usize,
    p50_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    max_ms: f64,
}

#[derive(Debug, Serialize)]
struct Report {
    schema: &'static str,
    business_purpose: &'static str,
    workload: Value,
    single_domain: Distribution,
    three_domains: Distribution,
    p99_improvement_percent: f64,
    expected_minimum_improvement_percent: f64,
    passed: bool,
}

fn main() -> Result<(), String> {
    let options = parse_options()?;
    if options.samples < 100 || options.min_background_ms == 0 {
        return Err("samples must be at least 100 and min-background-ms must be positive".into());
    }
    let calibrated = calibrate(&options)?;
    let (single_domain, three_domains) = run_comparison(&options, calibrated)?;
    let improvement = (single_domain.p99_ms - three_domains.p99_ms)
        / single_domain.p99_ms.max(f64::EPSILON)
        * 100.0;
    let report = Report {
        schema: "lilia.runtime-domain-reference.v1",
        business_purpose:
            "prepare the same product handoff while two Agent payload builds and a real Git worktree inspection are active",
        workload: json!({
            "samples": options.samples,
            "minimum_background_work_ms": options.min_background_ms,
            "workspace": options.workspace,
            "calibrated": calibrated,
            "product_work": "parse the production LiliaGithub handoff contract and build its production prompt",
            "agent_work": "build and serialize the production AgentKit AgentRunRequest contract without starting a provider",
            "workspace_work": "run production git worktree list --porcelain inspection",
            "single_domain_threads": 3,
            "three_domain_threads": {
                "product": 1,
                "agent": 1,
                "workspace": 1
            },
            "same_total_worker_budget": 3,
            "same_protocols_runners_payloads_and_outputs": true,
            "measurement": "product handoff submit to terminal outcome",
            "runtime_lifecycle": "reuse one warmed long-lived runtime per topology",
            "sample_order": "alternate single-domain-first and three-domain-first paired samples",
            "percentile_method": "nearest-rank"
        }),
        single_domain,
        three_domains,
        p99_improvement_percent: improvement,
        expected_minimum_improvement_percent: 50.0,
        passed: improvement >= 50.0,
    };
    let encoded = serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?;
    println!("{encoded}");
    if let Some(output) = options.output {
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::write(output, format!("{encoded}\n")).map_err(|error| error.to_string())?;
    }
    if !report.passed {
        return Err(format!(
            "three-domain p99 improvement {:.2}% is below the required 50%",
            report.p99_improvement_percent
        ));
    }
    Ok(())
}

fn calibrate(options: &Options) -> Result<CalibratedWork, String> {
    Ok(CalibratedWork {
        agent_iterations: calibrate_work(
            options.min_background_ms,
            |iterations| agent_payload(iterations),
            LiliaWorkload::AgentEvent,
        )?,
        workspace_iterations: calibrate_work(
            options.min_background_ms,
            |iterations| workspace_payload(&options.workspace, iterations),
            LiliaWorkload::WorkspaceIndex,
        )?,
    })
}

fn calibrate_work(
    minimum_ms: u64,
    payload: impl Fn(usize) -> Value,
    workload: LiliaWorkload,
) -> Result<usize, String> {
    let mut iterations = 1usize;
    loop {
        let reference = LiliaRuntimeDomainReference::start(LiliaRuntimeTopology::SingleDomain)?;
        let started = Instant::now();
        let handle = reference.submit(
            format!("calibrate-{workload:?}-{iterations}"),
            workload,
            payload(iterations),
        )?;
        ensure_completed(&reference, &handle, Duration::from_secs(30))?;
        if started.elapsed() >= Duration::from_millis(minimum_ms) {
            return Ok(iterations);
        }
        iterations = iterations
            .checked_mul(2)
            .filter(|value| *value <= 1_048_576)
            .ok_or_else(|| format!("unable to calibrate {workload:?} to {minimum_ms}ms"))?;
    }
}

fn run_comparison(
    options: &Options,
    calibrated: CalibratedWork,
) -> Result<(Distribution, Distribution), String> {
    let single = LiliaRuntimeDomainReference::start(LiliaRuntimeTopology::SingleDomain)?;
    let three = LiliaRuntimeDomainReference::start(LiliaRuntimeTopology::ProductAgentWorkspace)?;
    warm_up(&single, "single", &options.workspace)?;
    warm_up(&three, "three", &options.workspace)?;

    let mut single_values = Vec::with_capacity(options.samples);
    let mut three_values = Vec::with_capacity(options.samples);
    for sample in 0..options.samples {
        if sample % 2 == 0 {
            single_values.push(run_sample(&single, "single", sample, options, calibrated)?);
            three_values.push(run_sample(&three, "three", sample, options, calibrated)?);
        } else {
            three_values.push(run_sample(&three, "three", sample, options, calibrated)?);
            single_values.push(run_sample(&single, "single", sample, options, calibrated)?);
        }
    }
    Ok((distribution(single_values), distribution(three_values)))
}

fn warm_up(
    reference: &LiliaRuntimeDomainReference,
    topology: &str,
    workspace: &Path,
) -> Result<(), String> {
    for (suffix, workload, payload) in [
        (
            "product",
            LiliaWorkload::ProductCommand,
            handoff_payload(workspace),
        ),
        ("agent", LiliaWorkload::AgentEvent, agent_payload(1)),
        (
            "workspace",
            LiliaWorkload::WorkspaceIndex,
            workspace_payload(workspace, 1),
        ),
    ] {
        let handle = reference.submit(format!("warmup-{topology}-{suffix}"), workload, payload)?;
        ensure_completed(reference, &handle, Duration::from_secs(30))?;
    }
    Ok(())
}

fn run_sample(
    reference: &LiliaRuntimeDomainReference,
    topology: &str,
    sample: usize,
    options: &Options,
    calibrated: CalibratedWork,
) -> Result<f64, String> {
    let agent_one = reference.submit(
        format!("{topology}-{sample}-agent-one"),
        LiliaWorkload::AgentEvent,
        agent_payload(calibrated.agent_iterations),
    )?;
    let agent_two = reference.submit(
        format!("{topology}-{sample}-agent-two"),
        LiliaWorkload::AgentCompletion,
        agent_payload(calibrated.agent_iterations),
    )?;
    let workspace = reference.submit(
        format!("{topology}-{sample}-workspace"),
        LiliaWorkload::WorkspaceIndex,
        workspace_payload(&options.workspace, calibrated.workspace_iterations),
    )?;
    wait_running(reference, &agent_one)?;
    wait_running(reference, &workspace)?;
    if reference.route(LiliaWorkload::ProductCommand)?
        == reference.route(LiliaWorkload::AgentEvent)?
    {
        wait_running(reference, &agent_two)?;
    }

    let started = Instant::now();
    let product = reference.submit(
        format!("{topology}-{sample}-product"),
        LiliaWorkload::ProductCommand,
        handoff_payload(&options.workspace),
    )?;
    ensure_completed(reference, &product, Duration::from_secs(30))?;
    let elapsed_ms = started.elapsed().as_secs_f64() * 1_000.0;

    for handle in [&agent_one, &agent_two, &workspace] {
        ensure_completed(reference, handle, Duration::from_secs(30))?;
    }
    Ok(elapsed_ms)
}

fn ensure_completed(
    reference: &LiliaRuntimeDomainReference,
    handle: &DomainTaskHandle,
    timeout: Duration,
) -> Result<Value, String> {
    match reference.wait_outcome(handle, timeout)? {
        Some(TaskOutcome::Completed {
            output: Some(output),
            ..
        }) => Ok(output),
        other => Err(format!(
            "task {} did not complete with business output: {other:?}",
            handle.task.task_id
        )),
    }
}

fn wait_running(
    reference: &LiliaRuntimeDomainReference,
    handle: &DomainTaskHandle,
) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        let status = reference
            .group()
            .domain(&handle.domain_id)
            .and_then(|runtime| runtime.task_status(&handle.task.task_id));
        if status == Some(TaskStatus::Running) {
            return Ok(());
        }
        if matches!(
            status,
            Some(
                TaskStatus::Completed
                    | TaskStatus::Failed
                    | TaskStatus::Cancelled
                    | TaskStatus::Expired
                    | TaskStatus::DeadLetter
            )
        ) {
            return Err(format!(
                "background task {} completed before pressure was established",
                handle.task.task_id
            ));
        }
        thread::yield_now();
    }
    Err(format!(
        "background task {} did not start",
        handle.task.task_id
    ))
}

fn handoff_payload(workspace: &Path) -> Value {
    json!({
        "protocol": "lilia-code-task-handoff",
        "version": 1,
        "id": "issue43-performance",
        "createdAt": "2026-07-26T00:00:00Z",
        "title": "验证 RuntimeDomain 性能",
        "kind": "repository",
        "repository": {
            "fullName": "sena-nana/LiliaCode",
            "worktreePath": workspace,
            "branch": "main",
            "remoteUrl": "https://github.com/sena-nana/LiliaCode.git"
        },
        "source": {
            "application": "LiliaGithub",
            "route": "/repos/sena-nana/LiliaCode",
            "objectUrl": null
        },
        "problem": "在多 Agent 和 full index 满载时推进产品 projection。",
        "relatedFiles": [],
        "logSummary": null,
        "acceptanceCriteria": ["产品命令尾延迟至少改善 50%"],
        "pullRequest": null,
        "workflow": null
    })
}

fn agent_payload(iterations: usize) -> Value {
    json!({
        "taskId": "issue43-agent",
        "profileId": "lilia.product.native-coding",
        "sessionId": "issue43-performance-session",
        "cwd": ".",
        "prompt": "检查 RuntimeDomain reference profile",
        "iterations": iterations
    })
}

fn workspace_payload(path: &Path, iterations: usize) -> Value {
    json!({
        "path": path,
        "iterations": iterations
    })
}

fn distribution(mut values: Vec<f64>) -> Distribution {
    values.sort_by(f64::total_cmp);
    Distribution {
        samples: values.len(),
        p50_ms: percentile(&values, 0.50),
        p95_ms: percentile(&values, 0.95),
        p99_ms: percentile(&values, 0.99),
        max_ms: *values.last().unwrap_or(&0.0),
    }
}

fn percentile(values: &[f64], percentile: f64) -> f64 {
    let index = ((values.len() as f64 * percentile).ceil() as usize).saturating_sub(1);
    values[index.min(values.len().saturating_sub(1))]
}

fn parse_options() -> Result<Options, String> {
    let mut options = Options {
        samples: 100,
        min_background_ms: 20,
        workspace: std::env::current_dir().map_err(|error| error.to_string())?,
        output: None,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--samples" => {
                options.samples = args
                    .next()
                    .ok_or("--samples requires a value")?
                    .parse()
                    .map_err(|_| "invalid --samples")?;
            }
            "--min-background-ms" => {
                options.min_background_ms = args
                    .next()
                    .ok_or("--min-background-ms requires a value")?
                    .parse()
                    .map_err(|_| "invalid --min-background-ms")?;
            }
            "--workspace" => {
                options.workspace =
                    PathBuf::from(args.next().ok_or("--workspace requires a path")?);
            }
            "--output" => {
                options.output = Some(PathBuf::from(
                    args.next().ok_or("--output requires a path")?,
                ));
            }
            _ => return Err(format!("unknown argument: {arg}")),
        }
    }
    Ok(options)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_rank_p99_requires_one_hundred_samples_and_excludes_the_maximum() {
        let values = (1..=100).map(f64::from).collect::<Vec<_>>();
        assert_eq!(percentile(&values, 0.50), 50.0);
        assert_eq!(percentile(&values, 0.95), 95.0);
        assert_eq!(percentile(&values, 0.99), 99.0);
    }
}
