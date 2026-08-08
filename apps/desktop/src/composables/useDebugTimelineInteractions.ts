import {
  agentTimelinePayloadRecord,
  ASK_USER_CANCEL_ANSWER_VALUE,
  ASK_USER_INTERACTION_KIND,
  ASK_USER_MULTI_SELECT_MODE,
  ASK_USER_SINGLE_SELECT_MODE,
  createPlanApprovalAskUserSpec,
  PLAN_APPROVAL_REVISION_REQUEST_ANSWER_VALUE,
  TOOL_CONSENT_INTERACTION_KIND,
  type AgentTimelineEvent,
  type AgentTimelinePayload,
  type AskUserResult,
  type AskUserSpec,
} from "@lilia/contracts";
import { askUserForTask } from "./useAskUser";
import { emitDebugTimelineEvent } from "./useDebugTimelineEvents";
import { requestLocalToolConsent } from "./useToolConsentBridge";
import type { ToolConsentRequest } from "../services/chat";
import { applyAgentTodoEvent, type AgentTodoInput } from "../services/todos";

let debugSeq = 0;

function nextDebugIds(taskId: string, presetId: string) {
  const now = Date.now();
  debugSeq += 1;
  const prefix = `debug-${taskId}-${presetId}-${now}-${debugSeq}`;
  return {
    now,
    eventId: prefix,
    turnId: `${prefix}:turn`,
    requestId: `${prefix}:request`,
    order: debugSeq,
  };
}

function createDebugEvent(input: {
  id: string;
  taskId: string;
  turnId: string | null;
  kind: AgentTimelineEvent["kind"];
  status: AgentTimelineEvent["status"];
  title: string;
  summary: string;
  payload: AgentTimelinePayload;
  now: number;
  order: number;
}): AgentTimelineEvent {
  return {
    id: input.id,
    taskId: input.taskId,
    turnId: input.turnId,
    backend: "native-agentkit",
    kind: input.kind,
    status: input.status,
    title: input.title,
    summary: input.summary,
    payload: input.payload,
    createdAt: input.now,
    updatedAt: input.now,
    turnSeq: Number.MAX_SAFE_INTEGER,
    intraTurnOrder: input.order,
  };
}

function readAnswer(
  result: AskUserResult,
  questionId: string,
): { value: unknown; notes?: string } | null {
  return result.answers[questionId] ?? null;
}

function updateEvent(
  event: AgentTimelineEvent,
  patch: Pick<AgentTimelineEvent, "status" | "summary" | "payload">,
) {
  emitDebugTimelineEvent({
    ...event,
    ...patch,
    updatedAt: Date.now(),
  });
}

function patchPayload(
  event: AgentTimelineEvent,
  patch: Record<string, AgentTimelinePayload>,
): AgentTimelinePayload {
  const payload = agentTimelinePayloadRecord(event) ?? {};
  return { ...payload, ...patch };
}

function resultPayload(result: AskUserResult): AgentTimelinePayload {
  return result as unknown as AgentTimelinePayload;
}

function completeAskEvent(event: AgentTimelineEvent, result: AskUserResult) {
  const label = event.title === "Debug AskUser" ? "Debug 提问" : event.title;
  updateEvent(event, {
    status: result.cancelled ? "cancelled" : "success",
    summary: result.cancelled ? `${label}已取消` : `${label}已回答`,
    payload: patchPayload(event, {
      result: resultPayload(result),
      cancelled: result.cancelled,
    }),
  });
}

function debugAskEvent(
  taskId: string,
  ids: ReturnType<typeof nextDebugIds>,
  title: string,
  summary: string,
  spec: AskUserSpec,
): AgentTimelineEvent {
  return createDebugEvent({
    id: ids.eventId,
    taskId,
    turnId: ids.turnId,
    kind: ASK_USER_INTERACTION_KIND,
    title,
    summary,
    status: "requires_action",
    now: ids.now,
    order: ids.order,
    payload: {
      requestId: ids.requestId,
      questions: spec.questions as unknown as AgentTimelinePayload,
    },
  });
}

function emitStaticDebugEvent(
  taskId: string,
  presetId: string,
  event: Pick<Parameters<typeof createDebugEvent>[0],
    "kind" | "title" | "summary" | "payload"
  >,
) {
  const ids = nextDebugIds(taskId, presetId);
  emitDebugTimelineEvent(createDebugEvent({
    id: ids.eventId,
    taskId,
    turnId: ids.turnId,
    status: "success",
    now: ids.now,
    order: ids.order,
    ...event,
  }));
}

export function useDebugTimelineInteractions(taskId: string) {
  function emitTodo() {
    const ids = nextDebugIds(taskId, "todo");
    emitDebugTimelineEvent(createDebugEvent({
      id: ids.eventId,
      taskId,
      turnId: null,
      kind: "todo_list",
      title: "Debug TodoWrite",
      summary: "调试待办事件",
      status: "success",
      now: ids.now,
      order: ids.order,
      payload: {
        items: [
          { text: "确认 debug 面板已出现", status: "completed" },
          { text: "点击预制事件按钮", status: "in_progress" },
          { text: "观察时间线渲染", status: "pending" },
        ],
      },
    }));
  }

  async function emitTodoTool() {
    const ids = nextDebugIds(taskId, "todo-tool");
    const items: AgentTodoInput[] = [
      { content: "完成 Claude TodoWrite 调试接线", status: "completed" },
      { content: "确认 TodoFloat 自动刷新", status: "in_progress" },
      { content: "验证手动 Todo 不被 agent 覆盖", status: "pending" },
    ];
    try {
      await applyAgentTodoEvent(taskId, items);
      emitDebugTimelineEvent(createDebugEvent({
        id: ids.eventId,
        taskId,
        turnId: ids.turnId,
        kind: "todo_list",
        title: "Debug TodoWrite",
        summary: "模拟 Claude TodoWrite 并刷新 Lilia Todo",
        status: "success",
        now: ids.now,
        order: ids.order,
        payload: { items: items as unknown as AgentTimelinePayload },
      }));
    } catch (err) {
      emitDebugTimelineEvent(createDebugEvent({
        id: ids.eventId,
        taskId,
        turnId: ids.turnId,
        kind: "error",
        title: "Debug TodoWrite 失败",
        summary: String(err),
        status: "error",
        now: ids.now,
        order: ids.order,
        payload: { message: String(err) },
      }));
    }
  }

  function emitPlan() {
    const ids = nextDebugIds(taskId, "plan");
    const spec = createPlanApprovalAskUserSpec({
      title: "确认 Debug 计划",
      source: "Debug",
    });
    const questionId = spec.questions[0]?.id ?? "";
    const event = createDebugEvent({
      id: ids.eventId,
      taskId,
      turnId: ids.turnId,
      kind: "plan",
      title: "Debug Plan",
      summary: "调试计划事件",
      status: "requires_action",
      now: ids.now,
      order: ids.order,
      payload: {
        plan: "## Debug 计划\n- 梳理当前状态\n- 注入临时事件\n- 验证时间线渲染",
        approved: null,
        requestId: ids.requestId,
        allowedPrompts: [
          { tool: "Debug", prompt: "插入计划事件" },
        ],
      },
    });
    emitDebugTimelineEvent(event);
    void askUserForTask(taskId, spec, ids.turnId, ids.requestId).then((result) => {
      const answer = readAnswer(result, questionId);
      const notes = answer?.notes?.trim();
      const revisionRequest = answer?.value === PLAN_APPROVAL_REVISION_REQUEST_ANSWER_VALUE && notes ? notes : "";
      const cancelled = !revisionRequest && (result.cancelled || answer?.value === ASK_USER_CANCEL_ANSWER_VALUE);
      updateEvent(event, {
        status: cancelled ? "cancelled" : "success",
        summary: revisionRequest || (cancelled ? "Debug 计划已取消" : "Debug 计划已同意"),
        payload: patchPayload(event, {
          approved: revisionRequest || cancelled ? false : true,
          ...(revisionRequest ? { revisionRequest } : {}),
          result: resultPayload(result),
        }),
      });
    });
  }

  function emitAskUser() {
    const ids = nextDebugIds(taskId, "ask-user");
    const questionId = "debug-choice";
    const questions = [
      {
        id: questionId,
        header: "Debug",
        question: "要注入哪类临时事件？",
        mode: ASK_USER_SINGLE_SELECT_MODE,
        options: [
          { id: "plan", label: "计划" },
          { id: "todo", label: "待办" },
          { id: ASK_USER_INTERACTION_KIND, label: "提问" },
        ],
      },
    ];
    const spec: AskUserSpec = {
      title: "Debug 提问",
      source: "Debug",
      dismissable: true,
      questions,
    };
    const event = debugAskEvent(taskId, ids, "Debug AskUser", "选择调试事件注入方式", spec);
    emitDebugTimelineEvent(event);
    void askUserForTask(taskId, spec, ids.turnId, ids.requestId)
      .then((result) => completeAskEvent(event, result));
  }

  function emitAskUserMulti() {
    const ids = nextDebugIds(taskId, "ask-user-multi");
    const questions: AskUserSpec["questions"] = [
      {
        id: "debug-multi",
        header: "多选",
        question: "这轮调试要覆盖哪些流程？",
        mode: ASK_USER_MULTI_SELECT_MODE,
        minSelections: 2,
        maxSelections: 3,
        allowOther: true,
        options: [
          { id: "plan", label: "计划确认", description: "同意、修改要求和取消态。" },
          { id: "ask", label: "提问回答", description: "单题、多题和选择结果。" },
          { id: "permission", label: "权限申请", description: "允许、拒绝和入参展开。", recommended: true },
          { id: "cards", label: "普通卡片", description: "待办、命令和文件事件。" },
        ],
      },
    ];
    const spec: AskUserSpec = {
      title: "Debug 多选提问",
      source: "Debug",
      dismissable: true,
      questions,
    };
    const event = debugAskEvent(taskId, ids, "Debug 多选提问", "选择多个调试覆盖项", spec);
    emitDebugTimelineEvent(event);
    void askUserForTask(taskId, spec, ids.turnId, ids.requestId)
      .then((result) => completeAskEvent(event, result));
  }

  function emitAskUserPreview() {
    const ids = nextDebugIds(taskId, "ask-user-preview");
    const questions: AskUserSpec["questions"] = [
      {
        id: "debug-preview",
        header: "示例",
        question: "选择一种响应模板。",
        mode: ASK_USER_SINGLE_SELECT_MODE,
        allowOther: true,
        options: [
          {
            id: "concise",
            label: "精简",
            description: "只保留结论和验证结果。",
            recommended: true,
            preview: "已完成：debug 计划/提问/权限申请均可交互。\n验证：定向测试通过。",
          },
          {
            id: "detailed",
            label: "详细",
            description: "展开关键改动、边界和剩余风险。",
            preview: "改动：\n1. 注入 overlay timeline。\n2. 注册 pending action。\n3. 本地 resolve 后回写卡片状态。",
          },
          {
            id: "risk",
            label: "风险优先",
            description: "先列出行为风险，再补充实现说明。",
            danger: true,
            preview: "风险：如果 requestId 不匹配，卡片会显示已失效。\n处理：统一生成 taskId/turnId/requestId。",
          },
        ],
      },
    ];
    const spec: AskUserSpec = {
      title: "Debug 示例提问",
      source: "Debug",
      dismissable: true,
      questions,
    };
    const event = debugAskEvent(taskId, ids, "Debug 示例提问", "带选项预览的调试提问", spec);
    emitDebugTimelineEvent(event);
    void askUserForTask(taskId, spec, ids.turnId, ids.requestId)
      .then((result) => completeAskEvent(event, result));
  }

  function emitAskUserFlow() {
    const ids = nextDebugIds(taskId, "ask-user-flow");
    const questions: AskUserSpec["questions"] = [
      {
        id: "debug-flow-target",
        header: "目标",
        question: "这次要先验证哪个入口？",
        mode: ASK_USER_SINGLE_SELECT_MODE,
        options: [
          { id: "sidebar", label: "侧栏", description: "验证动态注册和切换。" },
          { id: "timeline", label: "时间线", description: "验证卡片和 pending action。" },
        ],
      },
      {
        id: "debug-flow-checks",
        header: "检查项",
        question: "需要一起检查哪些状态？",
        mode: ASK_USER_MULTI_SELECT_MODE,
        minSelections: 1,
        options: [
          { id: "success", label: "完成态" },
          { id: "cancelled", label: "取消态" },
          { id: "expired", label: "失效态" },
        ],
      },
    ];
    const spec: AskUserSpec = {
      title: "Debug 多题提问",
      source: "Debug",
      dismissable: true,
      questions,
    };
    const event = debugAskEvent(taskId, ids, "Debug 多题提问", "连续提问调试流程", spec);
    emitDebugTimelineEvent(event);
    void askUserForTask(taskId, spec, ids.turnId, ids.requestId)
      .then((result) => completeAskEvent(event, result));
  }

  function emitPermission() {
    const ids = nextDebugIds(taskId, "permission");
    const request: ToolConsentRequest = {
      taskId,
      turnId: ids.turnId,
      backend: "native-agentkit",
      requestId: ids.requestId,
      toolName: "Write",
      input: {
        file_path: "apps/desktop/src/debug-fixture.ts",
        content: "export const debugFixture = true;\n",
      },
      title: "写入调试夹具",
      displayName: "Write",
      description: "Debug 本地权限申请，不会写入文件或通知 runner。",
      blockedPath: "apps/desktop/src/debug-fixture.ts",
      decisionReason: null,
      toolUseId: null,
    };
    const event = createDebugEvent({
      id: ids.eventId,
      taskId,
      turnId: ids.turnId,
      kind: "file_change",
      title: "Debug Permission",
      summary: "apps/desktop/src/debug-fixture.ts",
      status: "requires_action",
      now: ids.now,
      order: ids.order,
      payload: {
        subkind: "write",
        interaction: TOOL_CONSENT_INTERACTION_KIND,
        requestId: ids.requestId,
        toolName: request.toolName,
        path: "apps/desktop/src/debug-fixture.ts",
        input: request.input as unknown as AgentTimelinePayload,
      },
    });
    emitDebugTimelineEvent(event);
    void requestLocalToolConsent(request).then(({ decision, message }) => {
      updateEvent(event, {
        status: decision === "allow" ? "success" : "cancelled",
        summary: decision === "allow" ? "Debug 权限已同意" : message || "Debug 权限已拒绝",
        payload: patchPayload(event, {
          decision,
          ...(message ? { message } : {}),
        }),
      });
    });
  }

  function emitCommand() {
    emitStaticDebugEvent(taskId, "command", {
      kind: "command",
      title: "Debug Bash",
      summary: "yarn verify:contracts",
      payload: {
        command: "yarn verify:contracts",
        cwd: "D:/PROJECT/workspace/Lilia",
        output: "Contracts verified.",
        exitCode: 0,
      },
    });
  }

  function emitFileRead() {
    emitStaticDebugEvent(taskId, "file-read", {
      kind: "file_read",
      title: "Debug Read",
      summary: "packages/contracts/src/index.ts",
      payload: {
        path: "packages/contracts/src/index.ts",
        output: "export interface AgentInteractionSettings { debug: boolean }",
      },
    });
  }

  function emitFileChange() {
    emitStaticDebugEvent(taskId, "file-change", {
      kind: "file_change",
      title: "Debug Edit",
      summary: "apps/desktop/src/pages/TaskDetail.vue",
      payload: {
        subkind: "edit",
        path: "apps/desktop/src/pages/TaskDetail.vue",
        changes: [
          { kind: "edit", path: "apps/desktop/src/pages/TaskDetail.vue" },
          { kind: "edit", path: "apps/desktop/src/components/chat/DebugTimelinePanel.vue" },
        ],
      },
    });
  }

  return {
    emitPlan,
    emitTodo,
    emitTodoTool,
    emitAskUser,
    emitAskUserMulti,
    emitAskUserPreview,
    emitAskUserFlow,
    emitPermission,
    emitCommand,
    emitFileRead,
    emitFileChange,
  };
}

