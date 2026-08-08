/**
 * 跨页面共享连接状态和当前 Agent provider。
 * 用模块级 ref 而不是 Pinia，因为状态就一份、读多写少。
 */

import { computed, ref } from "vue";
import {
  checkEnv,
  getActiveBackend,
  setActiveBackend as persistActiveBackend,
  type EnvStatusReport,
} from "../services/chat";
import type {
  BackendEnvStatus,
  ChatBackendKind,
  RouterMode,
} from "@lilia/contracts";
import {
  CHAT_BACKENDS,
  DEFAULT_CHAT_BACKEND,
} from "@lilia/contracts/chatBackendsContract.mjs";

const report = ref<EnvStatusReport | null>(null);
const activeBackend = ref<ChatBackendKind>(DEFAULT_CHAT_BACKEND);
const probing = ref(false);
let inflight: Promise<void> | null = null;
let backendInflight: Promise<ChatBackendKind> | null = null;
let activeBackendLoaded = false;
const CHAT_BACKEND_SET = new Set<string>(CHAT_BACKENDS);

function normalizeChatBackendKind(
  value: unknown,
  fallback: ChatBackendKind = DEFAULT_CHAT_BACKEND,
): ChatBackendKind {
  return typeof value === "string" && CHAT_BACKEND_SET.has(value)
    ? value as ChatBackendKind
    : fallback;
}

async function probeOnce(forceRefresh = false) {
  if (inflight) return inflight;
  probing.value = true;
  inflight = (async () => {
    try {
      report.value = await checkEnv({ forceRefresh });
    }
    catch (err) { console.error("[connection] checkEnv failed", err); }
    finally {
      probing.value = false;
      inflight = null;
    }
  })();
  return inflight;
}

async function loadActiveBackend(force = false): Promise<ChatBackendKind> {
  if (backendInflight) return backendInflight;
  if (activeBackendLoaded && !force) return activeBackend.value;
  backendInflight = getActiveBackend()
    .then((backend) => {
      activeBackend.value = normalizeChatBackendKind(backend);
      activeBackendLoaded = true;
      return activeBackend.value;
    })
    .catch((err) => {
      console.error("[connection] getActiveBackend failed", err);
      activeBackend.value = DEFAULT_CHAT_BACKEND;
      activeBackendLoaded = true;
      return activeBackend.value;
    })
    .finally(() => {
      backendInflight = null;
    });
  return backendInflight;
}

async function refreshAll(forceRefresh = true) {
  await Promise.all([probeOnce(forceRefresh), loadActiveBackend(true)]);
}

async function setActiveBackend(backend: ChatBackendKind): Promise<ChatBackendKind> {
  const next = normalizeChatBackendKind(backend);
  const previous = activeBackend.value;
  activeBackend.value = next;
  try {
    await persistActiveBackend(next);
    activeBackendLoaded = true;
    return activeBackend.value;
  } catch (err) {
    activeBackend.value = previous;
    throw err;
  }
}

interface UseConnectionStatusOptions {
  probe?: boolean;
  loadBackend?: boolean;
}

export function useConnectionStatus(options: UseConnectionStatusOptions = {}) {
  const shouldProbe = options.probe !== false;
  const shouldLoadBackend = options.loadBackend !== false;

  if (shouldProbe && report.value === null && !inflight) {
    void probeOnce();
  }
  if (shouldLoadBackend && !activeBackendLoaded && !backendInflight) {
    void loadActiveBackend();
  }

  const nodeAvailable = computed(() => report.value?.nodeAvailable ?? false);

  function statusFor(backend: ChatBackendKind): BackendEnvStatus | null {
    return report.value?.backends?.[backend] ?? null;
  }
  function routerFor(backend: ChatBackendKind): RouterMode | null {
    return report.value?.routerModes?.[backend] ?? null;
  }

  return {
    report,
    activeBackend,
    probing,
    refresh: refreshAll,
    setActiveBackend,
    nodeAvailable,
    statusFor,
    routerFor,
  };
}
