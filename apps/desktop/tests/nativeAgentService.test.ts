import { describe, expect, it } from "vitest";
import {
  productApprovalDecisionFromPermissionResult,
} from "../src/services/nativeAgent";

describe("nativeAgent approval mapping", () => {
  it("builds ProductApprovalDecision from native providerContext", () => {
    expect(productApprovalDecisionFromPermissionResult("fallback-id", {
      action: "approve",
      providerContext: {
        native: {
          sessionId: "sess-1",
          turnId: "turn-1",
          actionId: "act-1",
          version: 2,
        },
      },
    })).toEqual({
      sessionId: "sess-1",
      turnId: "turn-1",
      actionId: "act-1",
      version: 2,
      approved: true,
    });
  });

  it("marks deny decisions and falls back to requestId", () => {
    expect(productApprovalDecisionFromPermissionResult("req-9", {
      action: "decline",
      providerContext: {
        native: {
          sessionId: "sess-9",
          turnId: "turn-9",
          version: 1,
        },
      },
    })).toEqual({
      sessionId: "sess-9",
      turnId: "turn-9",
      actionId: "req-9",
      version: 1,
      approved: false,
    });
  });

  it("returns null for legacy codex permission context", () => {
    expect(productApprovalDecisionFromPermissionResult("permission-1", {
      action: "approve",
      providerContext: {
        codex: {
          threadId: "thread-1",
          turnId: "turn-1",
          itemId: "item-1",
        },
      },
    })).toBeNull();
  });
});
