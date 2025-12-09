import React, { useState, useCallback, useId } from "react";
import type { Node, Edge } from "reactflow";
import { generateWorkflow } from "../utils/workflowUtils";
import {
  getCachedOptions,
  setCachedOptions,
} from "../utils/dynamicOptionsCache";
import type { SelectOption, DynamicSelectInputSpec } from "../types";

interface DynamicSelectControlProps {
  input: DynamicSelectInputSpec;
  nodeId: string;
  workflowName: string | null;
  allNodeData: Record<string, unknown>;
  onChange: (field: string, value: unknown) => void;
  disabled: boolean;
  value: string;
  onFocus: () => void;
  getNodes: () => Node[];
  getEdges: () => Edge[];
  envBadge?: { visible: boolean; active: boolean; envVar?: string };
  placeholder?: string;
}

export const DynamicSelectControl: React.FC<DynamicSelectControlProps> = ({
  input,
  nodeId,
  workflowName,
  allNodeData,
  onChange,
  disabled,
  value,
  onFocus,
  getNodes,
  getEdges,
  envBadge,
  placeholder: parentPlaceholder,
}) => {
  const cachedOptions = getCachedOptions(workflowName, nodeId, input.name);

  const [options, setOptions] = useState<SelectOption[]>(cachedOptions);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [firstOption, setFirstOption] = useState<string | null>(
    cachedOptions.length > 0 ? cachedOptions[0].value : null,
  );
  const listId = useId();

  const fetchOptions = useCallback(async () => {
    setLoading(true);
    setError(null);

    try {
      if (!getNodes || !getEdges) {
        throw new Error("Cannot fetch options: workflow context not available");
      }
      const nodes = getNodes();
      const edges = getEdges();
      const workflow = generateWorkflow(nodes, edges);

      const saveRes = await fetch(`/api/workflows/${workflowName}`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(workflow),
      });

      if (!saveRes.ok) {
        const errText = await saveRes.text();
        throw new Error(
          `Failed to save temporary workflow: ${saveRes.status} ${errText}`,
        );
      }

      const inputValues: Record<string, unknown> = {};
      input.ui_component.DynamicSelect.depends_on.forEach((depInputName) => {
        inputValues[depInputName] = allNodeData[depInputName];
      });

      const res = await fetch(
        `/api/workflows/${workflowName}/nodes/${nodeId}/options/${input.name}`,
        {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ inputs: inputValues }),
        },
      );

      if (!res.ok) {
        const errorText = await res.text();
        throw new Error(`Failed to fetch options: ${res.status} ${errorText}`);
      }

      const data: SelectOption[] = await res.json();
      setOptions(data);
      setCachedOptions(workflowName, nodeId, input.name, data);

      if (data.length > 0) {
        setFirstOption(data[0].value);
      }
    } catch (e) {
      console.error("Error fetching dynamic options:", e);
      setError((e as Error).message || "Unknown error fetching options");
    } finally {
      setLoading(false);
    }
  }, [
    workflowName,
    nodeId,
    input.name,
    input.ui_component.DynamicSelect.depends_on,
    allNodeData,
    getNodes,
    getEdges,
  ]);

  return (
    <div
      style={{
        display: "flex",
        gap: "8px",
        alignItems: "center",
        width: "100%",
        minWidth: 0,
      }}
    >
      <div style={{ position: "relative", flex: 1, minWidth: 0 }}
        title={envBadge?.active ? `Default from environment variable ${envBadge.envVar}` : undefined}
      >
        <input
          className="nodrag"
          type="text"
          list={listId}
          value={value}
          disabled={disabled || loading}
          onChange={(e) => onChange(input.name, e.target.value)}
          onFocus={onFocus}
          placeholder={
            loading ? "loading..." : error ? "error loading options"
              : (parentPlaceholder || firstOption || "")
          }
          title={error || undefined}
          style={{
            width: "100%",
            padding: "6px 8px",
            boxSizing: "border-box",
            textOverflow: "ellipsis",
          }}
        />
        {envBadge?.visible && (
          <span
            style={{
              position: "absolute",
              right: "6px",
              top: "50%",
              transform: "translateY(-50%)",
              fontSize: "9px",
              fontWeight: "bold",
              padding: "2px 5px",
              borderRadius: "3px",
              background: "#4a90e2",
              color: "white",
              pointerEvents: "none",
              letterSpacing: "0.5px",
              opacity: envBadge.active ? 1 : 0.35,
            }}
          >
            ENV
          </span>
        )}
      </div>
      <datalist id={listId}>
        {options.map((opt) => (
          <option key={opt.value} value={opt.value}>
            {opt.label || opt.value}
          </option>
        ))}
      </datalist>
      <button
        className="nodrag toolbar-btn"
        onClick={fetchOptions}
        disabled={loading || disabled}
        title="Refresh Options"
        style={{
          padding: "6px 8px",
          display: "flex",
          alignItems: "center",
          gap: "4px",
        }}
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="14"
          height="14"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
          className={`lucide lucide-refresh-ccw ${loading ? "spin" : ""}`}
        >
          <path d="M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8" />
          <path d="M17 2v6h6" />
        </svg>
      </button>
    </div>
  );
};
