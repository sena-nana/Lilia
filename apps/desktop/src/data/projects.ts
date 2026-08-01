/**
 * 项目 store：所有数据经生成契约的 Product Core facade 持久化。
 * 内部维护 `PROJECTS` reactive ref 作为 UI 缓存。
 */
import { ref } from "vue";
import type {
  ProductEntity,
  ProductConversation,
  ProductProject,
  ProductTask,
  Project,
} from "@lilia/contracts";
import { singleFlight } from "@lilia/ui/utils/singleFlight";
import {
  createProductEntity,
  getProductEntity,
  listProductEntities,
  newProductCommandMeta,
  newProductId,
  updateProductEntity,
} from "../services/productCore";

export const PROJECTS = ref<Project[]>([]);
const PRODUCT_PROJECTS = new Map<string, ProductProject>();
const projectsLoaded = ref(false);
let projectLoad: Promise<void> | null = null;
const projectLoads = new Map<string, Promise<Project | null>>();
let onProjectRemoved:
  | ((projectId: string) => void | Promise<void>)
  | null = null;

export function registerProjectRemovalHandler(
  handler: (projectId: string) => void | Promise<void>,
): void {
  onProjectRemoved = handler;
}

async function refresh(): Promise<void> {
  const [projectEntities, conversationEntities] = await Promise.all([
    listProductEntities("project"),
    listProductEntities("conversation"),
  ]);
  PRODUCT_PROJECTS.clear();
  const sessionCounts = new Map<string, number>();
  for (const entity of conversationEntities) {
    if (entity.kind !== "conversation" || entity.value.archived || !entity.value.projectId) continue;
    sessionCounts.set(
      entity.value.projectId,
      (sessionCounts.get(entity.value.projectId) ?? 0) + 1,
    );
  }
  const projects = projectEntities
    .filter((entity): entity is Extract<ProductEntity, { kind: "project" }> =>
      entity.kind === "project" && entity.value.archive === "active"
    )
    .map((entity) => {
      PRODUCT_PROJECTS.set(entity.value.id, entity.value);
      return productProjectToProject(entity.value, sessionCounts.get(entity.value.id) ?? 0);
    })
    .sort((left, right) => {
      const leftProduct = PRODUCT_PROJECTS.get(left.id)!;
      const rightProduct = PRODUCT_PROJECTS.get(right.id)!;
      return Number(right.pinned) - Number(left.pinned) ||
        leftProduct.sortOrder - rightProduct.sortOrder ||
        left.id.localeCompare(right.id);
    });
  PROJECTS.value = projects;
  projectsLoaded.value = true;
}

function productProjectToProject(project: ProductProject, sessionCount: number): Project {
  return {
    id: project.id,
    name: project.name,
    cwd: project.workspacePath,
    sessionCount,
    pinned: project.pinned,
  };
}

function upsertProject(project: Project): Project {
  const index = PROJECTS.value.findIndex((p) => p.id === project.id);
  if (index === -1) {
    PROJECTS.value = [...PROJECTS.value, project];
    return project;
  }
  const next = [...PROJECTS.value];
  next[index] = project;
  PROJECTS.value = next;
  return project;
}

async function loadProductProject(id: string): Promise<ProductProject | null> {
  const entity = await getProductEntity("project", id);
  if (!entity || entity.kind !== "project" || entity.value.archive === "archived") return null;
  PRODUCT_PROJECTS.set(id, entity.value);
  return entity.value;
}

function shouldDeferInitialRefresh(): boolean {
  return typeof window !== "undefined" && window.location.hash.startsWith("#/popup");
}

export function ensureProjectsLoaded(force = false): Promise<void> {
  if (shouldDeferInitialRefresh()) return Promise.resolve();
  if (!force && projectsLoaded.value) return Promise.resolve();
  if (!force && projectLoad) return projectLoad;
  projectLoad = refresh().finally(() => {
    projectLoad = null;
  });
  return projectLoad;
}

export function listProjects(): Project[] {
  return PROJECTS.value;
}

export function getProject(id: string): Project | undefined {
  return PROJECTS.value.find((p) => p.id === id);
}

export async function ensureProjectLoaded(id: string): Promise<Project | null> {
  const existing = getProject(id);
  if (existing) return existing;
  return singleFlight(projectLoads, id, async () => {
    const product = await loadProductProject(id);
    if (!product) return null;
    return upsertProject(productProjectToProject(product, 0));
  });
}

/**
 * 侧栏「添加项目」入口：本地文件夹 / clone / 空分类三类都进这里。
 * cwd 传 null 表示「分类型」项目，仅做侧栏归类用。
 */
export async function createProject(input: {
  name: string;
  cwd: string | null;
}): Promise<Project> {
  const trimmedName = input.name.trim();
  const project: ProductProject = {
    id: newProductId("project"),
    name: trimmedName || "未命名项目",
    workspacePath: input.cwd && input.cwd.trim() ? input.cwd.trim() : null,
    pinned: false,
    sortOrder: PRODUCT_PROJECTS.size,
    archive: "active",
    gitWorkspace: null,
    settings: {
      defaultAgentProfileId: null,
      values: {},
    },
    assetIds: [],
    revision: 1,
  };
  const result = await createProductEntity(
    newProductCommandMeta("create-project"),
    { kind: "project", value: project },
    "created",
  );
  if (result.value.kind !== "project") throw new Error("Product Core 返回了错误的项目实体。");
  PRODUCT_PROJECTS.set(result.value.value.id, result.value.value);
  return upsertProject(productProjectToProject(result.value.value, 0));
}

export async function ensureFolderProjects(paths: string[]): Promise<Project[]> {
  await ensureProjectsLoaded();
  const projects: Project[] = [];
  for (const path of paths) {
    const normalized = path.trim();
    if (!normalized) continue;
    const existing = [...PRODUCT_PROJECTS.values()]
      .find((project) => project.archive === "active" && project.workspacePath === normalized);
    if (existing) {
      projects.push(upsertProject(productProjectToProject(
        existing,
        getProject(existing.id)?.sessionCount ?? 0,
      )));
      continue;
    }
    projects.push(await createProject({
      name: deriveProjectName(normalized) || "未命名项目",
      cwd: normalized,
    }));
  }
  return projects;
}

/** 更新项目名称；trim 后为空时不改动。返回是否真正更新。 */
export async function renameProject(id: string, nextName: string): Promise<boolean> {
  const name = nextName.trim();
  const current = await loadProductProject(id);
  if (!current || !name || current.name === name) return false;
  const result = await updateProductEntity(
    newProductCommandMeta("rename-project", current.revision),
    { kind: "project", value: { ...current, name } },
    "renamed",
  );
  if (result.value.kind !== "project") throw new Error("Product Core 返回了错误的项目实体。");
  PRODUCT_PROJECTS.set(id, result.value.value);
  await refresh();
  return true;
}

/**
 * 「移除项目」：从侧栏摘掉项目本身，它的 tasks 变成孤儿（project_id → null）。
 * 不动磁盘上的 cwd 目录。
 */
export async function removeProject(id: string): Promise<boolean> {
  const current = await loadProductProject(id);
  if (!current || current.archive === "archived") return false;

  const conversationEntities = await listProductEntities("conversation");
  for (const entity of conversationEntities) {
    if (entity.kind !== "conversation" || entity.value.projectId !== id || entity.value.archived) {
      continue;
    }
    const conversation: ProductConversation = { ...entity.value, projectId: null };
    await updateProductEntity(
      newProductCommandMeta("detach-project-conversation", conversation.revision),
      { kind: "conversation", value: conversation },
      "detached_from_project",
    );
  }
  const taskEntities = await listProductEntities("task");
  for (const entity of taskEntities) {
    if (entity.kind !== "task" || entity.value.projectId !== id || entity.value.archived) continue;
    const task: ProductTask = { ...entity.value, projectId: null };
    await updateProductEntity(
      newProductCommandMeta("detach-project-task", task.revision),
      { kind: "task", value: task },
      "detached_from_project",
    );
  }
  await updateProductEntity(
    newProductCommandMeta("archive-project", current.revision),
    { kind: "project", value: { ...current, archive: "archived" } },
    "archived",
  );
  PRODUCT_PROJECTS.delete(id);
  await refresh();
  await onProjectRemoved?.(id);
  return true;
}

/** 从绝对路径取末尾段作为项目名候选；Windows / Unix 分隔符都吃。 */
export function deriveProjectName(absPath: string): string {
  const cleaned = absPath.trim().replace(/[\\/]+$/, "");
  if (!cleaned) return "";
  const parts = cleaned.split(/[\\/]/);
  return parts[parts.length - 1] ?? cleaned;
}

/** 切换项目置顶状态。 */
export async function toggleProjectPin(id: string): Promise<boolean> {
  const current = await loadProductProject(id);
  if (!current) return false;
  const result = await updateProductEntity(
    newProductCommandMeta("toggle-project-pin", current.revision),
    { kind: "project", value: { ...current, pinned: !current.pinned } },
    "pin_changed",
  );
  if (result.value.kind !== "project") throw new Error("Product Core 返回了错误的项目实体。");
  const pinned = result.value.value.pinned;
  PRODUCT_PROJECTS.set(id, result.value.value);
  await refresh();
  return pinned;
}

/** 项目列表拖拽排序后调用。`orderedIds` 按显示顺序传入。 */
export async function reorderProjects(orderedIds: string[]): Promise<void> {
  const entities = await listProductEntities("project");
  for (const entity of entities) {
    if (entity.kind === "project") PRODUCT_PROJECTS.set(entity.value.id, entity.value);
  }
  for (const [sortOrder, id] of orderedIds.entries()) {
    const current = PRODUCT_PROJECTS.get(id);
    if (!current || current.sortOrder === sortOrder) continue;
    const result = await updateProductEntity(
      newProductCommandMeta("reorder-project", current.revision),
      { kind: "project", value: { ...current, sortOrder } },
      "reordered",
    );
    if (result.value.kind === "project") {
      PRODUCT_PROJECTS.set(id, result.value.value);
    }
  }
  // 本地缓存只重排参与本次拖动的项目；其它 pinned 分组保持原位。
  const byId = new Map(PROJECTS.value.map((p) => [p.id, p]));
  const reordered = orderedIds
    .map((id) => byId.get(id))
    .filter((project): project is Project => Boolean(project));
  if (reordered.length === 0) return;
  let nextIndex = 0;
  const affected = new Set(orderedIds);
  PROJECTS.value = PROJECTS.value.map((project) => {
    if (!affected.has(project.id)) return project;
    return reordered[nextIndex++] ?? project;
  });
}
