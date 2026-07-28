import { isRecord, stringOrNull } from "./utils.mjs";

export function readRuntimeExtensions(cmd) {
  return isRecord(cmd?.extensions) ? cmd.extensions : {};
}

export function stringArray(value) {
  return Array.isArray(value)
    ? value.filter((item) => typeof item === "string" && item.trim())
    : [];
}

export function readClaudeRuntimeMcpServers(value, warnings) {
  if (!isRecord(value)) return {};
  const mcpServers = {};
  for (const [name, server] of Object.entries(value)) {
    if (name === "lilia") {
      warnings.push("跳过外部 Claude MCP server：lilia 是 Lilia 内置 server");
      continue;
    }
    if (!isRecord(server) || server.type !== "stdio" || typeof server.command !== "string" || !server.command.trim()) {
      warnings.push(`跳过外部 Claude MCP server：${name}`);
      continue;
    }
    const args = stringArray(server.args);
    const env = Object.fromEntries(
      Object.entries(isRecord(server.env) ? server.env : {})
        .filter(([key, val]) => key.trim() && typeof val === "string" && val),
    );
    mcpServers[name] = {
      type: "stdio",
      command: server.command,
      ...(args.length > 0 ? { args } : {}),
      ...(Object.keys(env).length > 0 ? { env } : {}),
    };
  }
  return mcpServers;
}

export function readClaudeRuntimeExtensions(cmd) {
  const ext = readRuntimeExtensions(cmd).claude;
  const plugins = Array.isArray(ext?.plugins)
    ? ext.plugins
      .filter((plugin) =>
        isRecord(plugin) &&
        plugin.type === "local" &&
        typeof plugin.path === "string" &&
        plugin.path.trim(),
      )
      .map((plugin) => ({ type: "local", path: plugin.path }))
    : [];
  const warnings = stringArray(ext?.warnings);
  const hooks = isRecord(ext?.hooks) ? ext.hooks : null;
  return {
    skills: stringArray(ext?.skills),
    plugins,
    mcpServers: readClaudeRuntimeMcpServers(ext?.mcpServers, warnings),
    hooks,
    warnings,
  };
}

export function readCodexRuntimeExtensions(cmd) {
  const ext = readRuntimeExtensions(cmd).codex;
  const mcpServers = Array.isArray(ext?.mcpServers) ? ext.mcpServers : [];
  const configPath = typeof ext?.configPath === "string" && ext.configPath.trim()
    ? ext.configPath
    : null;
  return { mcpServers, configPath, warnings: stringArray(ext?.warnings) };
}

export function emitRuntimeExtensionWarnings(protocol, backend, warnings) {
  if (!Array.isArray(warnings) || warnings.length === 0) return;
  protocol.emitTimeline({
    kind: "diagnostic",
    status: "info",
    title: `${backend} config requirement`,
    summary: warnings.join("\n"),
    payload: { backend, subkind: "config_requirement", warnings },
    sourceId: `${backend}:extensions:warnings`,
  });
}
