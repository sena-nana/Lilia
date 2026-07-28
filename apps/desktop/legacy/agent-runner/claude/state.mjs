export function rememberClaudeTool(ctx, sourceId, patch) {
  if (!ctx?.activeTools || !sourceId) return null;
  const current = ctx.activeTools.get(sourceId) || {};
  const payload = {
    ...(current.payload && typeof current.payload === "object" ? current.payload : {}),
    ...(patch.payload && typeof patch.payload === "object" ? patch.payload : {}),
  };
  const next = {
    ...current,
    ...patch,
    payload,
  };
  ctx.activeTools.set(sourceId, next);
  return next;
}
