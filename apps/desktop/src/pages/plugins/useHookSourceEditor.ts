import { computed, ref } from "vue";
import {
  updateHookSource,
  type HookDocumentView,
  type HookHandlerUpdateInput,
  type HookSourceSummary,
} from "../../services/plugins";

export interface HookHandlerDraftRow {
  id: string;
  event: string;
  matcher: string;
  type: string;
  command: string;
  commandWindows: string;
  timeoutSeconds: string;
  statusMessage: string;
}

function createDraftRow(): HookHandlerDraftRow {
  return {
    id: "",
    event: "",
    matcher: "",
    type: "command",
    command: "",
    commandWindows: "",
    timeoutSeconds: "",
    statusMessage: "",
  };
}

function draftFromHandler(handler: HookDocumentView["handlers"][number]): HookHandlerDraftRow {
  return {
    id: handler.id,
    event: handler.event,
    matcher: handler.matcher ?? "",
    type: handler.type,
    command: handler.command ?? "",
    commandWindows: handler.commandWindows ?? "",
    timeoutSeconds: handler.timeoutSeconds == null ? "" : String(handler.timeoutSeconds),
    statusMessage: handler.statusMessage ?? "",
  };
}

function normalizeOptionalText(value: string) {
  const trimmed = value.trim();
  return trimmed ? trimmed : null;
}

export function useHookSourceEditor({
  refresh,
  onSaved,
  isDisposed,
}: {
  refresh: () => Promise<void>;
  onSaved?: (document: HookDocumentView) => void;
  isDisposed?: () => boolean;
}) {
  const showHookEditor = ref(false);
  const editingSource = ref<HookSourceSummary | null>(null);
  const hookHandlerRows = ref<HookHandlerDraftRow[]>([]);
  const hookSaving = ref(false);
  const hookError = ref<string | null>(null);
  const disposed = () => isDisposed?.() === true;

  const hookEditorTitle = computed(() =>
    editingSource.value ? `编辑 Hooks 来源：${editingSource.value.name}` : "编辑 Hooks 来源",
  );

  function openHookEditor(document: HookDocumentView) {
    if (disposed()) return;
    editingSource.value = document.source;
    hookHandlerRows.value = document.handlers.length
      ? document.handlers.map((handler) => draftFromHandler(handler))
      : [createDraftRow()];
    hookError.value = null;
    showHookEditor.value = true;
  }

  function addHookHandler() {
    if (disposed()) return;
    hookHandlerRows.value.push(createDraftRow());
  }

  function removeHookHandler(index: number) {
    if (disposed()) return;
    hookHandlerRows.value.splice(index, 1);
    if (hookHandlerRows.value.length === 0) {
      hookHandlerRows.value.push(createDraftRow());
    }
  }

  function buildHookHandlersInput(): HookHandlerUpdateInput[] {
    return hookHandlerRows.value.flatMap((row) => {
      const hasContent = [
        row.event,
        row.matcher,
        row.command,
        row.commandWindows,
        row.timeoutSeconds,
        row.statusMessage,
      ].some((value) => value.trim());
      if (!hasContent && !row.type.trim()) return [];
      if (!row.event.trim()) {
        throw new Error("每条 hook 都需要 event");
      }
      if (!row.type.trim()) {
        throw new Error("每条 hook 都需要 type");
      }
      const timeoutText = row.timeoutSeconds.trim();
      let timeoutSeconds: number | null = null;
      if (timeoutText) {
        const parsed = Number.parseInt(timeoutText, 10);
        if (!Number.isFinite(parsed) || parsed < 1 || parsed > 300) {
          throw new Error("Timeout 必须是 1 到 300 秒之间的整数");
        }
        timeoutSeconds = parsed;
      }
      return [{
        id: row.id || undefined,
        event: row.event.trim(),
        matcher: normalizeOptionalText(row.matcher),
        type: row.type.trim(),
        command: normalizeOptionalText(row.command),
        commandWindows: normalizeOptionalText(row.commandWindows),
        timeoutSeconds,
        statusMessage: normalizeOptionalText(row.statusMessage),
      }];
    });
  }

  async function saveHookSource() {
    if (disposed() || hookSaving.value || !editingSource.value) return;
    hookError.value = null;
    hookSaving.value = true;
    try {
      const saved = await updateHookSource(editingSource.value, {
        expectedRevision: editingSource.value.revision,
        handlers: buildHookHandlersInput(),
      });
      if (disposed()) return;
      onSaved?.(saved);
      showHookEditor.value = false;
      await refresh();
    } catch (err) {
      if (!disposed()) hookError.value = err instanceof Error ? err.message : String(err);
    } finally {
      if (!disposed()) hookSaving.value = false;
    }
  }

  return {
    showHookEditor,
    editingSource,
    hookHandlerRows,
    hookSaving,
    hookError,
    hookEditorTitle,
    openHookEditor,
    addHookHandler,
    removeHookHandler,
    saveHookSource,
  };
}

