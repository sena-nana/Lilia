export type ProductEntityKind =
  | "project"
  | "task"
  | "conversation"
  | "milestone"
  | "binding"
  | "workflow"
  | "workflow_run"
  | "assignment"
  | "artifact"
  | "project_asset";

export const PRODUCT_CORE_CONTRACT: Readonly<Record<string, unknown>>;
export const PRODUCT_CREATE_ENTITY_COMMAND: "product_create_entity";
export const PRODUCT_UPDATE_ENTITY_COMMAND: "product_update_entity";
export const PRODUCT_GET_ENTITY_COMMAND: "product_get_entity";
export const PRODUCT_LIST_ENTITIES_COMMAND: "product_list_entities";
export const PRODUCT_LIST_EVENTS_COMMAND: "product_list_events";
export const PRODUCT_EVENT_NAME: "product-event";
export const PRODUCT_ENTITY_KINDS: readonly ProductEntityKind[];
