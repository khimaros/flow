import React from "react";
import { Handle, Position, NodeResizer } from "reactflow";
import { GripVertical, Link, Play, Code, Undo2 } from "lucide-react";

// width of invisible border on each side for handles (matches grid size for alignment)
export const HANDLE_MARGIN = 15;

// define custom CSS properties to allow --variables
interface CustomCSSProperties extends React.CSSProperties {
  [key: `--${string}`]: string | number;
}

// node visual state type for styling
type NodeVisualState = "error" | "running" | "bypassed" | "default";

// determine the primary visual state of a node (priority-based)
function getNodeVisualState(
  error?: string,
  isRunning?: boolean,
  isBypassed?: boolean,
): NodeVisualState {
  if (error) return "error";
  if (isRunning) return "running";
  if (isBypassed) return "bypassed";
  return "default";
}

export const NodeContainer = ({
  label,
  children,
  selected,
  isBypassed,
  onRun,
  isRunning,
  justFinished,
  isCached,
  error,
  minHeight = 150,
  hasSource,
  showSource,
  onToggleSource,
}: {
  label: string;
  children: React.ReactNode;
  selected?: boolean;
  isBypassed?: boolean;
  onRun?: (force?: boolean) => void;
  isRunning?: boolean;
  justFinished?: boolean;
  isCached?: boolean;
  error?: string;
  minHeight?: number;
  hasSource?: boolean;
  showSource?: boolean;
  onToggleSource?: () => void;
}) => {
  const visualState = getNodeVisualState(error, isRunning, isBypassed);

  const getClassName = () => {
    const classes: string[] = [];
    if (error && !isRunning) classes.push("node-has-error");
    if (justFinished && !isRunning && !error) {
      classes.push("node-just-finished");
      if (isCached) classes.push("node-cached");
    }
    if (isRunning) classes.push("node-running");
    return classes.join(" ");
  };

  const frameClasses = [
    "node-frame",
    isBypassed ? "node-bypassed" : "",
    getClassName(),
  ]
    .filter(Boolean)
    .join(" ");

  const frameStyle: CustomCSSProperties = {
    minHeight: `${minHeight}px`,
    "--finish-glow-color": isCached ? "#f59e0b" : "#22c55e",
  };

  return (
    <div className="node-wrapper">
      <div
        className={frameClasses}
        data-visual-state={visualState}
        data-cached={isCached ? "true" : undefined}
        style={frameStyle}
      >
        <NodeResizer
          minWidth={240 + HANDLE_MARGIN * 2}
          minHeight={minHeight}
          isVisible={true}
          lineStyle={{
            border: "4px solid transparent",
          }}
          handleStyle={{
            width: 10,
            height: 10,
            background: "transparent",
            border: "none",
          }}
        />

        <div
          data-node-header="true"
          style={{
            display: "flex",
            justifyContent: "space-between",
            alignItems: "center",
            padding: "8px 12px",
            background: error
              ? "color-mix(in srgb, var(--danger-color), var(--node-header-bg) 85%)"
              : selected
                ? "color-mix(in srgb, var(--primary-color), var(--node-header-bg) 85%)"
                : "var(--node-header-bg)",
            borderRadius: "6px 6px 0 0",
            borderBottom: "1px solid var(--node-border)",
            fontWeight: 600,
            fontSize: "13px",
            flexShrink: 0,
          }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: "6px" }}>
            <GripVertical size={14} style={{ opacity: 0.5 }} />
            {label}
          </div>
          <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
            {hasSource && (
              <div
                className="nodrag nopan noselect"
                onMouseDownCapture={(e) => {
                  e.stopPropagation();
                }}
                onPointerDownCapture={(e) => {
                  e.stopPropagation();
                }}
                onClick={(e) => {
                  e.stopPropagation();
                  onToggleSource?.();
                }}
                style={{
                  cursor: "pointer",
                  color: showSource
                    ? "var(--primary-color)"
                    : "var(--node-text)",
                  opacity: showSource ? 1 : 0.6,
                  display: "flex",
                  padding: "2px",
                }}
                title={showSource ? "Hide Source" : "View Source"}
              >
                {showSource ? <Undo2 size={14} /> : <Code size={14} />}
              </div>
            )}
            <div
              className="nodrag nopan noselect"
              onMouseDownCapture={(e) => {
                e.stopPropagation();
              }}
              onPointerDownCapture={(e) => {
                e.stopPropagation();
              }}
              onClick={(e) => {
                e.stopPropagation();
                onRun?.(e.shiftKey);
              }}
              style={{
                cursor: "pointer",
                color: "var(--primary-color)",
                display: "flex",
                padding: "2px",
              }}
              title="Run Node (shift+click to force, skip cache)"
            >
              <Play size={14} fill="currentColor" />
            </div>
          </div>
        </div>
        <div
          style={{
            padding: "12px",
            flex: 1,
            flexShrink: 1,
            display: "flex",
            flexDirection: "column",
            position: "relative",
            overflow: "visible",
            minWidth: 0, // Allow shrinking in flex container
            minHeight: 0, // Allow shrinking vertically
            maxHeight: "100%",
            boxSizing: "border-box",
          }}
        >
          {children}
        </div>
      </div>
    </div>
  );
};

export const InputField = ({
  label,
  children,
  id,
  type = "string",
  connected,
  required,
  description,
}: {
  label: string;
  children: React.ReactNode;
  id: string;
  type?: string;
  connected?: boolean;
  required?: boolean;
  description?: string;
}) => {
  const color = `var(--type-${type})`;
  // check if children should flex to fill available space
  const isTextArea =
    React.isValidElement(children) && children.type === "textarea";
  const shouldFlex = isTextArea || type === "list";

  return (
    <div
      style={{
        position: "relative",
        marginBottom: "12px",
        flex: shouldFlex ? 1 : "unset",
        flexShrink: shouldFlex ? 1 : 0,
        display: "flex",
        flexDirection: "column",
        minWidth: 0, // Allow shrinking in flex container
        minHeight: shouldFlex ? 0 : "unset",
        maxHeight: shouldFlex ? "100%" : "unset",
      }}
    >
      <Handle
        type="target"
        position={Position.Left}
        id={id}
        className="input-handle"
        title={description}
        style={{
          position: "absolute",
          left: "-22px",
          top: "4px",
          width: "8px",
          height: "8px",
          background: color,
          pointerEvents: connected ? "none" : "auto",
        }}
      />
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: "4px",
          marginBottom: "4px",
        }}
        title={description}
      >
        <label
          style={{
            display: "block",
            fontSize: "11px",
            fontWeight: 500,
            color: "var(--node-text)",
            opacity: 0.8,
          }}
        >
          {label}
          {required && (
            <span style={{ color: "var(--danger-color)", marginLeft: "2px" }}>
              *
            </span>
          )}
        </label>
        <span
          style={{
            fontSize: "9px",
            color: color,
            background: `color-mix(in srgb, ${color}, transparent 90%)`,
            padding: "1px 3px",
            borderRadius: "2px",
            textTransform: "uppercase",
          }}
        >
          {type.slice(0, 3)}
        </span>
        {connected && (
          <Link
            size={10}
            style={{
              marginLeft: "auto",
              color: "var(--node-text)",
              opacity: 0.5,
            }}
          />
        )}
      </div>
      <div
        style={{
          ...(connected
            ? {
                opacity: 0.6,
                filter: "grayscale(1)",
                border: "1px dashed var(--node-border)",
                borderRadius: "4px",
                padding: "2px",
              }
            : {}),
          flex: 1,
          flexShrink: 1,
          display: "flex",
          flexDirection: "column",
          minWidth: 0, // Allow shrinking in flex container
          minHeight: 0, // Allow shrinking vertically
          maxHeight: "100%",
          boxSizing: "border-box",
        }}
      >
        <div
          style={{
            flex: 1,
            flexShrink: 1,
            minWidth: 0,
            minHeight: 0,
            maxHeight: "100%",
            overflow: "hidden",
            display: "flex",
            flexDirection: "column",
            boxSizing: "border-box",
          }}
        >
          {connected &&
          React.isValidElement(children) &&
          (children.type === "input" || children.type === "textarea")
            ? React.cloneElement(
                children as React.ReactElement<{
                  readOnly?: boolean;
                  tabIndex?: number;
                }>,
                {
                  readOnly: true,
                  tabIndex: -1,
                },
              )
            : children}
        </div>
      </div>
    </div>
  );
};

export const OutputHandle = ({
  id,
  label,
  type = "string",
  description,
}: {
  id: string;
  label: string;
  type?: string;
  description?: string;
}) => {
  return (
    <div
      style={{
        position: "relative",
        display: "flex",
        justifyContent: "flex-end",
        alignItems: "center",
        minHeight: "18px",
        width: "100%",
      }}
      title={description}
    >
      <div style={{ display: "flex", alignItems: "center", gap: "4px" }}>
        <span
          style={{
            fontSize: "9px",
            color: `var(--type-${type})`,
            background: `color-mix(in srgb, var(--type-${type}), transparent 90%)`,
            padding: "1px 3px",
            borderRadius: "2px",
            textTransform: "uppercase",
          }}
        >
          {type.slice(0, 3)}
        </span>
        <div
          style={{
            fontSize: "11px",
            fontWeight: 500,
            color: "var(--node-text)",
            opacity: 0.8,
          }}
        >
          {label}
        </div>
      </div>
      <Handle
        type="source"
        position={Position.Right}
        id={id}
        className="output-handle"
        style={{
          position: "absolute",
          top: "4px",
          right: "-22px",
          width: "8px",
          height: "8px",
          background: `var(--type-${type})`,
        }}
      />
    </div>
  );
};
