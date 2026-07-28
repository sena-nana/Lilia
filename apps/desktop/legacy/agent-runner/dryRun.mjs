import { RUNNER_DONE_EVENT_TYPE } from "@lilia/contracts/runnerProtocolContract.mjs";

export async function runDryRun(cmd, context) {
  const { protocol } = context;
  const backend = cmd.backend === "codex" ? "codex" : "claude";
  const sid = `dry-${backend}-xxx`;
  protocol.emitTimeline({
    kind: "turn",
    status: "started",
    title: `${backend} turn started`,
    summary: "Dry-run agent turn",
    payload: { backend, sessionId: sid },
    sourceId: `${sid}:turn:start`,
  });
  protocol.emitTimeline({
    kind: "reasoning",
    status: "running",
    title: "公开思考摘要",
    summary: "正在检查当前实现并规划下一步。",
    payload: { backend, source: "dry-run" },
    sourceId: `${sid}:reasoning`,
  });
  protocol.emitTimeline({
    kind: "command",
    status: "success",
    title: "yarn verify:contracts",
    summary: "Contracts verification completed.",
    payload: {
      backend,
      command: "yarn verify:contracts",
      exitCode: 0,
      aggregatedOutput: "ok",
    },
    sourceId: `${sid}:command`,
  });
  protocol.emitTimeline({
    kind: "file_change",
    status: "success",
    title: "File changes",
    summary: "update apps/desktop/src/components/chat/AgentTimeline.vue",
    payload: {
      backend,
      changes: [
        {
          kind: "update",
          path: "apps/desktop/src/components/chat/AgentTimeline.vue",
        },
      ],
    },
    sourceId: `${sid}:file-change`,
  });
  protocol.emitTimeline({
    kind: "subagent",
    status: "success",
    title: "Worker summary",
    summary: "子代理完成 timeline 样式切片。",
    payload: { backend, agentType: "worker" },
    sourceId: `${sid}:subagent`,
  });
  protocol.emitTimeline({
    kind: "todo_list",
    status: "success",
    title: "计划",
    summary: "[x] 接线 runner\n[x] 渲染 timeline",
    payload: {
      backend,
      items: [
        { text: "接线 runner", completed: true },
        { text: "渲染 timeline", completed: true },
      ],
    },
    sourceId: `${sid}:todo`,
  });
  protocol.emitTimeline({
    kind: "error",
    status: "error",
    title: "示例错误",
    summary: "Dry-run error sample for UI coverage.",
    payload: { backend, message: "Dry-run error sample" },
    sourceId: `${sid}:error`,
  });
  protocol.emitAssistantMessageTimeline("hello ", "running", backend);
  protocol.emitAssistantMessageTimeline("hello from ", "running", backend);
  protocol.emitAssistantMessageTimeline(`hello from ${backend}`, "running", backend);
  protocol.emitAssistantMessageTimeline(`hello from ${backend}`, "success", backend);
  protocol.emitTimeline({
    kind: "turn",
    status: "success",
    title: `${backend} turn completed`,
    summary: "",
    payload: {
      backend,
      sessionId: sid,
    },
    sourceId: `${sid}:turn:done`,
  });
  protocol.emit({ type: RUNNER_DONE_EVENT_TYPE, sessionId: sid, subtype: "success" });
}
