import productCoreContract from "./product-core-contract.json" with { type: "json" };

const manifest = Object.freeze(productCoreContract);

export const PRODUCT_CORE_CONTRACT = manifest;
export const PRODUCT_CREATE_ENTITY_COMMAND = manifest.commands.createEntity;
export const PRODUCT_UPDATE_ENTITY_COMMAND = manifest.commands.updateEntity;
export const PRODUCT_GET_ENTITY_COMMAND = manifest.commands.getEntity;
export const PRODUCT_LIST_ENTITIES_COMMAND = manifest.commands.listEntities;
export const PRODUCT_LIST_EVENTS_COMMAND = manifest.commands.listEvents;
export const PRODUCT_EVENT_NAME = manifest.events.product;
export const PRODUCT_ENTITY_KINDS = Object.freeze([...manifest.entityKinds]);
