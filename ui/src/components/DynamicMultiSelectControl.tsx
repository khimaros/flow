import React, { useMemo, useRef, useState } from "react";
import type { Node, Edge } from "reactflow";
import { useDynamicOptions } from "../hooks/useDynamicOptions";
import { DynamicOptionsPopup } from "./DynamicOptionsPopup";
import type { DynamicMultiSelectInputSpec } from "../types";

interface DynamicMultiSelectControlProps {
  input: DynamicMultiSelectInputSpec;
  nodeId: string;
  workflowName: string | null;
  allNodeData: Record<string, unknown>;
  onChange: (field: string, value: unknown) => void;
  disabled: boolean;
  value: string[];
  onFocus: () => void;
  getNodes: () => Node[];
  getEdges: () => Edge[];
}

export const DynamicMultiSelectControl: React.FC<
  DynamicMultiSelectControlProps
> = ({
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
}) => {
  const [open, setOpen] = useState(false);
  const triggerRef = useRef<HTMLButtonElement | null>(null);

  const { options, loading, error, fetchOptions } = useDynamicOptions({
    workflowName,
    nodeId,
    inputName: input.name,
    dependsOn: input.ui_component.DynamicMultiSelect.depends_on,
    allNodeData,
    getNodes,
    getEdges,
  });

  const selectedSet = useMemo(() => new Set(value ?? []), [value]);

  // auto-fetch on first open if no cached options
  const handleToggleOpen = () => {
    setOpen((prev) => {
      const next = !prev;
      if (next && options.length === 0 && !loading && !error) {
        fetchOptions();
      }
      return next;
    });
  };

  const handlePick = (optValue: string) => {
    const next = new Set(selectedSet);
    if (next.has(optValue)) next.delete(optValue);
    else next.add(optValue);
    // preserve registry order
    const ordered = options.map((o) => o.value).filter((v) => next.has(v));
    Array.from(next).forEach((v) => {
      if (!ordered.includes(v)) ordered.push(v);
    });
    onChange(input.name, ordered);
  };

  const summary = (() => {
    const sel = value ?? [];
    if (sel.length === 0) return "no tools enabled";
    if (sel.length <= 2) return sel.join(", ");
    return `${sel.slice(0, 2).join(", ")} (+${sel.length - 2})`;
  })();

  return (
    <div
      style={{
        display: "flex",
        gap: "8px",
        alignItems: "center",
        width: "100%",
        minWidth: 0,
      }}
      onFocus={onFocus}
    >
      <button
        ref={triggerRef}
        type="button"
        className="nodrag"
        disabled={disabled}
        onClick={handleToggleOpen}
        title={(value ?? []).join(", ")}
        style={{
          flex: 1,
          minWidth: 0,
          padding: "6px 8px",
          boxSizing: "border-box",
          fontFamily: "monospace",
          fontSize: "12px",
          textAlign: "left",
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: "6px",
          cursor: disabled ? "default" : "pointer",
          overflow: "hidden",
          background: "var(--input-bg)",
          color: "var(--input-text)",
          border: "1px solid var(--input-border)",
          borderRadius: "3px",
        }}
      >
        <span
          style={{
            flex: 1,
            minWidth: 0,
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
            color:
              (value ?? []).length === 0
                ? "var(--type-any)"
                : "var(--input-text)",
          }}
        >
          {summary}
        </span>
        {(value ?? []).length > 0 && !disabled && (
          <span
            role="button"
            aria-label="clear"
            title="clear"
            onClick={(e) => {
              e.stopPropagation();
              onChange(input.name, []);
            }}
            style={{
              flexShrink: 0,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              width: 14,
              height: 14,
              borderRadius: "50%",
              opacity: 0.5,
              cursor: "pointer",
            }}
            onMouseEnter={(e) => (e.currentTarget.style.opacity = "1")}
            onMouseLeave={(e) => (e.currentTarget.style.opacity = "0.5")}
          >
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="10"
              height="10"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2.5"
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <path d="M18 6 6 18" />
              <path d="m6 6 12 12" />
            </svg>
          </span>
        )}
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="10"
          height="10"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
          style={{
            flexShrink: 0,
            transform: open ? "rotate(180deg)" : undefined,
            transition: "transform 0.1s",
          }}
        >
          <path d="m6 9 6 6 6-6" />
        </svg>
      </button>

      <button
        className="nodrag toolbar-btn"
        onClick={fetchOptions}
        disabled={loading || disabled}
        title={error ?? "refresh options"}
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

      <DynamicOptionsPopup
        open={open}
        onClose={() => setOpen(false)}
        triggerRef={triggerRef}
        options={options}
        loading={loading}
        error={error}
        mode="multi"
        selectedValues={selectedSet}
        onPick={handlePick}
      />
    </div>
  );
};
