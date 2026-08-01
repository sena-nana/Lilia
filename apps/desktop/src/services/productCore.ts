import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  ProductCommandMeta,
  ProductCommandResult,
  ProductEntity,
  ProductEntityKind,
  ProductEvent,
  ProductPage,
} from "@lilia/contracts";
import {
  PRODUCT_CREATE_ENTITY_COMMAND,
  PRODUCT_EVENT_NAME,
  PRODUCT_GET_ENTITY_COMMAND,
  PRODUCT_LIST_ENTITIES_COMMAND,
  PRODUCT_LIST_EVENTS_COMMAND,
  PRODUCT_UPDATE_ENTITY_COMMAND,
} from "@lilia/contracts/productCoreContract.mjs";
import { invoke } from "../tauri/runtime";

let fallbackNonce = 0;

export function newProductId(prefix: string): string {
  const uuid = globalThis.crypto?.randomUUID?.();
  if (uuid) return `${prefix}-${uuid}`;
  fallbackNonce += 1;
  return `${prefix}-${Date.now().toString(36)}-${fallbackNonce.toString(36)}`;
}

export function newProductCommandMeta(
  scope: string,
  expectedRevision: number | null = null,
): ProductCommandMeta {
  const nonce = newProductId(scope);
  return {
    commandId: nonce,
    idempotencyKey: nonce,
    expectedRevision,
  };
}

export function createProductEntity(
  meta: ProductCommandMeta,
  entity: ProductEntity,
  action = "created",
): Promise<ProductCommandResult<ProductEntity>> {
  return invoke(PRODUCT_CREATE_ENTITY_COMMAND, { meta, entity, action });
}

export function updateProductEntity(
  meta: ProductCommandMeta,
  entity: ProductEntity,
  action = "updated",
): Promise<ProductCommandResult<ProductEntity>> {
  return invoke(PRODUCT_UPDATE_ENTITY_COMMAND, { meta, entity, action });
}

export function getProductEntity(
  kind: ProductEntityKind,
  id: string,
): Promise<ProductEntity | null> {
  return invoke(PRODUCT_GET_ENTITY_COMMAND, { kind, id });
}

export function listProductEntities(
  kind: ProductEntityKind,
): Promise<ProductEntity[]> {
  return invoke(PRODUCT_LIST_ENTITIES_COMMAND, { kind });
}

export function listProductEvents(
  after: number | null,
  limit = 100,
): Promise<ProductPage<ProductEvent>> {
  return invoke(PRODUCT_LIST_EVENTS_COMMAND, {
    request: { after, limit },
  });
}

export function onProductEvent(
  handler: (event: ProductEvent) => void,
): Promise<UnlistenFn> {
  return listen<ProductEvent>(PRODUCT_EVENT_NAME, (event) => {
    handler(event.payload);
  });
}
