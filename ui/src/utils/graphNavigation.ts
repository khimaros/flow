import type { Node, Edge } from "reactflow";

export const sortNodes = (nodes: Node[]) => {
  return [...nodes].sort((a, b) => {
    // sort by Y first (with a small tolerance), then X
    if (Math.abs(a.position.y - b.position.y) > 10)
      return a.position.y - b.position.y;
    return a.position.x - b.position.x;
  });
};

export const getRootNodes = (nodes: Node[], edges: Edge[]) => {
  const nodesWithInputs = new Set(edges.map((e) => e.target));
  return sortNodes(nodes.filter((n) => !nodesWithInputs.has(n.id)));
};

export const getLeafNodes = (nodes: Node[], edges: Edge[]) => {
  const nodesWithOutputs = new Set(edges.map((e) => e.source));
  return sortNodes(nodes.filter((n) => !nodesWithOutputs.has(n.id)));
};

export const getUpstreamNodes = (
  nodeId: string,
  nodes: Node[],
  edges: Edge[],
) => {
  const incomingEdges = edges.filter((e) => e.target === nodeId);
  const sourceNodes = nodes.filter((n) =>
    incomingEdges.some((edge) => edge.source === n.id),
  );
  return sortNodes(sourceNodes);
};

export const getDownstreamNodes = (
  nodeId: string,
  nodes: Node[],
  edges: Edge[],
) => {
  const outgoingEdges = edges.filter((e) => e.source === nodeId);
  const targetNodes = nodes.filter((n) =>
    outgoingEdges.some((edge) => edge.target === n.id),
  );
  return sortNodes(targetNodes);
};

export const getSiblingNodes = (
  nodeId: string,
  nodes: Node[],
  edges: Edge[],
) => {
  const incomingEdges = edges.filter((e) => e.target === nodeId);
  const parentIds = new Set(incomingEdges.map((e) => e.source));

  if (parentIds.size === 0) {
    // it's a root node, siblings are other root nodes
    return getRootNodes(nodes, edges);
  }

  // siblings are children of the same parents
  const siblingIds = new Set<string>();
  edges.forEach((edge) => {
    if (parentIds.has(edge.source)) {
      siblingIds.add(edge.target);
    }
  });
  const siblings = nodes.filter((n) => siblingIds.has(n.id));
  return sortNodes(siblings);
};
