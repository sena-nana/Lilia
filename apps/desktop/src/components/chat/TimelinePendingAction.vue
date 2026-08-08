<script setup lang="ts">
import { computed, ref, watch } from "vue";
import {
  ARCHITECTURE_INTERACTION_KIND,
  MCP_ELICITATION_INTERACTION_KIND,
  PERMISSION_APPROVAL_INTERACTION_KIND,
  PLAN_APPROVAL_INTERACTION_KIND,
  projectArchitectureChangeText,
  TITLE_UPDATE_ACTION_KIND,
  TOOL_CONSENT_INTERACTION_KIND,
  type AskUserResult,
} from "@lilia/contracts";
import { useAgentInteractionSettings } from "../../composables/useAgentInteractionSettings";
import { useAskUserInteraction } from "../../composables/useAskUserInteraction";
import { useEditableToolCommand } from "../../composables/useEditableToolCommand";
import type {
  PendingAgentAction,
  PendingAgentActionResolution,
} from "../../composables/pendingAgentActions";
import {
  pendingAgentActionAutoDecisionLabel,
  isPendingAskUserAgentAction,
  pendingAgentActionAutoDecisionKey,
  pendingAgentActionAutoResolution,
  pendingAgentActionKey,
  pendingAgentActionResolutionSubmittingTarget,
  pendingAgentActionResolution,
} from "../../composables/pendingAgentActions";
import { useToolConsentPresentation } from "../../composables/useToolConsentPresentation";
import type { ToolConsentDecision } from "../../services/chat";
import {
  recommendedAskUserResult,
  useFreeImplementationCountdown,
} from "./freeImplementationMode";
import {
  timelineMcpCanSubmit,
  timelineMcpContentFromForm,
  timelineMcpFieldInputType,
  timelineMcpFieldsForSchema,
  timelineMcpInitialValues,
  timelineMcpMultiSelected,
  timelineMcpToggleMultiValue,
} from "./timelineMcpForm";
import AskUserInlinePrompt from "./AskUserInlinePrompt.vue";
import TimelineArchitectureAction from "./TimelineArchitectureAction.vue";
import TimelineMcpAction from "./TimelineMcpAction.vue";
import TimelinePermissionAction from "./TimelinePermissionAction.vue";
import TimelineTitleUpdateAction from "./TimelineTitleUpdateAction.vue";
import ToolConsentInlinePrompt from "./ToolConsentInlinePrompt.vue";

const props = defineProps<{
  action: PendingAgentAction;
}>();

const emit = defineEmits<{
  resolve: [resolution: PendingAgentActionResolution];
}>();

const freeformText = ref("");
const toolExpanded = ref(false);
const toolMessage = ref("");
const toolSubmitting = ref<ToolConsentDecision | null>(null);
const codexSubmitting = ref(false);
const mcpValues = ref<Record<string, unknown>>({});
const mcpJsonText = ref("{}");

const agentInteractionSettings = useAgentInteractionSettings();
const freeImplementation = computed(() => agentInteractionSettings.permissionMode.value === "free");

const actionKey = computed(() => pendingAgentActionKey(props.action));
const activeAsk = computed(() =>
  isPendingAskUserAgentAction(props.action)
    ? props.action.ask
    : null,
);
const {
  askIndex,
  askTotal,
  askQuestion,
  askDismissable,
  askIsLast,
  canGoPrev,
  askTitle,
  askOptionsWithId,
  askHasPreview,
  askFocusedOption,
  askOtherSelected,
  canAskSubmit,
  activeOptionId,
  singlePick,
  multiPicks,
  focusOption,
  highlightOption,
  clearOptionHighlight,
  selectSingleOption: selectSingleOptionBase,
  toggleMulti: toggleMultiBase,
  submitAsk: submitAskBase,
  submitAskFreeform: submitAskFreeformBase,
  confirmAskNo: confirmAskNoBase,
  skipAsk: skipAskBase,
  backAsk: backAskBase,
  cancelAsk: cancelAskBase,
} = useAskUserInteraction(activeAsk, freeformText, resolveAsk);

const toolRequest = computed(() =>
  props.action.kind === TOOL_CONSENT_INTERACTION_KIND ? props.action.request : null,
);
const titleUpdateAction = computed(() =>
  props.action.kind === TITLE_UPDATE_ACTION_KIND ? props.action : null,
);
const { toolDanger, toolIcon, toolHeadline, toolInputJson, toolSubtitle } =
  useToolConsentPresentation(toolRequest);
const {
  commandDraft: toolCommandDraftBase,
  isEditingCommand: isEditingToolCommand,
  hasEditableCommand,
  commandIsEmpty: toolCommandIsEmpty,
  updatedCommandInput,
  beginCommandEdit: beginCommandEditBase,
  cancelCommandEdit: cancelCommandEditBase,
} = useEditableToolCommand(toolRequest);
const toolCommandDraft = computed({
  get: () => toolCommandDraftBase.value,
  set: (value: string) => {
    if (value !== toolCommandDraftBase.value) cancelAutoDecision();
    toolCommandDraftBase.value = value;
  },
});
const freeformTextModel = computed({
  get: () => freeformText.value,
  set: (value: string) => {
    if (value !== freeformText.value) cancelAutoDecision();
    freeformText.value = value;
  },
});
const toolMessageModel = computed({
  get: () => toolMessage.value,
  set: (value: string) => {
    if (value !== toolMessage.value) cancelAutoDecision();
    toolMessage.value = value;
  },
});
const mcpJsonTextModel = computed({
  get: () => mcpJsonText.value,
  set: (value: string) => {
    if (value !== mcpJsonText.value) cancelAutoDecision();
    mcpJsonText.value = value;
  },
});
const hasFreeformText = computed(() => freeformText.value.trim().length > 0);
const hasToolMessage = computed(() => toolMessage.value.trim().length > 0);
const mcpAction = computed(() =>
  props.action.kind === MCP_ELICITATION_INTERACTION_KIND ? props.action : null,
);
const permissionAction = computed(() =>
  props.action.kind === PERMISSION_APPROVAL_INTERACTION_KIND ? props.action : null,
);
const architectureAction = computed(() =>
  props.action.kind === ARCHITECTURE_INTERACTION_KIND ? props.action : null,
);
const architectureChangeRows = computed(() => {
  const changes = architectureAction.value?.payload.changes ?? [];
  return changes.slice(0, 5).map((change, index) => ({
    key: `${index}:${change.type}`,
    text: projectArchitectureChangeText(change),
  }));
});
const architectureExtraCount = computed(() =>
  Math.max(0, (architectureAction.value?.payload.changes.length ?? 0) - architectureChangeRows.value.length),
);
const mcpFields = computed(() => {
  return timelineMcpFieldsForSchema(mcpAction.value?.payload.requestedSchema);
});
const canSubmitMcp = computed(() => {
  return timelineMcpCanSubmit({
    fields: mcpFields.value,
    jsonText: mcpJsonText.value,
    mode: mcpAction.value?.payload.mode,
    values: mcpValues.value,
  });
});
const permissionJson = computed(() =>
  JSON.stringify(permissionAction.value?.payload.requestedAccess ?? {}, null, 2),
);
const permissionCwd = computed(() =>
  permissionAction.value?.payload.providerContext?.codex?.cwd ?? "",
);

function mcpFieldInputType(type: string): string {
  return timelineMcpFieldInputType(type);
}

function mcpMultiSelected(key: string, value: string): boolean {
  return timelineMcpMultiSelected(mcpValues.value, key, value);
}

function toggleMcpMultiValue(key: string, value: string) {
  cancelAutoDecision();
  mcpValues.value = timelineMcpToggleMultiValue(mcpValues.value, key, value);
}

function mcpContentFromForm(): Record<string, unknown> {
  return timelineMcpContentFromForm({
    fields: mcpFields.value,
    jsonText: mcpJsonText.value,
    values: mcpValues.value,
  });
}

function emitResolution(
  resolution: PendingAgentActionResolution | null,
): boolean {
  if (!resolution) return false;
  emit("resolve", resolution);
  return true;
}

function applySubmittingStateForResolution(resolution: PendingAgentActionResolution) {
  const target = pendingAgentActionResolutionSubmittingTarget(resolution);
  if (target === "tool" && resolution.kind === TOOL_CONSENT_INTERACTION_KIND) {
    toolSubmitting.value = resolution.decision;
  } else if (target === "native" || target === "codex") {
    codexSubmitting.value = true;
  }
}

function resolveAsk(result: AskUserResult) {
  emitResolution(pendingAgentActionResolution(props.action, { askResult: result }));
}

function decideTool(decision: ToolConsentDecision, source: "manual" | "auto" = "manual") {
  if (source === "manual") cancelAutoDecision();
  if (props.action.kind !== TOOL_CONSENT_INTERACTION_KIND || toolSubmitting.value) return;
  if (decision === "allow" && toolCommandIsEmpty.value) return;
  const updatedInput = decision === "allow" ? updatedCommandInput.value : undefined;
  const resolution = pendingAgentActionResolution(props.action, {
    toolDecision: decision,
    toolMessage: decision === "deny"
      ? toolMessage.value.trim() || "用户拒绝了此次工具调用"
      : undefined,
    toolUpdatedInput: updatedInput,
  });
  if (!resolution) return;
  applySubmittingStateForResolution(resolution);
  emitResolution(resolution);
}

function decideTitleUpdate(decision: "accept" | "decline", source: "manual" | "auto" = "manual") {
  if (source === "manual") cancelAutoDecision();
  if (props.action.kind !== TITLE_UPDATE_ACTION_KIND) return;
  emitResolution(pendingAgentActionResolution(props.action, { titleDecision: decision }));
}

function decideMcp(action: "accept" | "decline" | "cancel", source: "manual" | "auto" = "manual") {
  if (source === "manual") cancelAutoDecision();
  if (props.action.kind !== MCP_ELICITATION_INTERACTION_KIND || codexSubmitting.value) return;
  if (action === "accept" && !canSubmitMcp.value) return;
  const resolution = pendingAgentActionResolution(props.action, {
    mcpDecision: action,
    mcpContent: action === "accept" ? mcpContentFromForm() : undefined,
  });
  if (!resolution) return;
  applySubmittingStateForResolution(resolution);
  emitResolution(resolution);
}

function decidePermission(decision: "allow" | "deny", source: "manual" | "auto" = "manual") {
  if (source === "manual") cancelAutoDecision();
  if (props.action.kind !== PERMISSION_APPROVAL_INTERACTION_KIND || codexSubmitting.value) return;
  const resolution = pendingAgentActionResolution(props.action, { permissionDecision: decision });
  if (!resolution) return;
  applySubmittingStateForResolution(resolution);
  emitResolution(resolution);
}

function decideArchitecture(decision: "allow" | "deny", source: "manual" | "auto" = "manual") {
  if (source === "manual") cancelAutoDecision();
  if (props.action.kind !== ARCHITECTURE_INTERACTION_KIND || codexSubmitting.value) return;
  const resolution = pendingAgentActionResolution(props.action, { architectureDecision: decision });
  if (!resolution) return;
  applySubmittingStateForResolution(resolution);
  emitResolution(resolution);
}

function runManualAction(action: () => void) {
  cancelAutoDecision();
  action();
}

function runManualActionWith<T>(action: (value: T) => void, value: T) {
  cancelAutoDecision();
  action(value);
}

function submitAsk() {
  runManualAction(submitAskBase);
}

function submitAskFreeform(value?: string) {
  runManualAction(() => submitAskFreeformBase(value));
}

function confirmAskNo() {
  runManualAction(confirmAskNoBase);
}

function skipAsk() {
  runManualAction(skipAskBase);
}

function backAsk() {
  runManualAction(backAskBase);
}

function cancelAsk() {
  runManualAction(cancelAskBase);
}

function selectSingleOption(id: string) {
  runManualActionWith(selectSingleOptionBase, id);
}

function toggleMulti(id: string) {
  runManualActionWith(toggleMultiBase, id);
}

function beginCommandEdit() {
  runManualAction(beginCommandEditBase);
}

function cancelCommandEdit() {
  runManualAction(cancelCommandEditBase);
}

function updateMcpField(key: string, value: unknown) {
  cancelAutoDecision();
  mcpValues.value[key] = value;
}

function autoDecisionKeyForCurrentState(): string {
  return pendingAgentActionAutoDecisionKey(props.action, autoResolutionState());
}

function autoResolutionState() {
  return {
    askHasRecommendedResult: !!recommendedAskUserResult(activeAsk.value?.spec),
    askQuestionId: askQuestion.value?.id,
    editingToolCommand: isEditingToolCommand.value,
    mcpCanSubmit: canSubmitMcp.value,
    submitting: codexSubmitting.value,
    toolCommandIsEmpty: toolCommandIsEmpty.value,
    toolDanger: toolDanger.value,
    toolSubmitting: !!toolSubmitting.value,
  };
}

const autoDecisionKey = computed(autoDecisionKeyForCurrentState);

function autoDecisionLabelForCurrentState(): string {
  return pendingAgentActionAutoDecisionLabel(props.action);
}

function runAutoDecision() {
  const resolution = pendingAgentActionAutoResolution(props.action, {
    ...autoResolutionState(),
    askResult: activeAsk.value ? recommendedAskUserResult(activeAsk.value.spec) : null,
    mcpContent: props.action.kind === MCP_ELICITATION_INTERACTION_KIND && canSubmitMcp.value
      ? mcpContentFromForm()
      : undefined,
    toolUpdatedInput: props.action.kind === TOOL_CONSENT_INTERACTION_KIND
      ? updatedCommandInput.value
      : undefined,
  });
  if (!resolution) return;
  applySubmittingStateForResolution(resolution);
  emit("resolve", resolution);
}

const {
  text: autoDecisionText,
  cancel: cancelAutoDecision,
  reset: resetAutoDecision,
} = useFreeImplementationCountdown({
  enabled: freeImplementation,
  decisionKey: autoDecisionKey,
  decisionLabel: autoDecisionLabelForCurrentState,
  runDecision: runAutoDecision,
});

watch(actionKey, () => {
  resetAutoDecision();
  toolExpanded.value = false;
  toolMessage.value = "";
  toolSubmitting.value = null;
  codexSubmitting.value = false;
  mcpJsonText.value = "{}";
  mcpValues.value = timelineMcpInitialValues(mcpFields.value);
}, { immediate: true });
</script>

<template>
  <ToolConsentInlinePrompt
    v-if="props.action.kind === TOOL_CONSENT_INTERACTION_KIND && toolRequest"
    root-class="timeline-pending-action composer-inline composer-inline--tool"
    :active-tool-consent="toolRequest"
    :tool-danger="toolDanger"
    :tool-icon="toolIcon"
    :tool-headline="toolHeadline"
    :tool-input-json="toolInputJson"
    :tool-subtitle="toolSubtitle"
    :tool-expanded="toolExpanded"
    :is-editing-tool-command="isEditingToolCommand"
    :has-editable-command="hasEditableCommand"
    :tool-command-draft="toolCommandDraft"
    @update-tool-expanded="toolExpanded = $event"
    @update-tool-command-draft="toolCommandDraft = $event"
    @begin-command-edit="beginCommandEdit"
  >
    <div
      v-if="autoDecisionText"
      class="composer-inline__auto-decision timeline-pending-action__auto-decision"
      role="status"
    >
      {{ autoDecisionText }}
    </div>
    <div class="timeline-pending-action__row">
      <textarea
        v-model="toolMessageModel"
        class="timeline-pending-action__input"
        rows="1"
        data-agent-id="timeline.tool.message"
        placeholder="拒绝理由"
      />
      <div class="composer-inline__actions">
        <button
          type="button"
          class="ui-button ui-button--ghost composer-inline__btn"
          data-agent-id="timeline.tool.secondary"
          :disabled="toolSubmitting !== null || (!isEditingToolCommand && !hasToolMessage)"
          @click="isEditingToolCommand ? cancelCommandEdit() : decideTool('deny')"
        >
          {{ toolSubmitting === "deny" ? "处理中..." : isEditingToolCommand ? "取消" : hasToolMessage ? "修改" : "忽略" }}
        </button>
        <button
          type="button"
          class="ui-button ui-button--primary composer-inline__btn"
          data-agent-id="timeline.tool.allow"
          :disabled="toolSubmitting !== null || toolCommandIsEmpty"
          @click="decideTool('allow')"
        >
          {{ toolSubmitting === "allow" ? "处理中..." : toolDanger ? "同意执行" : "同意" }}
        </button>
      </div>
    </div>
  </ToolConsentInlinePrompt>

  <TimelineTitleUpdateAction
    v-else-if="titleUpdateAction"
    :proposed-title="titleUpdateAction.proposedTitle"
    :auto-decision-text="autoDecisionText"
    @decline="decideTitleUpdate('decline')"
    @accept="decideTitleUpdate('accept')"
  />

  <TimelineMcpAction
    v-else-if="mcpAction"
    :action="mcpAction"
    :fields="mcpFields"
    :values="mcpValues"
    :json-text="mcpJsonText"
    :auto-decision-text="autoDecisionText"
    :can-submit="canSubmitMcp"
    :disabled="codexSubmitting"
    :field-input-type="mcpFieldInputType"
    :multi-selected="mcpMultiSelected"
    @update-field="updateMcpField"
    @toggle-multi-value="toggleMcpMultiValue"
    @update-json-text="mcpJsonTextModel = $event"
    @cancel="decideMcp('cancel')"
    @decline="decideMcp('decline')"
    @accept="decideMcp('accept')"
  />

  <TimelineArchitectureAction
    v-else-if="architectureAction"
    :reason="architectureAction.payload.reason || 'Agent 提议更新项目架构图'"
    :rows="architectureChangeRows"
    :extra-count="architectureExtraCount"
    :auto-decision-text="autoDecisionText"
    :disabled="codexSubmitting"
    @deny="decideArchitecture('deny')"
    @allow="decideArchitecture('allow')"
  />

  <TimelinePermissionAction
    v-else-if="permissionAction"
    :reason="permissionAction.payload.reason || 'Codex 请求额外权限'"
    :cwd="permissionCwd"
    :permission-json="permissionJson"
    :auto-decision-text="autoDecisionText"
    :disabled="codexSubmitting"
    @deny="decidePermission('deny')"
    @allow="decidePermission('allow')"
  />

  <section
    v-else-if="props.action.kind === PLAN_APPROVAL_INTERACTION_KIND"
    class="timeline-pending-action timeline-pending-action--plan"
    role="region"
    :aria-label="askTitle"
    data-agent-id="timeline.plan"
  >
    <textarea
      v-model="freeformTextModel"
      class="timeline-pending-action__input"
      rows="1"
      data-agent-id="timeline.plan.freeform"
      placeholder="修改要求"
    />
    <div
      v-if="autoDecisionText"
      class="composer-inline__auto-decision timeline-pending-action__auto-decision"
      role="status"
    >
      {{ autoDecisionText }}
    </div>
    <div class="composer-inline__actions">
      <button
        type="button"
        class="ui-button ui-button--ghost composer-inline__btn"
        data-agent-id="timeline.plan.modify"
        :disabled="!hasFreeformText"
        @click="submitAskFreeform()"
      >
        {{ hasFreeformText ? "修改" : "忽略" }}
      </button>
      <button type="button" class="ui-button ui-button--primary composer-inline__btn" data-agent-id="timeline.plan.accept" @click="submitAsk">
        同意
      </button>
    </div>
  </section>

  <AskUserInlinePrompt
    v-else-if="activeAsk && askQuestion"
    v-model:freeform-text="freeformTextModel"
    root-class="timeline-pending-action composer-inline composer-inline--ask"
    input-class="timeline-pending-action__input composer-inline__other-input"
    :active-ask="activeAsk"
    :ask-question="askQuestion"
    :ask-title="askTitle"
    :ask-index="askIndex"
    :ask-total="askTotal"
    :ask-dismissable="askDismissable"
    :ask-is-last="askIsLast"
    :ask-options-with-id="askOptionsWithId"
    :ask-has-preview="askHasPreview"
    :ask-focused-option="askFocusedOption"
    :active-option-id="activeOptionId"
    :single-pick="singlePick"
    :multi-picks="multiPicks"
    :can-go-prev="canGoPrev"
    :can-ask-submit="canAskSubmit"
    :ask-other-selected="askOtherSelected"
    show-choice-footer
    @cancel-ask="cancelAsk"
    @highlight-option="highlightOption"
    @clear-option-highlight="clearOptionHighlight"
    @focus-option="focusOption"
    @select-single-option="selectSingleOption"
    @toggle-multi="toggleMulti"
    @skip-ask="skipAsk"
    @back-ask="backAsk"
    @confirm-ask-no="confirmAskNo"
    @submit-ask="submitAsk"
  >
    <template #auto-decision>
      <div
        v-if="autoDecisionText"
        class="composer-inline__auto-decision timeline-pending-action__auto-decision"
        role="status"
      >
        {{ autoDecisionText }}
      </div>
    </template>
  </AskUserInlinePrompt>
</template>

