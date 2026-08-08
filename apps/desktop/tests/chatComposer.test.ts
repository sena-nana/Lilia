import { fireEvent, render, waitFor } from "@testing-library/vue";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  ASSISTANT_AI_OPTIMIZE_PROMPT_COMMAND,
  CHAT_READ_CLIPBOARD_FILE_PATHS_COMMAND,
  CHAT_SAVE_CLIPBOARD_IMAGE_COMMAND,
  CHAT_SAVE_CLIPBOARD_TEXT_COMMAND,
  type AskUserSpec,
  type ChatAttachment,
  type ChatComposerState,
  type ChatModelOption,
  type ModelFeatureSettings,
} from "@lilia/contracts";
import type { PendingAsk } from "../src/composables/useAskUser";
import { useAgentInteractionSettings } from "../src/composables/useAgentInteractionSettings";
import type { ToolConsentRequest } from "../src/services/chat";
import { setModelFeatureSettings } from "../src/services/chat";
import ChatComposer from "../src/components/chat/ChatComposer.vue";
import { mockInvoke, setMockClipboardFilePaths } from "./tauriMock";
import { placeEditableCaret } from "./domTestHelpers";

const baseState: ChatComposerState = {
  taskId: "task-1",
  backend: "claude",
  model: "claude-sonnet-4-6",
  planMode: false,
  goalMode: false,
  permission: "full",
};

const codexState: ChatComposerState = {
  ...baseState,
  backend: "codex",
  model: "gpt-5.5",
};

const codexModelOptions: ChatModelOption[] = [
  { id: "gpt-5.5", label: "GPT 5.5", backend: "codex" },
  { id: "gpt-5.4", label: "GPT 5.4", backend: "codex" },
  { id: "gpt-5.4-mini", label: "GPT 5.4 Mini", backend: "codex" },
];

const defaultModelFeatureSettings: ModelFeatureSettings = {
  chat: { light: null, normal: null, deep: null },
  title: null,
  suggestion: null,
  promptRouter: null,
  promptOptimize: null,
  autoTurnDecision: null,
};

function pendingAsk(
  spec: AskUserSpec,
  overrides: Partial<Pick<PendingAsk, "id" | "requestId">> = {},
): PendingAsk {
  return {
    id: overrides.id ?? 1,
    requestId: overrides.requestId ?? null,
    spec,
    taskId: "task-1",
    turnId: "turn-1",
    resolve: () => {},
  };
}

const singleAskWithOtherSpec: AskUserSpec = {
  title: "Claude 想确认一下",
  questions: [
    {
      id: "q-1",
      question: "选哪个方案？",
      mode: "single",
      allowOther: true,
      options: [
        { id: "a", label: "A" },
        { id: "b", label: "B" },
      ],
    },
  ],
};

const toolConsent: ToolConsentRequest = {
  taskId: "task-1",
  turnId: "turn-tool",
  backend: "claude",
  requestId: "tool-1",
  toolName: "Write",
  input: { file_path: "src/main.ts" },
  title: null,
  displayName: null,
  description: null,
  blockedPath: null,
  decisionReason: null,
  toolUseId: null,
};

const bashToolConsent: ToolConsentRequest = {
  ...toolConsent,
  requestId: "bash-tool-1",
  toolName: "Bash",
  input: { command: "pwd" },
};

const codexCommandToolConsent: ToolConsentRequest = {
  ...toolConsent,
  backend: "codex",
  requestId: "codex-command-tool-1",
  toolName: "item/commandExecution/requestApproval",
  input: { command: "yarn test" },
  title: "Codex command approval",
  displayName: "item/commandExecution/requestApproval",
  description: "yarn test",
  toolUseId: "codex-approval-1",
  availableDecisions: ["accept", "decline", "cancel"],
  cwd: "D:/PROJECT/workspace/Lilia",
};

const projectCwd = "D:\\PROJECT\\workspace\\Lilia";

function renderComposer(
  props: { state?: ChatComposerState; attachments?: ChatAttachment[]; [key: string]: unknown } = {},
) {
  return render(ChatComposer, {
    props: {
      state: baseState,
      attachments: [],
      ...props,
    },
  });
}

function renderRunningComposer() {
  return renderComposer({ sending: true });
}

const scrollHeights: number[] = [];
let scrollHeightDescriptor: PropertyDescriptor | undefined;

async function flushDeferredComposerWork(promiseTicks = 5) {
  await vi.advanceTimersByTimeAsync(32);
  await vi.dynamicImportSettled();
  for (let index = 0; index < promiseTicks; index += 1) {
    await Promise.resolve();
  }
}

const flushContextSearch = flushDeferredComposerWork;
const flushPasteTasks = flushDeferredComposerWork;
const flushToolbarLoad = () => flushDeferredComposerWork(2);

async function requestComposerChrome(view: ReturnType<typeof render>) {
  const composer = view.container.querySelector(".chat-composer");
  expect(composer).not.toBeNull();
  await fireEvent.pointerEnter(composer as HTMLElement);
  await fireEvent.focusIn(view.getByRole("textbox"));
  await flushToolbarLoad();
}

async function setComposerText(view: ReturnType<typeof render>, text: string) {
  const input = view.getByRole("textbox") as HTMLElement;
  if (input instanceof HTMLTextAreaElement) {
    await fireEvent.update(input, text);
    return input;
  }
  input.textContent = text;
  placeEditableCaret(input, text.length);
  await fireEvent.input(input);
  return input;
}

function composerText(input: HTMLElement): string {
  return input instanceof HTMLTextAreaElement ? input.value : input.textContent ?? "";
}

async function openComposerActionMenu(view: ReturnType<typeof render>) {
  await requestComposerChrome(view);
  await fireEvent.click(await view.findByRole("button", { name: "更多输入操作" }));
}

function createTextPasteEvent(text: string, html = ""): ClipboardEvent {
  const event = new Event("paste", { bubbles: true, cancelable: true }) as ClipboardEvent;
  Object.defineProperty(event, "clipboardData", {
    configurable: true,
    value: {
      files: [],
      items: [],
      getData: (type: string) => {
        if (type === "text/plain") return text;
        if (type === "text/html") return html;
        return "";
      },
    },
  });
  return event;
}

function createFilePasteEvent(files: File[]): ClipboardEvent {
  const event = new Event("paste", { bubbles: true, cancelable: true }) as ClipboardEvent;
  Object.defineProperty(event, "clipboardData", {
    configurable: true,
    value: {
      files,
      items: files.map((file) => ({
        kind: "file",
        type: file.type,
        getAsFile: () => file,
      })),
      getData: (type: string) => type === "text/plain" ? "" : "",
    },
  });
  return event;
}

function createPasteFile(input: {
  name: string;
  type?: string;
  bytes?: number[];
}): File {
  const bytes = new Uint8Array(input.bytes ?? []);
  return {
    name: input.name,
    type: input.type ?? "",
    arrayBuffer: async () => bytes.buffer.slice(0),
  } as File;
}

beforeEach(() => {
  vi.useFakeTimers();
  scrollHeights.length = 0;

  const textareaProto = HTMLTextAreaElement.prototype;
  scrollHeightDescriptor = Object.getOwnPropertyDescriptor(textareaProto, "scrollHeight");
  Object.defineProperty(textareaProto, "scrollHeight", {
    configurable: true,
    get() {
      return scrollHeights.shift() ?? 30;
    },
  });
});

afterEach(() => {
  const textareaProto = HTMLTextAreaElement.prototype;
  if (scrollHeightDescriptor) {
    Object.defineProperty(textareaProto, "scrollHeight", scrollHeightDescriptor);
  } else {
    delete (textareaProto as { scrollHeight?: number }).scrollHeight;
  }
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("ChatComposer", () => {
  it("权限下拉不显示已关闭的权限行为", async () => {
    const agentInteractionSettings = useAgentInteractionSettings();
    await agentInteractionSettings.update({
      permissionMode: "ask",
      permissionModeAvailability: {
        ask: true,
        readonly: true,
        full: false,
        free: false,
      },
    });

    try {
      const view = renderComposer({
        state: { ...baseState, permission: "ask" },
      });
      await requestComposerChrome(view);
      await fireEvent.click(await view.findByRole("button", { name: "询问" }));

      expect(await view.findByRole("option", { name: /询问/ })).toBeInTheDocument();
      expect(view.getByRole("option", { name: /只读/ })).toBeInTheDocument();
      expect(view.queryByRole("option", { name: /完全访问/ })).not.toBeInTheDocument();
      expect(view.queryByRole("option", { name: /自由实现/ })).not.toBeInTheDocument();
    } finally {
      await agentInteractionSettings.update({
        permissionModeAvailability: {
          ask: true,
          readonly: true,
          full: true,
          free: true,
        },
      });
    }
  });

  it("模型分配保存后刷新已挂载 Composer 的自动模型 state", async () => {
    await setModelFeatureSettings(defaultModelFeatureSettings);
    const view = renderComposer({
      state: {
        ...codexState,
        modelSelectionMode: "auto",
        reasoningEffort: null,
      },
      modelOptions: codexModelOptions,
    });
    await flushDeferredComposerWork();
    const emittedBefore = view.emitted("update:state")?.length ?? 0;

    await setModelFeatureSettings({
      ...defaultModelFeatureSettings,
      chat: { ...defaultModelFeatureSettings.chat, light: "gpt-5.4" },
    });
    await flushDeferredComposerWork();

    await waitFor(() => {
      const updates = view.emitted("update:state") ?? [];
      expect(updates.length).toBeGreaterThan(emittedBefore);
      expect(updates.at(-1)?.[0]).toMatchObject({
        model: "gpt-5.4",
        modelSelectionMode: "auto",
        reasoningEffort: null,
      });
    });
  });

  it("卸载时取消特殊输入刷新 paint 调度", async () => {
    const frameIds: number[] = [];
    const cancelAnimationFrame = vi.fn();
    vi.stubGlobal("requestAnimationFrame", vi.fn(() => {
      const nextId = 80 + frameIds.length;
      frameIds.push(nextId);
      return nextId;
    }));
    vi.stubGlobal("cancelAnimationFrame", cancelAnimationFrame);
    const view = renderComposer();

    await setComposerText(view, "@read");
    const lastFrameId = frameIds.at(-1);
    view.unmount();

    expect(lastFrameId).toBeDefined();
    expect(cancelAnimationFrame).toHaveBeenCalledWith(lastFrameId);
  });

  it("Agent 运行且空输入时发送按钮切为打断", async () => {
    const view = renderRunningComposer();

    const interrupt = view.getByRole("button", { name: "打断 Agent" });
    expect(interrupt).not.toBeDisabled();

    await fireEvent.click(interrupt);

    expect(view.emitted("interrupt")).toHaveLength(1);
    expect(view.emitted("send")).toBeUndefined();
  });


  it("Agent 运行但有输入时仍发送到调度队列", async () => {
    const view = renderRunningComposer();

    await setComposerText(view, "补充上下文");
    await fireEvent.click(view.getByRole("button", { name: "加入调度队列" }));

    expect(view.emitted("send")?.[0]).toEqual(["补充上下文", [], [], null]);
    expect(view.emitted("interrupt")).toBeUndefined();
  });


  it("主输入框只有空格时隐藏 placeholder 且不能发送", async () => {
    const view = renderComposer();

    const input = await setComposerText(view, "   ");

    expect(input).not.toHaveClass("is-empty");
    expect(view.getByRole("button", { name: "发送" })).toBeDisabled();

    await fireEvent.click(view.getByRole("button", { name: "发送" }));

    expect(view.emitted("send")).toBeUndefined();
  });

  it("空输入时禁用提示词优化", async () => {
    const view = renderComposer();

    await requestComposerChrome(view);

    expect(view.getByRole("button", { name: "优化提示词" })).toBeDisabled();
  });

  it("提示词优化按钮在发送按钮左侧且成功后替换草稿", async () => {
    const view = renderComposer({ projectCwd });
    const input = await setComposerText(view, "修一下输入框按钮");
    await requestComposerChrome(view);

    const optimize = view.getByRole("button", { name: "优化提示词" });
    const send = view.getByRole("button", { name: "发送" });
    expect(optimize.compareDocumentPosition(send) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();

    await fireEvent.click(optimize);
    await waitFor(() => {
      expect(composerText(input)).toBe("优化后：修一下输入框按钮");
    });

    expect(mockInvoke).toHaveBeenCalledWith(ASSISTANT_AI_OPTIMIZE_PROMPT_COMMAND, {
      input: {
        prompt: "修一下输入框按钮",
        attachments: [],
        conversationReferences: [],
        projectCwd,
        taskId: "task-1",
      },
    }, undefined);
    expect(view.emitted("send")).toBeUndefined();
  });

  it("提示词优化只展示 workflow 建议，应用后随发送透传", async () => {
    const view = renderComposer({ projectCwd });
    const input = await setComposerText(view, "请 review 当前改动");
    await requestComposerChrome(view);

    await fireEvent.click(view.getByRole("button", { name: "优化提示词" }));

    await waitFor(() => {
      expect(composerText(input)).toBe("优化后：请 review 当前改动");
    });
    expect(view.emitted("send")).toBeUndefined();

    expect(await view.findByText("建议作为 代码审查 发送")).toBeInTheDocument();
    await fireEvent.click(view.getByRole("button", { name: "应用" }));
    await fireEvent.click(view.getByRole("button", { name: "发送" }));

    expect(view.emitted("send")?.[0]).toEqual([
      "优化后：请 review 当前改动",
      [],
      [],
      {
        type: "lilia_review",
        target: { type: "uncommittedChanges" },
        delivery: "inline",
      },
    ]);
  });


  it("主输入框清空后只有浏览器填充 br 时显示 placeholder", async () => {
    const view = renderComposer();

    const input = await setComposerText(view, "写点东西");
    input.replaceChildren(document.createElement("br"));
    placeEditableCaret(input, 0);
    await fireEvent.input(input);

    expect(input).toHaveClass("is-empty");
  });


  it("@ 搜索选中结果后转为上下文附件并清理 mention", async () => {
    const view = renderComposer({ projectCwd });
    const input = view.getByRole("textbox") as HTMLElement;

    await setComposerText(view, "参考 @read");
    await flushContextSearch();
    await view.findByText("README.md");

    await fireEvent.keyDown(input, { key: "Enter", code: "Enter" });

    expect(view.emitted("add-context-attachment")?.[0]?.[0]).toMatchObject({
      name: "README.md",
      path: "D:\\PROJECT\\workspace\\Lilia\\README.md",
      kind: "file",
    });
    expect(view.emitted("send")).toBeUndefined();
    expect(composerText(input)).toContain("参考");
    expect(composerText(input)).toContain("README.md");
    expect(composerText(input)).not.toContain("D:\\PROJECT\\workspace\\Lilia\\README.md");
    expect(composerText(input)).not.toContain("@read");
  });

  it("@ 搜索会在首帧后再触发上下文查询，避免阻塞当前输入事件", async () => {
    const view = renderComposer({ projectCwd });

    mockInvoke.mockClear();
    await setComposerText(view, "参考 @read");
    expect(mockInvoke.mock.calls.some(([cmd]) => cmd === "chat_search_context_attachments")).toBe(false);

    await flushContextSearch();

    await waitFor(() => {
      expect(mockInvoke.mock.calls.some(([cmd]) => cmd === "chat_search_context_attachments")).toBe(true);
    });
    expect(await view.findByText("README.md")).toBeInTheDocument();
  });


  it("@ 搜索目录时 Tab 进入目录且不生成附件", async () => {
    const view = renderComposer({ projectCwd });
    const input = view.getByRole("textbox") as HTMLElement;

    await setComposerText(view, "@big");
    await flushContextSearch();
    await fireEvent.keyDown(input, { key: "Tab", code: "Tab" });
    await flushContextSearch();

    expect(view.emitted("add-context-attachment")).toBeUndefined();
    expect(composerText(input)).toBe("@big-dir/");
    expect(await view.findByText("inside.md")).toBeInTheDocument();
  });


  it("@ 绝对路径不存在时提示且不创建附件", async () => {
    const view = renderComposer({ projectCwd });
    const input = view.getByRole("textbox") as HTMLElement;

    await setComposerText(view, "@D:\\PROJECT\\workspace\\Lilia\\missing.md");
    await flushContextSearch();

    expect(await view.findByText(/路径不存在/)).toBeInTheDocument();
    await fireEvent.keyDown(input, { key: "Enter", code: "Enter" });

    expect(view.emitted("add-context-attachment")).toBeUndefined();
    expect(view.emitted("send")).toBeUndefined();
  });

  it("@ 普通无结果继续输入会自动收起搜索面板", async () => {
    const view = renderComposer({ projectCwd });

    await setComposerText(view, "@zzzz");
    await flushContextSearch();
    expect(await view.findByText("没有匹配的文件或目录")).toBeInTheDocument();

    await setComposerText(view, "@zzzzz");
    await waitFor(() => {
      expect(view.queryByRole("listbox", { name: "文件上下文搜索" })).toBeNull();
    });
  });

  it("@ 无结果自动收起后删回裸 @ 会重新打开搜索面板", async () => {
    const view = renderComposer({ projectCwd });

    await setComposerText(view, "@zzzz");
    await flushContextSearch();
    await setComposerText(view, "@zzzzz");
    await waitFor(() => {
      expect(view.queryByRole("listbox", { name: "文件上下文搜索" })).toBeNull();
    });
    await setComposerText(view, "@");
    await flushContextSearch();
    expect(await view.findByRole("listbox", { name: "文件上下文搜索" })).toBeInTheDocument();
  });

  it("@ 路径型无结果继续输入时不会自动收起搜索面板", async () => {
    const view = renderComposer({ projectCwd });

    await setComposerText(view, "@dist/not-there");
    await flushContextSearch();
    expect(await view.findByText("没有匹配的文件或目录")).toBeInTheDocument();

    await setComposerText(view, "@dist/not-there-more");
    await flushContextSearch();
    expect(await view.findByRole("listbox", { name: "文件上下文搜索" })).toBeInTheDocument();
  });

  it("输入 / 时展示斜杠命令并可选择执行", async () => {
    const view = renderComposer({ projectCwd });
    const input = view.getByRole("textbox") as HTMLElement;

    await setComposerText(view, "/he");
    await flushContextSearch();

    expect(await view.findByRole("listbox", { name: "斜杠命令" })).toBeInTheDocument();
    expect(await view.findByText("/help")).toBeInTheDocument();

    await fireEvent.keyDown(input, { key: "Enter", code: "Enter" });

    expect(view.emitted("execute-slash-command")?.[0]?.[0]).toEqual({
      type: "slash_command",
      commandId: "native:help",
      source: "native",
      arguments: {},
    });
    expect(view.emitted("send")).toBeUndefined();
    expect(composerText(input)).toBe("");
  });

  it("斜杠命令不会影响 @ 文件上下文选择", async () => {
    const view = renderComposer({ projectCwd });
    const input = view.getByRole("textbox") as HTMLElement;

    await setComposerText(view, "参考 @read");
    await flushContextSearch();
    await fireEvent.keyDown(input, { key: "Enter", code: "Enter" });

    expect(view.emitted("add-context-attachment")?.[0]?.[0]).toMatchObject({
      name: "README.md",
    });
    expect(view.emitted("execute-slash-command")).toBeUndefined();
  });

  it("文本粘贴会转成纯文本并去除富文本样式", async () => {
    const view = renderComposer();
    const input = await setComposerText(view, "前  后");
    placeEditableCaret(input, 1);
    const event = createTextPasteEvent(
      "粗体文本",
      '<span style="color: red"><strong>粗体文本</strong></span>',
    );

    input.dispatchEvent(event);

    expect(event.defaultPrevented).toBe(true);
    expect(composerText(input)).toBe("前粗体文本  后");
    expect(input.querySelector("span, strong")).toBeNull();
    expect(mockInvoke).not.toHaveBeenCalledWith(CHAT_READ_CLIPBOARD_FILE_PATHS_COMMAND, expect.anything());
    expect(mockInvoke).not.toHaveBeenCalledWith(CHAT_SAVE_CLIPBOARD_IMAGE_COMMAND, expect.anything());
    expect(mockInvoke).not.toHaveBeenCalledWith(CHAT_SAVE_CLIPBOARD_TEXT_COMMAND, expect.anything());
  });

  it("未达到阈值的长文本粘贴仍直接插入输入框", async () => {
    const view = renderComposer();
    const input = await setComposerText(view, "前  后");
    placeEditableCaret(input, 1);
    const text = "a".repeat(1999);
    const event = createTextPasteEvent(text);

    input.dispatchEvent(event);

    expect(event.defaultPrevented).toBe(true);
    expect(composerText(input)).toBe(`前${text}  后`);
    expect(mockInvoke).not.toHaveBeenCalledWith(CHAT_SAVE_CLIPBOARD_TEXT_COMMAND, expect.anything());
    expect(view.emitted("add-context-attachment")).toBeUndefined();
  });

  it("达到阈值的长文本粘贴会保存为临时文本上下文", async () => {
    const view = renderComposer();
    const input = await setComposerText(view, "参考  继续");
    placeEditableCaret(input, 3);
    const text = "b".repeat(2000);
    const event = createTextPasteEvent(text);

    input.dispatchEvent(event);
    await flushPasteTasks();

    expect(event.defaultPrevented).toBe(true);
    expect(mockInvoke).toHaveBeenCalledWith(CHAT_SAVE_CLIPBOARD_TEXT_COMMAND, {
      input: { text },
    }, undefined);
    await waitFor(() => {
      expect(view.emitted("add-context-attachment")?.length).toBe(1);
    });
    expect(view.emitted("add-context-attachment")?.[0]?.[0]).toMatchObject({
      name: "粘贴文本 1.txt",
      path: "C:\\Users\\mock\\.lilia\\cache\\clipboard-texts\\clipboard-1.txt",
      kind: "file",
      mime: null,
    });
    expect(composerText(input)).toContain("参考");
    expect(composerText(input)).toContain("粘贴文本 1.txt");
    expect(composerText(input)).toContain("继续");
    expect(composerText(input)).not.toContain(text);
  });

  it("粘贴文件和文件夹时按当前光标插入上下文引用", async () => {
    setMockClipboardFilePaths([
      "D:\\PROJECT\\workspace\\Lilia\\README.md",
      "D:\\PROJECT\\workspace\\Lilia\\big-dir",
    ]);
    const view = renderComposer();
    const input = await setComposerText(view, "参考  继续");
    placeEditableCaret(input, 3);
    const event = createFilePasteEvent([createPasteFile({ name: "README.md" })]);

    input.dispatchEvent(event);
    await waitFor(() => {
      expect(view.emitted("add-context-attachment")?.length).toBe(2);
    });

    expect(event.defaultPrevented).toBe(true);
    expect(view.emitted("add-context-attachment")?.map(([attachment]) => attachment)).toEqual([
      expect.objectContaining({
        name: "README.md",
        path: "D:\\PROJECT\\workspace\\Lilia\\README.md",
        kind: "file",
      }),
      expect.objectContaining({
        name: "big-dir",
        path: "D:\\PROJECT\\workspace\\Lilia\\big-dir",
        kind: "directory",
      }),
    ]);
    expect(composerText(input)).toContain("参考");
    expect(composerText(input)).toContain("README.md");
    expect(composerText(input)).toContain("big-dir");
    expect(composerText(input)).toContain("继续");
  });

  it("粘贴剪贴板图片时保存为临时图片上下文", async () => {
    const view = renderComposer();
    const input = await setComposerText(view, "看图 ");
    const event = createFilePasteEvent([createPasteFile({
      name: "screenshot.png",
      type: "image/png",
      bytes: [105, 109, 97, 103, 101],
    })]);

    input.dispatchEvent(event);
    await flushPasteTasks();

    expect(event.defaultPrevented).toBe(true);
    expect(mockInvoke).toHaveBeenCalledWith(CHAT_SAVE_CLIPBOARD_IMAGE_COMMAND, {
      input: expect.objectContaining({
        mime: "image/png",
        bytesBase64: expect.any(String),
      }),
    }, undefined);
    await waitFor(() => {
      expect(view.emitted("add-context-attachment")?.length).toBe(1);
    });
    expect(view.emitted("add-context-attachment")?.[0]?.[0]).toMatchObject({
      name: "图片 1.png",
      path: "C:\\Users\\mock\\.lilia\\cache\\clipboard-images\\clipboard-1.png",
      kind: "file",
      mime: "image/png",
    });
    expect(composerText(input)).toContain("图片 1.png");

    const pastedAttachment = view.emitted("add-context-attachment")?.[0]?.[0] as ChatAttachment;
    await view.rerender({
      state: baseState,
      attachments: [pastedAttachment],
    });
    await flushToolbarLoad();
    expect((await view.findByLabelText("图片预览")).querySelector("img")).toHaveAttribute(
      "src",
      "asset://C:/Users/mock/.lilia/cache/clipboard-images/clipboard-1.png",
    );
  });

  it("下方图片预览只显示缩略图", async () => {
    const view = renderComposer({
      attachments: [{
        id: "image-1",
        name: "图片 1.png",
        path: "D:\\PROJECT\\workspace\\Lilia\\shot.png",
        kind: "file",
        size: 42,
        exists: true,
        mime: "image/png",
        directory: null,
      }],
    });

    await flushToolbarLoad();
    const preview = await view.findByLabelText("图片预览");
    expect(preview.querySelector(".chat-attachment-chip--image-preview")).not.toBeNull();
    expect(preview.querySelector(".chat-attachment-chip__thumb")).not.toBeNull();
    expect(preview.querySelector(".chat-attachment-chip__name")).toBeNull();
    expect(preview.querySelector(".chat-attachment-chip__remove")).toBeNull();

    await fireEvent.click(view.getByRole("button", { name: "查看图片 图片 1.png" }));

    expect(view.emitted("open-image")?.[0]?.[0]).toMatchObject({
      src: "asset://D:/PROJECT/workspace/Lilia/shot.png",
      name: "图片 1.png",
      path: "D:\\PROJECT\\workspace\\Lilia\\shot.png",
      mime: "image/png",
      size: 42,
    });
  });

  it("重复粘贴同一路径不会重复插入", async () => {
    setMockClipboardFilePaths(["D:\\PROJECT\\workspace\\Lilia\\README.md"]);
    const view = renderComposer();
    const input = view.getByRole("textbox") as HTMLElement;

    input.dispatchEvent(createFilePasteEvent([createPasteFile({ name: "README.md" })]));
    await waitFor(() => {
      expect(view.emitted("add-context-attachment")?.length).toBe(1);
    });
    input.dispatchEvent(createFilePasteEvent([createPasteFile({ name: "README.md" })]));
    await Promise.resolve();

    const attachments = view.emitted("add-context-attachment") ?? [];
    expect(attachments).toHaveLength(1);
    expect(composerText(input).match(/README\.md/g)).toHaveLength(1);
  });


  it("加号菜单收纳输入框辅助动作", async () => {
    const view = renderComposer();

    await requestComposerChrome(view);
    expect(await view.findByRole("button", { name: "更多输入操作" })).toHaveAttribute(
      "aria-expanded",
      "false",
    );
    expect(view.queryByRole("menuitem", { name: "添加附件" })).toBeNull();
    expect(view.queryByRole("menuitemcheckbox", { name: "计划模式" })).toBeNull();

    await openComposerActionMenu(view);

    expect(view.getByRole("menuitem", { name: "添加附件" })).toBeInTheDocument();
    expect(view.getByRole("menuitem", { name: "引用其他对话" })).toBeInTheDocument();
    expect(view.getByRole("menuitemcheckbox", { name: "计划模式" })).toHaveAttribute(
      "aria-checked",
      "false",
    );
    expect(view.getByRole("menuitemcheckbox", { name: "目标模式" })).toHaveAttribute(
      "aria-checked",
      "false",
    );
  });

  it("显示并可清除分支锚点 chip", async () => {
    const view = renderComposer({
      pendingBranchAnchor: { sourceTurnId: "turn-1", mode: "fork" },
    });

    await requestComposerChrome(view);
    const chip = await view.findByRole("button", { name: /清除分叉锚点：turn-1/ });
    expect(chip).toHaveTextContent("分叉锚点");

    await fireEvent.click(chip);

    expect(view.emitted("clear-branch-anchor")?.length).toBe(1);
  });

  it("加号菜单会 Teleport 到 body，并在外部点击或 Escape 后关闭", async () => {
    const view = renderComposer();

    await openComposerActionMenu(view);

    const menu = await view.findByRole("menu");
    expect(document.body.contains(menu)).toBe(true);
    expect(view.container.contains(menu)).toBe(false);

    await fireEvent.pointerDown(document.body);

    await waitFor(() => {
      expect(view.queryByRole("menu")).toBeNull();
    });

    await openComposerActionMenu(view);
    expect(await view.findByRole("menu")).toBeInTheDocument();

    await fireEvent.keyDown(document, { key: "Escape" });

    await waitFor(() => {
      expect(view.queryByRole("menu")).toBeNull();
    });
  });

  it("加号菜单可触发添加附件", async () => {
    const view = renderComposer();

    await openComposerActionMenu(view);
    await fireEvent.click(view.getByRole("menuitem", { name: "添加附件" }));

    expect(view.emitted("pick-attachments")).toHaveLength(1);
    expect(view.queryByRole("menu")).toBeNull();
  });

  it("加号菜单会在输入框插入对话引用前缀", async () => {
    const view = renderComposer();

    await openComposerActionMenu(view);
    await fireEvent.click(view.getByRole("menuitem", { name: "引用其他对话" }));

    expect(composerText(view.getByRole("textbox") as HTMLElement)).toContain("#");
    expect(view.queryByRole("menu")).toBeNull();
  });

  it("输入 # 后可搜索并插入对话引用", async () => {
    const view = renderComposer();

    await setComposerText(view, "#Claude");
    await flushContextSearch();
    await fireEvent.click((await view.findAllByRole("option"))[0]);
    await fireEvent.click(view.getByRole("button", { name: "发送" }));

    expect(view.emitted("send")?.[0]?.[2]?.[0]).toEqual(expect.objectContaining({
      taskId: expect.any(String),
      title: expect.any(String),
      route: expect.any(String),
    }));
  });

  it("快速从上下文引用切到斜杠命令时只按最新输入加载控制器", async () => {
    const view = renderComposer({ state: codexState, projectCwd });

    await setComposerText(view, "@read");
    await setComposerText(view, "/re");
    await flushContextSearch();

    expect(await view.findByRole("option", { name: /\/review/ })).toBeInTheDocument();
    expect(view.queryByText("README.md")).toBeNull();
  });

  it("计划模式独立于执行权限切换", async () => {
    const view = renderComposer();

    await openComposerActionMenu(view);
    const planButton = view.getByRole("menuitemcheckbox", { name: "计划模式" });
    expect(planButton).toHaveAttribute("aria-checked", "false");

    await fireEvent.click(planButton);

    expect(view.emitted("update:state")?.[0]?.[0]).toMatchObject({
      planMode: true,
      goalMode: false,
      permission: "full",
    });
  });

  it("目标模式独立于计划模式和执行权限切换", async () => {
    const view = renderComposer();

    await openComposerActionMenu(view);
    const goalButton = view.getByRole("menuitemcheckbox", { name: "目标模式" });
    expect(goalButton).toHaveAttribute("aria-checked", "false");

    await fireEvent.click(goalButton);

    expect(view.emitted("update:state")?.[0]?.[0]).toMatchObject({
      planMode: false,
      goalMode: true,
      permission: "full",
    });
  });

  it("输入 /re 后可通过斜杠命令选择未提交改动审查", async () => {
    const view = renderComposer({ state: codexState, projectCwd });
    const input = view.getByRole("textbox") as HTMLElement;

    await setComposerText(view, "/re");
    await flushContextSearch();
    await fireEvent.click(await view.findByRole("option", { name: /\/review/ }));

    expect(await view.findByRole("option", { name: /未提交改动/ })).toBeInTheDocument();
    await fireEvent.click(view.getByRole("option", { name: /未提交改动/ }));

    expect(view.emitted("start-lilia-review")?.[0]).toEqual([
      "",
      [],
      [],
      { type: "uncommittedChanges" },
    ]);
    expect(view.emitted("execute-slash-command")).toBeUndefined();
    expect(composerText(input)).toBe("");
  });

  it("Codex review 斜杠命令会把输入框其他内容作为补充说明发出", async () => {
    const view = renderComposer({ state: codexState, projectCwd });
    await setComposerText(view, "重点看权限边界\n/re");
    await flushContextSearch();

    await fireEvent.click(await view.findByRole("option", { name: /\/review/ }));
    await fireEvent.click(await view.findByRole("option", { name: /未提交改动/ }));

    expect(view.emitted("start-lilia-review")?.[0]).toEqual([
      "重点看权限边界",
      [],
      [],
      { type: "uncommittedChanges" },
    ]);
  });

  it("输入 /frontend 后直接以 task workflow 发送", async () => {
    const view = renderComposer({ state: codexState, projectCwd });
    const input = view.getByRole("textbox") as HTMLElement;

    await setComposerText(view, "调整按钮布局\n/fr");
    await flushContextSearch();
    await fireEvent.click(await view.findByRole("option", { name: /\/frontend/ }));

    expect(view.emitted("send")?.[0]).toEqual([
      "调整按钮布局",
      [],
      [],
      { type: "lilia_task_workflow", kind: "frontend" },
    ]);
    expect(view.emitted("start-lilia-review")).toBeUndefined();
    expect(view.emitted("execute-slash-command")).toBeUndefined();
    expect(composerText(input)).toBe("");
  });

  it("Codex review 斜杠命令支持对比分支和指定提交目标", async () => {
    const promptSpy = vi.spyOn(window, "prompt");
    promptSpy.mockReturnValueOnce("main");
    promptSpy.mockReturnValueOnce("abc123");
    const view = renderComposer({ state: codexState, projectCwd });

    await setComposerText(view, "/re");
    await flushContextSearch();
    await fireEvent.click(await view.findByRole("option", { name: /\/review/ }));
    await fireEvent.click(await view.findByRole("option", { name: /对比分支/ }));
    await setComposerText(view, "/re");
    await flushContextSearch();
    await fireEvent.click(await view.findByRole("option", { name: /\/review/ }));
    await fireEvent.click(await view.findByRole("option", { name: /指定提交/ }));

    expect(view.emitted("start-lilia-review")?.[0]?.[3]).toEqual({
      type: "baseBranch",
      branch: "main",
    });
    expect(view.emitted("start-lilia-review")?.[1]?.[3]).toEqual({
      type: "commit",
      sha: "abc123",
    });
    promptSpy.mockRestore();
  });

  it("输入 /fi 后可通过斜杠命令发起修复建议", async () => {
    const view = renderComposer({ projectCwd });

    await setComposerText(view, "/fi");
    await flushContextSearch();
    await fireEvent.click(await view.findByRole("option", { name: /\/fix/ }));
    await fireEvent.click(await view.findByRole("option", { name: /未提交改动/ }));

    expect(view.emitted("start-lilia-fix-suggestion")?.[0]).toEqual([
      "",
      [],
      [],
      { type: "uncommittedChanges" },
    ]);

    await view.rerender({
      state: codexState,
      attachments: [],
      projectCwd,
    });
    await setComposerText(view, "/fi");
    await flushContextSearch();
    await fireEvent.click(await view.findByRole("option", { name: /\/fix/ }));
    await fireEvent.click(await view.findByRole("option", { name: /未提交改动/ }));
    expect(view.emitted("start-lilia-fix-suggestion")?.[1]).toEqual([
      "",
      [],
      [],
      { type: "uncommittedChanges" },
    ]);
  });

  it("Codex 修复建议会把输入框内容作为补充说明发出", async () => {
    const view = renderComposer({ state: codexState, projectCwd });
    await setComposerText(view, "优先给最小修复\n/fi");
    await flushContextSearch();

    await fireEvent.click(await view.findByRole("option", { name: /\/fix/ }));
    await fireEvent.click(await view.findByRole("option", { name: /未提交改动/ }));

    expect(view.emitted("start-lilia-fix-suggestion")?.[0]).toEqual([
      "优先给最小修复",
      [],
      [],
      { type: "uncommittedChanges" },
    ]);
  });

  it("Codex 修复建议支持对比分支和指定提交目标", async () => {
    const promptSpy = vi.spyOn(window, "prompt");
    promptSpy.mockReturnValueOnce("main");
    promptSpy.mockReturnValueOnce("abc123");
    const view = renderComposer({ state: codexState, projectCwd });

    await setComposerText(view, "/fi");
    await flushContextSearch();
    await fireEvent.click(await view.findByRole("option", { name: /\/fix/ }));
    await fireEvent.click(await view.findByRole("option", { name: /对比分支/ }));
    await setComposerText(view, "/fi");
    await flushContextSearch();
    await fireEvent.click(await view.findByRole("option", { name: /\/fix/ }));
    await fireEvent.click(await view.findByRole("option", { name: /指定提交/ }));

    expect(view.emitted("start-lilia-fix-suggestion")?.[0]?.[3]).toEqual({
      type: "baseBranch",
      branch: "main",
    });
    expect(view.emitted("start-lilia-fix-suggestion")?.[1]?.[3]).toEqual({
      type: "commit",
      sha: "abc123",
    });
    promptSpy.mockRestore();
  });

  it("工具栏不再显示代码审查和修复建议入口", async () => {
    const view = renderComposer();

    expect(view.queryByRole("button", { name: "代码审查" })).toBeNull();
    expect(view.queryByRole("button", { name: "修复建议" })).toBeNull();
  });

  it("阻塞 pending 交互时不显示斜杠 workflow 入口", async () => {
    const view = renderComposer({
      state: codexState,
      pendingAsk: pendingAsk(singleAskWithOtherSpec),
      projectCwd,
    });

    expect(view.queryByRole("button", { name: "代码审查" })).toBeNull();
    expect(view.queryByRole("listbox", { name: "斜杠命令" })).toBeNull();
    expect(view.emitted("start-lilia-review")).toBeUndefined();
    expect(view.emitted("start-lilia-fix-suggestion")).toBeUndefined();
  });

  it.skip("Claude 和 Codex 后端可从工具栏发起上下文压缩", async () => {
    const view = renderComposer();

    expect(view.queryByTitle("压缩上下文")).toBeNull();
    await requestComposerChrome(view);
    const compactButton = await view.findByRole("button", { name: /压缩上下文/ });
    expect(compactButton).not.toBeDisabled();
    expect(compactButton).toHaveClass("chat-composer__context-action");
    await fireEvent.click(compactButton);

    expect(view.emitted("start-lilia-compact")?.length).toBe(1);

    await view.rerender({
      state: codexState,
      attachments: [],
    });
    await requestComposerChrome(view);
    await fireEvent.click(await view.findByRole("button", { name: /压缩上下文/ }));
    expect(view.emitted("start-lilia-compact")?.length).toBe(2);
  });

  it("上下文圆环展示已用比例和具体占用信息", async () => {
    const view = renderComposer({
      contextUsage: {
        taskId: "task-1",
        backend: "claude",
        usedTokens: 7168,
        limitTokens: 8192,
        usedPercent: 87.5,
        source: "runtime",
        updatedAt: Date.UTC(2026, 5, 16, 8, 30),
        unavailableReason: null,
      },
    });

    await requestComposerChrome(view);
    const compactButton = await view.findByRole("button", { name: /已用 7,168 tokens/ });
    expect(compactButton).toHaveClass("chat-context-ring--warn");
    expect(compactButton).toHaveStyle({ "--quota-progress": "87.5" });
    const contextCard = await view.findByRole("tooltip");
    expect(contextCard).toHaveTextContent("已用");
    expect(contextCard).toHaveTextContent("占用");
    expect(contextCard).toHaveTextContent("87.5%");
  });

  it("上下文圆环无比例时显示空态但仍可触发压缩", async () => {
    const view = renderComposer({
      contextUsage: {
        taskId: "task-1",
        backend: "claude",
        usedTokens: 2048,
        limitTokens: null,
        usedPercent: null,
        source: "runtime",
        updatedAt: Date.UTC(2026, 5, 16, 8, 30),
        unavailableReason: "provider 未返回上下文上限",
      },
    });

    await requestComposerChrome(view);
    const compactButton = await view.findByRole("button", { name: /占用比例未知/ });
    expect(compactButton).toHaveClass("chat-context-ring--empty");
    expect(compactButton).toHaveStyle({ "--quota-progress": "0" });
    expect(await view.findByRole("tooltip")).toHaveTextContent("provider 未返回上下文上限");
    await fireEvent.click(compactButton);
    expect(view.emitted("start-lilia-compact")?.length).toBe(1);
  });

  it("Claude 和 Codex 后端不再从工具栏分叉当前会话", async () => {
    const view = renderComposer();

    expect(view.queryByRole("button", { name: "分叉当前会话" })).toBeNull();

    await view.rerender({
      state: codexState,
      attachments: [],
    });

    expect(view.queryByRole("button", { name: "分叉当前会话" })).toBeNull();
    expect(view.emitted("start-session-fork")).toBeUndefined();
  });

  it("工具栏不再暴露 IAB 入口", async () => {
    const view = renderComposer({ state: codexState });

    expect(view.queryByRole("button", { name: "打开 Lilia IAB" })).toBeNull();
    expect(view.queryByRole("button", { name: "回送 IAB 截图" })).toBeNull();
  });

  it("运行中也不在工具栏暴露 IAB 入口", async () => {
    const view = renderComposer();

    expect(view.queryByRole("button", { name: "打开 Lilia IAB" })).toBeNull();
    expect(view.queryByRole("button", { name: "回送 IAB 截图" })).toBeNull();

    await view.rerender({
      state: codexState,
      attachments: [],
      sending: true,
    });

    expect(view.queryByRole("button", { name: "打开 Lilia IAB" })).toBeNull();
    expect(view.queryByRole("button", { name: "回送 IAB 截图" })).toBeNull();
  });

  it("compact 入口在运行中或禁用状态时禁用", async () => {
    const view = renderComposer({ sending: true });

    await requestComposerChrome(view);
    expect(await view.findByRole("button", { name: /压缩上下文/ })).toBeDisabled();

    await view.rerender({
      state: baseState,
      attachments: [],
      sending: false,
      compactDisabled: true,
    });
    await requestComposerChrome(view);
    expect(await view.findByRole("button", { name: /压缩上下文/ })).toBeDisabled();
  });

  it("Codex 后端不再从工具栏清理后台终端", async () => {
    const view = renderComposer({ state: codexState });

    expect(view.queryByRole("button", { name: "清理 Codex 后台终端" })).toBeNull();
  });

  it("Codex 后端不再显示原生接口、Memory、Goal、Fork 和配置诊断入口", async () => {
    const view = renderComposer({ state: codexState });

    expect(view.queryByRole("button", { name: "Codex 原生接口" })).toBeNull();
    expect(view.queryByRole("button", { name: "Codex 高级字段" })).toBeNull();
    expect(view.queryByRole("menuitem", { name: "启用 Memory" })).toBeNull();
    expect(view.queryByRole("menuitem", { name: "关闭 Memory" })).toBeNull();
    expect(view.queryByRole("menuitem", { name: /重置 Memory/ })).toBeNull();
    expect(view.queryByRole("menuitem", { name: /设置 Goal/ })).toBeNull();
    expect(view.queryByRole("menuitem", { name: "Fork 当前 Thread" })).toBeNull();
    expect(view.queryByRole("menuitem", { name: "读取配置诊断" })).toBeNull();
  });

  it("原生接口入口在非 Codex、运行中和阻塞 pending 时都不显示", async () => {
    const view = renderComposer();

    expect(view.queryByRole("button", { name: "Codex 原生接口" })).toBeNull();

    await view.rerender({
      state: codexState,
      attachments: [],
      sending: true,
    });
    expect(view.queryByRole("button", { name: "Codex 原生接口" })).toBeNull();

    await view.rerender({
      state: codexState,
      attachments: [],
      sending: false,
      compactDisabled: true,
    });
    expect(view.queryByRole("button", { name: "Codex 原生接口" })).toBeNull();
  });

  it("后台终端清理入口不再显示在 composer 工具栏", async () => {
    const view = renderComposer();

    expect(view.queryByRole("button", { name: "清理 Codex 后台终端" })).toBeNull();

    await view.rerender({
      state: codexState,
      attachments: [],
      sending: true,
    });
    expect(view.queryByRole("button", { name: "清理 Codex 后台终端" })).toBeNull();

    await view.rerender({
      state: codexState,
      attachments: [],
      sending: false,
      compactDisabled: true,
    });
    expect(view.queryByRole("button", { name: "清理 Codex 后台终端" })).toBeNull();
  });

  it("pending AskUser 只有点击允许的其他选项后才显示输入框并返回 other", async () => {
    const view = renderComposer({
      pendingAsk: pendingAsk(singleAskWithOtherSpec),
    });

    expect(view.queryByRole("textbox")).toBeNull();
    await fireEvent.click(await view.findByRole("radio", { name: "其他" }));
    const input = view.getByRole("textbox");
    const entryRow = input.closest(".chat-composer__entry-row");
    expect(entryRow).not.toBeNull();
    expect(entryRow).toContainElement(view.getByRole("button", { name: "完成" }));
    await fireEvent.update(input, "我想选第三种");
    await fireEvent.click(view.getByRole("button", { name: "完成" }));

    expect(view.emitted("resolve-ask-user")?.[0]?.[0]).toEqual({
      cancelled: false,
      answers: {
        "q-1": {
          questionId: "q-1",
          value: "other",
          notes: "我想选第三种",
        },
      },
    });
  });

  it("同一 AskUser requestId 重新 hydrate 时不重新屏蔽关键动作", async () => {
    const view = renderComposer({
      pendingAsk: pendingAsk(singleAskWithOtherSpec, { id: 1, requestId: "ask-1" }),
    });

    await fireEvent.click(await view.findByRole("radio", { name: "A" }));
    await vi.advanceTimersByTimeAsync(320);
    expect(view.getByRole("button", { name: "完成" })).not.toBeDisabled();

    await view.rerender({
      state: baseState,
      attachments: [],
      pendingAsk: pendingAsk(singleAskWithOtherSpec, { id: 2, requestId: "ask-1" }),
    });

    expect(view.getByRole("button", { name: "完成" })).not.toBeDisabled();
  });

  it("状态切到工具授权后短暂屏蔽关键动作", async () => {
    const view = renderComposer();

    await view.rerender({
      state: baseState,
      attachments: [],
      toolConsent,
    });
    await vi.dynamicImportSettled();
    await fireEvent.update(await view.findByRole("textbox"), "先不要写这个文件");

    expect(await view.findByRole("button", { name: "同意" })).toBeDisabled();
    expect(view.getByRole("button", { name: "修改" })).toBeDisabled();

    await vi.advanceTimersByTimeAsync(320);
    expect(view.getByRole("button", { name: "同意" })).not.toBeDisabled();
    expect(view.getByRole("button", { name: "修改" })).not.toBeDisabled();
  });


  it("pending 工具授权中输入文本后修改按钮会作为拒绝备注返回", async () => {
    const view = renderComposer({ toolConsent });

    expect(await view.findByRole("button", { name: "忽略" })).toBeDisabled();
    await fireEvent.update(await view.findByRole("textbox"), "先不要写这个文件");
    await fireEvent.click(view.getByRole("button", { name: "修改" }));

    expect(view.emitted("resolve-tool-consent")?.[0]).toEqual([
      "deny",
      "先不要写这个文件",
    ]);
  });


  it("pending Bash 授权同意时返回用户修改后的 updatedInput", async () => {
    const view = renderComposer({ toolConsent: bashToolConsent });

    await fireEvent.click(await view.findByRole("button", { name: "编辑完整命令" }));
    await fireEvent.update(await view.findByRole("textbox", { name: "编辑命令" }), "pwd && echo ok");
    await fireEvent.click(view.getByRole("button", { name: "同意" }));

    expect(view.emitted("resolve-tool-consent")?.[0]).toEqual([
      "allow",
      undefined,
      { command: "pwd && echo ok" },
    ]);
  });

  it("pending Codex command approval 同意时返回用户修改后的 updatedInput", async () => {
    const view = renderComposer({ toolConsent: codexCommandToolConsent });

    await fireEvent.click(await view.findByRole("button", { name: "编辑完整命令" }));
    await fireEvent.update(await view.findByRole("textbox", { name: "编辑命令" }), "yarn test --runInBand");
    await fireEvent.click(view.getByRole("button", { name: "同意" }));

    expect(view.emitted("resolve-tool-consent")?.[0]).toEqual([
      "allow",
      undefined,
      { command: "yarn test --runInBand" },
    ]);
  });
});

