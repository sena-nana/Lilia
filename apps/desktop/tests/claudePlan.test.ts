import { describe, expect, it } from "vitest";
import {
  CLAUDE_PLAN_TOOL_NAMES,
  CLAUDE_READONLY_ALLOWED_TOOLS,
  CLAUDE_READONLY_DENIED_TOOLS,
} from "@lilia/contracts/claudePlanContract.mjs";
import {
  PLAN_APPROVAL_QUESTION_ID,
  buildPlanApprovalSpec,
  buildPlanPayload,
  extractPlanResult,
  isClaudePlanTool,
  isPlanApprovalAccepted,
  isReadonlyDeniedClaudeTool,
  normalizeClaudePermissionMode,
  readPlanRevisionRequest,
} from "../legacy/agent-runner/claudePlan.mjs";

describe("claudePlan helpers", () => {
  it("从 ExitPlanMode input 提取计划和允许提示", () => {
    const payload = buildPlanPayload({
      input: {
        plan: "## 计划\n- 先读代码\n- 再修改",
        allowedPrompts: [
          { tool: "Bash", prompt: "yarn test" },
          { tool: "Write", prompt: "" },
        ],
      },
      approved: null,
      executionPermission: "full",
    });

    expect(payload).toMatchObject({
      source: "ExitPlanMode",
      plan: "## 计划\n- 先读代码\n- 再修改",
      approved: null,
      executionPermission: "full",
      allowedPrompts: [{ tool: "Bash", prompt: "yarn test" }],
    });
  });

  it("从工具结果 JSON 提取 Claude 返回的计划元数据", () => {
    const result = extractPlanResult(JSON.stringify({
      plan: "确认后的计划",
      filePath: "plans/current.md",
      planWasEdited: true,
      awaitingLeaderApproval: true,
    }));

    expect(result).toMatchObject({
      plan: "确认后的计划",
      filePath: "plans/current.md",
      planWasEdited: true,
      awaitingLeaderApproval: true,
    });
  });

  it("解析计划确认的接受、取消和修改要求结果", () => {
    expect(isPlanApprovalAccepted({
      answers: { [PLAN_APPROVAL_QUESTION_ID]: { value: "yes" } },
    })).toBe(true);
    expect(isPlanApprovalAccepted({ cancelled: true })).toBe(false);
    expect(isPlanApprovalAccepted({
      answers: { [PLAN_APPROVAL_QUESTION_ID]: { value: "no" } },
    })).toBe(false);
    expect(readPlanRevisionRequest({
      cancelled: false,
      answers: {
        [PLAN_APPROVAL_QUESTION_ID]: {
          questionId: PLAN_APPROVAL_QUESTION_ID,
          value: "revision_request",
          notes: "先补充失败回滚方案",
        },
      },
    })).toBe("先补充失败回滚方案");
    expect(readPlanRevisionRequest({
      answers: { [PLAN_APPROVAL_QUESTION_ID]: { value: "no" } },
    })).toBe("");
  });

  it("执行权限映射与只读写工具门禁保持分层", () => {
    expect(normalizeClaudePermissionMode("full")).toBe("bypassPermissions");
    expect(normalizeClaudePermissionMode("free")).toBe("bypassPermissions");
    expect(normalizeClaudePermissionMode("ask")).toBe("default");
    expect(normalizeClaudePermissionMode("readonly")).toBe("default");
    expect(CLAUDE_PLAN_TOOL_NAMES).toEqual(["ExitPlanMode", "exit_plan_mode"]);
    expect(CLAUDE_READONLY_DENIED_TOOLS).toContain("Write");
    expect(CLAUDE_READONLY_ALLOWED_TOOLS).toContain("Read");
    expect(isClaudePlanTool("ExitPlanMode")).toBe(true);
    expect(isClaudePlanTool("exit_plan_mode")).toBe(true);
    expect(isClaudePlanTool("Write")).toBe(false);
    expect(isReadonlyDeniedClaudeTool("Write")).toBe(true);
    expect(isReadonlyDeniedClaudeTool("UnknownTool")).toBe(true);
    expect(isReadonlyDeniedClaudeTool("Read")).toBe(false);
    expect(isReadonlyDeniedClaudeTool("TodoWrite")).toBe(false);
  });
});

