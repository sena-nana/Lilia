import type {
  AgentTimelineDisplay,
  AgentTimelineDisplayDetail,
  AgentTimelineEvent,
  AgentTimelineEventKind,
  AgentInteractionRequest,
  AgentInteractionResponse,
  AgentInteractionSettings,
  ChatAttachment,
  ChatContextSearchResult,
  ChatRuntimeSnapshot,
  ChatMessage,
  ChatRuntimeCommand,
  ChatSlashCommandSearchResult,
  ChatWorkflow,
  LiliaIabSnapshot,
  LiliaIabSubmitResult,
  LiliaReviewTarget,
  ProjectRoadmap,
  SuggestionItem,
  TaskTodo,
  TimelineDisplayInput,
  ToolConsentRequest,
  ToolConsentResponsePayload,
  ProjectArchitectureGraph,
  ProjectArchitectureChange,
  ProjectArchitectureInteractionPayload,
  PermissionMode,
  ProviderRuntimeOptions,
  QuotaUsageStats,
  AutomationRunSummary,
  HookDocumentUpdateInput,
  HooksOverview,
  SidebarConversationSummary,
  RemoteRetryChatRequest,
  RemoteTimelineSnapshotRequest,
  RemoteResponsePayload,
  AgentDebugSnapshot,
  AgentDebugAction,
} from "./index";
import { deriveTimelineDisplay } from "./index";

type Assert<T extends true> = T;
type Extends<From, To> = From extends To ? true : false;

type DetailCases =
  | { type: "line"; text: "done"; tone: "muted" }
  | { type: "fields"; fields: [{ label: "cwd"; value: "C:/repo" }] }
  | { type: "code"; label: "OUTPUT"; content: "ok"; language: "text" }
  | { type: "markdown"; content: "**ok**"; singleLine: false }
  | { type: "list"; items: [{ text: "one"; tone: "success" }]; ordered: true };

export type AgentTimelineOpenKindTypeTest = Assert<
  Extends<"extension_index", AgentTimelineEventKind>
>;

export type AgentTimelineEventHasNoDisplayTypeTest = Assert<
  Extends<"display" extends keyof AgentTimelineEvent ? true : false, false>
>;

export type DeriveTimelineDisplayReturnsDisplayTypeTest = Assert<
  Extends<ReturnType<typeof deriveTimelineDisplay>, AgentTimelineDisplay>
>;

export type TimelineDisplayInputShapeTypeTest = Assert<
  Extends<
    {
      kind: "command";
      status: "success";
      title: "Bash";
      summary: null;
      payload: { command: "ls" };
    },
    TimelineDisplayInput
  >
>;

export type AgentTimelineDisplayDetailSchemaTypeTest = Assert<
  Extends<DetailCases, AgentTimelineDisplayDetail>
>;

export type ChatAttachmentSchemaTypeTest = Assert<
  Extends<
    {
      id: "att-1";
      name: "README.md";
      path: "D:/PROJECT/workspace/Lilia/README.md";
      kind: "file";
      size: 42;
      exists: true;
      mime: null;
      directory: null;
    },
    ChatAttachment
  >
>;

export type LiliaIabSnapshotTypeTest = Assert<
  Extends<
    {
      taskId: "task-1";
      url: "https://example.com/";
      title: "Example";
      note: "按钮没有响应";
      capturedAt: 1;
      screenshotPath: "C:/Users/me/.lilia/cache/iab-snapshots/iab.png";
      screenshotAttachment: ChatAttachment;
      status: "captured";
      warning: null;
    },
    LiliaIabSnapshot
  >
>;

export type LiliaIabSubmitResultTypeTest = Assert<
  Extends<
    {
      snapshot: LiliaIabSnapshot;
      delivery: "runner";
      stdinForwarded: true;
    },
    LiliaIabSubmitResult
  >
>;

export type ChatContextSearchResultSchemaTypeTest = Assert<
  Extends<
    {
      attachment: {
        id: "att-1";
        name: "src";
        path: "D:/PROJECT/workspace/Lilia/src";
        kind: "directory";
        size: null;
        directory: {
          fileCount: 12;
          directoryCount: 3;
          totalSize: 2048;
          truncated: false;
          unreadableCount: 0;
        };
      };
      relativePath: "src";
      matchedBy: "name";
    },
    ChatContextSearchResult
  >
>;

export type ChatMessageIncludesAttachmentsTypeTest = Assert<
  Extends<
    {
      id: "u-1";
      taskId: "t-1";
      role: "user";
      content: "see attached";
      attachments: [];
      createdAt: 1;
    },
    ChatMessage
  >
>;

export type LiliaFixSuggestionWorkflowTypeTest = Assert<
  Extends<
    {
      type: "lilia_fix_suggestion";
      target: { type: "baseBranch"; branch: "main" };
      instructions: "只给建议";
      mode: "suggest";
    },
    ChatWorkflow
  >
>;

export type LiliaBatchApplyWorkflowTypeTest = Assert<
  Extends<
    {
      type: "lilia_batch_apply";
      sourceTurnId: "turn-1";
      sourceKind: "fix_suggestion";
      sourceSummary: "建议修复权限边界";
      instructions: "应用最小改动";
    },
    ChatWorkflow
  >
>;

export type LiliaTaskWorkflowTypeTest = Assert<
  Extends<
    {
      type: "lilia_task_workflow";
      kind: "frontend";
      instructions: "调整按钮布局";
    },
    ChatWorkflow
  >
>;

export type SessionForkRuntimeCommandTypeTest = Assert<
  Extends<
    {
      type: "session_fork";
      excludeTurns: true;
      sourceTurnId: "turn-1";
      mode: "fork";
    },
    ChatRuntimeCommand
  >
>;

export type SessionForkIsNotWorkflowTypeTest = Assert<
  Extends<
    {
      type: "session_fork";
      excludeTurns: true;
    },
    ChatWorkflow
  > extends true ? false : true
>;

export type RemoteTimelineSnapshotPaginationRequestTypeTest = Assert<
  Extends<
    {
      type: "timeline.snapshot";
      taskId: "task-1";
      limit: 80;
      direction: "before";
      cursor: "opaque-cursor";
    },
    RemoteTimelineSnapshotRequest
  >
>;

export type RemoteTimelinePaginationResponseTypeTest = Assert<
  Extends<
    {
      type: "timeline.snapshot";
      taskId: "task-1";
      events: [];
      page: {
        beforeCursor: "older";
        afterCursor: "newer";
        hasMoreBefore: true;
        hasMoreAfter: false;
      };
    },
    RemoteResponsePayload
  >
>;

export type SessionManagementRuntimeCommandTypeTest = Assert<
  Extends<
    {
      type: "session_management";
      action: "tag";
      sessionId: "thread-1";
      tag: "release";
      archived: true;
      limit: 20;
      cursor: "cursor-1";
      searchTerm: "bug";
      includeSystemMessages: true;
    },
    ChatRuntimeCommand
  >
>;

export type RemoteRetryChatRequestWithoutEventTypeTest = Assert<
  Extends<
    {
      type: "chat.retry";
      taskId: "task-1";
    },
    RemoteRetryChatRequest
  >
>;

export type RemoteRetryChatRequestWithEventTypeTest = Assert<
  Extends<
    {
      type: "chat.retry";
      taskId: "task-1";
      eventId: "error-1";
    } | {
      type: "chat.retry";
      taskId: "task-1";
      eventId: null;
    },
    RemoteRetryChatRequest
  >
>;

export type AgentDebugSnapshotTypeTest = Assert<
  Extends<
    {
      enabled: true;
      route: "/projects";
      title: "LiliaCode";
      capturedAt: 1;
      viewport: { x: 0; y: 0; width: 960; height: 600 };
      activeElement: "chat.composer.input";
      visibleText: "Lilia";
      elements: [{
        id: "chat.composer.send";
        role: "button";
        text: "发送";
        tagName: "button";
        selector: "[data-agent-id=\"chat.composer.send\"]";
        rect: { x: 1; y: 2; width: 3; height: 4 };
        visible: true;
        enabled: true;
        checked: null;
        expanded: null;
      }];
      missingAgentIds: [{
        role: "menuitemradio";
        text: "未标记模型";
        tagName: "button";
        selector: "button:nth-of-type(2)";
        rect: { x: 1; y: 2; width: 3; height: 4 };
        nearestAgentId: null;
      }];
      errors: [];
      invokes: [];
    },
    AgentDebugSnapshot
  >
>;

export type AgentDebugActionTypeTest = Assert<
  Extends<
    | { type: "click"; target: "chat.composer.send"; actionId: "act-1" }
    | { type: "focus"; target: "chat.composer.input" }
    | { type: "type"; target: "chat.composer.input"; text: "hello"; clear: true }
    | { type: "hotkey"; keys: ["Control", "Enter"] },
    AgentDebugAction
  >
>;

export type SessionManagementIsNotWorkflowTypeTest = Assert<
  Extends<
    {
      type: "session_management";
      action: "list";
    },
    ChatWorkflow
  > extends true ? false : true
>;

export type RuntimeSettingsCommandTypeTest = Assert<
  Extends<
    {
      type: "runtime_settings";
      action: "update";
    },
    ChatRuntimeCommand
  >
>;

export type RuntimeSettingsCommandRejectsInlineOptionsTypeTest = Assert<
  Extends<
    {
      type: "runtime_settings";
      action: "update";
      common: { model: "gpt-5.5"; permission: "ask" };
      runtimeOptions: ProviderRuntimeOptions;
    },
    ChatRuntimeCommand
  > extends true ? false : true
>;

export type RuntimeSettingsIsNotWorkflowTypeTest = Assert<
  Extends<
    {
      type: "runtime_settings";
      action: "update";
    },
    ChatWorkflow
  > extends true ? false : true
>;

export type RemoteEnvironmentRuntimeCommandTypeTest = Assert<
  Extends<
    {
      type: "remote_environment";
      action: "add";
      environmentId: "env-1";
      environment: { id: "env-1"; name: "Windows VM" };
    },
    ChatRuntimeCommand
  >
>;

export type RemoteEnvironmentIsNotWorkflowTypeTest = Assert<
  Extends<
    {
      type: "remote_environment";
      action: "select";
      environmentId: "env-1";
    },
    ChatWorkflow
  > extends true ? false : true
>;

export type SandboxDiagnosticsRuntimeCommandTypeTest = Assert<
  Extends<
    {
      type: "sandbox_diagnostics";
      includeDetails: true;
    },
    ChatRuntimeCommand
  >
>;

export type SandboxDiagnosticsIsNotWorkflowTypeTest = Assert<
  Extends<
    {
      type: "sandbox_diagnostics";
      includeDetails: true;
    },
    ChatWorkflow
  > extends true ? false : true
>;

export type ProcessSessionRuntimeCommandTypeTest = Assert<
  Extends<
    {
      type: "process_session";
      action: "spawn";
      command: "npm test -- --watch=false";
      cwd: "D:/PROJECT/workspace/Lilia";
      env: { CI: "1" };
      tty: true;
      rows: 24;
      cols: 100;
      permissionProfile: ":workspace";
    },
    ChatRuntimeCommand
  >
>;

export type ProcessSessionControlRuntimeCommandTypeTest = Assert<
  Extends<
    {
      type: "process_session";
      action: "write_stdin";
      processId: "proc-1";
      stdin: "q";
    },
    ChatRuntimeCommand
  >
>;

export type ProcessSessionIsNotWorkflowTypeTest = Assert<
  Extends<
    {
      type: "process_session";
      action: "kill";
      processId: "proc-1";
    },
    ChatWorkflow
  > extends true ? false : true
>;

export type HooksOverviewShapeTypeTest = Assert<
  Extends<
    {
      sources: [
        {
          id: "native-user";
          backend: "native-agentkit";
          scope: "user";
          format: "settings_json";
          name: "Native User Hooks";
          path: "C:/Users/mock/.lilia/settings.json";
          exists: true;
          editable: true;
          managed: false;
          enabled: true;
          handlerCount: 1;
          warnings: [];
          limitations: [];
          trustState: "unknown";
          description: "用户 hooks 配置";
        },
      ];
      warnings: [];
    },
    HooksOverview
  >
>;

export type HookDocumentUpdateInputShapeTypeTest = Assert<
  Extends<
    {
      handlers: [
        {
          id: "handler-1";
          event: "PostToolUse";
          matcher: "Bash";
          type: "command";
          command: "node hook.js";
          commandWindows: "powershell -File hook.ps1";
          timeoutSeconds: 30;
          statusMessage: "Running hook";
          groupAdvancedJson: "{\"shared\":true}";
          advancedJson: "{\"env\":{\"MODE\":\"test\"}}";
        },
      ];
    },
    HookDocumentUpdateInput
  >
>;

export type ProviderRuntimeOptionsTypeTest = Assert<
  Extends<
    {
      common: {
        model: "gpt-5.5";
        permission: "ask";
        runtimeWorkspaceRoots: ["D:/PROJECT/workspace/Lilia"];
      };
      provider: {
        "native-agentkit": {
          profile: "deep";
          reasoningEffort: "high";
          runtimeWorkspaceRoots: ["D:/PROJECT/workspace/Lilia"];
          persistExtendedHistory: true;
          environments: [{ id: "env-1" }];
          experimentalRawEvents: true;
          responsesApiClientMetadata: { surface: "lilia" };
          allowedTools: ["Read"];
          disallowedTools: ["Bash"];
          additionalDirectories: ["D:/PROJECT/workspace/Lilia/docs"];
          maxTurns: 8;
          maxBudgetUsd: 1.5;
          tools: { type: "preset"; preset: "default" };
          permissionPromptToolName: "mcp__lilia__permission_prompt";
          settings: { model: "claude-sonnet-4-6" };
          managedSettings: { sandbox: { enabled: true } };
          settingSources: ["user", "project"];
          sandbox: { enabled: true };
          outputFormat: { type: "json" };
          includeHookEvents: true;
          forwardSubagentText: true;
          agentProgressSummaries: true;
          continue: true;
          resumeSessionAt: "message-uuid";
          sessionId: "00000000-0000-4000-8000-000000000001";
          abortAfterMs: 3000;
          sessionStore: { explicit: true };
        };
      };
      experimentalProviderOptions: [{
        provider: "native-agentkit";
        capability: "raw-events";
        payload: { enabled: true };
        fallback: "diagnostic";
      }];
    },
    ProviderRuntimeOptions
  >
>;

export type ChatRuntimeSnapshotContextUsageTypeTest = Assert<
  Extends<
    {
      taskId: "task-1";
      phase: "running";
      backend: "native-agentkit";
      turnId: "turn-1";
      processSessionId: "jsonl-process-1";
      queuedCount: 0;
      pendingRollback: false;
      pendingResetCleanup: false;
      contextUsage: {
        taskId: "task-1";
        backend: "native-agentkit";
        usedTokens: 1024;
        limitTokens: 8192;
        usedPercent: 12.5;
        source: "runtime";
        updatedAt: 1;
        unavailableReason: null;
      };
      rollback: null;
    },
    ChatRuntimeSnapshot
  >
>;

export type SidebarConversationSummaryDependencyTypeTest = Assert<
  Extends<
    {
      taskId: "task-1";
      projectId: "project-1";
      projectName: "Lilia";
      title: "Android remote";
      status: "blocked";
      dependsOn: ["dep-1"];
      createdAt: 1;
      pinned: false;
      route: "/projects/project-1/tasks/task-1";
    },
    SidebarConversationSummary
  >
>;

export type QuotaUsageStatsTypeTest = Assert<
  Extends<
    {
      days: 7;
      backend: "all";
      rangeStart: 1;
      rangeEnd: 2;
      totals: {
        inputTokens: 100;
        outputTokens: 50;
        cacheReadTokens: 10;
        cacheCreationTokens: 5;
        totalTokens: 165;
      };
      cost: {
        knownCostUsd: 0.12;
        costRecordCount: 1;
        totalRecordCount: 2;
      };
      daily: [{
        dayStart: 1;
        inputTokens: 100;
        outputTokens: 50;
        cacheReadTokens: 10;
        cacheCreationTokens: 5;
        totalTokens: 165;
        knownCostUsd: 0.12;
        costRecordCount: 1;
        recordCount: 2;
      }];
      backends: [{
        backend: "native-agentkit";
        inputTokens: 100;
        outputTokens: 50;
        cacheReadTokens: 10;
        cacheCreationTokens: 5;
        totalTokens: 165;
        knownCostUsd: 0.12;
        costRecordCount: 1;
        recordCount: 1;
      }];
      recent: [{
        eventId: "event-1";
        taskId: "task-1";
        turnId: "turn-1";
        backend: "native-agentkit";
        sessionId: "session-1";
        inputTokens: 100;
        outputTokens: 50;
        cacheReadTokens: 10;
        cacheCreationTokens: 5;
        totalTokens: 165;
        knownCostUsd: 0.12;
        createdAt: 1;
      }];
      projects: [{
        projectId: "project-1";
        projectName: "Lilia";
        projectCwd: "C:/repo";
        inputTokens: 100;
        outputTokens: 50;
        cacheReadTokens: 10;
        cacheCreationTokens: 5;
        totalTokens: 165;
        knownCostUsd: 0.12;
        costRecordCount: 1;
        recordCount: 1;
      }];
      conversations: [{
        taskId: "task-1";
        taskTitle: "额度统计";
        taskStatus: "running";
        projectId: "project-1";
        projectName: "Lilia";
        inputTokens: 100;
        outputTokens: 50;
        cacheReadTokens: 10;
        cacheCreationTokens: 5;
        totalTokens: 165;
        knownCostUsd: 0.12;
        costRecordCount: 1;
        recordCount: 1;
      }];
      tools: [{
        key: "command::";
        label: "命令";
        kind: "command";
        subkind: null;
        toolName: null;
        callCount: 1;
        sharePercent: 100;
      }];
    },
    QuotaUsageStats
  >
>;

export type AutomationRunWorkflowTypeTest = Assert<
  Extends<
    {
      type: "automation";
      automationRunId: "run-1";
    },
    ChatWorkflow
  >
>;

export type AutomationRunSummaryTypeTest = Assert<
  Extends<
    {
      id: "run-1";
      workflowId: "workflow-1";
      workflowVersionId: "version-1";
      status: "succeeded";
      triggerKind: "manual";
      projectId: null;
      taskId: "task-1";
      backend: "native-agentkit";
      eventKind: "timeline_event";
      startedAt: 1;
      finishedAt: 2;
      error: null;
    },
    AutomationRunSummary
  >
>;

export type SlashCommandSearchResultTypeTest = Assert<
  Extends<
    {
      command: {
        id: "project:release";
        name: "release";
        title: "生成发布检查";
        description: "按项目模板生成发布检查。";
        source: "project";
        parameters: [];
      };
      matchedBy: "name";
    },
    ChatSlashCommandSearchResult
  >
>;

export type SlashCommandWorkflowTypeTest = Assert<
  Extends<
    {
      type: "slash_command";
      commandId: "native:help";
      source: "native";
      arguments: {};
    },
    ChatWorkflow
  >
>;

export type LiliaFixSuggestionTargetTypeTest = Assert<
  Extends<{ type: "commit"; sha: "abc123" }, LiliaReviewTarget>
>;

export type LiliaFixSuggestionRejectsInvalidTargetTypeTest = Assert<
  Extends<
    Extends<
      {
        type: "lilia_fix_suggestion";
        target: { type: "baseBranch" };
      },
      ChatWorkflow
    >,
    false
  >
>;

export type SuggestionItemIncludesGitHubActivityTypeTest = Assert<
  Extends<
    {
      id: "sg-1";
      projectId: "project-1";
      taskIds: [];
      source: "github";
      githubActivities: [{
        id: "gh-1";
        repoFullName: "sena-nana/LiliaCode";
        kind: "pull_request";
        title: "PR #1: 接入 GitHub 活动";
        url: "https://github.com/sena-nana/LiliaCode/pull/1";
      }];
      localGitContexts: [];
      codexThreads: [];
      summary: "跟进 PR";
      reason: "近期 PR 有新活动。";
      prompt: "请继续跟进 PR #1。";
      generatedAt: 1;
    },
    SuggestionItem
  >
>;

export type SuggestionItemIncludesLocalGitContextTypeTest = Assert<
  Extends<
    {
      id: "sg-local-1";
      projectId: "project-1";
      taskIds: [];
      source: "local-git";
      githubActivities: [];
      localGitContexts: [{
        id: "local-git-current";
        branch: "feature/local-git";
        status: "dirty: 2 changed files";
        changedFiles: ["M apps/desktop/src-tauri/src/conversation_suggestions.rs"];
        recentCommits: ["abc1234 add suggestions"];
      }];
      codexThreads: [];
      summary: "跟进本地变更";
      reason: "当前分支有未提交变更。";
      prompt: "请基于 feature/local-git 的未提交变更继续处理。";
      generatedAt: 1;
    },
    SuggestionItem
  >
>;

export type SuggestionItemIncludesProviderSourceTypeTest = Assert<
  Extends<
    {
      id: "provider-suggestion-1";
      projectId: "project-1";
      taskIds: ["task-1"];
      source: "provider";
      githubActivities: [];
      localGitContexts: [];
      codexThreads: [];
      summary: "继续检查建议展示";
      reason: "根据上一轮对话预测的下一条提示。";
      prompt: "请继续检查 Provider 建议展示。";
      generatedAt: 1;
    },
    SuggestionItem
  >
>;

export type SuggestionItemIncludesSessionThreadSourceTypeTest = Assert<
  Extends<
    {
      id: "sg-session-thread-1";
      projectId: "project-1";
      taskIds: [];
      source: "session-thread";
      githubActivities: [];
      localGitContexts: [];
      codexThreads: [{
        id: "thread-1";
        title: "补齐会话建议";
        updatedAt: 2000;
        preview: "继续补齐 conversation suggestions";
      }];
      summary: "继续会话线程";
      reason: "最近会话线程显示建议来源还没补完。";
      prompt: "请基于 thread-1 继续处理。";
      generatedAt: 1;
    },
    SuggestionItem
  >
>;

export type AssistantAIConfigShapeTypeTest = Assert<
  Extends<
    {
      baseUrl: "https://api.example.com/v1";
      apiKey: null;
      model: "gpt-4o-mini";
      hasApiKey: false;
    },
    import("./provider").AssistantAIConfig
  >
>;

export type AgentInteractionSettingsNativeTypeTest = Assert<
  Extends<
    {
      nonInterruptMode: false;
      debug: true;
      permissionMode: "free";
      permissionModeAvailability: {
        ask: true;
        readonly: true;
        full: true;
        free: true;
      };
      mainAgentPromptMode: "aggressive";
      mainAgentCustomPrompt: "Prefer direct implementation.";
      subagentMode: {
        enabled: false;
        forwardSubagentText: true;
        agentProgressSummaries: true;
      };
      autoTurnDecision: {
        enabled: true;
        allowModelTier: true;
        allowReasoningEffort: true;
        allowPlanMode: true;
        allowGoalMode: true;
        allowSessionFork: true;
      };
    },
    AgentInteractionSettings
  >
>;

export type PermissionModeIncludesFreeTypeTest = Assert<Extends<"free", PermissionMode>>;

export type CustomSubagentDefinitionShapeTypeTest = Assert<
  Extends<
    {
      id: "agent-1";
      name: "Reviewer";
      description: "Checks code";
      instruction: "Review the diff and summarize risks.";
      enabled: true;
    },
    import("./chat").CustomSubagentDefinition
  >
>;

export type TaskTodoAllowsGuideAttachmentsTypeTest = Assert<
  Extends<
    {
      id: "todo-1";
      taskId: "task-1";
      text: "see file";
      done: false;
      order: 0;
      source: "lilia";
      priority: "normal";
      guideStatus: "pending";
      attachments: ChatAttachment[];
      createdAt: 1;
      updatedAt: 1;
    },
    TaskTodo
  >
>;

export type ProjectRoadmapSnapshotTypeTest = Assert<
  Extends<
    {
      milestones: [{
        id: "m-1";
        projectId: "p-1";
        title: "M1";
        description: "";
        status: "upcoming";
        dueDate: null;
        order: 0;
        createdAt: 1;
      }];
      links: [{ taskId: "t-1"; milestoneId: "m-1" }];
    },
    ProjectRoadmap
  >
>;

export type ProjectArchitectureGraphTypeTest = Assert<
  Extends<
    {
      projectId: "p-1";
      version: 1;
      summary: "桌面应用";
      nodes: [{
        id: "desktop-ui";
        label: "Desktop UI";
        type: "component";
        summary: "Vue 前端";
        paths: ["apps/desktop/src"];
        tags: ["frontend"];
      }];
      edges: [{
        id: "ui-tauri";
        from: "desktop-ui";
        to: "tauri";
        type: "calls";
        label: "IPC";
        summary: "通过 invoke 调用 Tauri 命令";
      }];
      updatedAt: 1;
    },
    ProjectArchitectureGraph
  >
>;

export type ProjectArchitectureChangeTypeTest = Assert<
  Extends<
    | {
        type: "upsert_node";
        node: ProjectArchitectureGraph["nodes"][number];
      }
    | {
        type: "set_summary";
        summary: "跨端契约优先";
      },
    ProjectArchitectureChange
  >
>;

export type ToolConsentRequestTypeTest = Assert<
  Extends<
    {
      taskId: "task-1";
      turnId: "turn-1";
      backend: "native-agentkit";
      requestId: "tool-1";
      toolName: "Bash";
      input: { command: "pwd" };
      title: null;
      displayName: null;
      description: null;
      blockedPath: null;
      decisionReason: null;
      toolUseId: null;
      additionalPermissions: [{ permission: "network" }];
      availableDecisions: ["accept", "decline"];
      proposedExecpolicyAmendment: { sandbox: "workspace-write" };
      proposedNetworkPolicyAmendments: [{ host: "example.com" }];
      networkApprovalContext: { host: "example.com" };
      cwd: "D:/PROJECT/workspace/Lilia";
      reason: "needs tests";
      commandActions: [{ text: "pwd" }];
    },
    ToolConsentRequest
  >
>;

export type ToolConsentResponseUpdatedInputTypeTest = Assert<
  Extends<
    {
      taskId: "task-1";
      requestId: "tool-1";
      decision: "allow";
      message: null;
      updatedInput: { command: "pwd && echo ok" };
      codexDecision: "accept";
    },
    ToolConsentResponsePayload
  >
>;

export type AgentInteractionAskRequestTypeTest = Assert<
  Extends<
    {
      taskId: "task-1";
      turnId: "turn-1";
      backend: "native-agentkit";
      requestId: "ask-1";
      kind: "ask_user";
      payload: {
        title: "需要确认一下";
        questions: [{
          id: "q-1";
          question: "选哪个方案？";
          mode: "single";
          options: [{ id: "o-1"; label: "A" }, { id: "o-2"; label: "B" }];
        }];
      };
    },
    AgentInteractionRequest
  >
>;

export type AgentInteractionToolRequestTypeTest = Assert<
  Extends<
    {
      taskId: "task-1";
      turnId: "turn-1";
      backend: "native-agentkit";
      requestId: "tool-1";
      kind: "tool_consent";
      payload: {
        toolName: "commandExecution";
        input: { command: "yarn test" };
        title: "Run command";
        backend: "native-agentkit";
        additionalPermissions: [{ permission: "network" }];
        availableDecisions: ["accept", "decline", "cancel"];
        proposedExecpolicyAmendment: { sandbox: "workspace-write" };
        proposedNetworkPolicyAmendments: [{ host: "registry.npmjs.org" }];
        networkApprovalContext: { host: "registry.npmjs.org" };
        cwd: "D:/PROJECT/workspace/Lilia";
        reason: "run tests";
        commandActions: [{ text: "yarn test" }];
      };
    },
    AgentInteractionRequest
  >
>;

export type AgentInteractionPlanApprovalRequestTypeTest = Assert<
  Extends<
    {
      taskId: "task-1";
      turnId: "turn-1";
      backend: "native-agentkit";
      requestId: "plan-1";
      kind: "plan_approval";
      payload: {
        title: "确认计划";
        source: "Plan";
        intent: "plan_approval";
        questions: [{
          id: "approve-plan";
          question: "";
          mode: "confirm";
          confirmLabel: "按计划执行";
        }];
      };
    },
    AgentInteractionRequest
  >
>;

export type AgentInteractionArchitectureRequestTypeTest = Assert<
  Extends<
    {
      taskId: "task-1";
      turnId: "turn-1";
      backend: "native-agentkit";
      requestId: "arch-1";
      kind: "architecture_change";
      payload: ProjectArchitectureInteractionPayload;
    },
    AgentInteractionRequest
  >
>;

export type AgentInteractionToolResponseTypeTest = Assert<
  Extends<
    {
      taskId: "task-1";
      requestId: "tool-1";
      kind: "tool_consent";
      result: {
        taskId: "task-1";
        requestId: "tool-1";
        decision: "deny";
        message: "先不执行";
        codexDecision: "decline";
      };
    },
    AgentInteractionResponse
  >
>;
