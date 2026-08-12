<script setup lang="ts">
import Plus from "@lucide/vue/dist/esm/icons/plus.mjs";
import Trash2 from "@lucide/vue/dist/esm/icons/trash-2.mjs";
import Workflow from "@lucide/vue/dist/esm/icons/workflow.mjs";
import type { HookHandlerDraftRow } from "./useHookSourceEditor";

defineProps<{
  open: boolean;
  title: string;
  sourceName: string;
  sourcePath: string | null;
  handlerRows: HookHandlerDraftRow[];
  saving: boolean;
  error: string | null;
}>();

const emit = defineEmits<{
  "update:open": [value: boolean];
  "add-handler": [];
  "remove-handler": [index: number];
  confirm: [];
}>();
</script>

<template>
  <Teleport to="body">
    <Transition name="search-palette">
      <div
        v-if="open"
        class="search-palette"
        role="dialog"
        aria-modal="true"
        aria-label="Hooks 编辑器"
        data-agent-id="plugins.hook-editor"
        @click.self="emit('update:open', false)"
      >
        <div class="search-palette__card dialog__card plugins-hook-editor">
          <div class="dialog__header">
            <Workflow :size="14" aria-hidden="true" />
            <span>{{ title }}</span>
          </div>
          <div class="dialog__body">
            <p class="plugins-create__hint">
              正在编辑 <strong>{{ sourceName }}</strong>
              <template v-if="sourcePath">
                ，保存到 <code>{{ sourcePath }}</code>
              </template>
              。
            </p>
            <div class="plugins-hook-editor__head">
              <span>Handlers</span>
              <button type="button" class="ui-button ui-button--ghost" data-agent-id="plugins.hook-editor.add-handler" @click="emit('add-handler')">
                <Plus :size="12" aria-hidden="true" /> 添加 Handler
              </button>
            </div>
            <div
              v-for="(row, index) in handlerRows"
              :key="row.id || `${index}:${row.event}:${row.matcher}`"
              class="plugins-hook-editor__card"
            >
              <div class="plugins-hook-editor__card-head">
                <strong>Handler {{ index + 1 }}</strong>
                <button
                  type="button"
                  class="ui-button ui-button--ghost"
                  aria-label="删除 Handler"
                  :data-agent-id="`plugins.hook-editor.handler.${index}.remove`"
                  @click="emit('remove-handler', index)"
                >
                  <Trash2 :size="12" aria-hidden="true" />
                </button>
              </div>
              <div class="plugins-hook-editor__grid">
                <label>
                  <span>Event</span>
                  <input v-model="row.event" type="text" class="ui-input" :data-agent-id="`plugins.hook-editor.handler.${index}.event`" placeholder="UserPromptSubmit 或 Stop" />
                </label>
                <label>
                  <span>Matcher</span>
                  <input v-model="row.matcher" type="text" class="ui-input" :data-agent-id="`plugins.hook-editor.handler.${index}.matcher`" placeholder="可选通配表达式，例如 *发布*" />
                </label>
                <label>
                  <span>Type</span>
                  <input v-model="row.type" type="text" class="ui-input" :data-agent-id="`plugins.hook-editor.handler.${index}.type`" placeholder="command" />
                </label>
                <label>
                  <span>Timeout</span>
                  <input v-model="row.timeoutSeconds" type="text" class="ui-input" :data-agent-id="`plugins.hook-editor.handler.${index}.timeout`" placeholder="30" />
                </label>
                <label>
                  <span>Command</span>
                  <input v-model="row.command" type="text" class="ui-input" :data-agent-id="`plugins.hook-editor.handler.${index}.command`" placeholder="node hook.js" />
                </label>
                <label>
                  <span>Windows Command</span>
                  <input v-model="row.commandWindows" type="text" class="ui-input" :data-agent-id="`plugins.hook-editor.handler.${index}.command-windows`" placeholder="powershell -File hook.ps1" />
                </label>
                <label class="plugins-hook-editor__field-wide">
                  <span>Status Message</span>
                  <input v-model="row.statusMessage" type="text" class="ui-input" :data-agent-id="`plugins.hook-editor.handler.${index}.status-message`" placeholder="Running hook…" />
                </label>
              </div>
            </div>
            <p v-if="error" class="plugins-create__error">{{ error }}</p>
          </div>
          <div class="dialog__actions">
            <button
              type="button"
              class="ui-button ui-button--ghost"
              data-agent-id="plugins.hook-editor.cancel"
              :disabled="saving"
              @click="emit('update:open', false)"
            >
              取消
            </button>
            <button type="button" class="ui-button ui-button--primary" data-agent-id="plugins.hook-editor.save" :disabled="saving" @click="emit('confirm')">
              {{ saving ? "保存中…" : "保存" }}
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

