import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import {
  hooksOverview,
  type HookSourceSummary,
  pluginsOverview,
  type PluginMcpServer,
  type PluginPackage,
  type PluginSkill,
} from "../../services/plugins";
import { ensureProjectsLoaded, listProjects } from "../../services/projectsStore";

const NATIVE_BACKEND = "native-agentkit" as const;

function uniqueBy<T>(items: T[], keyFor: (item: T) => string): T[] {
  const seen = new Set<string>();
  const out: T[] = [];
  for (const item of items) {
    const key = keyFor(item);
    if (seen.has(key)) continue;
    seen.add(key);
    out.push(item);
  }
  return out;
}

function pathKey(value: string) {
  return value.replace(/\\/g, "/").replace(/\/+$/, "").toLocaleLowerCase();
}

function skillKey(skill: PluginSkill) {
  return `${skill.backend}:${skill.scope}:${pathKey(skill.path)}`;
}

function packageKey(plugin: PluginPackage) {
  return `${plugin.backend}:${plugin.scope}:${pathKey(plugin.path)}`;
}

function mcpServerKey(server: PluginMcpServer) {
  return `${server.backend}:${server.name}`;
}

function hookSourceKey(source: HookSourceSummary) {
  return source.id || `${source.backend}:${source.scope}:${source.format}:${pathKey(source.path)}`;
}

function overviewTargets(projectCwds: string[]): Array<string | null> {
  return projectCwds.length ? projectCwds : [null];
}

export function usePluginsOverview(options: {
  isDisposed?: () => boolean;
} = {}) {
  const projects = computed(() => listProjects());
  const projectsWithCwd = computed(() =>
    projects.value.filter((p): p is typeof p & { cwd: string } => !!p.cwd),
  );
  const projectCwdSignature = computed(() => projectsWithCwd.value.map((p) => p.cwd).join("\n"));
  const projectOptions = computed(() =>
    projectsWithCwd.value.map((p) => ({ value: p.cwd, label: p.name, hint: p.cwd })),
  );

  const userSkills = ref<PluginSkill[]>([]);
  const projectSkills = ref<PluginSkill[]>([]);
  const packages = ref<PluginPackage[]>([]);
  const mcpServers = ref<PluginMcpServer[]>([]);
  const mcpConfigPath = ref<string | null>(null);
  const hookSources = ref<HookSourceSummary[]>([]);
  const warnings = ref<string[]>([]);
  const loading = ref(false);
  const errorText = ref<string | null>(null);
  let refreshSeq = 0;
  let disposed = false;
  const isDisposed = () => disposed || options.isDisposed?.() === true;

  async function refresh() {
    if (isDisposed()) return;
    const seq = ++refreshSeq;
    loading.value = true;
    errorText.value = null;
    try {
      await ensureProjectsLoaded();
      if (isDisposed() || seq !== refreshSeq) return;
      const targets = overviewTargets(projectsWithCwd.value.map((project) => project.cwd));
      const [pluginOverviews, hookOverviews] = await Promise.all([
        Promise.all(targets.map((cwd) => pluginsOverview(cwd))),
        Promise.all(targets.map((cwd) => hooksOverview(cwd))),
      ]);
      if (isDisposed() || seq !== refreshSeq) return;
      const allSkills = pluginOverviews.flatMap((data) => data.skills);
      const allPackages = pluginOverviews.flatMap((data) => data.packages);
      const allMcpServers = pluginOverviews.flatMap((data) => data.mcpServers);
      const allHookSources = hookOverviews.flatMap((data) => data.sources);
      userSkills.value = uniqueBy(allSkills.filter(
        (skill) => skill.backend === NATIVE_BACKEND && skill.scope === "user",
      ), skillKey);
      projectSkills.value = uniqueBy(allSkills.filter(
        (skill) => skill.backend === NATIVE_BACKEND && skill.scope === "project",
      ), skillKey);
      packages.value = uniqueBy(
        allPackages.filter((plugin) => plugin.backend === NATIVE_BACKEND),
        packageKey,
      );
      mcpServers.value = uniqueBy(
        allMcpServers.filter((server) => server.backend === NATIVE_BACKEND),
        mcpServerKey,
      );
      mcpConfigPath.value = pluginOverviews[0]?.configPaths?.[NATIVE_BACKEND] ?? null;
      hookSources.value = uniqueBy(
        allHookSources.filter((source) => source.backend === NATIVE_BACKEND),
        hookSourceKey,
      );
      warnings.value = Array.from(new Set([
        ...pluginOverviews.flatMap((data) => data.warnings),
        ...hookOverviews.flatMap((data) => data.warnings),
      ]));
    } catch (err) {
      if (!isDisposed() && seq === refreshSeq) errorText.value = String(err);
    } finally {
      if (!isDisposed() && seq === refreshSeq) loading.value = false;
    }
  }

  onMounted(() => refresh());
  watch(projectCwdSignature, () => {
    void refresh();
  });

  onBeforeUnmount(() => {
    disposed = true;
    refreshSeq += 1;
  });

  return {
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
  };
}
