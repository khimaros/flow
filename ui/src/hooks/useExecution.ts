import { useCallback, useRef, useLayoutEffect } from "react";
import type { Node, Edge } from "reactflow";
import { generateWorkflow } from "../utils/workflowUtils";

export interface ExecutionEvent {
  type: "Started" | "Progress" | "Finished" | "Error";
  data: {
    node_id: string;
    progress?: number;
    message?: string;
    result?: unknown;
    error?: string;
    cached?: boolean;
  };
}

// generate prompt text for a Read node based on its connected edges
function getReadNodePrompt(
  nodeId: string,
  edges: Edge[],
  nodes: Node[],
): string {
  const edge = edges.find(
    (e) => e.source === nodeId && e.sourceHandle === "output",
  );
  if (edge) {
    const targetNode = nodes.find((n) => n.id === edge.target);
    if (targetNode) {
      const nodeType = targetNode.type || "Unknown";
      const inputName = edge.targetHandle || "input";
      return `${nodeType} (${inputName}):`;
    }
  }
  return "input:";
}

export const useExecution = (
  nodes: Node[],
  edges: Edge[],
  currentWorkflow: string | null,
) => {
  const nodesRef = useRef(nodes);
  const edgesRef = useRef(edges);

  useLayoutEffect(() => {
    nodesRef.current = nodes;
    edgesRef.current = edges;
  }, [nodes, edges]);

  const submitJob = useCallback(
    async (targetNodeId?: string, force: boolean = false) => {
      const currentNodes = nodesRef.current;
      const currentEdges = edgesRef.current;
      console.log(
        `[useExecution] submitJob called. target=${targetNodeId} force=${force}`,
      );
      console.log(
        `[useExecution] Nodes: ${currentNodes.length}, Edges: ${currentEdges.length}`,
      );
      console.log(
        `[useExecution] Edges content:`,
        JSON.stringify(currentEdges),
      );

      try {
        const workflow = generateWorkflow(
          currentNodes,
          currentEdges,
          force,
          targetNodeId,
        );

        // check for Read nodes and prompt for input
        const readNodes = workflow.nodes.filter((n) => n.type === "Read");
        for (const readNode of readNodes) {
          // skip if input already has a value
          if (
            readNode.inputs.input &&
            String(readNode.inputs.input).trim() !== ""
          ) {
            continue;
          }
          const promptText = getReadNodePrompt(
            readNode.id,
            currentEdges,
            currentNodes,
          );
          const userInput = window.prompt(promptText);
          if (userInput === null) {
            // user cancelled
            return null;
          }
          readNode.inputs.input = userInput;
        }

        const response = await fetch("/api/queue/submit", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({
            workflow,
            workflow_name: currentWorkflow,
          }),
        });

        if (!response.ok) {
          throw new Error("Failed to submit job");
        }

        return await response.json();
      } catch (e) {
        console.error("Job submission error:", e);
        if (e instanceof Error) {
          alert("Error: " + e.message);
        } else {
          alert("Error: Unknown error occurred");
        }
        return null;
      }
    },
    [currentWorkflow],
  );

  return { submitJob };
};
