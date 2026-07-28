import { computed, shallowRef, watch, type ComputedRef } from "vue";
import {
  DEFAULT_AUTOMATION_AGENT_PROMPT,
  DEFAULT_AUTOMATION_HUMAN_PROMPT,
  DEFAULT_AUTOMATION_LOGIC_KIND,
  DEFAULT_AUTOMATION_LOGIC_PATH,
  DEFAULT_AUTOMATION_TRIGGER_KIND,
  DEFAULT_AUTOMATION_TOOL_ACTION,
  normalizeAutomationLogicKind,
  normalizeAutomationScope,
  normalizePermissionMode,
  type AutomationEdge,
  type AutomationNode,
  type AutomationNodeKind,
  type AutomationRunNodeState,
  type AutomationWorkflow,
} from "@lilia/contracts";
import { NODE_KIND_LABELS } from "./constants";
import type {
  AutomationFlowConnection,
  AutomationFlowEdge,
  AutomationFlowNode,
} from "./types";

export function useAutomationGraph(options: {
  runNodeStates: ComputedRef<AutomationRunNodeState[]>;
}) {
  const selectedNodeId = shallowRef<string | null>(null);
  const nodes = shallowRef<AutomationFlowNode[]>([]);
  const edges = shallowRef<AutomationFlowEdge[]>([]);

  const selectedNode = computed(() =>
    nodes.value.find((node) => node.id === selectedNodeId.value) ?? null,
  );
  const hasTriggerNode = computed(() =>
    nodes.value.some((node) => node.data.node.kind === "trigger"),
  );

  function defaultWorkflowDraft() {
    const trigger = createAutomationNode("trigger", 80, 100);
    const agent = createAutomationNode("agent", 360, 100);
    return {
      nodes: [trigger, agent],
      edges: [{
        id: `${trigger.id}-${agent.id}`,
        source: trigger.id,
        target: agent.id,
      }],
      scope: normalizeAutomationScope(null),
    };
  }

  function createAutomationNode(kind: AutomationNodeKind, x = 160, y = 160): AutomationNode {
    const id = `${kind}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 6)}`;
    const config: Record<string, unknown> = {};
    if (kind === "trigger") config.triggerKind = DEFAULT_AUTOMATION_TRIGGER_KIND;
    if (kind === "agent") {
      // Execution is Native AgentKit (#49). Claude/Codex remain scope/filter labels only.
      config.backend = "native-agentkit";
      config.model = "native-default";
      config.permission = normalizePermissionMode(null);
      config.prompt = DEFAULT_AUTOMATION_AGENT_PROMPT;
    }
    if (kind === "tool") config.action = DEFAULT_AUTOMATION_TOOL_ACTION;
    if (kind === "logic") {
      config.logic = DEFAULT_AUTOMATION_LOGIC_KIND;
      config.path = DEFAULT_AUTOMATION_LOGIC_PATH;
    }
    if (kind === "human") config.prompt = DEFAULT_AUTOMATION_HUMAN_PROMPT;
    return {
      id,
      kind,
      title: NODE_KIND_LABELS[kind],
      position: { x, y },
      config,
    };
  }

  function nodeToFlowNode(node: AutomationNode): AutomationFlowNode {
    return {
      id: node.id,
      type: "automation",
      position: node.position,
      data: {
        node,
        selected: node.id === selectedNodeId.value,
        status: nodeStatus(node.id),
      },
    };
  }

  function edgeToFlowEdge(edge: AutomationEdge): AutomationFlowEdge {
    return {
      id: edge.id,
      source: edge.source,
      target: edge.target,
      sourceHandle: edge.sourceHandle ?? undefined,
      targetHandle: edge.targetHandle ?? undefined,
      animated: false,
    };
  }

  function sourceHandlesForNode(
    node: AutomationNode,
  ): Array<{ id: string; label: string; top: string }> {
    if (node.kind !== "logic") return [{ id: "success", label: "out", top: "50%" }];
    const logic = normalizeAutomationLogicKind(node.config.logic);
    if (logic === "condition") {
      return [
        { id: "true", label: "true", top: "38%" },
        { id: "false", label: "false", top: "66%" },
      ];
    }
    if (logic === "switch") {
      const cases = typeof node.config.cases === "string"
        ? node.config.cases.split(",").map((item) => item.trim()).filter(Boolean).slice(0, 3)
        : [];
      const handles = cases.map((item, index) => ({
        id: normalizeHandle(item),
        label: item,
        top: `${32 + index * 18}%`,
      }));
      return [...handles, { id: "default", label: "else", top: `${32 + handles.length * 18}%` }];
    }
    return [{ id: "success", label: "out", top: "50%" }];
  }

  function flowNodeToAutomation(node: AutomationFlowNode): AutomationNode {
    const existing = node.data?.node as AutomationNode | undefined;
    return {
      ...(existing ?? createAutomationNode("tool")),
      id: node.id,
      position: {
        x: node.position.x,
        y: node.position.y,
      },
    };
  }

  function flowEdgeToAutomation(edge: AutomationFlowEdge): AutomationEdge {
    return {
      id: edge.id,
      source: edge.source,
      target: edge.target,
      sourceHandle: edge.sourceHandle ?? null,
      targetHandle: edge.targetHandle ?? null,
    };
  }

  function applyWorkflow(workflow: AutomationWorkflow | null) {
    if (!workflow) {
      const draft = defaultWorkflowDraft();
      nodes.value = draft.nodes.map(nodeToFlowNode);
      edges.value = draft.edges.map(edgeToFlowEdge);
      selectedNodeId.value = draft.nodes[0]?.id ?? null;
      return draft;
    }
    nodes.value = workflow.draft.nodes.map(nodeToFlowNode);
    edges.value = workflow.draft.edges.map(edgeToFlowEdge);
    selectedNodeId.value = workflow.draft.nodes[0]?.id ?? null;
    return workflow.draft;
  }

  function addNode(kind: AutomationNodeKind) {
    if (kind === "trigger" && hasTriggerNode.value) return;
    const node = createAutomationNode(kind, 180 + nodes.value.length * 24, 160 + nodes.value.length * 18);
    nodes.value = [...nodes.value, nodeToFlowNode(node)];
    selectedNodeId.value = node.id;
  }

  function updateSelectedConfig(key: string, value: unknown) {
    const node = selectedNode.value;
    if (!node) return;
    const nextAutomationNode = {
      ...node.data.node,
      config: {
        ...(node.data.node.config ?? {}),
        [key]: value,
      },
    };
    nodes.value = nodes.value.map((item) =>
      item.id === node.id
        ? { ...item, data: { ...item.data, node: nextAutomationNode } }
        : item,
    );
  }

  function updateSelectedTitle(value: string) {
    const node = selectedNode.value;
    if (!node) return;
    const nextAutomationNode = {
      ...node.data.node,
      title: value,
    };
    nodes.value = nodes.value.map((item) =>
      item.id === node.id
        ? { ...item, data: { ...item.data, node: nextAutomationNode } }
        : item,
    );
  }

  function onNodeClick(event: { node: AutomationFlowNode }) {
    selectedNodeId.value = event.node.id;
  }

  function onConnect(connection: AutomationFlowConnection) {
    if (!connection.source || !connection.target) return;
    const sourceHandle = connection.sourceHandle ?? "success";
    const id = `${connection.source}-${sourceHandle}-${connection.target}`;
    if (edges.value.some((edge) => edge.id === id)) return;
    edges.value = [
      ...edges.value,
      {
        id,
        source: connection.source,
        target: connection.target,
        sourceHandle,
        targetHandle: connection.targetHandle ?? undefined,
      },
    ];
  }

  function configString(key: string): string {
    const value = selectedNode.value?.data.node.config?.[key];
    return typeof value === "string" ? value : "";
  }

  function configBoolean(key: string): boolean {
    const value = selectedNode.value?.data.node.config?.[key];
    return value === true;
  }

  function nodeStatus(nodeId: string): string | null {
    return options.runNodeStates.value.find((state) => state.nodeId === nodeId)?.status ?? null;
  }

  function refreshNodeStatuses() {
    nodes.value = nodes.value.map((node) => ({
      ...node,
      data: {
        ...node.data,
        selected: node.id === selectedNodeId.value,
        status: nodeStatus(node.id),
      },
    }));
  }

  function automationNodes() {
    return nodes.value.map(flowNodeToAutomation);
  }

  function automationEdges() {
    return edges.value.map(flowEdgeToAutomation);
  }

  watch(selectedNodeId, refreshNodeStatuses);

  return {
    selectedNodeId,
    nodes,
    edges,
    selectedNode,
    hasTriggerNode,
    applyWorkflow,
    addNode,
    updateSelectedConfig,
    updateSelectedTitle,
    onNodeClick,
    onConnect,
    configString,
    configBoolean,
    sourceHandlesForNode,
    refreshNodeStatuses,
    automationNodes,
    automationEdges,
  };
}

function normalizeHandle(value: string): string {
  return value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9_-]+/g, "_")
    .replace(/^_+|_+$/g, "");
}

