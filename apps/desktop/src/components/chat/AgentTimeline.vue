<script setup lang="ts">
import { computed, defineAsyncComponent, ref, watch } from "vue";
import type { AgentTimelineEvent, ChatBranchAnchor } from "@lilia/contracts";
import type {
  PendingAgentAction,
  PendingAgentActionResolution,
} from "../../composables/pendingAgentActions";
import ChatBubble from "./ChatBubble.vue";
import type { LiliaBatchApplyInput } from "@lilia/contracts";
import type { ChatImageViewerSource } from "./imageViewer";
import {
  processGroupEntries,
  type TimelineEntry,
  type TimelineGroupEntry,
} from "./timelineEntries";
import {
  isTimelineExpanded,
  isTimelineMessageEvent,
  isTimelineProcessAnchor,
  pruneTimelineExpandedIds,
  toggleTimelineExpandedId,
} from "./timelineDisplay";
import {
  hasTimelinePendingActionState,
  timelinePendingActionState,
} from "./timelinePendingActions";
import { useAgentTimelineEntries } from "./useAgentTimelineEntries";
import { useTimelineRailMask } from "./useTimelineRailMask";
import { measurePerfAsync } from "@lilia/ui/diagnostics";
import { createLazyLoadState } from "@lilia/ui/utils/lazyLoadState";

const timelineEntryItemLoad = createLazyLoadState(() =>
  measurePerfAsync(
    "timeline.entry-item.load",
    async () => (await import("./TimelineEntryItem.vue")).default,
  )
);

const TimelineEntryItem = defineAsyncComponent({
  suspensible: false,
  loader: () => timelineEntryItemLoad.load(),
});

const TIMELINE_RENDER_WINDOW_SIZE = 160;
const TIMELINE_RENDER_WINDOW_STEP = 160;

const props = defineProps<{
  events: AgentTimelineEvent[];
  isThinking?: boolean;
  projectCwd?: string | null;
  activePlanApprovalTurnId?: string | null;
  pendingActions?: PendingAgentAction[];
  showExpiredPendingActions?: boolean;
  canRetryEvent?: (event: AgentTimelineEvent) => boolean;
  canStartLiliaBatchApply?: boolean;
  canStartSessionFork?: boolean;
}>();

const emit = defineEmits<{
  eventToggled: [payload: { event: AgentTimelineEvent; expanded: boolean }];
  resolvePendingAction: [resolution: PendingAgentActionResolution];
  "retry-event": [event: AgentTimelineEvent];
  "open-image": [image: ChatImageViewerSource];
  "start-lilia-batch-apply": [input: LiliaBatchApplyInput];
  "start-session-fork": [anchor: ChatBranchAnchor];
}>();

const toggledIds = ref<Set<string>>(new Set());
const expandedGroupIds = ref<Set<string>>(new Set());
const expandedProcessGroupIds = ref<Set<string>>(new Set());
const renderedEntryCount = ref(TIMELINE_RENDER_WINDOW_SIZE);

const {
  displayContext,
  isTimelineUserMessage,
  orderedEntries,
  previewText,
  processGroupLabel,
  processGroupRunning,
  showThinkingIndicator,
  thinkingIndicatorLabel,
  timelineGroupEntryIds,
  turnState,
  userMessage,
  visibleEvents,
} = useAgentTimelineEntries({
  activePlanApprovalTurnId: computed(() => props.activePlanApprovalTurnId),
  events: computed(() => props.events),
  isThinking: computed(() => props.isThinking),
  projectCwd: computed(() => props.projectCwd),
});

const renderedEntries = computed(() => {
  const entries = orderedEntries.value;
  if (entries.length <= renderedEntryCount.value) return entries;
  return entries.slice(entries.length - renderedEntryCount.value);
});

const hiddenOlderEntryCount = computed(() =>
  Math.max(0, orderedEntries.value.length - renderedEntries.value.length),
);

const { railLineStyle, timelineRef } = useTimelineRailMask([
  renderedEntries,
  showThinkingIndicator,
  toggledIds,
  expandedGroupIds,
  expandedProcessGroupIds,
]);
void timelineRef;

const processAnchorEventIds = computed(() =>
  renderedEntries.value.flatMap((entry) => processAnchorIdsForEntry(entry)),
);

watch(
  () => timelinePruneKey(visibleEvents.value),
  () => {
    toggledIds.value = pruneTimelineExpandedIds(toggledIds.value, visibleEvents.value);
  },
);

watch(
  renderedEntries,
  (entries) => {
    const valid = timelineGroupEntryIds(entries);
    expandedGroupIds.value = new Set(
      [...expandedGroupIds.value].filter((id) => valid.has(id)),
    );
  },
);

watch(
  () => processAnchorEventIds.value,
  () => {
    const valid = new Set(processAnchorEventIds.value);
    expandedProcessGroupIds.value = new Set(
      [...expandedProcessGroupIds.value].filter((id) => valid.has(id)),
    );
  },
);

watch(
  () => turnState.value.interrupted,
  () => {
    const turnIds = turnState.value.interrupted;
    if (turnIds.size === 0) return;
    const interruptedEventIds = new Set(
      visibleEvents.value
        .filter((event) => event.turnId && turnIds.has(event.turnId))
        .map((event) => event.id),
    );
    toggledIds.value = new Set(
      [...toggledIds.value].filter((id) => !interruptedEventIds.has(id)),
    );
    expandedProcessGroupIds.value = new Set(
      [...expandedProcessGroupIds.value].filter((id) => !interruptedEventIds.has(id)),
    );
  },
);

watch(
  () => orderedEntries.value.length,
  (count, previousCount) => {
    if (count <= TIMELINE_RENDER_WINDOW_SIZE) {
      renderedEntryCount.value = TIMELINE_RENDER_WINDOW_SIZE;
      return;
    }
    if (previousCount !== undefined && renderedEntryCount.value >= previousCount) {
      renderedEntryCount.value = count;
    }
  },
);

function expanded(event: AgentTimelineEvent): boolean {
  if (hasTimelinePendingActionState(pendingState(event))) return true;
  return isTimelineExpanded(event, toggledIds.value, displayContext.value);
}

function toggleEvent(event: AgentTimelineEvent) {
  if (isTimelineMessageEvent(event)) return;
  const nextExpanded = !expanded(event);
  toggledIds.value = toggleTimelineExpandedId(toggledIds.value, event.id);
  emit("eventToggled", { event, expanded: nextExpanded });
}

function processGroupExpanded(event: AgentTimelineEvent): boolean {
  return expandedProcessGroupIds.value.has(event.id);
}

function toggleProcessGroup(event: AgentTimelineEvent) {
  const next = new Set(expandedProcessGroupIds.value);
  if (next.has(event.id)) next.delete(event.id);
  else next.add(event.id);
  expandedProcessGroupIds.value = next;
}

function groupExpanded(entry: TimelineGroupEntry): boolean {
  if (entry.events.some((event) =>
    hasTimelinePendingActionState(pendingState(event))
  )) {
    return true;
  }
  return expandedGroupIds.value.has(entry.id);
}

function toggleGroup(entry: TimelineGroupEntry) {
  const next = new Set(expandedGroupIds.value);
  if (next.has(entry.id)) next.delete(entry.id);
  else next.add(entry.id);
  expandedGroupIds.value = next;
}

function revealOlderEntries() {
  renderedEntryCount.value = Math.min(
    orderedEntries.value.length,
    renderedEntryCount.value + TIMELINE_RENDER_WINDOW_STEP,
  );
}

const pendingActions = computed(() => props.pendingActions ?? []);

function pendingState(event: AgentTimelineEvent) {
  return timelinePendingActionState(
    event,
    pendingActions.value,
    props.showExpiredPendingActions,
  );
}

function timelinePruneKey(events: AgentTimelineEvent[]): string {
  const first = events[0];
  const last = events[events.length - 1];
  return [
    events.length,
    first?.id ?? "",
    last?.id ?? "",
    last?.updatedAt ?? "",
  ].join(":");
}

function processAnchorIdsForEntry(entry: TimelineEntry): string[] {
  if (entry.type === "group") {
    return entry.events.filter(isTimelineProcessAnchor).map((event) => event.id);
  }
  const ids: string[] = [];
  if (isTimelineProcessAnchor(entry.event)) ids.push(entry.event.id);
  for (const event of entry.processEvents ?? []) {
    if (isTimelineProcessAnchor(event)) ids.push(event.id);
  }
  return ids;
}

</script>

<template>
  <section
    v-if="orderedEntries.length || showThinkingIndicator"
    ref="timelineRef"
    class="agent-timeline"
    aria-label="Agent 工作过程"
    data-agent-id="timeline.surface"
    :data-agent-total-count="props.events.length"
    :data-agent-rendered-count="renderedEntries.length"
  >
    <div class="agent-timeline__rail-layer" aria-hidden="true">
      <span class="agent-timeline__rail-line" :style="railLineStyle" />
    </div>
    <ol class="agent-timeline__list">
      <li v-if="hiddenOlderEntryCount > 0" class="agent-timeline__window-row">
    <button
      type="button"
      class="agent-timeline__window-button"
      data-agent-id="timeline.reveal-older"
      @click="revealOlderEntries"
    >
          显示更早 {{ hiddenOlderEntryCount }} 项
        </button>
      </li>
      <template v-for="entry in renderedEntries" :key="entry.id">
        <li
          v-if="entry.type === 'event' && isTimelineUserMessage(entry.event)"
          class="agent-timeline__message-row"
          :data-scroll-anchor-id="entry.event.id"
          :class="[
            `agent-timeline__message-row--${userMessage(entry.event).role}`,
            { 'is-queued': userMessage(entry.event).queued },
          ]"
        >
          <ChatBubble
            :message="userMessage(entry.event)"
            @open-image="emit('open-image', $event)"
          />
        </li>

        <TimelineEntryItem
          v-else
          :entry="entry"
          :expanded="expanded"
          :group-expanded="groupExpanded"
          :process-group-expanded="processGroupExpanded"
          :process-group-label="processGroupLabel"
          :process-group-running="processGroupRunning"
          :process-entries-for="processGroupEntries"
          :preview-text="previewText"
          :project-cwd="projectCwd"
          :pending-state="pendingState"
          :can-retry-event="canRetryEvent"
          :can-start-lilia-batch-apply="canStartLiliaBatchApply"
          :can-start-session-fork="canStartSessionFork"
          @toggle-event="toggleEvent"
          @toggle-group="toggleGroup"
          @toggle-process-group="toggleProcessGroup"
          @resolve-pending-action="emit('resolvePendingAction', $event)"
          @retry-event="emit('retry-event', $event)"
          @open-image="emit('open-image', $event)"
          @start-lilia-batch-apply="emit('start-lilia-batch-apply', $event)"
          @start-session-fork="emit('start-session-fork', $event)"
        />
      </template>
      <li
        v-if="showThinkingIndicator"
        class="agent-timeline__item agent-timeline__item--thinking is-status-running"
        aria-live="polite"
      >
        <article class="agent-timeline__event">
          <div class="agent-timeline__rail" />
          <div class="agent-timeline__body">
            <p class="agent-timeline__thinking-label">{{ thinkingIndicatorLabel }}</p>
          </div>
        </article>
      </li>
    </ol>
  </section>
</template>
