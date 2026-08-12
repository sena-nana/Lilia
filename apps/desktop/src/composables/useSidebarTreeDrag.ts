import { computed, onBeforeUnmount, ref, type ComputedRef } from "vue";
import type { Project } from "@lilia/contracts";
import { reorderProjects } from "../services/projectsStore";
import {
  listProjectConversations,
  reorderTasks,
  reparentTask,
  type OrphanConversation,
} from "../services/tasksStore";
import { addDomEventListener, runUnlistenFns } from "@lilia/ui/utils/eventListeners";

export type TreeDragKind = "project" | "task";
export type TreeDropPosition = "before" | "after" | "inside";

export interface TreeDragSource {
  kind: TreeDragKind;
  pointerId: number;
  startX: number;
  startY: number;
  active: boolean;
  projectId: string | null;
  taskId: string | null;
  pinned: boolean;
}

export interface TreeDropTarget {
  kind: "project" | "task" | "orphans";
  projectId: string | null;
  taskId: string | null;
  pinned: boolean | null;
  position: TreeDropPosition;
  valid: boolean;
}

const DRAG_THRESHOLD = 4;

function normalizeDatasetId(value: string | undefined): string | null {
  return value && value.length > 0 ? value : null;
}

function isInteractiveTreeTarget(target: HTMLElement): boolean {
  const treeRow = target.closest<HTMLElement>("[data-tree-kind]");
  const interactive = target.closest<HTMLElement>(
    "button,input,textarea,select,.sb-tree__hover-tools,.sb-menu,[data-no-tree-drag]",
  );
  return !!interactive && interactive !== treeRow;
}

function sourceFromTreeRow(row: HTMLElement, event: PointerEvent): TreeDragSource | null {
  const kind = row.dataset.treeKind;
  const pinned = row.dataset.pinned === "true";
  if (kind === "project") {
    const projectId = normalizeDatasetId(row.dataset.projectId);
    if (!projectId) return null;
    return {
      kind,
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      active: false,
      projectId,
      taskId: null,
      pinned,
    };
  }
  if (kind === "task") {
    const taskId = normalizeDatasetId(row.dataset.taskId);
    if (!taskId) return null;
    return {
      kind,
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      active: false,
      projectId: normalizeDatasetId(row.dataset.projectId),
      taskId,
      pinned,
    };
  }
  return null;
}

function closestTreeDropElement(target: EventTarget | null): HTMLElement | null {
  if (!(target instanceof HTMLElement)) return null;
  return target.closest<HTMLElement>("[data-tree-kind],[data-tree-drop-zone]");
}

function elementAtPointer(event: PointerEvent): HTMLElement | null {
  const direct = closestTreeDropElement(event.target);
  if (direct) return direct;
  const fromPoint = document.elementFromPoint?.(event.clientX, event.clientY);
  return closestTreeDropElement(fromPoint);
}

function positionForElement(
  event: PointerEvent,
  element: HTMLElement,
): Exclude<TreeDropPosition, "inside"> {
  const rect = element.getBoundingClientRect();
  return event.clientY > rect.top + rect.height / 2 ? "after" : "before";
}

function positionForTaskElement(event: PointerEvent, element: HTMLElement): TreeDropPosition {
  const rect = element.getBoundingClientRect();
  if (event.clientX > rect.left + 28) return "inside";
  return event.clientY > rect.top + rect.height / 2 ? "after" : "before";
}

function isSameTreeRow(
  marker: TreeDragSource | TreeDropTarget | null | undefined,
  kind: TreeDragKind,
  projectId: string | null,
  taskId: string | null,
): boolean {
  return !!marker &&
    marker.kind === kind &&
    marker.projectId === projectId &&
    marker.taskId === taskId;
}

export function insertRelative(
  ids: string[],
  sourceId: string,
  targetId: string,
  position: Exclude<TreeDropPosition, "inside">,
): string[] {
  const withoutSource = ids.filter((id) => id !== sourceId);
  const targetIndex = withoutSource.indexOf(targetId);
  if (targetIndex < 0) return ids;
  const insertIndex = position === "after" ? targetIndex + 1 : targetIndex;
  return [
    ...withoutSource.slice(0, insertIndex),
    sourceId,
    ...withoutSource.slice(insertIndex),
  ];
}

export function useSidebarTreeDrag(
  projects: ComputedRef<Project[]>,
  orphans: ComputedRef<OrphanConversation[]>,
  reportError: (message: string) => void,
) {
  const treeDrag = ref<TreeDragSource | null>(null);
  const treeDropTarget = ref<TreeDropTarget | null>(null);
  const suppressTreeClick = ref(false);
  let treeDragUnlisteners: Array<() => void> = [];
  let treeDragSeq = 0;
  let disposed = false;

  function resolveTreeDropTarget(event: PointerEvent): TreeDropTarget | null {
    const source = treeDrag.value;
    if (!source) return null;
    const element = elementAtPointer(event);
    if (!element) return null;

    if (element.dataset.treeDropZone === "orphans") {
      return source.kind === "task"
        ? {
            kind: "orphans",
            projectId: null,
            taskId: null,
            pinned: null,
            position: "inside",
            valid: source.projectId !== null || taskParentId(null, source.taskId) !== null,
          }
        : null;
    }

    if (element.dataset.treeKind === "project") {
      const projectId = normalizeDatasetId(element.dataset.projectId);
      if (!projectId) return null;
      const pinned = element.dataset.pinned === "true";
      if (source.kind === "project") {
        return {
          kind: "project",
          projectId,
          taskId: null,
          pinned,
          position: positionForElement(event, element),
          valid: source.projectId !== projectId && source.pinned === pinned,
        };
      }
      return {
        kind: "project",
        projectId,
        taskId: null,
        pinned,
        position: "inside",
        valid: source.projectId !== projectId || taskParentId(source.projectId, source.taskId) !== null,
      };
    }

    if (element.dataset.treeKind === "task") {
      const taskId = normalizeDatasetId(element.dataset.taskId);
      if (!taskId || source.kind !== "task") return null;
      const projectId = normalizeDatasetId(element.dataset.projectId);
      const pinned = element.dataset.pinned === "true";
      return {
        kind: "task",
        projectId,
        taskId,
        pinned,
        position: positionForTaskElement(event, element),
        valid: source.taskId !== taskId && source.pinned === pinned,
      };
    }

    return null;
  }

  function treeRowStateClass(
    kind: TreeDragKind,
    projectId: string | null,
    taskId: string | null,
  ) {
    const isSource = treeDrag.value?.active === true &&
      isSameTreeRow(treeDrag.value, kind, projectId, taskId);
    const isTarget = isSameTreeRow(treeDropTarget.value, kind, projectId, taskId);
    const position = isTarget ? treeDropTarget.value?.position : null;
    const valid = isTarget && treeDropTarget.value?.valid === true;
    return {
      "is-tree-drag-source": isSource,
      "is-tree-drop-target": isTarget,
      "is-tree-drop-invalid": isTarget && !valid,
      "is-tree-drop-before": valid && position === "before",
      "is-tree-drop-after": valid && position === "after",
      "is-tree-drop-inside": valid && position === "inside",
    };
  }

  const orphanDropZoneClass = computed(() => {
    const target = treeDropTarget.value;
    const valid = target?.kind === "orphans" && target.valid;
    return {
      "is-tree-drop-target": valid,
      "is-tree-drop-inside": valid,
    };
  });

  function taskIdsForProject(projectId: string | null, pinned: boolean): string[] {
    return projectId
      ? listProjectConversations(projectId)
          .filter((task) => task.pinned === pinned)
          .map((task) => task.id)
      : orphans.value
          .filter((orphan) => orphan.pinned === pinned)
          .map((orphan) => orphan.id);
  }

  function taskParentId(projectId: string | null, taskId: string | null): string | null {
    if (!taskId) return null;
    return projectId
      ? listProjectConversations(projectId).find((task) => task.id === taskId)?.parentId ?? null
      : orphans.value.find((orphan) => orphan.id === taskId)?.parentId ?? null;
  }

  async function applyProjectDrop(source: TreeDragSource, target: TreeDropTarget) {
    if (!source.projectId || !target.projectId || target.kind !== "project" || !target.valid) {
      return;
    }
    const orderedIds = insertRelative(
      projects.value
        .filter((project) => project.pinned === source.pinned)
        .map((project) => project.id),
      source.projectId,
      target.projectId,
      target.position === "inside" ? "after" : target.position,
    );
    await reorderProjects(orderedIds);
  }

  async function applyTaskDrop(source: TreeDragSource, target: TreeDropTarget) {
    if (!source.taskId || !target.valid) return;
    const sourceProjectId = source.projectId;
    const targetProjectId = target.projectId;
    const reorderPosition = target.position === "inside" ? null : target.position;
    const shouldReorder = target.kind === "task" && !!target.taskId && reorderPosition !== null;
    const targetParentId = target.kind === "task" && target.taskId
      ? target.position === "inside"
        ? target.taskId
        : taskParentId(targetProjectId, target.taskId)
      : null;
    await reparentTask(source.taskId, sourceProjectId, targetProjectId, targetParentId);

    if (sourceProjectId === targetProjectId) {
      if (!shouldReorder || !target.taskId || !reorderPosition) return;
      const orderedIds = insertRelative(
        taskIdsForProject(sourceProjectId, source.pinned === true),
        source.taskId,
        target.taskId,
        reorderPosition,
      );
      await reorderTasks(sourceProjectId, orderedIds);
      return;
    }

    const targetIds = taskIdsForProject(targetProjectId, source.pinned === true);
    const orderedIds = shouldReorder && target.taskId && reorderPosition
      ? insertRelative(targetIds, source.taskId, target.taskId, reorderPosition)
      : targetIds;
    if (orderedIds.includes(source.taskId)) {
      await reorderTasks(targetProjectId, orderedIds);
    }
  }

  async function applyTreeDrop(
    source = treeDrag.value,
    target = treeDropTarget.value,
    seq = treeDragSeq,
  ) {
    if (disposed || seq !== treeDragSeq || !source || !target || !target.valid) return;
    try {
      if (source.kind === "project") {
        await applyProjectDrop(source, target);
      } else {
        await applyTaskDrop(source, target);
      }
    } catch (err) {
      if (disposed || seq !== treeDragSeq) return;
      reportError(`调整项目树失败：${String(err)}`);
    }
  }

  function onTreePointerDown(event: PointerEvent) {
    if (disposed) return;
    if (event.button !== 0) return;
    const target = event.target;
    if (!(target instanceof HTMLElement)) return;
    if (isInteractiveTreeTarget(target)) return;
    const row = target.closest<HTMLElement>("[data-tree-kind]");
    if (!row) return;
    const source = sourceFromTreeRow(row, event);
    if (!source) return;
    treeDragSeq += 1;
    treeDrag.value = source;
    treeDropTarget.value = null;
    clearTreeDragListeners();
    treeDragUnlisteners = [
      addDomEventListener(window, "pointermove", onTreePointerMove),
      addDomEventListener(window, "pointerup", onTreePointerUp, { once: true }),
    ];
  }

  function onTreePointerMove(event: PointerEvent) {
    if (disposed) return;
    const source = treeDrag.value;
    if (!source || source.pointerId !== event.pointerId) return;
    const moved = Math.hypot(event.clientX - source.startX, event.clientY - source.startY);
    if (!source.active && moved < DRAG_THRESHOLD) return;
    if (!source.active) {
      treeDrag.value = { ...source, active: true };
      suppressTreeClick.value = true;
    }
    event.preventDefault();
    treeDropTarget.value = resolveTreeDropTarget(event);
  }

  async function onTreePointerUp(event: PointerEvent) {
    if (disposed) return;
    const seq = treeDragSeq;
    const source = treeDrag.value;
    clearTreeDragListeners();
    if (!source || source.pointerId !== event.pointerId) {
      treeDrag.value = null;
      treeDropTarget.value = null;
      return;
    }
    if (source.active) {
      event.preventDefault();
      treeDropTarget.value = resolveTreeDropTarget(event);
      await applyTreeDrop(source, treeDropTarget.value, seq);
      if (disposed || seq !== treeDragSeq) return;
    }
    treeDrag.value = null;
    treeDropTarget.value = null;
  }

  function onTreeClickCapture(event: MouseEvent) {
    if (disposed) return;
    if (!suppressTreeClick.value) return;
    suppressTreeClick.value = false;
    event.preventDefault();
    event.stopPropagation();
  }

  function clearTreeDragListeners() {
    runUnlistenFns(treeDragUnlisteners.splice(0).reverse());
  }

  onBeforeUnmount(() => {
    disposed = true;
    treeDragSeq += 1;
    clearTreeDragListeners();
  });

  return {
    orphanDropZoneClass,
    onTreeClickCapture,
    onTreePointerDown,
    treeDrag,
    treeDropTarget,
    treeRowStateClass,
  };
}
