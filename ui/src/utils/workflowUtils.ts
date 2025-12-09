import type { Node, Edge } from "reactflow";
import type { Workflow, WorkflowNode, WorkflowEdge } from "../types";

// execution state: map of node ID to its outputs
export type WorkflowState = Record<string, Record<string, unknown>>;

/// propagate execution state into downstream node inputs via edge connections.
/// mutates nodes in place.
export const resolveOutputs = (
  nodes: WorkflowNode[],
  edges: WorkflowEdge[],
  state: WorkflowState,
): void => {
  for (const e of edges) {
    const sourceOutputs = state[e.source];
    const targetNode = nodes.find((n) => n.id === e.target);
    if (!sourceOutputs || !targetNode) continue;
    const sourceHandle = e.sourceHandle || "output";
    const targetHandle = e.targetHandle;
    if (targetHandle && sourceHandle in sourceOutputs) {
      targetNode.inputs[targetHandle] = sourceOutputs[sourceHandle];
    }
  }
};

// keys that are internal UI/callback props, not workflow inputs
const UI_KEYS = new Set([
  "onChange",
  "label",
  "onDelete",
  "onRun",
  "metadata",
  "isPinned",
  "isBypassed",
  "skip_cache",
  "workflowName",
  "getNodes",
  "getEdges",
  "onSelect",
  "onToggleSource",
  "showSource",
  "isRunning",
  "justFinished",
  "isCached",
  "progress",
  "progressMessage",
  "outputs",
  "error",
  "inputs",
]);

export const generateWorkflow = (
  nodes: Node[],
  edges: Edge[],
  forceRun: boolean = false,
  targetNodeId?: string,
): Workflow => {
  // build set of inputs that are connected via edges (these are derived,
  // not user-set, so we exclude them from saved inputs)
  const connectedInputs = new Set<string>();
  for (const e of edges) {
    if (e.targetHandle) {
      connectedInputs.add(`${e.target}/${e.targetHandle}`);
    }
  }

  const workflowNodes: WorkflowNode[] = nodes.map((n) => {
    const inputs: Record<string, unknown> = {};

    for (const [key, value] of Object.entries(n.data || {})) {
      if (UI_KEYS.has(key)) continue;
      // skip inputs that are fed by an edge (derived from upstream outputs)
      if (connectedInputs.has(`${n.id}/${key}`)) continue;
      inputs[key] = value;
    }

    return {
      id: n.id,
      type: n.type!,
      position: n.position,
      size:
        n.width && n.height ? { width: n.width, height: n.height } : undefined,
      inputs,
      skipCache: n.data?.skip_cache,
      bypassed: n.data?.isBypassed,
    };
  });

  const workflowEdges: WorkflowEdge[] = edges.map((e) => {
    const src = e.sourceHandle || "output";
    const tgt = e.targetHandle || "input";
    return {
      id: `e-${e.source}-${src}-${e.target}-${tgt}`,
      source: e.source,
      sourceHandle: src,
      target: e.target,
      targetHandle: tgt,
    };
  });

  return {
    nodes: workflowNodes,
    edges: workflowEdges,
    forceRun,
    targetNodeId,
  };
};

/// extract execution state from react flow nodes
export const generateState = (nodes: Node[]): WorkflowState => {
  const state: WorkflowState = {};
  for (const n of nodes) {
    const outputs = n.data?.outputs as Record<string, unknown> | undefined;
    if (outputs && Object.keys(outputs).length > 0) {
      state[n.id] = outputs;
    }
  }
  return state;
};
