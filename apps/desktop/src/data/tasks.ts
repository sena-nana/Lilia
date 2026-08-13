/**
 * 任务 + 孤儿对话 store：所有数据经生成契约的 Product Core facade 持久化。
 *
 * - OrphanConversation = `project_id IS NULL` 的 task，同一张表。
 * - 草稿（draft）留在前端内存，不落库；`promote` 后才 INSERT。
 */
import type { UnlistenFn } from "@tauri-apps/api/event";
import { ref } from "vue";
import {
  TASK_ARCHIVE_COMMAND,
  TASK_ARCHIVE_PROJECT_COMMAND,
  TASK_REORDER_COMMAND,
  TASK_REPARENT_COMMAND,
  type ProductConversation,
  type ProductEvent,
  type ProductTask,
  type Task,
} from "@lilia/contracts";
import {
  ensureProjectsLoaded,
  listProjects,
  registerProjectRemovalHandler,
} from "./projects";
import { addDomEventListener } from "@lilia/ui/utils/eventListeners";
import { singleFlight } from "@lilia/ui/utils/singleFlight";
import {
  createProductEntity,
  getProductEntity,
  listProductEntities,
  newProductCommandMeta,
  onProductEvent,
  updateProductEntity,
} from "../services/productCore";
import { invoke } from "../tauri/runtime";

// OrphanConversation 形状沿用 Task 的子集，project_id 为 null。
export interface OrphanConversation {
  id: string;
  sessionId: string;
  title: string;
  createdAt: number;
  pinned: boolean;
  parentId: string | null;
}

export const TASKS = ref<Record<string, Task[]>>({});
export const ORPHAN_LIST = ref<OrphanConversation[]>([]);
export const PROJECT_TASKS_LOADED = ref<Record<string, boolean>>({});
export const ORPHANS_LOADED = ref(false);

const DRAFT_TASKS = new Map<string, Task>();
const DRAFT_ORPHANS = new Map<string, OrphanConversation>();
const PRODUCT_TASKS = new Map<string, ProductTask>();
const DRAFT_TASK_PROMOTIONS = new Map<string, Promise<void>>();
const DRAFT_ORPHAN_PROMOTIONS = new Map<string, Promise<void>>();
const projectTaskLoads = new Map<string, Promise<void>>();
const taskRowLoads = new Map<string, Promise<ProductTask | null>>();
let productTaskSnapshotEpoch = 0;
let orphanLoad: Promise<void> | null = null;
let tasksChangedListenerInstalled = false;
let tasksChangedListenerInstallPromise: Promise<void> | null = null;
let tasksChangedListenerUnlisten: UnlistenFn | null = null;
let tasksChangedBeforeUnloadUnlisten: UnlistenFn | null = null;

function rememberDraftPromotion(
  promotions: Map<string, Promise<void>>,
  id: string,
  run: () => Promise<void>,
): Promise<void> {
  const existing = promotions.get(id);
  if (existing) return existing;
  const promotion = run().finally(() => {
    if (promotions.get(id) === promotion) promotions.delete(id);
  });
  promotions.set(id, promotion);
  return promotion;
}

async function loadProductTasks(): Promise<ProductTask[]> {
  for (;;) {
    const epoch = productTaskSnapshotEpoch;
    const entities = await listProductEntities("task");
    const tasks = entities
      .filter((entity): entity is Extract<typeof entity, { kind: "task" }> =>
        entity.kind === "task"
      )
      .map((entity) => entity.value);
    if (epoch !== productTaskSnapshotEpoch) continue;
    PRODUCT_TASKS.clear();
    for (const task of tasks) PRODUCT_TASKS.set(task.id, task);
    return tasks;
  }
}

function rememberProductTask(row: ProductTask): void {
  productTaskSnapshotEpoch += 1;
  PRODUCT_TASKS.set(row.id, row);
}

function loadTaskRow(taskId: string): Promise<ProductTask | null> {
  return singleFlight(taskRowLoads, taskId, async () => {
    const entity = await getProductEntity("task", taskId);
    if (!entity || entity.kind !== "task" || entity.value.archived) return null;
    rememberProductTask(entity.value);
    return entity.value;
  });
}

async function refreshTasks(projectId: string): Promise<void> {
  const rows = (await loadProductTasks())
    .filter((task) => !task.archived && task.projectId === projectId)
    .sort(compareProductTasks);
  TASKS.value = {
    ...TASKS.value,
    [projectId]: rows.map(rowToTask),
  };
  PROJECT_TASKS_LOADED.value = {
    ...PROJECT_TASKS_LOADED.value,
    [projectId]: true,
  };
}

async function refreshOrphans(): Promise<void> {
  const rows = (await loadProductTasks())
    .filter((task) => !task.archived && task.projectId === null)
    .sort(compareProductTasks);
  ORPHAN_LIST.value = rows.map(rowToOrphan);
  ORPHANS_LOADED.value = true;
}

function compareProductTasks(left: ProductTask, right: ProductTask): number {
  return Number(right.pinned) - Number(left.pinned) ||
    left.sortOrder - right.sortOrder ||
    right.createdAt - left.createdAt ||
    left.id.localeCompare(right.id);
}

function rowToTask(r: ProductTask): Task {
  return {
    id: r.id,
    projectId: r.projectId ?? "",
    sessionId: r.id,
    title: r.title,
    status: r.status as Task["status"],
    createdAt: r.createdAt,
    pinned: r.pinned,
    parentId: r.parentId,
    dependsOn: r.dependsOn,
  };
}

function rowToOrphan(r: ProductTask): OrphanConversation {
  return {
    id: r.id,
    sessionId: r.id,
    title: r.title,
    createdAt: r.createdAt,
    pinned: r.pinned,
    parentId: r.parentId,
  };
}

function upsertTaskRow(row: ProductTask): Task | OrphanConversation {
  rememberProductTask(row);
  if (row.projectId) {
    const task = rowToTask(row);
    const existing = TASKS.value[row.projectId] ?? [];
    const index = existing.findIndex((t) => t.id === task.id);
    const nextProjectTasks = [...existing];
    if (index === -1) {
      nextProjectTasks.unshift(task);
    } else {
      nextProjectTasks[index] = task;
    }
    TASKS.value = {
      ...TASKS.value,
      [row.projectId]: nextProjectTasks,
    };
    ORPHAN_LIST.value = ORPHAN_LIST.value.filter((o) => o.id !== task.id);
    return task;
  }

  const orphan = rowToOrphan(row);
  const index = ORPHAN_LIST.value.findIndex((o) => o.id === orphan.id);
  const nextOrphans = [...ORPHAN_LIST.value];
  if (index === -1) {
    nextOrphans.unshift(orphan);
  } else {
    nextOrphans[index] = orphan;
  }
  ORPHAN_LIST.value = nextOrphans;
  for (const [projectId, list] of Object.entries(TASKS.value)) {
    if (list.some((t) => t.id === orphan.id)) {
      TASKS.value = {
        ...TASKS.value,
        [projectId]: list.filter((t) => t.id !== orphan.id),
      };
    }
  }
  return orphan;
}

export function isProjectTasksLoaded(projectId: string): boolean {
  return PROJECT_TASKS_LOADED.value[projectId] === true ||
    Object.prototype.hasOwnProperty.call(TASKS.value, projectId);
}

export function areOrphansLoaded(): boolean {
  return ORPHANS_LOADED.value || ORPHAN_LIST.value.length > 0;
}

export function ensureOrphansLoaded(force = false): Promise<void> {
  if (!force && ORPHANS_LOADED.value) return Promise.resolve();
  if (!force && orphanLoad) return orphanLoad;
  orphanLoad = refreshOrphans().finally(() => {
    orphanLoad = null;
  });
  return orphanLoad;
}

export function ensureProjectTasksLoaded(projectId: string, force = false): Promise<void> {
  if (!projectId) return Promise.resolve();
  if (!force && isProjectTasksLoaded(projectId)) return Promise.resolve();
  const pending = projectTaskLoads.get(projectId);
  if (!force && pending) return pending;
  const load = refreshTasks(projectId).finally(() => {
    if (projectTaskLoads.get(projectId) === load) {
      projectTaskLoads.delete(projectId);
    }
  });
  projectTaskLoads.set(projectId, load);
  return load;
}

export async function ensureAllProjectTasksLoaded(): Promise<void> {
  await ensureProjectsLoaded();
  const projs = listProjects();
  await Promise.all(projs.map((p) => ensureProjectTasksLoaded(p.id)));
}

async function refreshChangedTasks(event: ProductEvent) {
  if (event.entity !== "task") return;
  const loadedProjects = Object.keys(PROJECT_TASKS_LOADED.value)
    .filter((projectId) => isProjectTasksLoaded(projectId));
  await Promise.all([
    ...loadedProjects.map((projectId) => ensureProjectTasksLoaded(projectId, true)),
    ...(areOrphansLoaded() ? [ensureOrphansLoaded(true)] : []),
  ]);
}

export function installTasksChangedListener(options: { force?: boolean } = {}) {
  if (options.force) {
    disposeTasksChangedListener();
  }
  if (tasksChangedListenerInstalled || tasksChangedListenerInstallPromise) return;
  tasksChangedListenerInstallPromise = onProductEvent((event) => {
    void refreshChangedTasks(event);
  })
    .then((unlisten) => {
      tasksChangedListenerUnlisten = unlisten;
      tasksChangedListenerInstalled = true;
      installTasksChangedBeforeUnloadCleanup();
    })
    .catch((err) => {
      console.error("[tasks] listen product-event failed", err);
    })
    .finally(() => {
      tasksChangedListenerInstallPromise = null;
    });
}

function installTasksChangedBeforeUnloadCleanup() {
  if (tasksChangedBeforeUnloadUnlisten || typeof window === "undefined") return;
  tasksChangedBeforeUnloadUnlisten = addDomEventListener(
    window,
    "beforeunload",
    disposeTasksChangedListener,
    { once: true },
  );
}

export function disposeTasksChangedListener() {
  tasksChangedListenerUnlisten?.();
  tasksChangedListenerUnlisten = null;
  tasksChangedListenerInstalled = false;
  tasksChangedListenerInstallPromise = null;
  tasksChangedBeforeUnloadUnlisten?.();
  tasksChangedBeforeUnloadUnlisten = null;
}

installTasksChangedListener();

export function listTasks(projectId: string): Task[] {
  return TASKS.value[projectId] ?? [];
}

export function getTask(projectId: string, taskId: string): Task | undefined {
  const persisted = (TASKS.value[projectId] ?? []).find((t) => t.id === taskId);
  const draft = DRAFT_TASKS.get(taskId);
  return draft?.projectId === projectId ? draft : persisted;
}

export async function ensureTaskLoaded(
  taskId: string,
  expectedProjectId?: string | null,
): Promise<Task | OrphanConversation | null> {
  if (expectedProjectId) {
    const existing = getTask(expectedProjectId, taskId);
    if (existing) return existing;
  } else if (expectedProjectId === null) {
    const existing = getOrphanConversation(taskId);
    if (existing) return existing;
  }

  const row = await loadTaskRow(taskId);
  if (!row) return null;
  if (expectedProjectId !== undefined && row.projectId !== expectedProjectId) return null;
  return upsertTaskRow(row);
}

export function listProjectConversations(projectId: string): Task[] {
  return listTasks(projectId);
}

export function isDraftTask(id: string): boolean {
  return DRAFT_TASKS.has(id);
}

export function resolveConversationRouteState(
  projectId: string | null | undefined,
  taskId: string | null | undefined,
) {
  if (!taskId) {
    return {
      isDraftRoute: false,
      isLiveDraft: false,
      isLostDraft: false,
    };
  }

  const projectScoped = !!projectId;
  const isDraftRoute = projectScoped
    ? taskId.startsWith("t-draft-")
    : taskId.startsWith("o-draft-");
  const isLiveDraft = projectScoped
    ? isDraftRoute && isDraftTask(taskId)
    : isDraftRoute && isDraftOrphan(taskId);
  const exists = projectScoped
    ? !!getTask(projectId, taskId)
    : !!getOrphanConversation(taskId);

  return {
    isDraftRoute,
    isLiveDraft,
    isLostDraft: isDraftRoute && !isLiveDraft && !exists,
  };
}

export function createDraftTask(projectId: string, parentId: string | null = null): Task {
  const id = `t-draft-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 6)}`;
  const draft: Task = {
    id,
    projectId,
    sessionId: id,
    title: "新对话",
    status: "draft",
    createdAt: Date.now(),
    pinned: false,
    parentId,
    dependsOn: [],
  };
  DRAFT_TASKS.set(id, draft);
  return draft;
}

export async function promoteDraftTask(id: string, title: string): Promise<void> {
  const draft = DRAFT_TASKS.get(id);
  if (!draft) return DRAFT_TASK_PROMOTIONS.get(id);
  return rememberDraftPromotion(DRAFT_TASK_PROMOTIONS, id, async () => {
    const now = Date.now();
    const productTask: ProductTask = {
      id,
      projectId: draft.projectId,
      title: title || draft.title,
      description: null,
      status: "waiting",
      priority: "normal",
      assignmentId: null,
      completionCriteria: [],
      milestoneId: null,
      workflowId: null,
      agentProfileId: null,
      blockedReason: null,
      dependsOn: draft.dependsOn,
      parentId: draft.parentId,
      pinned: draft.pinned,
      sortOrder: 0,
      archived: false,
      tags: [],
      createdAt: draft.createdAt,
      updatedAt: now,
      revision: 1,
      legacySource: null,
    };
    const taskResult = await createProductEntity(
      {
        commandId: `promote-task-${id}`,
        idempotencyKey: `promote-task-${id}`,
        expectedRevision: null,
      },
      { kind: "task", value: productTask },
      "promoted",
    );
    if (taskResult.value.kind !== "task") throw new Error("Product Core 返回了错误的任务实体。");
    const conversation: ProductConversation = {
      id,
      projectId: draft.projectId,
      taskId: id,
      title: productTask.title,
      status: "active",
      archived: false,
      labels: [],
      bindingIds: [],
      forkedFrom: null,
      migratedFrom: null,
      legacySource: null,
      timelineCursor: 0,
      createdAt: draft.createdAt,
      updatedAt: now,
      revision: 1,
    };
    await createProductEntity(
      {
        commandId: `promote-conversation-${id}`,
        idempotencyKey: `promote-conversation-${id}`,
        expectedRevision: null,
      },
      { kind: "conversation", value: conversation },
      "created",
    );
    const row = taskResult.value.value;
    rememberProductTask(row);
    const task = rowToTask(row);
    const existing = TASKS.value[draft.projectId] ?? [];
    if (!existing.some((t) => t.id === id)) {
      TASKS.value = {
        ...TASKS.value,
        [draft.projectId]: [task, ...existing],
      };
    }
    DRAFT_TASKS.delete(id);
  });
}

export async function archiveTask(taskId: string): Promise<boolean> {
  const archived = await invoke<boolean>(TASK_ARCHIVE_COMMAND, { id: taskId });
  if (!archived) return false;
  await loadProductTasks();
  removeArchivedTaskFromLists(taskId);
  return true;
}

export function removeArchivedTaskFromLists(taskId: string): void {
  for (const [pid, list] of Object.entries(TASKS.value)) {
    const idx = list.findIndex((t) => t.id === taskId);
    if (idx !== -1) {
      const next = [...list];
      next.splice(idx, 1);
      TASKS.value = { ...TASKS.value, [pid]: next };
      return;
    }
  }
  ORPHAN_LIST.value = ORPHAN_LIST.value.filter((o) => o.id !== taskId);
}

export async function archiveProjectConversations(projectId: string): Promise<number> {
  const archived = await invoke<number>(TASK_ARCHIVE_PROJECT_COMMAND, { projectId });
  await loadProductTasks();
  const next = { ...TASKS.value };
  delete next[projectId];
  TASKS.value = next;
  let draftCleared = 0;
  for (const [draftId, draft] of DRAFT_TASKS) {
    if (draft.projectId === projectId) {
      DRAFT_TASKS.delete(draftId);
      draftCleared += 1;
    }
  }
  return archived + draftCleared;
}

export async function toggleTaskPin(taskId: string): Promise<boolean> {
  const current = await loadTaskRow(taskId);
  if (!current || current.archived) return false;
  const result = await updateProductEntity(
    newProductCommandMeta("toggle-task-pin", current.revision),
    {
      kind: "task",
      value: { ...current, pinned: !current.pinned, updatedAt: Date.now() },
    },
    "pin_changed",
  );
  if (result.value.kind !== "task") throw new Error("Product Core 返回了错误的任务实体。");
  const pinned = result.value.value.pinned;
  rememberProductTask(result.value.value);
  for (const [pid, list] of Object.entries(TASKS.value)) {
    if (list.some((t) => t.id === taskId)) {
      await refreshTasks(pid);
      return pinned;
    }
  }
  if (ORPHAN_LIST.value.some((o) => o.id === taskId)) {
    await refreshOrphans();
  }
  return pinned;
}

export function removeProjectTasks(projectId: string): void {
  const next = { ...TASKS.value };
  delete next[projectId];
  TASKS.value = next;
  const loaded = { ...PROJECT_TASKS_LOADED.value };
  delete loaded[projectId];
  PROJECT_TASKS_LOADED.value = loaded;
  for (const [draftId, draft] of DRAFT_TASKS) {
    if (draft.projectId === projectId) DRAFT_TASKS.delete(draftId);
  }
}

export async function detachProjectTasksToOrphans(projectId: string): Promise<void> {
  removeProjectTasks(projectId);
  await refreshOrphans();
}

registerProjectRemovalHandler(detachProjectTasksToOrphans);

export function listOrphanConversations(): OrphanConversation[] {
  return ORPHAN_LIST.value;
}

export function getOrphanConversation(id: string): OrphanConversation | undefined {
  const persisted = ORPHAN_LIST.value.find((o) => o.id === id);
  return DRAFT_ORPHANS.get(id) ?? persisted;
}

export function isDraftOrphan(id: string): boolean {
  return DRAFT_ORPHANS.has(id);
}

export function createDraftOrphan(parentId: string | null = null): OrphanConversation {
  const id = `o-draft-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 6)}`;
  const draft: OrphanConversation = {
    id,
    sessionId: id,
    title: "新对话",
    createdAt: Date.now(),
    pinned: false,
    parentId,
  };
  DRAFT_ORPHANS.set(id, draft);
  return draft;
}

export async function promoteDraftOrphan(id: string, title: string): Promise<void> {
  const draft = DRAFT_ORPHANS.get(id);
  if (!draft) return DRAFT_ORPHAN_PROMOTIONS.get(id);
  return rememberDraftPromotion(DRAFT_ORPHAN_PROMOTIONS, id, async () => {
    const now = Date.now();
    const productTask: ProductTask = {
      id,
      projectId: null,
      title: title || draft.title,
      description: null,
      status: "waiting",
      priority: "normal",
      assignmentId: null,
      completionCriteria: [],
      milestoneId: null,
      workflowId: null,
      agentProfileId: null,
      blockedReason: null,
      dependsOn: [],
      parentId: draft.parentId,
      pinned: draft.pinned,
      sortOrder: 0,
      archived: false,
      tags: [],
      createdAt: draft.createdAt,
      updatedAt: now,
      revision: 1,
      legacySource: null,
    };
    const taskResult = await createProductEntity(
      {
        commandId: `promote-task-${id}`,
        idempotencyKey: `promote-task-${id}`,
        expectedRevision: null,
      },
      { kind: "task", value: productTask },
      "promoted",
    );
    if (taskResult.value.kind !== "task") throw new Error("Product Core 返回了错误的任务实体。");
    await createProductEntity(
      {
        commandId: `promote-conversation-${id}`,
        idempotencyKey: `promote-conversation-${id}`,
        expectedRevision: null,
      },
      {
        kind: "conversation",
        value: {
          id,
          projectId: null,
          taskId: id,
          title: productTask.title,
          status: "active",
          archived: false,
          labels: [],
          bindingIds: [],
          forkedFrom: null,
          migratedFrom: null,
          legacySource: null,
          timelineCursor: 0,
          createdAt: draft.createdAt,
          updatedAt: now,
          revision: 1,
        },
      },
      "created",
    );
    const row = taskResult.value.value;
    rememberProductTask(row);
    if (!ORPHAN_LIST.value.some((o) => o.id === id)) {
      ORPHAN_LIST.value = [
        {
          id: row.id,
          sessionId: row.id,
          title: row.title,
          createdAt: row.createdAt,
          pinned: row.pinned,
          parentId: row.parentId,
        },
        ...ORPHAN_LIST.value,
      ];
    }
    DRAFT_ORPHANS.delete(id);
  });
}

export async function reorderTasks(
  projectId: string | null,
  orderedIds: string[],
): Promise<void> {
  await invoke<ProductTask[]>(TASK_REORDER_COMMAND, { projectId, orderedIds });
  if (projectId) {
    await refreshTasks(projectId);
  } else {
    await refreshOrphans();
  }
}

export async function reparentTask(
  taskId: string,
  sourceProjectId: string | null,
  targetProjectId: string | null,
  targetParentId: string | null = null,
): Promise<void> {
  const row = await invoke<ProductTask>(TASK_REPARENT_COMMAND, {
    taskId,
    newProjectId: targetProjectId,
    newParentId: targetParentId,
  });
  if (sourceProjectId) {
    const list = TASKS.value[sourceProjectId] ?? [];
    TASKS.value = {
      ...TASKS.value,
      [sourceProjectId]: list.filter((t) => t.id !== taskId),
    };
  } else {
    ORPHAN_LIST.value = ORPHAN_LIST.value.filter((o) => o.id !== taskId);
  }
  upsertTaskRow(row);
}

export async function updateTaskDependencies(
  taskId: string,
  projectId: string | null,
  dependsOn: string[],
): Promise<void> {
  const current = await loadTaskRow(taskId);
  if (!current) return;
  const result = await updateProductEntity(
    newProductCommandMeta("update-task-dependencies", current.revision),
    {
      kind: "task",
      value: { ...current, dependsOn, updatedAt: Date.now() },
    },
    "dependencies_updated",
  );
  if (result.value.kind !== "task") throw new Error("Product Core 返回了错误的任务实体。");
  const row = result.value.value;
  upsertTaskRow(row);
  if (projectId && row.projectId !== projectId) {
    await refreshTasks(projectId);
  } else if (!projectId && row.projectId !== null) {
    await refreshOrphans();
  }
}
