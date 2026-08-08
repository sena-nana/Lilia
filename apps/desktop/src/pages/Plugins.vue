<script setup lang="ts">
import "../styles/pages/plugins.css";
import { computed, defineAsyncComponent, onBeforeUnmount, ref, watch, type Component } from "vue";
import AlertTriangle from "@lucide/vue/dist/esm/icons/triangle-alert.mjs";
import Check from "@lucide/vue/dist/esm/icons/check.mjs";
import FolderOpen from "@lucide/vue/dist/esm/icons/folder-open.mjs";
import Loader2 from "@lucide/vue/dist/esm/icons/loader-circle.mjs";
import Pencil from "@lucide/vue/dist/esm/icons/pencil.mjs";
import Plus from "@lucide/vue/dist/esm/icons/plus.mjs";
import Search from "@lucide/vue/dist/esm/icons/search.mjs";
import Trash2 from "@lucide/vue/dist/esm/icons/trash-2.mjs";
import {
  hookScopeLabel,
  hookSourceEditLabel,
  hookSourceFormatLabel,
  hookSourceStateLabel,
  hookTrustStateLabel,
  pluginEnabledLabel,
  pluginMcpBackendLabel,
  pluginMcpTransportLabel,
  pluginToggleActionLabel,
} from "@lilia/contracts";
import ConfirmDialog from "@lilia/ui/components/ConfirmDialog";
import {
  createHookSource,
  createSkill,
  deleteHookSource,
  deleteMcpServer,
  deleteSkill,
  openHookConfig,
  openMcpConfig,
  readHookSource,
  setMcpServerEnabled,
  setPackageEnabled,
  setSkillEnabled,
  type HookDocumentView,
  type HookHandlerView,
  type HookSourceSummary,
  type PluginMcpServer,
  type PluginPackage,
  type PluginSkill,
} from "../services/plugins";
import { measurePerfAsync } from "@lilia/ui/diagnostics";
import { usePluginsOverview } from "./plugins/usePluginsOverview";
import { useHookSourceEditor } from "./plugins/useHookSourceEditor";
import { useMcpServerEditor } from "./plugins/useMcpServerEditor";
import { createLazyLoadState } from "@lilia/ui/utils/lazyLoadState";

type PluginsSection = "skills" | "packages" | "hooks" | "mcp";

const props = withDefaults(defineProps<{
  section?: PluginsSection;
}>(), {
  section: "skills",
});

const skillCreateDialogLoad = createLazyLoadState<Component>(() =>
  measurePerfAsync(
    "plugins.skill-create-dialog.load",
    async () => (await import("./plugins/SkillCreateDialog.vue")).default,
  )
);
const hookSourceEditorDialogLoad = createLazyLoadState<Component>(() =>
  measurePerfAsync(
    "plugins.hook-editor-dialog.load",
    async () => (await import("./plugins/HookSourceEditorDialog.vue")).default,
  )
);
const mcpServerEditorDialogLoad = createLazyLoadState<Component>(() =>
  measurePerfAsync(
    "plugins.mcp-editor-dialog.load",
    async () => (await import("./plugins/McpServerEditorDialog.vue")).default,
  )
);

async function loadSkillCreateDialog(): Promise<Component> {
  return skillCreateDialogLoad.load();
}

async function loadHookSourceEditorDialog(): Promise<Component> {
  return hookSourceEditorDialogLoad.load();
}

async function loadMcpServerEditorDialog(): Promise<Component> {
  return mcpServerEditorDialogLoad.load();
}

const SkillCreateDialog = defineAsyncComponent({
  suspensible: false,
  loader: loadSkillCreateDialog,
});

const HookSourceEditorDialog = defineAsyncComponent({
  suspensible: false,
  loader: loadHookSourceEditorDialog,
});

const McpServerEditorDialog = defineAsyncComponent({
  suspensible: false,
  loader: loadMcpServerEditorDialog,
});

const {
  projectOptions,
  userSkills,
  projectSkills,
  packages,
  mcpServers,
  mcpConfigPath,
  hookSources,
  warnings,
  loading,
  errorText,
  refresh,
} = usePluginsOverview({
  isDisposed: () => disposed,
});

const showCreate = ref(false);
const newName = ref("");
const newDesc = ref("");
const creating = ref(false);
const createError = ref<string | null>(null);
const pendingRemoveSkill = ref<PluginSkill | null>(null);
const pendingRemoveMcp = ref<PluginMcpServer | null>(null);
const pendingRemoveHook = ref<HookSourceSummary | null>(null);
const removing = ref(false);
const query = ref("");
const selectedKey = ref<string | null>(null);
const selectedHookDocument = ref<HookDocumentView | null>(null);
const hookDocumentLoading = ref(false);
const hookDocumentError = ref<string | null>(null);
let hookDocumentRequestId = 0;
let disposed = false;

const showTopbar = computed(() => props.section === "skills");

const mcpEditor = useMcpServerEditor<PluginMcpServer>({
  backend: "native-agentkit",
  label: "Native MCP",
  refresh,
  isDisposed: () => disposed,
});
const hookEditor = useHookSourceEditor({
  refresh,
  isDisposed: () => disposed,
  onSaved: (document) => {
    if (disposed) return;
    selectedHookDocument.value = document;
    hookDocumentError.value = null;
  },
});
const USER_SKILLS_ROOT = "~/.lilia/skills/";

type PluginEntry =
  | { kind: "skill"; key: string; title: string; meta: string; searchText: string; item: PluginSkill }
  | { kind: "plugin"; key: string; title: string; meta: string; searchText: string; item: PluginPackage }
  | {
      kind: "mcp";
      key: string;
      title: string;
      meta: string;
      searchText: string;
      item: PluginMcpServer;
    }
  | {
      kind: "hook";
      key: string;
      title: string;
      meta: string;
      searchText: string;
      item: HookSourceSummary;
    };

interface DetailRow {
  label: string;
  value: string;
  code?: boolean;
}

type HookEntry = Extract<PluginEntry, { kind: "hook" }>;

function commandLine(command: string, args: string[]) {
  return [command, ...args].filter(Boolean).join(" ").trim();
}

function mcpServerSummary(server: PluginMcpServer) {
  return commandLine(server.command, server.args) || `${pluginMcpTransportLabel(server.transport)} MCP server`;
}

const SKILL_PROJECT_MARKERS = ["/.lilia/skills/", "/.claude/skills/"];
const HOOK_PROJECT_MARKERS = [
  "/.lilia/hooks.json",
  "/.lilia/settings.json",
  "/.claude/settings.json",
  "/.claude/settings.local.json",
  "/.codex/hooks.json",
  "/.codex/config.toml",
  "/.codex/requirements.toml",
];

function normalizePath(value: string | null | undefined) {
  return normalizedDisplayPath(value).toLocaleLowerCase();
}

function normalizedDisplayPath(value: string | null | undefined) {
  return (value ?? "").replace(/\\/g, "/").replace(/\/+/g, "/").replace(/\/+$/, "");
}

function pathStartsWithProject(path: string, projectPath: string) {
  const normalizedPath = normalizePath(path);
  const normalizedProject = normalizePath(projectPath);
  return normalizedPath === normalizedProject || normalizedPath.startsWith(`${normalizedProject}/`);
}

function projectOptionForPath(path: string) {
  let matched: (typeof projectOptions.value)[number] | null = null;
  for (const project of projectOptions.value) {
    if (!pathStartsWithProject(path, project.value)) continue;
    if (!matched || normalizePath(project.value).length > normalizePath(matched.value).length) {
      matched = project;
    }
  }
  return matched;
}

function projectCwdFromPath(path: string, markers: string[]) {
  const matchedProject = projectOptionForPath(path);
  if (matchedProject) return matchedProject.value;
  const normalized = normalizedDisplayPath(path);
  const lower = normalized.toLocaleLowerCase();
  for (const marker of markers) {
    const index = lower.indexOf(marker);
    if (index > 0) return normalized.slice(0, index);
  }
  return null;
}

function projectCwdForSkill(skill: PluginSkill) {
  if (skill.scope !== "project") return null;
  return projectCwdFromPath(skill.path, SKILL_PROJECT_MARKERS);
}

function projectCwdForHookSource(source: HookSourceSummary) {
  if (source.scope !== "project" && source.scope !== "local") return null;
  return projectCwdFromPath(source.path, HOOK_PROJECT_MARKERS);
}

function requireProjectCwd(cwd: string | null, label: string) {
  if (cwd) return cwd;
  throw new Error(`${label} 缺少项目路径，已取消项目级操作`);
}

function projectFolderFromCwd(cwd: string | null) {
  if (!cwd) return "";
  const parts = normalizedDisplayPath(cwd).split("/");
  return parts[parts.length - 1] ?? "";
}

function projectSkillsRootFromPath(path: string) {
  const projectCwd = projectCwdFromPath(path, SKILL_PROJECT_MARKERS);
  return projectCwd ? `${projectCwd}\\.lilia\\skills\\` : "";
}

function projectLabelForSkill(skill: PluginSkill) {
  if (skill.scope !== "project") return "";
  const matchedProject = projectOptionForPath(skill.path);
  if (matchedProject) return matchedProject.label;
  return projectFolderFromCwd(projectCwdForSkill(skill)) || "项目";
}

function skillLocation(skill: PluginSkill) {
  if (skill.scope === "user") return USER_SKILLS_ROOT;
  const project = projectOptionForPath(skill.path);
  return project
    ? `${project.value}\\.lilia\\skills\\`
    : projectSkillsRootFromPath(skill.path) || "未选择项目";
}

function projectLabelForHookSource(source: HookSourceSummary) {
  if (source.scope !== "project" && source.scope !== "local") return "";
  const matchedProject = projectOptionForPath(source.path);
  if (matchedProject) return matchedProject.label;
  return projectFolderFromCwd(projectCwdForHookSource(source)) || "项目";
}

function hookSourceMeta(source: HookSourceSummary) {
  return [
    hookScopeLabel(source.scope),
    projectLabelForHookSource(source),
    hookSourceEditLabel(source),
    hookSourceStateLabel(source),
    source.managed ? "Managed" : "",
    source.warnings.length ? `${source.warnings.length} 警告` : "",
  ].filter(Boolean).join(" · ");
}

function joinSearchText(...parts: Array<string | null | undefined>) {
  return parts.filter(Boolean).join("\n");
}

function buildHookEntry(kind: HookEntry["kind"], source: HookSourceSummary): HookEntry {
  return {
    kind,
    key: `${kind}:${source.id}`,
    title: source.name,
    meta: hookSourceMeta(source),
    searchText: joinSearchText(
      source.name,
      source.path,
      source.description,
      source.warnings.join("\n"),
      source.limitations.join("\n"),
    ),
    item: source,
  };
}

function isHookEntry(
  entry: PluginEntry | null,
): entry is HookEntry {
  return !!entry && entry.kind === "hook";
}

function hookDocumentFor(source: HookSourceSummary | null) {
  return source && selectedHookDocument.value?.source.id === source.id
    ? selectedHookDocument.value
    : null;
}

function mergeUniqueTexts(...groups: string[][]) {
  return Array.from(new Set(groups.flat()));
}

const allEntries = computed<PluginEntry[]>(() => {
  if (props.section === "skills") {
    return [...userSkills.value, ...projectSkills.value].map((skill) => {
      const projectLabel = projectLabelForSkill(skill);
      return {
        kind: "skill",
        key: `skill:${skill.scope}:${skill.path}`,
        title: skill.name,
        meta: [pluginEnabledLabel(skill.enabled), projectLabel].filter(Boolean).join(" · "),
        searchText: [skill.name, skill.description, skill.path, projectLabel].join("\n"),
        item: skill,
      };
    });
  }

  if (props.section === "packages") {
    return packages.value.map((plugin) => ({
      kind: "plugin",
      key: `plugin:${plugin.path}`,
      title: plugin.name,
      meta: [pluginEnabledLabel(plugin.enabled), plugin.version ? `v${plugin.version}` : ""]
        .filter(Boolean)
        .join(" · "),
      searchText: [plugin.name, plugin.description, plugin.version, plugin.path].join("\n"),
      item: plugin,
    }));
  }

  if (props.section === "hooks") {
    return hookSources.value.map((source) => buildHookEntry("hook", source));
  }

  return mcpServers.value.map((server) => ({
    kind: "mcp",
    key: `mcp:${server.name}`,
    title: server.name,
    meta: [
      server.editable ? pluginEnabledLabel(server.enabled) : "只读",
      pluginMcpTransportLabel(server.transport),
    ].join(" · "),
    searchText: [
      server.name,
      server.transport,
      server.command,
      server.args.join(" "),
      server.envKeys.join(" "),
    ].join("\n"),
    item: server,
  }));
});

const filteredEntries = computed(() => {
  const needle = query.value.trim().toLocaleLowerCase();
  if (!needle) return allEntries.value;
  return allEntries.value.filter((entry) =>
    entry.searchText.toLocaleLowerCase().includes(needle),
  );
});

const selectedEntry = computed(
  () => filteredEntries.value.find((entry) => entry.key === selectedKey.value) ?? null,
);

const selectedHookSource = computed(() => (
  isHookEntry(selectedEntry.value) ? selectedEntry.value.item : null
));

const selectedHookSourceData = computed(() => {
  const selected = selectedHookSource.value;
  if (!selected) return null;
  return hookDocumentFor(selected)?.source ?? selected;
});

const selectedHookHandlers = computed(() => {
  const selected = selectedHookSource.value;
  return hookDocumentFor(selected)?.handlers ?? [];
});

const selectedDetailWarnings = computed(() => {
  const source = selectedHookSourceData.value;
  if (!source) return [];
  return mergeUniqueTexts(source.warnings, hookDocumentFor(source)?.warnings ?? []);
});

const selectedDetailLimitations = computed(() => {
  const source = selectedHookSourceData.value;
  if (!source) return [];
  return mergeUniqueTexts(source.limitations, hookDocumentFor(source)?.limitations ?? []);
});

const selectedMeta = computed(() => {
  const entry = selectedEntry.value;
  if (!entry) return "";
  if (entry.kind === "skill") return `${entry.item.scope === "user" ? "全局" : "项目"} · ${entry.meta}`;
  if (entry.kind === "plugin") return `Plugin · ${entry.meta}`;
  if (entry.kind === "mcp") return `${pluginMcpBackendLabel(entry.item.backend)} · ${entry.meta}`;
  return `Hooks · ${entry.meta}`;
});

const detailRows = computed<DetailRow[]>(() => {
  const entry = selectedEntry.value;
  if (!entry) return [];
  if (entry.kind === "skill") {
    return [
      { label: "描述", value: entry.item.description || "无描述" },
      { label: "路径", value: entry.item.path, code: true },
      { label: "Scope", value: entry.item.scope === "user" ? "全局" : "项目" },
      ...(entry.item.scope === "project"
        ? [{ label: "项目", value: projectLabelForSkill(entry.item) }]
        : []),
      { label: "位置", value: skillLocation(entry.item), code: true },
    ];
  }
  if (entry.kind === "plugin") {
    return [
      { label: "描述", value: entry.item.description || "无描述" },
      { label: "版本", value: entry.item.version || "-" },
      { label: "路径", value: entry.item.path, code: true },
    ];
  }
  if (entry.kind === "mcp") {
    return [
      { label: "Transport", value: pluginMcpTransportLabel(entry.item.transport) },
      { label: "命令", value: mcpServerSummary(entry.item), code: true },
      { label: "环境变量", value: entry.item.envKeys.join(", ") || "-" },
      { label: "权限", value: entry.item.editable ? "可编辑" : "只读" },
      {
        label: "配置",
        value: mcpConfigPath.value || "~/.lilia/config/mcp-servers.json",
        code: true,
      },
    ];
  }
  const source = selectedHookSourceData.value ?? entry.item;
  return [
    { label: "Scope", value: hookScopeLabel(source.scope) },
    { label: "格式", value: hookSourceFormatLabel(source.format) },
    { label: "路径", value: source.path, code: true },
    { label: "状态", value: hookSourceStateLabel(source) },
    { label: "权限", value: hookSourceEditLabel(source) },
    { label: "Trust", value: hookTrustStateLabel(source.trustState) },
    { label: "Handlers", value: String(source.handlerCount) },
    ...(source.description ? [{ label: "说明", value: source.description }] : []),
  ];
});

const emptyText = computed(() => {
  if (query.value.trim()) return "没有匹配项";
  if (props.section === "skills") return "没有 Skill";
  if (props.section === "packages") return "没有 Plugin";
  if (props.section === "hooks") return "没有 Hooks 来源";
  return "没有 MCP";
});

function handlerMeta(handler: HookHandlerView) {
  return [
    handler.matcher ? `matcher: ${handler.matcher}` : "",
    handler.type,
    handler.supported ? "" : "未结构化支持",
    handler.executable ? "" : "当前不会执行",
    handler.warnings.length ? `${handler.warnings.length} 警告` : "",
  ].filter(Boolean).join(" · ");
}

watch(
  filteredEntries,
  (entries) => {
    if (entries.some((entry) => entry.key === selectedKey.value)) return;
    selectedKey.value = entries[0]?.key ?? null;
  },
  { immediate: true },
);

watch(
  () => props.section,
  () => {
    query.value = "";
    selectedKey.value = null;
  },
);

watch(
  selectedHookSource,
  async (source) => {
    hookDocumentRequestId += 1;
    const requestId = hookDocumentRequestId;
    if (disposed) return;
    selectedHookDocument.value = null;
    hookDocumentError.value = null;
    if (!source) {
      hookDocumentLoading.value = false;
      return;
    }
    hookDocumentLoading.value = true;
    try {
      const document = await readHookSource(source);
      if (disposed || requestId !== hookDocumentRequestId) return;
      selectedHookDocument.value = document;
    } catch (err) {
      if (disposed || requestId !== hookDocumentRequestId) return;
      hookDocumentError.value = String(err);
    } finally {
      if (!disposed && requestId === hookDocumentRequestId) {
        hookDocumentLoading.value = false;
      }
    }
  },
  { immediate: true },
);

function openCreate() {
  if (disposed) return;
  newName.value = "";
  newDesc.value = "";
  createError.value = null;
  void loadSkillCreateDialog();
  showCreate.value = true;
}

async function confirmCreate() {
  if (disposed || creating.value) return;
  createError.value = null;
  creating.value = true;
  try {
    await createSkill(
      "user",
      null,
      newName.value,
      newDesc.value,
    );
    if (disposed) return;
    showCreate.value = false;
    await refresh();
  } catch (err) {
    if (!disposed) createError.value = String(err);
  } finally {
    if (!disposed) creating.value = false;
  }
}

async function toggleSkill(skill: PluginSkill) {
  if (disposed) return;
  try {
    const targetProjectCwd = skill.scope === "project"
      ? requireProjectCwd(projectCwdForSkill(skill), "Skill")
      : null;
    await setSkillEnabled(
      skill.scope,
      targetProjectCwd,
      skill.name,
      !skill.enabled,
    );
    if (disposed) return;
    skill.enabled = !skill.enabled;
  } catch (err) {
    if (!disposed) errorText.value = String(err);
  }
}

async function togglePlugin(plugin: PluginPackage) {
  if (disposed) return;
  try {
    await setPackageEnabled(plugin.backend, plugin.scope, plugin.name, !plugin.enabled);
    if (disposed) return;
    plugin.enabled = !plugin.enabled;
  } catch (err) {
    if (!disposed) errorText.value = String(err);
  }
}

async function toggleMcp(server: PluginMcpServer) {
  if (disposed || !server.editable) return;
  try {
    await setMcpServerEnabled(server.backend, server.name, !server.enabled);
    if (disposed) return;
    server.enabled = !server.enabled;
  } catch (err) {
    if (!disposed) errorText.value = String(err);
  }
}

async function createSelectedHookSource(entry: HookEntry) {
  if (disposed) return;
  try {
    const targetProjectCwd = entry.item.scope === "project" || entry.item.scope === "local"
      ? requireProjectCwd(projectCwdForHookSource(entry.item), "Hooks 来源")
      : null;
    await createHookSource(
      entry.item.backend,
      entry.item.scope,
      targetProjectCwd,
    );
    if (disposed) return;
    await refresh();
  } catch (err) {
    if (!disposed) errorText.value = String(err);
  }
}

async function openSelectedHookEditor(entry: HookEntry) {
  if (disposed) return;
  try {
    const document = hookDocumentFor(entry.item) ?? await readHookSource(entry.item);
    if (disposed) return;
    selectedHookDocument.value = document;
    hookDocumentError.value = null;
    void loadHookSourceEditorDialog();
    hookEditor.openHookEditor(document);
  } catch (err) {
    if (!disposed) errorText.value = String(err);
  }
}

async function confirmRemove() {
  if (disposed || removing.value) return;
  removing.value = true;
  try {
    if (pendingRemoveSkill.value) {
      const skill = pendingRemoveSkill.value;
      const targetProjectCwd = skill.scope === "project"
        ? requireProjectCwd(projectCwdForSkill(skill), "Skill")
        : null;
      await deleteSkill(
        skill.scope,
        targetProjectCwd,
        skill.name,
      );
      if (disposed) return;
      pendingRemoveSkill.value = null;
    } else if (pendingRemoveMcp.value) {
      await deleteMcpServer(pendingRemoveMcp.value.backend, pendingRemoveMcp.value.name);
      if (disposed) return;
      pendingRemoveMcp.value = null;
    } else if (pendingRemoveHook.value) {
      await deleteHookSource(pendingRemoveHook.value);
      if (disposed) return;
      pendingRemoveHook.value = null;
    }
    await refresh();
  } catch (err) {
    if (!disposed) errorText.value = String(err);
  } finally {
    if (!disposed) removing.value = false;
  }
}

async function openMcp(server: PluginMcpServer) {
  if (disposed) return;
  try {
    await openMcpConfig(server.backend);
  } catch (err) {
    if (!disposed) errorText.value = String(err);
  }
}

async function openHookSourceConfig(source: HookSourceSummary) {
  if (disposed) return;
  try {
    await openHookConfig(source);
  } catch (err) {
    if (!disposed) errorText.value = String(err);
  }
}

function selectEntry(entry: PluginEntry) {
  selectedKey.value = entry.key;
}

function openMcpCreate() {
  if (disposed) return;
  void loadMcpServerEditorDialog();
  mcpEditor.openCreateMcp();
}

function canCreateInDetail() {
  if (props.section === "mcp") return true;
  return !!selectedHookSource.value && selectedHookSource.value.editable && !selectedHookSource.value.exists;
}

function detailCreateLabel() {
  return props.section === "mcp" ? "新增 MCP" : "创建来源";
}

function openDetailCreate() {
  if (selectedEntry.value && isHookEntry(selectedEntry.value)) {
    void createSelectedHookSource(selectedEntry.value);
    return;
  }
  openMcpCreate();
}

function toggleSelected(entry: PluginEntry) {
  if (entry.kind === "skill") void toggleSkill(entry.item);
  else if (entry.kind === "plugin") void togglePlugin(entry.item);
  else if (entry.kind === "mcp") void toggleMcp(entry.item);
}

function editSelected(entry: PluginEntry) {
  if (entry.kind === "mcp" && entry.item.editable) {
    void loadMcpServerEditorDialog();
    mcpEditor.openEditMcp(entry.item);
  } else if (isHookEntry(entry)) {
    void openSelectedHookEditor(entry);
  }
}

function removeSelected(entry: PluginEntry) {
  if (disposed) return;
  if (entry.kind === "skill") pendingRemoveSkill.value = entry.item;
  else if (entry.kind === "mcp" && entry.item.editable) pendingRemoveMcp.value = entry.item;
  else if (isHookEntry(entry)) pendingRemoveHook.value = entry.item;
}

function openSelectedConfig(entry: PluginEntry) {
  if (entry.kind === "mcp") void openMcp(entry.item);
  else if (isHookEntry(entry)) void openHookSourceConfig(entry.item);
}

onBeforeUnmount(() => {
  disposed = true;
  hookDocumentRequestId += 1;
});

function actionLabel(entry: PluginEntry) {
  return pluginToggleActionLabel(entry.item.enabled);
}

function canToggle(entry: PluginEntry) {
  if (isHookEntry(entry)) return false;
  return entry.kind !== "mcp" || entry.item.editable;
}

function canEdit(entry: PluginEntry) {
  if (isHookEntry(entry)) return entry.item.editable && entry.item.exists;
  return entry.kind === "mcp" && entry.item.editable;
}

function canRemove(entry: PluginEntry) {
  if (isHookEntry(entry)) return entry.item.editable && entry.item.exists;
  return entry.kind === "skill" || (entry.kind === "mcp" && entry.item.editable);
}

function canOpenConfig(entry: PluginEntry) {
  return entry.kind === "mcp" || isHookEntry(entry);
}
</script>

<template>
  <section class="plugins-page">
    <div class="plugins-browser">
      <div v-if="errorText" class="conn-banner conn-banner--err plugins-browser__banner">
        <AlertTriangle :size="16" aria-hidden="true" />
        <div>
          <div class="conn-banner__title">操作失败</div>
          <div class="conn-banner__hint">{{ errorText }}</div>
        </div>
      </div>

      <div v-if="warnings.length" class="conn-banner conn-banner--warn plugins-browser__banner">
        <AlertTriangle :size="16" aria-hidden="true" />
        <div>
          <div class="conn-banner__title">解析期警告</div>
          <ul class="plugins-warning-list">
            <li v-for="(w, i) in warnings" :key="i">{{ w }}</li>
          </ul>
        </div>
      </div>

      <div v-if="showTopbar" class="plugins-browser__topbar">
        <div class="plugins-browser__topbar-spacer" aria-hidden="true" />
        <div class="plugins-browser__topbar-actions">
          <button
            v-if="props.section === 'skills'"
            type="button"
            class="ui-button ui-button--ghost"
            data-agent-id="plugins.skill.create"
            @click="openCreate"
          >
            <Plus :size="14" aria-hidden="true" />
            <span>新建 Skill</span>
          </button>
        </div>
      </div>

      <div class="plugins-browser__content">
        <aside class="plugins-browser__sidebar" aria-label="插件和技能列表">
          <div class="plugins-browser__search">
            <label class="plugins-browser__searchbox">
              <Search :size="14" aria-hidden="true" />
              <input
                v-model="query"
                type="search"
                placeholder="搜索当前列表"
                aria-label="搜索插件和技能"
                data-agent-id="plugins.search"
              />
            </label>
          </div>

          <section class="plugins-browser__list ui-list" aria-label="检索结果">
            <div v-if="loading && !filteredEntries.length" class="plugins-browser__notice">
              <Loader2 :size="14" class="is-spinning" aria-hidden="true" />
              <span>读取中</span>
            </div>
            <div v-else-if="!filteredEntries.length" class="plugins-browser__notice">
              {{ emptyText }}
            </div>
            <template v-else>
              <button
                v-for="entry in filteredEntries"
                :key="entry.key"
                type="button"
                class="plugins-browser__row ui-list-item"
                :class="{ 'is-active': selectedKey === entry.key, 'is-disabled': !entry.item.enabled }"
                :data-agent-id="`plugins.entry.${entry.key}`"
                :title="entry.title"
                @click="selectEntry(entry)"
              >
                <span class="plugins-browser__row-title">{{ entry.title }}</span>
                <span class="plugins-browser__row-meta">{{ entry.meta }}</span>
              </button>
            </template>
          </section>
        </aside>

        <section class="plugins-browser__detail" aria-label="插件和技能详情">
          <template v-if="selectedEntry">
            <div class="plugins-browser__detail-head">
              <div class="plugins-browser__detail-heading">
                <div class="plugins-browser__detail-title">{{ selectedEntry.title }}</div>
                <div class="plugins-browser__detail-meta">{{ selectedMeta }}</div>
              </div>
              <div class="plugins-browser__actions">
                <button
                  v-if="canCreateInDetail()"
                  type="button"
                  class="ui-button ui-button--ghost"
                  data-agent-id="plugins.detail.create"
                  @click="openDetailCreate"
                >
                  <Plus :size="14" aria-hidden="true" />
                  <span>{{ detailCreateLabel() }}</span>
                </button>
                <button
                  v-if="canOpenConfig(selectedEntry)"
                  type="button"
                  class="ui-button ui-button--ghost"
                  data-agent-id="plugins.detail.open-config"
                  @click="openSelectedConfig(selectedEntry)"
                >
                  <FolderOpen :size="14" aria-hidden="true" />
                  <span>打开配置</span>
                </button>
                <button
                  v-if="canEdit(selectedEntry)"
                  type="button"
                  class="ui-button ui-button--ghost"
                  data-agent-id="plugins.detail.edit"
                  @click="editSelected(selectedEntry)"
                >
                  <Pencil :size="14" aria-hidden="true" />
                  <span>编辑</span>
                </button>
                <button
                  v-if="canRemove(selectedEntry)"
                  type="button"
                  class="ui-button ui-button--ghost ui-button--danger"
                  data-agent-id="plugins.detail.remove"
                  @click="removeSelected(selectedEntry)"
                >
                  <Trash2 :size="14" aria-hidden="true" />
                  <span>删除</span>
                </button>
                <button
                  v-if="canToggle(selectedEntry)"
                  type="button"
                  class="ui-button ui-button--primary"
                  data-agent-id="plugins.detail.toggle"
                  @click="toggleSelected(selectedEntry)"
                >
                  <Check :size="14" aria-hidden="true" />
                  <span>{{ actionLabel(selectedEntry) }}</span>
                </button>
              </div>
            </div>

            <div class="plugins-browser__detail-list">
              <dl class="plugins-browser__detail-grid">
                <div
                  v-for="row in detailRows"
                  :key="row.label"
                  class="plugins-browser__detail-row"
                >
                  <dt>{{ row.label }}</dt>
                  <dd>
                    <code v-if="row.code">{{ row.value }}</code>
                    <span v-else>{{ row.value }}</span>
                  </dd>
                </div>
              </dl>

              <template v-if="selectedHookSource">
                <div v-if="hookDocumentLoading" class="plugins-browser__notice">
                  <Loader2 :size="14" class="is-spinning" aria-hidden="true" />
                  <span>读取 Hooks 文档…</span>
                </div>
                <div v-else-if="hookDocumentError" class="plugins-hook-section">
                  <div class="plugins-hook-section__title">读取失败</div>
                  <div class="plugins-hook-section__card plugins-hook-section__card--warn">
                    {{ hookDocumentError }}
                  </div>
                </div>
                <div v-if="selectedDetailWarnings.length" class="plugins-hook-section">
                  <div class="plugins-hook-section__title">Warnings</div>
                  <ul class="plugins-hook-section__list">
                    <li v-for="warning in selectedDetailWarnings" :key="warning">{{ warning }}</li>
                  </ul>
                </div>
                <div v-if="selectedDetailLimitations.length" class="plugins-hook-section">
                  <div class="plugins-hook-section__title">限制与说明</div>
                  <ul class="plugins-hook-section__list">
                    <li v-for="limitation in selectedDetailLimitations" :key="limitation">{{ limitation }}</li>
                  </ul>
                </div>
                <div class="plugins-hook-section">
                  <div class="plugins-hook-section__title">Handlers</div>
                  <div v-if="!selectedHookHandlers.length" class="plugins-browser__notice">
                    {{ selectedHookSourceData?.exists ? "当前来源没有 Handler" : "当前来源尚未创建" }}
                  </div>
                  <div v-else class="plugins-hook-handlers">
                    <article
                      v-for="handler in selectedHookHandlers"
                      :key="handler.id"
                      class="plugins-hook-handler"
                    >
                      <div class="plugins-hook-handler__head">
                        <strong>{{ handler.event }}</strong>
                        <span class="plugins-browser__row-meta">{{ handlerMeta(handler) }}</span>
                      </div>
                      <div class="plugins-hook-handler__body">
                        <code v-if="handler.command">{{ handler.command }}</code>
                        <code v-if="handler.commandWindows">{{ handler.commandWindows }}</code>
                        <span v-if="handler.statusMessage">{{ handler.statusMessage }}</span>
                        <span v-if="handler.timeoutSeconds != null">timeout: {{ handler.timeoutSeconds }}s</span>
                      </div>
                      <ul v-if="handler.warnings.length" class="plugins-hook-section__list">
                        <li v-for="warning in handler.warnings" :key="warning">{{ warning }}</li>
                      </ul>
                      <pre v-if="handler.groupAdvancedJson" class="plugins-hook-raw">{{ handler.groupAdvancedJson }}</pre>
                      <pre v-if="handler.advancedJson" class="plugins-hook-raw">{{ handler.advancedJson }}</pre>
                    </article>
                  </div>
                </div>
                <div class="plugins-hook-section">
                  <div class="plugins-hook-section__title">
                    原始文档
                    <span v-if="selectedHookDocument?.rawFormat" class="plugins-browser__row-meta">
                      {{ selectedHookDocument.rawFormat.toUpperCase() }}
                    </span>
                  </div>
                  <pre v-if="selectedHookDocument?.rawDocument" class="plugins-hook-raw">{{ selectedHookDocument.rawDocument }}</pre>
                  <div v-else class="plugins-browser__notice">当前来源没有原始文档内容</div>
                </div>
              </template>
            </div>
          </template>
          <template v-else>
            <div class="plugins-browser__detail-head">
              <div class="plugins-browser__detail-heading">
                <div class="plugins-browser__detail-title">未选择</div>
              </div>
              <div class="plugins-browser__actions">
                <button
                  v-if="canCreateInDetail()"
                  type="button"
                  class="ui-button ui-button--ghost"
                  data-agent-id="plugins.detail.create"
                  @click="openDetailCreate"
                >
                  <Plus :size="14" aria-hidden="true" />
                  <span>{{ detailCreateLabel() }}</span>
                </button>
              </div>
            </div>
            <div class="plugins-browser__empty-detail">选择一项</div>
          </template>
        </section>
      </div>
    </div>

    <SkillCreateDialog
      v-if="showCreate"
      v-model:open="showCreate"
      v-model:name="newName"
      v-model:description="newDesc"
      :scope-hint="USER_SKILLS_ROOT"
      :creating="creating"
      :error="createError"
      @confirm="confirmCreate"
    />

    <HookSourceEditorDialog
      v-if="hookEditor.showHookEditor.value"
      v-model:open="hookEditor.showHookEditor.value"
      :title="hookEditor.hookEditorTitle.value"
      :source-name="hookEditor.editingSource.value?.name ?? ''"
      :source-path="hookEditor.editingSource.value?.path ?? null"
      :handler-rows="hookEditor.hookHandlerRows.value"
      :saving="hookEditor.hookSaving.value"
      :error="hookEditor.hookError.value"
      @add-handler="hookEditor.addHookHandler"
      @remove-handler="hookEditor.removeHookHandler"
      @confirm="hookEditor.saveHookSource"
    />

    <McpServerEditorDialog
      v-if="mcpEditor.showMcpEditor.value"
      v-model:open="mcpEditor.showMcpEditor.value"
      v-model:name="mcpEditor.mcpName.value"
      v-model:command="mcpEditor.mcpCommand.value"
      v-model:args-text="mcpEditor.mcpArgsText.value"
      :env-rows="mcpEditor.mcpEnvRows.value"
      :editing-mcp="mcpEditor.editingMcp.value"
      :title="mcpEditor.mcpEditorTitle.value"
      server-label="Native MCP"
      :saving="mcpEditor.mcpSaving.value"
      :error="mcpEditor.mcpError.value"
      :config-path="mcpConfigPath || '~/.lilia/config/mcp-servers.json'"
      @add-env-row="mcpEditor.addMcpEnvRow"
      @remove-env-row="mcpEditor.removeMcpEnvRow"
      @confirm="mcpEditor.saveMcpServer"
    />

    <ConfirmDialog
      :open="pendingRemoveSkill !== null || pendingRemoveMcp !== null || pendingRemoveHook !== null"
      :title="pendingRemoveSkill
        ? `删除 skill「${pendingRemoveSkill?.name ?? ''}」？`
        : pendingRemoveMcp
          ? `删除 ${pluginMcpBackendLabel(pendingRemoveMcp.backend)}「${pendingRemoveMcp?.name ?? ''}」？`
          : `删除 Hooks 来源「${pendingRemoveHook?.name ?? ''}」？`"
      :message="pendingRemoveSkill
        ? '该 skill 目录会被整体删除，不可恢复。'
        : pendingRemoveMcp
          ? '该 MCP server 会从 Lilia 配置中删除，不可恢复。'
          : '该 hooks 来源会从对应配置文件中移除，不可恢复。'"
      confirm-text="删除"
      busy-text="删除中…"
      :busy="removing"
      danger
      @cancel="pendingRemoveSkill = null; pendingRemoveMcp = null; pendingRemoveHook = null"
      @confirm="confirmRemove"
    />
  </section>
</template>
