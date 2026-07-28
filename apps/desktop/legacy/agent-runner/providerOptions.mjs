import { isRecord, stringOrNull } from "./utils.mjs";

const FALLBACKS = new Set(["diagnostic", "unsupported", "ignore"]);

function normalizeExperimentalProviderOption(option, backend) {
  if (!isRecord(option) || stringOrNull(option.provider) !== backend) return null;
  const fallback = FALLBACKS.has(stringOrNull(option.fallback))
    ? stringOrNull(option.fallback)
    : "diagnostic";
  return {
    fallback,
    capability: stringOrNull(option.capability)?.trim() || "unknown",
    payloadKeys: isRecord(option.payload) ? Object.keys(option.payload) : [],
  };
}

function emitExperimentalProviderDiagnostic(protocol, backend, option, status) {
  protocol.emitTimeline({
    kind: "diagnostic",
    status,
    title: status === "error"
      ? "Unsupported experimental provider option"
      : "Ignored experimental provider option",
    summary: `${backend} adapter does not support experimental capability: ${option.capability}`,
    payload: {
      backend,
      subkind: "experimental_provider_option",
      ...option,
    },
    sourceId: `${backend}:experimental-provider-option:${option.capability}`,
  });
}

export function handleExperimentalProviderOptions(cmd, context, backend) {
  for (const raw of cmd?.runtimeOptions?.experimentalProviderOptions || []) {
    const option = normalizeExperimentalProviderOption(raw, backend);
    if (!option || option.fallback === "ignore") continue;
    if (option.fallback === "unsupported") {
      emitExperimentalProviderDiagnostic(context.protocol, backend, option, "error");
      throw new Error(`${backend} experimental provider capability is unsupported: ${option.capability}`);
    }
    emitExperimentalProviderDiagnostic(context.protocol, backend, option, "info");
  }
}

export function readProviderRuntimeOptions(runtimeOptions, backend) {
  const normalizedRuntimeOptions = isRecord(runtimeOptions) ? runtimeOptions : {};
  const common = isRecord(normalizedRuntimeOptions.common)
    ? normalizedRuntimeOptions.common
    : {};
  const provider = isRecord(normalizedRuntimeOptions.provider)
    ? normalizedRuntimeOptions.provider
    : {};
  const settings = isRecord(provider[backend]) ? provider[backend] : {};
  const ignoredProviderKeys = Object.entries(provider)
    .filter(([providerName]) => providerName !== backend)
    .flatMap(([, value]) => isRecord(value) ? Object.keys(value) : []);
  return {
    runtimeOptions: normalizedRuntimeOptions,
    common,
    provider,
    settings,
    ignoredProviderKeys,
  };
}

export function ensureProviderRuntimeOptions(cmd, backend) {
  if (!isRecord(cmd.runtimeOptions)) cmd.runtimeOptions = {};
  if (!isRecord(cmd.runtimeOptions.provider)) cmd.runtimeOptions.provider = {};
  if (!isRecord(cmd.runtimeOptions.provider[backend])) cmd.runtimeOptions.provider[backend] = {};
  return cmd.runtimeOptions.provider[backend];
}

export function withProviderRuntimeOptions(cmd, backend, settings) {
  const runtimeOptions = isRecord(cmd?.runtimeOptions) ? cmd.runtimeOptions : {};
  const provider = isRecord(runtimeOptions.provider) ? runtimeOptions.provider : {};
  return {
    ...cmd,
    runtimeOptions: {
      ...runtimeOptions,
      provider: {
        ...provider,
        [backend]: settings,
      },
    },
  };
}
