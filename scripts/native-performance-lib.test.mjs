import assert from "node:assert/strict";
import test from "node:test";

import {
  evaluateNativePerformanceGate,
  isJavaScriptEntrypoint,
  percentile,
  processSampleDelta,
  summarizeSamples,
} from "./native-performance-lib.mjs";

test("accepts Corepack JavaScript entrypoints but rejects temporary shell shims", () => {
  assert.equal(isJavaScriptEntrypoint("C:/corepack/dist/yarn.js"), true);
  assert.equal(isJavaScriptEntrypoint("C:/temp/xfs/yarn"), false);
  assert.equal(isJavaScriptEntrypoint("C:/temp/xfs/yarn.cmd"), false);
});

test("summarizes unsorted latency samples with nearest-rank percentiles", () => {
  const summary = summarizeSamples([8, 2, 10, 4, 6]);
  assert.deepEqual(summary, {
    count: 5,
    min: 2,
    max: 10,
    mean: 6,
    p50: 6,
    p95: 10,
  });
  assert.equal(percentile([1, 2, 3, 4, 5], 0), 1);
  assert.equal(percentile([1, 2, 3, 4, 5], 1), 5);
});

test("normalizes process CPU to the whole machine and retains memory facts", () => {
  assert.deepEqual(
    processSampleDelta(
      { capturedAtMs: 1_000, cpuMs: 250, workingSetBytes: 1, privateBytes: 1, processCount: 1 },
      { capturedAtMs: 2_000, cpuMs: 650, workingSetBytes: 120, privateBytes: 90, processCount: 4 },
      8,
    ),
    {
      elapsedMs: 1_000,
      cpuMs: 400,
      cpuPercentOfMachine: 5,
      workingSetBytes: 120,
      privateBytes: 90,
      processCount: 4,
    },
  );
});

test("performance gate requires real sample volume and every explicit threshold", () => {
  const samples = Array.from({ length: 30 }, (_, index) => 4 + index / 10);
  const report = {
    tauri: {
      coldStartMs: summarizeSamples([900, 1_000, 1_100]),
      idleCpuPercent: summarizeSamples([0.4, 0.5, 0.6]),
      idleWorkingSetBytes: summarizeSamples([200, 220, 240]),
    },
    native: {
      coldStartMs: summarizeSamples([700, 800, 900]),
      idleCpuPercent: summarizeSamples([0.2, 0.3, 0.4]),
      idleWorkingSetBytes: summarizeSamples([120, 140, 160]),
      composerFrameMs: summarizeSamples(samples),
      panelResizeFrameMs: summarizeSamples(samples),
    },
  };
  const passed = evaluateNativePerformanceGate(report);
  assert.equal(passed.status, "passed");

  report.native.panelResizeFrameMs = summarizeSamples([20]);
  const failed = evaluateNativePerformanceGate(report);
  assert.equal(failed.status, "failed");
  assert.deepEqual(
    failed.checks.filter((check) => !check.passed).map((check) => check.id),
    ["native.panel-resize.sample-count", "native.panel-resize.p95-frame-budget"],
  );
});

test("invalid or empty samples are rejected instead of producing a false pass", () => {
  assert.throws(() => summarizeSamples([]), /non-empty/);
  assert.throws(() => summarizeSamples([Number.NaN]), /finite non-negative/);
  assert.throws(
    () => processSampleDelta(
      { capturedAtMs: 1, cpuMs: 0 },
      { capturedAtMs: 1, cpuMs: 0, workingSetBytes: 0, privateBytes: 0, processCount: 1 },
      4,
    ),
    /distinct timestamps/,
  );
});
