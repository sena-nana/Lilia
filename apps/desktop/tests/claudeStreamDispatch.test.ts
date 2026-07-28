import { describe, expect, it, vi } from "vitest";
import {
  appendClaudeBlockText,
  CLAUDE_BLOCK_TYPES,
  closeClaudeBlock,
  createClaudeStreamState,
  dispatchClaudeStreamEvent,
  extractClaudeBlockInitialText,
  extractClaudeTextDeltaText,
  extractClaudeThinkingDeltaText,
  finalizeClaudeReasoningBlocks,
  getClaudeBlockKey,
  getClaudeBlockText,
  getClaudeBlockType,
  openClaudeBlock,
} from "../legacy/agent-runner/claudeStream.mjs";

type StreamEvent = Record<string, unknown>;

function startBlock(index: number, blockType: string, extra: Record<string, unknown> = {}): StreamEvent {
  return {
    type: "content_block_start",
    index,
    content_block: { type: blockType, ...extra },
  };
}
function stopBlock(index: number): StreamEvent {
  return { type: "content_block_stop", index };
}
function thinkingDelta(index: number, text: string): StreamEvent {
  return {
    type: "content_block_delta",
    index,
    delta: { type: "thinking_delta", thinking: text },
  };
}
function textDelta(index: number, text: string): StreamEvent {
  return {
    type: "content_block_delta",
    index,
    delta: { type: "text_delta", text },
  };
}

function runDispatcher(events: StreamEvent[]) {
  const state = createClaudeStreamState();
  const calls: Array<
    | { channel: "textStart"; index: number; blockKey: number | null; initialText: string }
    | { channel: "textDelta"; index: number; blockKey: number | null; text: string }
    | { channel: "textClose"; index: number; blockKey: number | null }
    | { channel: "reasoning"; index: number; blockKey: number | null; text: string; blockType: string }
    | { channel: "reasoningClose"; index: number; blockKey: number | null; text: string; blockType: string }
  > = [];
  const textStarts: Array<{ index: number; blockKey: number | null; initialText: string }> = [];
  const textChunks: Array<{ index: number; blockKey: number | null; text: string }> = [];
  const textCloses: Array<{ index: number; blockKey: number | null }> = [];
  const reasoningSnapshots: Array<{
    index: number;
    blockKey: number | null;
    text: string;
    blockType: string;
  }> = [];
  const reasoningCloses: Array<{
    index: number;
    blockKey: number | null;
    text: string;
    blockType: string;
  }> = [];
  const onTextStart = vi.fn(
    (info: { index: number; blockKey: number | null; initialText: string }) => {
      textStarts.push(info);
      calls.push({ channel: "textStart", ...info });
    },
  );
  const onTextDelta = vi.fn(
    (info: { index: number; blockKey: number | null; text: string }) => {
      textChunks.push(info);
      calls.push({ channel: "textDelta", ...info });
    },
  );
  const onTextClose = vi.fn(
    (info: { index: number; blockKey: number | null }) => {
      textCloses.push(info);
      calls.push({ channel: "textClose", ...info });
    },
  );
  const onReasoning = vi.fn(
    (info: { index: number; blockKey: number | null; text: string; blockType: string }) => {
      reasoningSnapshots.push({
        index: info.index,
        blockKey: info.blockKey,
        text: info.text,
        blockType: info.blockType,
      });
      calls.push({
        channel: "reasoning",
        index: info.index,
        blockKey: info.blockKey,
        text: info.text,
        blockType: info.blockType,
      });
    },
  );
  const onReasoningClose = vi.fn(
    (info: { index: number; blockKey: number | null; text: string; blockType: string }) => {
      reasoningCloses.push(info);
      calls.push({ channel: "reasoningClose", ...info });
    },
  );
  for (const event of events) {
    dispatchClaudeStreamEvent({
      event,
      state,
      onTextStart,
      onTextDelta,
      onTextClose,
      onReasoning,
      onReasoningClose,
    });
  }
  return {
    state,
    calls,
    textStarts,
    textChunks,
    textCloses,
    reasoningSnapshots,
    reasoningCloses,
    onTextStart,
    onTextDelta,
    onTextClose,
    onReasoning,
    onReasoningClose,
  };
}

describe("dispatchClaudeStreamEvent", () => {
  it("fixture A：thinking → text 完整序列时两通道互不串字", () => {
    const thinkingParts = ["我先想想…", "也许应该", "拆成两步。"];
    const textParts = ["第一步：", "读文件；", "第二步：写文件。"];

    const { textChunks, reasoningSnapshots } = runDispatcher([
      { type: "message_start" },
      startBlock(0, CLAUDE_BLOCK_TYPES.THINKING),
      ...thinkingParts.map((t) => thinkingDelta(0, t)),
      stopBlock(0),
      startBlock(1, CLAUDE_BLOCK_TYPES.TEXT),
      ...textParts.map((t) => textDelta(1, t)),
      stopBlock(1),
    ]);

    expect(textChunks.map((c) => c.text)).toEqual(textParts);

    // 每条 thinking_delta 都产生一条累积快照。
    expect(reasoningSnapshots).toHaveLength(thinkingParts.length);
    const accumulated = thinkingParts.reduce<string[]>((acc, part) => {
      acc.push((acc[acc.length - 1] || "") + part);
      return acc;
    }, []);
    expect(reasoningSnapshots.map((s) => s.text)).toEqual(accumulated);

    // 关键不变量：reasoning 通道任何快照都不能含 text_delta 的文本。
    const allReasoningText = reasoningSnapshots.map((s) => s.text).join("\n");
    for (const t of textParts) {
      expect(allReasoningText).not.toContain(t);
    }

    // blockKey 跨 block 单调递增，dispatcher 维护跨 LLM turn 的稳定标识。
    const thinkingKey = reasoningSnapshots[0].blockKey;
    const textKey = textChunks[0].blockKey;
    expect(thinkingKey).not.toBeNull();
    expect(textKey).not.toBeNull();
    expect(textKey).not.toBe(thinkingKey);
  });


  it("fixture B：未知 block 类型两边都不路由，不抛错", () => {
    const { textChunks, reasoningSnapshots } = runDispatcher([
      startBlock(0, "future_unknown_block_type"),
      textDelta(0, "看上去像 text 但实际不是"),
      thinkingDelta(0, "也不该当 thinking"),
      stopBlock(0),
    ]);

    expect(textChunks).toEqual([]);
    expect(reasoningSnapshots).toEqual([]);
  });


  it("fixture C：text block 中混入伪造的 thinking_delta 不会污染思考缓冲（c23c0ef 反向回归）", () => {
    const { textChunks, reasoningSnapshots, state } = runDispatcher([
      startBlock(0, CLAUDE_BLOCK_TYPES.TEXT),
      textDelta(0, "正常 text"),
      // 模拟 SDK bug：在 text block 内部错发 thinking_delta。
      thinkingDelta(0, "<这段绝不能进 thinking 缓冲>"),
      textDelta(0, "继续正常 text"),
      stopBlock(0),
    ]);

    expect(textChunks.map((c) => c.text)).toEqual(["正常 text", "继续正常 text"]);
    expect(reasoningSnapshots).toEqual([]);
    expect(getClaudeBlockText(state, 0)).toBe(""); // text block 不累积 text 到 state（pacer 负责）
  });


  it("text block content_block_start 同步触发 onTextStart，先于任何 delta", () => {
    // 关键不变量：上层 runner 用 onTextStart 抢在 tool_use 落库前占住 timeline order。
    // 如果回调顺序错了（onTextDelta 先到），pacer 的 33ms 节流会让短开场白被
    // 同 turn 的 tool_use 抢到更小的 order——这是 commit before this 的回归点。
    const { calls } = runDispatcher([
      startBlock(0, CLAUDE_BLOCK_TYPES.TEXT),
      textDelta(0, "让我快速扫一下"),
      textDelta(0, "项目现状。"),
      stopBlock(0),
    ]);

    expect(calls[0]).toMatchObject({ channel: "textStart", index: 0, initialText: "" });
    expect(calls.slice(1)).toEqual([
      { channel: "textDelta", index: 0, blockKey: calls[0].blockKey, text: "让我快速扫一下" },
      { channel: "textDelta", index: 0, blockKey: calls[0].blockKey, text: "项目现状。" },
      { channel: "textClose", index: 0, blockKey: calls[0].blockKey },
    ]);
  });
});

describe("finalizeClaudeReasoningBlocks", () => {
  it("只回调 reasoning 类 block，回调后 state 清空", () => {
    const state = createClaudeStreamState();
    openClaudeBlock(state, 0, CLAUDE_BLOCK_TYPES.THINKING);
    openClaudeBlock(state, 1, CLAUDE_BLOCK_TYPES.TEXT);
    openClaudeBlock(state, 2, CLAUDE_BLOCK_TYPES.REDACTED_THINKING);
    appendClaudeBlockText(state, 0, "t0");
    appendClaudeBlockText(state, 2, "t2");

    const seen: Array<{ index: number; blockKey: number | null; text: string; blockType: string }> = [];
    finalizeClaudeReasoningBlocks(state, (info) => seen.push(info));

    expect(seen.map((s) => s.index).sort()).toEqual([0, 2]);
    expect(seen.find((s) => s.index === 0)?.text).toBe("t0");
    expect(seen.find((s) => s.index === 2)?.text).toBe("t2");
    expect(seen.find((s) => s.index === 0)?.blockKey).toBe(0);
    expect(seen.find((s) => s.index === 2)?.blockKey).toBe(2);
    expect(state.streamBlocks.size).toBe(0);
  });
});

