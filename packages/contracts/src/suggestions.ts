import {
  CONVERSATION_SUGGESTIONS_GET_COMMAND,
  CONVERSATION_SUGGESTIONS_GET_SETTINGS_COMMAND,
  CONVERSATION_SUGGESTIONS_GET_SOURCES_COMMAND,
  CONVERSATION_SUGGESTIONS_SET_SETTINGS_COMMAND,
  DEFAULT_SUGGESTION_LOADING_TEXT,
  DEFAULT_SUGGESTION_SOURCE,
  DEFAULT_SUGGESTION_STATUS,
  SUGGESTION_ITEM_SOURCES,
  SUGGESTION_LOADING_SOURCE_LABELS,
  SUGGESTION_LOCAL_GIT_LOADING_LABELS,
  SUGGESTION_SOURCES,
  SUGGESTION_SOURCE_LABELS,
  SUGGESTION_SOURCE_SETTING_ORDER,
  SUGGESTION_STATUSES,
  SUGGESTION_STATUS_DISPLAY_TEXT,
} from "./suggestionsContract.mjs";

export type SuggestionSource = typeof SUGGESTION_SOURCES[number];
export type SuggestionItemSource = typeof SUGGESTION_ITEM_SOURCES[number];
export type SuggestionStatus = typeof SUGGESTION_STATUSES[number];
export type ConversationSuggestionSourceKind = SuggestionItemSource;

export {
  CONVERSATION_SUGGESTIONS_GET_COMMAND,
  CONVERSATION_SUGGESTIONS_GET_SETTINGS_COMMAND,
  CONVERSATION_SUGGESTIONS_GET_SOURCES_COMMAND,
  CONVERSATION_SUGGESTIONS_SET_SETTINGS_COMMAND,
  DEFAULT_SUGGESTION_LOADING_TEXT,
  DEFAULT_SUGGESTION_SOURCE,
  DEFAULT_SUGGESTION_STATUS,
  SUGGESTION_ITEM_SOURCES,
  SUGGESTION_LOADING_SOURCE_LABELS,
  SUGGESTION_LOCAL_GIT_LOADING_LABELS,
  SUGGESTION_SOURCES,
  SUGGESTION_SOURCE_LABELS,
  SUGGESTION_SOURCE_SETTING_ORDER,
  SUGGESTION_STATUSES,
  SUGGESTION_STATUS_DISPLAY_TEXT,
};

const SUGGESTION_SOURCE_SET = new Set<string>(SUGGESTION_SOURCES);
const SUGGESTION_ITEM_SOURCE_SET = new Set<string>(SUGGESTION_ITEM_SOURCES);
const SUGGESTION_STATUS_SET = new Set<string>(SUGGESTION_STATUSES);

export function isSuggestionSource(value: unknown): value is SuggestionSource {
  return typeof value === "string" && SUGGESTION_SOURCE_SET.has(value);
}

export function isSuggestionItemSource(value: unknown): value is SuggestionItemSource {
  return typeof value === "string" && SUGGESTION_ITEM_SOURCE_SET.has(value);
}

export function isSuggestionStatus(value: unknown): value is SuggestionStatus {
  return typeof value === "string" && SUGGESTION_STATUS_SET.has(value);
}

export function normalizeSuggestionSource(
  value: unknown,
  fallback: SuggestionSource = DEFAULT_SUGGESTION_SOURCE,
): SuggestionSource {
  return isSuggestionSource(value) ? value : fallback;
}

export function normalizeSuggestionStatus(
  value: unknown,
  fallback: SuggestionStatus = DEFAULT_SUGGESTION_STATUS,
): SuggestionStatus {
  return isSuggestionStatus(value) ? value : fallback;
}

export function suggestionSourceSettingLabel(source: SuggestionSource): string {
  return SUGGESTION_SOURCE_LABELS[source];
}

export interface ConversationSuggestionSources {
  sources: ConversationSuggestionSourceKind[];
  localGit?: {
    hasRecentCommits: boolean;
    hasChangedFiles: boolean;
  } | null;
}

export interface SuggestionGitHubActivityRef {
  id: string;
  repoFullName: string;
  kind: string;
  title: string;
  url: string | null;
}

export interface SuggestionLocalGitContextRef {
  id: string;
  branch: string;
  status: string;
  changedFiles: string[];
  recentCommits: string[];
}

export interface SuggestionCodexThreadRef {
  id: string;
  title: string;
  updatedAt: number | null;
  preview: string | null;
}

export interface SuggestionItem {
  id: string;
  projectId: string | null;
  taskIds: string[];
  source: SuggestionItemSource;
  githubActivities: SuggestionGitHubActivityRef[];
  localGitContexts: SuggestionLocalGitContextRef[];
  codexThreads: SuggestionCodexThreadRef[];
  summary: string;
  reason: string;
  prompt: string;
  generatedAt: number;
}

export function suggestionGitHubActivityAnchor(activity: SuggestionGitHubActivityRef): string {
  const title = activity.title.trim();
  if (activity.kind === "pull_request") {
    const number = title.match(/#(\d+)/)?.[1];
    return number ? `PR #${number}` : "PR";
  }
  if (activity.kind === "issue") {
    const number = title.match(/#(\d+)/)?.[1];
    return number ? `Issue #${number}` : "Issue";
  }
  if (activity.kind === "push") {
    const branch = title.match(/^Push\s+([^:]+):/i)?.[1]?.trim();
    return branch ? `Push ${branch}` : "Push";
  }
  return title || activity.kind || "GitHub";
}

export function suggestionGitHubSourceLabel(suggestion: Pick<SuggestionItem, "githubActivities">): string {
  const githubActivities = suggestion.githubActivities ?? [];
  const [activity] = githubActivities;
  if (!activity) return "";
  const source = [
    activity.repoFullName.trim(),
    suggestionGitHubActivityAnchor(activity),
  ].filter(Boolean).join(" · ");
  const extraCount = githubActivities.length - 1;
  return extraCount > 0 ? `${source} +${extraCount}` : source;
}

export function suggestionSourceLabel(
  suggestion: Pick<SuggestionItem, "githubActivities" | "localGitContexts" | "codexThreads">,
): string {
  const githubLabel = suggestionGitHubSourceLabel(suggestion);
  if (githubLabel) return githubLabel;
  const localGitContexts = suggestion.localGitContexts ?? [];
  const [context] = localGitContexts;
  if (context) {
    const branch = context.branch.trim();
    const source = branch ? `本地 Git · ${branch}` : "本地 Git";
    const extraCount = localGitContexts.length - 1;
    return extraCount > 0 ? `${source} +${extraCount}` : source;
  }
  const codexThreads = suggestion.codexThreads ?? [];
  const [thread] = codexThreads;
  if (!thread) return "";
  const title = thread.title.trim();
  const source = title ? `会话线程 · ${title}` : "会话线程";
  const extraCount = codexThreads.length - 1;
  return extraCount > 0 ? `${source} +${extraCount}` : source;
}

export function conversationSuggestionLoadingText(probe: ConversationSuggestionSources): string {
  const sources = new Set(probe.sources as string[]);
  if (sources.has("provider") || sources.has("claude")) {
    const labels = SUGGESTION_LOADING_SOURCE_LABELS as Record<string, string>;
    return `正在读取 ${labels.provider ?? labels.claude ?? "Provider 建议"}`;
  }
  const labels: string[] = [];
  if (sources.has("task")) labels.push(SUGGESTION_LOADING_SOURCE_LABELS.task);
  if (sources.has("github")) labels.push(SUGGESTION_LOADING_SOURCE_LABELS.github);
  if (sources.has("local-git")) {
    labels.push(localGitLoadingLabel(probe.localGit));
  }
  if (sources.has("session-thread") || sources.has("codex-thread")) {
    const sourceLabels = SUGGESTION_LOADING_SOURCE_LABELS as Record<string, string>;
    labels.push(sourceLabels["session-thread"] ?? sourceLabels["codex-thread"] ?? "会话线程");
  }
  if (labels.length === 0) return DEFAULT_SUGGESTION_LOADING_TEXT;
  return `正在检查${joinSuggestionSourceLabels(labels)}`;
}

export function suggestionStatusDisplayText(options: {
  hasSuggestions: boolean;
  status: SuggestionStatus;
  loadingText?: string | null;
}): string {
  if (options.hasSuggestions) return "";
  if (options.status === "loading") {
    return options.loadingText?.trim() || DEFAULT_SUGGESTION_LOADING_TEXT;
  }
  if (options.status === "error") return SUGGESTION_STATUS_DISPLAY_TEXT.error ?? "";
  return "";
}

function localGitLoadingLabel(localGit: ConversationSuggestionSources["localGit"]): string {
  if (localGit?.hasRecentCommits && localGit.hasChangedFiles) {
    return SUGGESTION_LOCAL_GIT_LOADING_LABELS.recentCommitsAndChangedFiles;
  }
  if (localGit?.hasRecentCommits) return SUGGESTION_LOCAL_GIT_LOADING_LABELS.recentCommits;
  if (localGit?.hasChangedFiles) return SUGGESTION_LOCAL_GIT_LOADING_LABELS.changedFiles;
  return SUGGESTION_LOCAL_GIT_LOADING_LABELS.default;
}

function joinSuggestionSourceLabels(labels: string[]): string {
  if (labels.length <= 1) return labelAfterConjunction(labels[0] ?? "");
  if (labels.length === 2) return `${labels[0]}和${labelAfterConjunction(labels[1])}`;
  return `${labels.slice(0, -1).join("、")}和${labelAfterConjunction(labels[labels.length - 1])}`;
}

function labelAfterConjunction(label: string): string {
  return /^[A-Za-z]/.test(label) ? ` ${label}` : label;
}

export interface SuggestionSettings {
  enabled: boolean;
  source: SuggestionSource;
}
