<script setup lang="ts">
import Plus from "@lucide/vue/dist/esm/icons/plus.mjs";
import Server from "@lucide/vue/dist/esm/icons/server.mjs";
import Trash2 from "@lucide/vue/dist/esm/icons/trash-2.mjs";
import type { EnvDraftRow, EditableMcpServer } from "./useMcpServerEditor";

defineProps<{
  open: boolean;
  name: string;
  command: string;
  argsText: string;
  envRows: EnvDraftRow[];
  editingMcp: EditableMcpServer | null;
  title: string;
  serverLabel: string;
  saving: boolean;
  error: string | null;
  configPath: string | null;
}>();

const emit = defineEmits<{
  "update:open": [value: boolean];
  "update:name": [value: string];
  "update:command": [value: string];
  "update:argsText": [value: string];
  "add-env-row": [];
  "remove-env-row": [index: number];
  confirm: [];
}>();
</script>

<template>
  <Teleport to="body">
    <Transition name="search-palette">
      <div
        v-if="open"
        class="search-palette"
        role="dialog" aria-modal="true" :aria-label="serverLabel"
        data-agent-id="plugins.mcp-editor"
        @click.self="emit('update:open', false)"
      >
        <div class="search-palette__card dialog__card">
          <div class="dialog__header">
            <Server :size="14" aria-hidden="true" />
            <span>{{ title }}</span>
          </div>
          <div class="dialog__body">
            <label>
              <span>名称</span>
              <input
                :value="name" type="text"
                class="ui-input"
                :disabled="!!editingMcp"
                data-agent-id="plugins.mcp-editor.name"
                placeholder="weather-mcp"
                @input="emit('update:name', ($event.target as HTMLInputElement).value)"
              />
            </label>
            <label>
              <span>Command</span>
              <input
                :value="command" type="text"
                class="ui-input"
                data-agent-id="plugins.mcp-editor.command"
                placeholder="node"
                @input="emit('update:command', ($event.target as HTMLInputElement).value)"
              />
            </label>
            <label>
              <span>Args</span>
              <textarea
                :value="argsText"
                class="ui-input"
                data-agent-id="plugins.mcp-editor.args"
                rows="4"
                placeholder="每行一个参数"
                @input="emit('update:argsText', ($event.target as HTMLTextAreaElement).value)"
              />
            </label>
            <div class="plugins-env-editor">
              <div class="plugins-env-editor__head">
                <span>Env</span>
                <button type="button" class="ui-button ui-button--ghost" data-agent-id="plugins.mcp-editor.env.add" @click="emit('add-env-row')">
                  <Plus :size="12" aria-hidden="true" /> 添加
                </button>
              </div>
              <div
                v-for="(row, index) in envRows"
                :key="index"
                class="plugins-env-editor__row"
              >
                <input
                  v-model="row.key"
                  type="text"
                  class="ui-input"
                  :data-agent-id="`plugins.mcp-editor.env.${index}.key`"
                  placeholder="KEY"
                />
                <input
                  v-model="row.value"
                  type="password"
                  class="ui-input"
                  :data-agent-id="`plugins.mcp-editor.env.${index}.value`"
                  :placeholder="editingMcp?.envKeys.includes(row.key) ? '留空保留现有值' : 'value'"
                />
                <button
                  type="button"
                  class="ui-button ui-button--ghost"
                  aria-label="删除 Env"
                  :data-agent-id="`plugins.mcp-editor.env.${index}.remove`"
                  @click="emit('remove-env-row', index)"
                >
                  <Trash2 :size="12" aria-hidden="true" />
                </button>
              </div>
            </div>
            <p v-if="error" class="plugins-create__error">{{ error }}</p>
            <p class="plugins-create__hint">
              配置保存到 <code>{{ configPath }}</code>。
            </p>
          </div>
          <div class="dialog__actions">
            <button type="button" class="ui-button ui-button--ghost" data-agent-id="plugins.mcp-editor.cancel" :disabled="saving" @click="emit('update:open', false)">
              取消
            </button>
            <button type="button" class="ui-button ui-button--primary" data-agent-id="plugins.mcp-editor.save" :disabled="saving" @click="emit('confirm')">
              {{ saving ? "保存中…" : "保存" }}
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

