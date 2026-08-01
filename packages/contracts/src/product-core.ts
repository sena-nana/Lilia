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

export type ProductRevision = number;
export type ProductTaskStatus = "draft" | "waiting" | "running" | "blocked" | "done" | "cancelled";
export type ProductTaskPriority = "low" | "normal" | "high" | "urgent";
export type ProductConversationStatus = "active" | "waiting" | "closed";
export type ProductMilestoneStatus = "planned" | "active" | "completed" | "cancelled";
export type ProductWorkflowStatus = "draft" | "published" | "disabled";
export type ProductWorkflowRunStatus =
  | "queued"
  | "running"
  | "waiting"
  | "completed"
  | "cancelled"
  | "failed";

export interface ProductProject {
  id: string;
  name: string;
  workspacePath: string | null;
  pinned: boolean;
  sortOrder: number;
  archive: "active" | "archived";
  gitWorkspace: {
    repository: string | null;
    branch: string | null;
    worktreePath: string | null;
  } | null;
  settings: {
    defaultAgentProfileId: string | null;
    values: Record<string, string>;
  };
  assetIds: string[];
  revision: ProductRevision;
}

export interface ProductTask {
  id: string;
  projectId: string | null;
  title: string;
  description: string | null;
  status: ProductTaskStatus;
  priority: ProductTaskPriority;
  assignmentId: string | null;
  completionCriteria: string[];
  milestoneId: string | null;
  workflowId: string | null;
  agentProfileId: string | null;
  blockedReason: string | null;
  dependsOn: string[];
  parentId: string | null;
  pinned: boolean;
  sortOrder: number;
  archived: boolean;
  tags: string[];
  createdAt: number;
  updatedAt: number;
  revision: ProductRevision;
  legacySource: string | null;
}

export interface ProductConversation {
  id: string;
  projectId: string | null;
  taskId: string | null;
  title: string;
  status: ProductConversationStatus;
  archived: boolean;
  labels: string[];
  bindingIds: string[];
  forkedFrom: string | null;
  migratedFrom: string | null;
  legacySource: string | null;
  timelineCursor: number;
  createdAt: number;
  updatedAt: number;
  revision: ProductRevision;
}

export interface ProductMilestone {
  id: string;
  projectId: string;
  title: string;
  description: string | null;
  status: ProductMilestoneStatus;
  sortOrder: number;
  startDate: string | null;
  dueDate: string | null;
  revision: ProductRevision;
}

export interface ProductSessionBinding {
  bindingId: string;
  taskId: string;
  conversationId: string | null;
  agentSession: string;
  profileId: string | null;
  revision: ProductRevision;
}

export interface ProductWorkflow {
  id: string;
  name: string;
  version: number;
  status: ProductWorkflowStatus;
  definitionRef: string | null;
  revision: ProductRevision;
}

export interface ProductWorkflowRun {
  id: string;
  workflowId: string;
  workflowVersion: number;
  taskId: string | null;
  status: ProductWorkflowRunStatus;
  nodeProjectionRef: string | null;
  agentSession: string | null;
  revision: ProductRevision;
}

export interface ProductAssignment {
  id: string;
  taskId: string;
  role: string;
  assignee: string;
  agentProfileId: string | null;
  status: "proposed" | "accepted" | "active" | "completed" | "released";
  revision: ProductRevision;
}

export interface ProductArtifact {
  id: string;
  taskId: string;
  agentSession: string;
  sourceEventId: string | null;
  artifactRef: string;
  resourceRef: string | null;
  mediaType: string;
  materialization: "referenced" | "materialized" | "missing" | "archived";
  retention: "session" | "task" | "project" | "permanent";
  provenance: string | null;
  revision: ProductRevision;
}

export interface ProductProjectAsset {
  id: string;
  projectId: string;
  kind: "architecture" | "design_principle" | "specification" | "other";
  title: string;
  contentRef: string;
  version: number;
  proposalStatus: "draft" | "proposed" | "applied" | "rejected" | "rolled_back";
  rollbackOf: string | null;
  revision: ProductRevision;
}

export type ProductEntity =
  | { kind: "project"; value: ProductProject }
  | { kind: "task"; value: ProductTask }
  | { kind: "conversation"; value: ProductConversation }
  | { kind: "milestone"; value: ProductMilestone }
  | { kind: "binding"; value: ProductSessionBinding }
  | { kind: "workflow"; value: ProductWorkflow }
  | { kind: "workflow_run"; value: ProductWorkflowRun }
  | { kind: "assignment"; value: ProductAssignment }
  | { kind: "artifact"; value: ProductArtifact }
  | { kind: "project_asset"; value: ProductProjectAsset };

export interface ProductCommandMeta {
  commandId: string;
  idempotencyKey: string;
  expectedRevision: number | null;
}

export interface ProductCommandResult<T> {
  commandId: string;
  eventSequence: number;
  value: T;
  duplicate: boolean;
}

export interface ProductEvent {
  sequence: number;
  commandId: string;
  entity: ProductEntityKind;
  entityId: string;
  action: string;
  revision: number | null;
}

export interface ProductPage<T> {
  items: T[];
  next: number | null;
}
