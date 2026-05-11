import { useCallback, useState } from "react";
import type { Node, Edge } from "reactflow";
import { generateWorkflow } from "../utils/workflowUtils";
import {
  getCachedOptions,
  setCachedOptions,
} from "../utils/dynamicOptionsCache";
import type { SelectOption } from "../types";

interface UseDynamicOptionsArgs {
  workflowName: string | null;
  nodeId: string;
  inputName: string;
  dependsOn: string[];
  allNodeData: Record<string, unknown>;
  getNodes: () => Node[];
  getEdges: () => Edge[];
}

/**
 * shared options-fetch state + logic for DynamicSelect and DynamicMultiSelect.
 * caches the most recent result so re-opens are instant; the refresh button
 * re-hits the server.
 */
export function useDynamicOptions({
  workflowName,
  nodeId,
  inputName,
  dependsOn,
  allNodeData,
  getNodes,
  getEdges,
}: UseDynamicOptionsArgs) {
  const cached = getCachedOptions(workflowName, nodeId, inputName);
  const [options, setOptions] = useState<SelectOption[]>(cached);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchOptions = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      if (!getNodes || !getEdges) {
        throw new Error("workflow context not available");
      }
      const workflow = generateWorkflow(getNodes(), getEdges());
      const saveRes = await fetch(`/api/workflows/${workflowName}`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(workflow),
      });
      if (!saveRes.ok) {
        throw new Error(`save failed: ${saveRes.status} ${await saveRes.text()}`);
      }

      const inputValues: Record<string, unknown> = {};
      dependsOn.forEach((dep) => {
        inputValues[dep] = allNodeData[dep];
      });

      const res = await fetch(
        `/api/workflows/${workflowName}/nodes/${nodeId}/options/${inputName}`,
        {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ inputs: inputValues }),
        },
      );
      if (!res.ok) {
        throw new Error(`fetch failed: ${res.status} ${await res.text()}`);
      }
      const data: SelectOption[] = await res.json();
      setOptions(data);
      setCachedOptions(workflowName, nodeId, inputName, data);
    } catch (e) {
      console.error(`error fetching options for ${inputName}:`, e);
      setError((e as Error).message || "unknown error");
    } finally {
      setLoading(false);
    }
  }, [
    workflowName,
    nodeId,
    inputName,
    dependsOn,
    allNodeData,
    getNodes,
    getEdges,
  ]);

  return { options, loading, error, fetchOptions };
}
