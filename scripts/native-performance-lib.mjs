const REQUIRED_INTERACTION_SAMPLE_COUNT = 30;

export function isJavaScriptEntrypoint(filePath) {
  return typeof filePath === "string" && /\.(?:cjs|mjs|js)$/i.test(filePath);
}

export function percentile(samples, quantile) {
  const values = normalizedSamples(samples);
  if (!Number.isFinite(quantile) || quantile < 0 || quantile > 1) {
    throw new TypeError("quantile must be between 0 and 1");
  }
  const index = Math.max(0, Math.ceil(values.length * quantile) - 1);
  return values[index];
}

export function summarizeSamples(samples) {
  const values = normalizedSamples(samples);
  const total = values.reduce((sum, value) => sum + value, 0);
  return {
    count: values.length,
    min: values[0],
    max: values.at(-1),
    mean: total / values.length,
    p50: percentile(values, 0.5),
    p95: percentile(values, 0.95),
  };
}

export function processSampleDelta(before, after, logicalProcessorCount) {
  const elapsedMs = finiteNonNegative(after.capturedAtMs - before.capturedAtMs, "elapsedMs");
  const cpuMs = finiteNonNegative(after.cpuMs - before.cpuMs, "cpuMs");
  if (!Number.isInteger(logicalProcessorCount) || logicalProcessorCount < 1) {
    throw new TypeError("logicalProcessorCount must be a positive integer");
  }
  if (elapsedMs === 0) throw new RangeError("process samples must have distinct timestamps");
  return {
    elapsedMs,
    cpuMs,
    cpuPercentOfMachine: (cpuMs / (elapsedMs * logicalProcessorCount)) * 100,
    workingSetBytes: finiteNonNegative(after.workingSetBytes, "workingSetBytes"),
    privateBytes: finiteNonNegative(after.privateBytes, "privateBytes"),
    processCount: finiteNonNegative(after.processCount, "processCount"),
  };
}

export function evaluateNativePerformanceGate(report, options = {}) {
  const frameBudgetMs = options.frameBudgetMs ?? 1000 / 60;
  const minimumInteractionSamples = options.minimumInteractionSamples ??
    REQUIRED_INTERACTION_SAMPLE_COUNT;
  const checks = [];
  const compare = (id, actual, limit, unit) => {
    const passed = Number.isFinite(actual) && Number.isFinite(limit) && actual <= limit;
    checks.push({ id, passed, actual, limit, unit });
  };
  const requireSamples = (id, summary) => {
    const actual = summary?.count ?? 0;
    checks.push({
      id,
      passed: actual >= minimumInteractionSamples,
      actual,
      limit: minimumInteractionSamples,
      comparison: "at-least",
      unit: "samples",
    });
  };

  requireSamples("native.composer.sample-count", report.native?.composerFrameMs);
  requireSamples("native.panel-resize.sample-count", report.native?.panelResizeFrameMs);
  compare(
    "native.composer.p95-frame-budget",
    report.native?.composerFrameMs?.p95,
    frameBudgetMs,
    "ms",
  );
  compare(
    "native.panel-resize.p95-frame-budget",
    report.native?.panelResizeFrameMs?.p95,
    frameBudgetMs,
    "ms",
  );
  compare(
    "native.cold-start-not-worse-than-tauri",
    report.native?.coldStartMs?.p50,
    report.tauri?.coldStartMs?.p50,
    "ms",
  );
  compare(
    "native.idle-cpu-not-worse-than-tauri",
    report.native?.idleCpuPercent?.p95,
    report.tauri?.idleCpuPercent?.p95,
    "% machine",
  );
  compare(
    "native.idle-working-set-not-worse-than-tauri",
    report.native?.idleWorkingSetBytes?.p95,
    report.tauri?.idleWorkingSetBytes?.p95,
    "bytes",
  );

  return {
    status: checks.every((check) => check.passed) ? "passed" : "failed",
    frameBudgetMs,
    minimumInteractionSamples,
    checks,
  };
}

function normalizedSamples(samples) {
  if (!Array.isArray(samples) || samples.length === 0) {
    throw new TypeError("samples must be a non-empty array");
  }
  return samples.map((value, index) => finiteNonNegative(value, `samples[${index}]`)).sort((a, b) => a - b);
}

function finiteNonNegative(value, label) {
  if (!Number.isFinite(value) || value < 0) {
    throw new TypeError(`${label} must be a finite non-negative number`);
  }
  return value;
}
