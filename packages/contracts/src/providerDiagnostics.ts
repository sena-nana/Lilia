import {
  chatBackendLabel,
  type ChatBackendKind,
} from "./chat";
import type {
  BackendEnvStatus,
  EnvStatusReport,
  RouterMode,
} from "./provider";
import {
  apiKeyEnvForBackend,
  connectionModeUsesCustomUrl,
  connectionModeUsesDefaultApi,
} from "./provider";

export { DIRECT_DEFAULT_URLS } from "./provider";

export type DiagnosticTone = "warn" | "err" | "probing";

export interface ProviderDiagnostic {
  tone: DiagnosticTone;
  title: string;
  hint: string;
}

export function runtimeDiagnostic(
  _backend: ChatBackendKind,
  report: EnvStatusReport | null,
): ProviderDiagnostic | null {
  if (!report) {
    return {
      tone: "probing",
      title: "检查中",
      hint: "正在读取本机运行时和连接配置。",
    };
  }
  if (!report.nodeAvailable) {
    return {
      tone: "err",
      title: "Node 运行时缺失",
      hint: "未找到 Node.js 18+。本地 Agent runner 需要本机 Node 运行时；安装后重启 Lilia 或重新检测。",
    };
  }
  return null;
}

export function connectionDiagnostic(
  backend: ChatBackendKind,
  status: BackendEnvStatus | null,
  _routerMode: RouterMode,
): ProviderDiagnostic | null {
  if (!status) {
    return {
      tone: "probing",
      title: "检查中",
      hint: "正在读取连接配置。",
    };
  }
  const label = chatBackendLabel(backend);
  if (connectionModeUsesCustomUrl(status.connectionMode)) {
    return status.hasApiKey ? null : {
      tone: "warn",
      title: `${label} 自定义 API 来源`,
      hint: `将通过 ${status.effectiveUrl ?? "-"} 发送请求，未设置密钥；仅适用于本地代理或不需要鉴权的兼容来源。`,
    };
  }
  if (connectionModeUsesDefaultApi(status.connectionMode) && status.hasApiKey) {
    return null;
  }
  if (connectionModeUsesDefaultApi(status.connectionMode)) {
    return {
      tone: "warn",
      title: `${label} API 缺少 API key`,
      hint: `当前选择 API，但未保存 ${apiKeyEnvForBackend(backend)} 或设置页密钥。请填写 API key 后保存。`,
    };
  }
  return {
    tone: "warn",
    title: `${label} API 未配置`,
    hint: "当前选择 API。请填写 API key；Base URL 留空时使用默认 OpenAI 兼容 API，也可以填写本地代理或兼容 API 地址。",
  };
}
