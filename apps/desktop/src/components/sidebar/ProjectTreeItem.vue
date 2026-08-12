<script setup lang="ts">
import { defineAsyncComponent, onBeforeUnmount, ref, watch, type Component } from "vue";
import { useRoute, useRouter } from "vue-router";
import Archive from "@lucide/vue/dist/esm/icons/archive.mjs";
import Code2 from "@lucide/vue/dist/esm/icons/code-xml.mjs";
import Folder from "@lucide/vue/dist/esm/icons/folder.mjs";
import FolderOpen from "@lucide/vue/dist/esm/icons/folder-open.mjs";
import LayoutGrid from "@lucide/vue/dist/esm/icons/layout-grid.mjs";
import MessageSquarePlus from "@lucide/vue/dist/esm/icons/message-square-plus.mjs";
import MoreHorizontal from "@lucide/vue/dist/esm/icons/ellipsis.mjs";
import Pencil from "@lucide/vue/dist/esm/icons/pencil.mjs";
import Pin from "@lucide/vue/dist/esm/icons/pin.mjs";
import Plus from "@lucide/vue/dist/esm/icons/plus.mjs";
import Trash2 from "@lucide/vue/dist/esm/icons/trash-2.mjs";
import type { Project } from "@lilia/contracts";
import type { ConversationActivity } from "../../composables/useConversationActivity";
import { openContextMenuAt, type ContextMenuItem } from "@lilia/ui/composables/useContextMenu";
import {
  archiveProjectConversations,
  removeProject,
  renameProject,
  toggleProjectPin,
} from "../../services/projectsStore";
import { archiveTask } from "../../services/tasksStore";
import type {
  TreeDragKind,
  TreeDragSource,
  TreeDropTarget,
} from "../../composables/useSidebarTreeDrag";
import { openInFileManager, openInVSCode } from "../../services/projects";
import { openPopupNewChat } from "../../services/popupWindows";
import { beginPerfStage, cancelIdleRun, measurePerfAsync, runWhenIdle, scheduleAfterPaint } from "@lilia/ui/diagnostics";
import { createLazyLoadState } from "@lilia/ui/utils/lazyLoadState";
import { useInlineRename } from "../../composables/useInlineRename";

const projectTreeConversationListLoad = createLazyLoadState<Component>(() =>
  measurePerfAsync(
    "sidebar.project-conversation-list.load",
    async () => (await import("./ProjectTreeConversationList.vue")).default as Component,
  )
);

const ProjectTreeConversationList = defineAsyncComponent({
  suspensible: false,
  loader: () => projectTreeConversationListLoad.load(),
});

const props = defineProps<{
  activityForTask: (taskId: string) => ConversationActivity | null;
  project: Project;
  isExpanded: boolean;
  dragSource?: TreeDragSource | null;
  dropTarget?: TreeDropTarget | null;
}>();

const emit = defineEmits<{
  toggle: [projectId: string];
  newChat: [projectId: string];
  error: [msg: string];
  deleted: [];
  archived: [];
}>();

const route = useRoute();
const router = useRouter();

const conversationListReady = ref(false);
let conversationListRevealHandle: number | null = null;
let cancelConversationListRevealPaint: (() => void) | null = null;
let conversationListRevealSeq = 0;
let disposed = false;

const {
  editingId,
  editingValue,
  startRename,
  commitRename,
  onEditingKeydown,
  bindEditingInput,
} = useInlineRename({
  currentId: () => props.project.id,
  currentValue: () => props.project.name,
  commit: renameProject,
});

async function openInExplorer() {
  if (disposed || !props.project.cwd) return;
  try {
    await openInFileManager(props.project.cwd);
  } catch (err) {
    if (!disposed) emit("error", `在资源管理器中打开失败：${String(err)}`);
  }
}

async function openWithVSCode() {
  if (disposed || !props.project.cwd) return;
  try {
    await openInVSCode(props.project.cwd);
  } catch (err) {
    if (!disposed) emit("error", `用 VSCode 打开失败：${String(err)}`);
  }
}

async function archiveAllConversations() {
  if (disposed) return;
  try {
    await archiveProjectConversations(props.project.id);
    if (disposed) return;
    if (
      route.params.projectId &&
      route.params.taskId &&
      String(route.params.projectId) === props.project.id
    ) {
      emit("archived");
    }
  } catch (err) {
    if (!disposed) emit("error", `归档所有对话失败：${String(err)}`);
  }
}

async function togglePin() {
  if (disposed) return;
  await toggleProjectPin(props.project.id);
}

async function deleteProject() {
  if (disposed) return;
  try {
    const removed = await removeProject(props.project.id);
    if (disposed || !removed) return;
    if (
      route.params.projectId &&
      String(route.params.projectId) === props.project.id
    ) {
      emit("deleted");
    }
  } catch (err) {
    if (!disposed) emit("error", `移除项目失败：${String(err)}`);
  }
}

async function openProjectChatInPopup() {
  if (disposed) return;
  try {
    await openPopupNewChat(props.project.id);
  } catch (err) {
    if (!disposed) emit("error", `创建弹出窗口对话失败：${String(err)}`);
  }
}

function onProjectAuxClick(e: MouseEvent) {
  if (e.button !== 1) return;
  e.preventDefault();
  e.stopPropagation();
  void openProjectChatInPopup();
}

async function archiveProjectTask(taskId: string): Promise<boolean> {
  if (disposed) return false;
  try {
    const archived = await archiveTask(taskId);
    if (disposed) return false;
    if (archived) onProjectTaskArchived(taskId);
    return archived;
  } catch (err) {
    if (!disposed) emit("error", `归档对话失败：${String(err)}`);
    return false;
  }
}

function onProjectTaskArchived(taskId: string) {
  if (route.path === `/projects/${props.project.id}/tasks/${taskId}`) {
    emit("archived");
  }
}

function openTask(taskId: string) {
  void router.push(`/projects/${props.project.id}/tasks/${taskId}`);
}

function isActiveProject() {
  return String(route.params.projectId ?? "") === props.project.id &&
    !route.params.taskId;
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

function treeRowStateClass(
  kind: TreeDragKind,
  projectId: string | null,
  taskId: string | null,
) {
  const isSource = props.dragSource?.active === true &&
    isSameTreeRow(props.dragSource, kind, projectId, taskId);
  const isTarget = isSameTreeRow(props.dropTarget, kind, projectId, taskId);
  const position = isTarget ? props.dropTarget?.position : null;
  const valid = isTarget && props.dropTarget?.valid === true;
  return {
    "is-tree-drag-source": isSource,
    "is-tree-drop-target": isTarget,
    "is-tree-drop-invalid": isTarget && !valid,
    "is-tree-drop-before": valid && position === "before",
    "is-tree-drop-after": valid && position === "after",
    "is-tree-drop-inside": valid && position === "inside",
  };
}

function buildMenu(): ContextMenuItem[] {
  const hasCwd = !!props.project.cwd;
  return [
    {
      id: "open-project",
      label: "进入项目",
      icon: LayoutGrid,
      onSelect: () => router.push(`/projects/${props.project.id}`),
    },
    {
      id: "open-popup-new-chat",
      label: "在弹出窗口中创建对话",
      icon: MessageSquarePlus,
      onSelect: () => openProjectChatInPopup(),
    },
    {
      id: "pin",
      label: props.project.pinned ? "取消置顶" : "置顶项目",
      icon: Pin,
      onSelect: () => togglePin(),
    },
    {
      id: "open-explorer",
      label: "在资源管理器中打开",
      icon: FolderOpen,
      disabled: !hasCwd,
      onSelect: () => openInExplorer(),
    },
    {
      id: "open-vscode",
      label: "在 VSCode 中打开",
      icon: Code2,
      disabled: !hasCwd,
      onSelect: () => openWithVSCode(),
    },
    {
      id: "rename",
      label: "重命名项目",
      icon: Pencil,
      onSelect: () => startRename(),
    },
    {
      id: "archive",
      label: "归档所有对话",
      icon: Archive,
      confirmLabel: "确认归档？再点一次",
      onSelect: () => archiveAllConversations(),
    },
    {
      id: "remove",
      label: "移除项目",
      icon: Trash2,
      danger: true,
      confirmLabel: "确认移除？再点一次",
      onSelect: () => deleteProject(),
    },
  ];
}

function onMoreClick(e: MouseEvent) {
  e.stopPropagation();
  const btn = e.currentTarget as HTMLElement | null;
  if (btn) {
    const rect = btn.getBoundingClientRect();
    openContextMenuAt(rect.right, rect.bottom + 2, buildMenu());
  } else {
    openContextMenuAt(e.clientX, e.clientY, buildMenu());
  }
}

function scheduleConversationListReveal() {
  if (disposed || conversationListReady.value || !props.isExpanded) return;
  cancelConversationListRevealPaint?.();
  cancelConversationListRevealPaint = null;
  const seq = ++conversationListRevealSeq;
  const stage = beginPerfStage("sidebar.project-conversations.reveal", {
    detail: props.project.id,
  });
  cancelConversationListRevealPaint = scheduleAfterPaint(() => {
    cancelConversationListRevealPaint = null;
    if (disposed || seq !== conversationListRevealSeq || conversationListReady.value || !props.isExpanded) {
      stage.end("cancelled");
      return;
    }
    conversationListRevealHandle = runWhenIdle(() => {
      conversationListRevealHandle = null;
      if (disposed || seq !== conversationListRevealSeq || conversationListReady.value || !props.isExpanded) {
        stage.end("cancelled");
        return;
      }
      conversationListReady.value = true;
      stage.end("idle");
    });
  });
}

watch(
  () => props.isExpanded,
  (isExpanded) => {
    if (isExpanded) {
      scheduleConversationListReveal();
      return;
    }
    conversationListRevealSeq += 1;
    cancelConversationListRevealPaint?.();
    cancelConversationListRevealPaint = null;
    if (conversationListRevealHandle !== null) {
      cancelIdleRun(conversationListRevealHandle);
      conversationListRevealHandle = null;
    }
  },
);

if (props.isExpanded) {
  scheduleConversationListReveal();
}

onBeforeUnmount(() => {
  disposed = true;
  cancelConversationListRevealPaint?.();
  cancelConversationListRevealPaint = null;
  if (conversationListRevealHandle !== null) {
    cancelIdleRun(conversationListRevealHandle);
    conversationListRevealHandle = null;
  }
  conversationListRevealSeq += 1;
});
</script>

<template>
  <div class="sb-tree__group" :data-agent-id="`sidebar.project.${project.id}`">
    <div class="sb-tree__row sb-tree__row--project"
      :class="[
        {
          'is-editing': editingId === project.id,
          'is-active': isActiveProject(),
        },
        treeRowStateClass('project', project.id, null),
      ]"
      data-tree-kind="project"
      :data-agent-id="`sidebar.project.${project.id}.row`"
      :data-project-id="project.id"
      :data-pinned="project.pinned ? 'true' : 'false'"
      :aria-expanded="isExpanded"
      v-context-menu="() => buildMenu()"
      @click="emit('toggle', project.id)"
      @auxclick="onProjectAuxClick"
    >
      <span
        class="sb-tree__project-icon"
        aria-hidden="true"
      >
        <FolderOpen v-if="isExpanded" :size="14" aria-hidden="true" />
        <Folder v-else :size="14" aria-hidden="true" />
      </span>
      <input
        v-if="editingId === project.id"
        :ref="bindEditingInput"
        v-model="editingValue"
        type="text"
        class="sb-tree__rename-input"
        :data-agent-id="`sidebar.project.${project.id}.rename-input`"
        spellcheck="false"
        @click.stop
        @pointerdown.stop
        @keydown="onEditingKeydown"
        @blur="commitRename"
      />
      <span
        v-else
        class="sb-tree__link"
      >
        <span class="sb-tree__name">{{ project.name }}</span>
        <Pin v-if="project.pinned" :size="12" class="sb-tree__pin-icon" aria-hidden="true" />
      </span>
      <div v-if="editingId !== project.id" class="sb-tree__hover-tools" @click.stop>
        <button type="button" class="sb-icon-btn" :data-agent-id="`sidebar.project.${project.id}.more`" title="更多" aria-label="更多" @click="onMoreClick">
          <MoreHorizontal :size="13" aria-hidden="true" />
        </button>
        <button type="button" class="sb-icon-btn" :data-agent-id="`sidebar.project.${project.id}.new-chat`" title="新对话" aria-label="新对话"
          @click="emit('newChat', project.id)">
          <Plus :size="13" aria-hidden="true" />
        </button>
      </div>
    </div>

    <div class="sb-collapse" :class="{ 'is-open': isExpanded }" :aria-hidden="!isExpanded">
      <div class="sb-collapse__inner">
        <ProjectTreeConversationList
          v-if="conversationListReady && isExpanded"
          :project-id="project.id"
          :activity-for-task="activityForTask"
          :archive="archiveProjectTask"
          :open="openTask"
          :tree-row-state-class="treeRowStateClass"
          @archived="onProjectTaskArchived"
          @error="emit('error', $event)"
        />
        <p v-else-if="isExpanded" class="sb-tree__empty">
          准备对话列表…
        </p>
      </div>
    </div>
  </div>
</template>
