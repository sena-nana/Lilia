// Claude `stream_event` 路由器：把 Anthropic SDK 流式事件按 **content block 类型**
// 精确分发到「最终回复文本」与「思考增量」两条互斥通道。runner 主文件负责
// 拼接 sourceId / 调用 pacer / emit timeline；本文件只做协议层的状态机与分类，
// 测试可以脱离 runner 主进程直接 import 验证。
//
// 设计原则：
//   1. 唯一权威信号 = `content_block_start.content_block.type`（block 性质），
//      通过 `state.streamBlocks: Map<index, {type, accumulatedText}>` 维护。
//   2. `content_block_delta` 的归属完全由 block index → blockType 决定，**不再**
//      根据 delta.type / 字段名做 substring 猜测。
//   3. 抽取器（extract*DeltaText）一对一对应 delta 类型，不做 fallback 链。

export const CLAUDE_BLOCK_TYPES = Object.freeze({
  TEXT: "text",
  THINKING: "thinking",
  REDACTED_THINKING: "redacted_thinking",
  TOOL_USE: "tool_use",
});

export const CLAUDE_REASONING_BLOCK_TYPES = new Set([
  CLAUDE_BLOCK_TYPES.THINKING,
  CLAUDE_BLOCK_TYPES.REDACTED_THINKING,
]);

export function createClaudeStreamState() {
  return { streamBlocks: new Map(), nextBlockKey: 0 };
}

// blockKey 是 dispatcher 维护的全局单调递增计数器，每开一个 content_block 分一个。
// 这层 key 在「同 Claude session 内多个 LLM turn 都从 SDK index 0 开始重新计数」时
// 给上层一个稳定、不冲突的 block 标识，方便 runner 拿来拼 sourceId（如
// `${sessionId}:text:${blockKey}`）——直接用 SDK index 会让第二个 LLM turn 的
// text:0 覆盖第一个 turn 的 text:0。
export function openClaudeBlock(state, index, blockType) {
  if (!state || index === undefined || index === null) return;
  const blockKey = state.nextBlockKey++;
  state.streamBlocks.set(index, {
    type: typeof blockType === "string" ? blockType : "",
    accumulatedText: "",
    blockKey,
  });
}

export function closeClaudeBlock(state, index) {
  if (!state || index === undefined || index === null) return;
  state.streamBlocks.delete(index);
}

export function getClaudeBlockType(state, index) {
  if (!state || index === undefined || index === null) return "";
  return state.streamBlocks.get(index)?.type || "";
}

export function getClaudeBlockKey(state, index) {
  if (!state || index === undefined || index === null) return null;
  const entry = state.streamBlocks.get(index);
  return entry ? entry.blockKey : null;
}

export function appendClaudeBlockText(state, index, delta) {
  if (!state || index === undefined || index === null) return "";
  if (typeof delta !== "string" || delta.length === 0) {
    return state.streamBlocks.get(index)?.accumulatedText || "";
  }
  const entry = state.streamBlocks.get(index);
  if (!entry) return "";
  entry.accumulatedText += delta;
  return entry.accumulatedText;
}

export function getClaudeBlockText(state, index) {
  return state?.streamBlocks.get(index)?.accumulatedText || "";
}

export function extractClaudeTextDeltaText(streamEvent) {
  if (!streamEvent || streamEvent.type !== "content_block_delta") return null;
  const delta = streamEvent.delta;
  if (!delta || delta.type !== "text_delta") return null;
  return typeof delta.text === "string" ? delta.text : null;
}

export function extractClaudeThinkingDeltaText(streamEvent) {
  if (!streamEvent || streamEvent.type !== "content_block_delta") return null;
  const delta = streamEvent.delta;
  if (!delta || delta.type !== "thinking_delta") return null;
  return typeof delta.thinking === "string" ? delta.thinking : null;
}

// `content_block_start` 里 SDK 偶尔会把首段文本直接塞在 content_block 上，
// 这里按 block 类型走对应字段（text 块拿 `text`，thinking 块拿 `thinking`）。
export function extractClaudeBlockInitialText(streamEvent, blockType) {
  if (!streamEvent || streamEvent.type !== "content_block_start") return null;
  const block = streamEvent.content_block;
  if (!block) return null;
  if (blockType === CLAUDE_BLOCK_TYPES.TEXT) {
    return typeof block.text === "string" && block.text.length > 0 ? block.text : null;
  }
  if (CLAUDE_REASONING_BLOCK_TYPES.has(blockType)) {
    return typeof block.thinking === "string" && block.thinking.length > 0
      ? block.thinking
      : null;
  }
  return null;
}

/**
 * 单一入口：把一条 SDK `stream_event` 按 block 类型路由出去。
 *   - text block → onTextStart({ index, blockKey, initialText }) 一次，之后 onTextDelta({ index, blockKey, text })
 *   - text block 收到 content_block_stop 时 → onTextClose({ index, blockKey })
 *   - thinking / redacted_thinking block → onReasoning({ index, blockKey, text, eventType, deltaType, blockType })
 *   - thinking / redacted_thinking block 收到 content_block_stop 时 → onReasoningClose({ index, blockKey, text, blockType })
 *   - 未知 block / 顶层 message 事件 → 不触发（含 tool_use 流式 partial JSON）
 *
 * `onTextStart` 在 `content_block_start` 时同步触发，给上层一次机会**抢在 tool_use
 * 落库前**注册 text fragment 的 sourceId / 占住 timeline order。pacer 的 33ms 节流
 * 决定了 onTextDelta 的首次回调必然延后，光等 onTextDelta 会让短开场白被同 turn
 * 的 tool_use 挤到后面。
 *
 * `onTextClose` / `onReasoningClose` 都在 `content_block_stop` 时同步触发，让上层能
 * 在 block 真正结束的那一刻 finalize 它的 per-block pacer（emit 终态 + cancel timer）。
 * 否则一旦 text 后面紧跟 tool_use / 二次 thinking，回复卡的 status 会拖到 turn
 * 末尾的 result 才翻成 success，UI 一直停在蓝色 streaming + 闪烁光标。
 *
 * dispatcher 不发 timeline、不知道 pacer、不知道 sessionId——这些组合在 runner 层完成。
 * blockKey 与 index 同生命周期但跨 LLM turn 不复用，runner 用它做 sourceId 隔离。
 */
export function dispatchClaudeStreamEvent({
  event,
  state,
  onTextStart,
  onTextDelta,
  onTextClose,
  onReasoning,
  onReasoningClose,
}) {
  if (!event || typeof event !== "object") return;

  if (event.type === "content_block_start") {
    const blockType = typeof event.content_block?.type === "string"
      ? event.content_block.type
      : "";
    openClaudeBlock(state, event.index, blockType);
    const blockKey = getClaudeBlockKey(state, event.index);
    const initial = extractClaudeBlockInitialText(event, blockType);
    if (blockType === CLAUDE_BLOCK_TYPES.TEXT) {
      if (onTextStart) {
        onTextStart({
          index: event.index,
          blockKey,
          initialText: typeof initial === "string" ? initial : "",
        });
      }
      if (initial && onTextDelta) {
        // initial 文本走正常 delta 通道，pacer 才会把它纳入 buffer 节流；
        // 不要在 onTextStart 里把 initial 当占位内容写死。
        onTextDelta({ index: event.index, blockKey, text: initial });
      }
    } else if (initial && CLAUDE_REASONING_BLOCK_TYPES.has(blockType)) {
      const accumulated = appendClaudeBlockText(state, event.index, initial);
      if (onReasoning) {
        onReasoning({
          index: event.index,
          blockKey,
          text: accumulated,
          eventType: event.type,
          deltaType: null,
          blockType,
        });
      }
    }
    return;
  }

  if (event.type === "content_block_stop") {
    const entry = state?.streamBlocks?.get(event.index);
    if (entry) {
      if (CLAUDE_REASONING_BLOCK_TYPES.has(entry.type) && onReasoningClose) {
        onReasoningClose({
          index: event.index,
          blockKey: entry.blockKey,
          text: entry.accumulatedText,
          blockType: entry.type,
        });
      } else if (entry.type === CLAUDE_BLOCK_TYPES.TEXT && onTextClose) {
        // text block 不在 state 里累积文本（pacer 独占），所以不带 text；
        // runner 持有 fragment.accumulatedText，按 blockKey 自己取。
        onTextClose({
          index: event.index,
          blockKey: entry.blockKey,
        });
      }
    }
    closeClaudeBlock(state, event.index);
    return;
  }

  if (event.type !== "content_block_delta") return;

  const blockType = getClaudeBlockType(state, event.index);
  const blockKey = getClaudeBlockKey(state, event.index);

  if (blockType === CLAUDE_BLOCK_TYPES.TEXT) {
    const text = extractClaudeTextDeltaText(event);
    if (text && onTextDelta) {
      onTextDelta({ index: event.index, blockKey, text });
    }
    return;
  }

  if (CLAUDE_REASONING_BLOCK_TYPES.has(blockType)) {
    const text = extractClaudeThinkingDeltaText(event);
    if (text) {
      const accumulated = appendClaudeBlockText(state, event.index, text);
      if (onReasoning) {
        onReasoning({
          index: event.index,
          blockKey,
          text: accumulated,
          eventType: event.type,
          deltaType: event.delta?.type,
          blockType,
        });
      }
    }
  }
  // 其它 blockType（tool_use / 未知 / 缺失）一律静默——避免误归类。
}

/**
 * Turn 结束时把所有仍在册的 reasoning block 翻成 success（标题从「正在思考」
 * → 「已思考」）。caller 通过 onReasoning 收到每个 block 的最终累积文本。
 * 调用后 state 会清空——下一个 turn 用 fresh state 即可。
 */
export function finalizeClaudeReasoningBlocks(state, onReasoning) {
  if (!state || state.streamBlocks.size === 0) return;
  for (const [index, entry] of state.streamBlocks) {
    if (!CLAUDE_REASONING_BLOCK_TYPES.has(entry.type)) continue;
    if (onReasoning) {
      onReasoning({
        index,
        blockKey: entry.blockKey,
        text: entry.accumulatedText,
        blockType: entry.type,
      });
    }
  }
  state.streamBlocks.clear();
}
