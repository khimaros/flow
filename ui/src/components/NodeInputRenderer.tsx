import { DynamicSelectControl } from "./DynamicSelectControl";
import { AudioRecorderControl } from "./AudioRecorderControl";
import { ListEditorControl } from "./ListEditorControl";
import type { InputSpec, UIComponent } from "../types";
import type { Node, Edge } from "reactflow";

// common styles for input controls
const INPUT_STYLE_BASE = {
  width: "100%",
  minWidth: 0, // Allow shrinking below content size in flex containers
  padding: "6px 8px",
  boxSizing: "border-box" as const,
  fontFamily: "monospace",
  textOverflow: "ellipsis" as const,
};

interface RenderInputData {
  [key: string]: unknown;
  inputs?: Record<string, { connected?: boolean }>;
  onSelect?: (id: string) => void;
  onChange: (field: string, value: unknown) => void;
  workflowName: string | null;
  getNodes: () => Node[];
  getEdges: () => Edge[];
}

export const renderInputControl = (
  input: InputSpec,
  data: RenderInputData,
  nodeId: string,
): React.ReactNode => {
  // env_value serves as default for env-backed inputs but can be overridden.
  // restore to env_value on blur when cleared.
  const hasEnvValue = !!(input.env_value && input.env_value.length > 0);

  const rawValue = data[input.name];
  const inputEmpty = rawValue === undefined || rawValue === null || rawValue === "";
  // when env_value is set and input is empty, leave value empty and use placeholder
  const usingEnvValue = hasEnvValue && inputEmpty;
  // leave text/number fields empty when input is empty so placeholders show;
  // boolValue still uses defaults for checkbox rendering
  const value = inputEmpty ? "" : rawValue;
  const boolValue = inputEmpty ? Boolean(input.default ?? false) : Boolean(rawValue);

  const disabled = data.inputs?.[input.name]?.connected;
  const handleFocus = () => data.onSelect?.(nodeId);
  const isPassword = "Password" in input.ui_component;
  // placeholder priority: env value > default value > <unset>
  const placeholder = usingEnvValue
    ? (isPassword ? "•".repeat(String(input.env_value).length) : String(input.env_value))
    : inputEmpty
      ? (input.default != null && String(input.default) !== "" ? String(input.default) : "<unset>")
      : undefined;

  // cap display string to avoid DOM bloat with large node outputs
  const MAX_DISPLAY_LEN = 10_000;
  const stringValue = (() => {
    const raw = String(value ?? "");
    return raw.length > MAX_DISPLAY_LEN
      ? raw.slice(0, MAX_DISPLAY_LEN) + "..."
      : raw;
  })();

  const disabledTitle = disabled ? stringValue : undefined;

  // common props shared by most input elements
  const commonProps = {
    className: "nodrag",
    disabled,
    onFocus: handleFocus,
    title: disabledTitle,
    placeholder,
  };

  // stable wrapper for env-backed inputs to avoid DOM restructuring on state change
  const wrapWithEnvBadge = (control: React.ReactNode): React.ReactNode => {
    if (!hasEnvValue) return control;
    return (
      <div
        style={{ position: "relative", width: "100%", minWidth: 0 }}
        title={usingEnvValue ? `Default from environment variable ${input.env_var}` : undefined}
      >
        {control}
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
            opacity: usingEnvValue ? 1 : 0.35,
          }}
        >
          ENV
        </span>
      </div>
    );
  };

  if ("DynamicSelect" in input.ui_component) {
    // use raw value so the default shows as placeholder, not as filled text
    const rawStr = String(rawValue ?? "");
    return (
      <DynamicSelectControl
        input={
          input as InputSpec & {
            ui_component: { DynamicSelect: { depends_on: string[] } };
          }
        }
        nodeId={nodeId}
        workflowName={data.workflowName}
        allNodeData={data as Record<string, unknown>}
        onChange={data.onChange}
        disabled={!!disabled}
        value={rawStr}
        onFocus={handleFocus}
        getNodes={data.getNodes}
        getEdges={data.getEdges}
        envBadge={hasEnvValue ? { visible: true, active: usingEnvValue, envVar: input.env_var ?? undefined } : undefined}
        placeholder={placeholder}
      />
    );
  }

  if ("Select" in input.ui_component) {
    return wrapWithEnvBadge(
      <select
        {...commonProps}
        value={stringValue}
        onChange={(e) => data.onChange(input.name, e.target.value)}
        style={INPUT_STYLE_BASE}
      >
        {input.ui_component.Select.options.map((opt) => (
          <option key={opt.value} value={opt.value}>
            {opt.label ?? opt.value}
          </option>
        ))}
      </select>,
    );
  }

  if ("TextArea" in input.ui_component) {
    return wrapWithEnvBadge(
      <textarea
        {...commonProps}
        value={
          typeof value === "object"
            ? (() => {
                const json = JSON.stringify(value, null, 2);
                return json.length > MAX_DISPLAY_LEN
                  ? json.slice(0, MAX_DISPLAY_LEN) + "..."
                  : json;
              })()
            : stringValue
        }
        onChange={(e) => data.onChange(input.name, e.target.value)}
        style={{
          ...INPUT_STYLE_BASE,
          height: "100%",
          minHeight: "60px",
          resize: "none",
        }}
      />,
    );
  }

  if ("Number" in input.ui_component) {
    const isInteger = input.type === "integer";
    return wrapWithEnvBadge(
      <input
        {...commonProps}
        type="number"
        step={isInteger ? 1 : 0.1}
        value={stringValue}
        onChange={(e) => {
          const parsed =
            e.target.value === ""
              ? null
              : isInteger
                ? parseInt(e.target.value, 10)
                : parseFloat(e.target.value);
          data.onChange(input.name, parsed);
        }}
        style={{ ...INPUT_STYLE_BASE, paddingRight: "6px" }}
      />,
    );
  }

  if ("BooleanSelect" in input.ui_component) {
    return wrapWithEnvBadge(
      <select
        {...commonProps}
        value={boolValue ? "true" : "false"}
        onChange={(e) => data.onChange(input.name, e.target.value === "true")}
        style={INPUT_STYLE_BASE}
      >
        <option value="true">true</option>
        <option value="false">false</option>
      </select>,
    );
  }

  if ("Checkbox" in input.ui_component) {
    return wrapWithEnvBadge(
      <input
        {...commonProps}
        type="checkbox"
        checked={boolValue}
        onChange={(e) => data.onChange(input.name, e.target.checked)}
        style={{ width: "16px", height: "16px" }}
      />,
    );
  }

  if ("Password" in input.ui_component) {
    return wrapWithEnvBadge(
      <input
        {...commonProps}
        type="password"
        value={stringValue}
        onChange={(e) => data.onChange(input.name, e.target.value)}
        style={INPUT_STYLE_BASE}
      />,
    );
  }

  if ("AudioRecorder" in input.ui_component) {
    return (
      <AudioRecorderControl
        value={stringValue}
        onChange={(value) => data.onChange(input.name, value)}
        disabled={!!disabled}
        onFocus={handleFocus}
      />
    );
  }

  if ("ListEditor" in input.ui_component) {
    return (
      <ListEditorControl
        value={value as string | string[]}
        onChange={(value) => data.onChange(input.name, value)}
        disabled={!!disabled}
        onFocus={handleFocus}
      />
    );
  }

  // auto: resolve to concrete component based on type, then re-render
  if ("Auto" in input.ui_component) {
    const typeToComponent: Record<string, UIComponent> = {
      boolean: { BooleanSelect: {} },
      integer: { Number: {} },
      float: { Number: {} },
      list: { ListEditor: {} },
    };
    const resolved = typeToComponent[input.type] ?? { Text: {} };
    return renderInputControl(
      { ...input, ui_component: resolved },
      data,
      nodeId,
    );
  }

  if (input.type === "file" || input.type === "object") {
    const rawJson =
      typeof value === "object" && value !== null
        ? JSON.stringify(value)
        : stringValue;
    const jsonValue =
      rawJson.length > MAX_DISPLAY_LEN
        ? rawJson.slice(0, MAX_DISPLAY_LEN) + "..."
        : rawJson;
    return wrapWithEnvBadge(
      <input
        {...commonProps}
        title={disabled ? jsonValue : undefined}
        type="text"
        value={jsonValue}
        onChange={(e) => data.onChange(input.name, e.target.value)}
        style={INPUT_STYLE_BASE}
      />,
    );
  }

  // default to text input
  return wrapWithEnvBadge(
    <input
      {...commonProps}
      type="text"
      value={stringValue}
      onChange={(e) => data.onChange(input.name, e.target.value)}
      style={INPUT_STYLE_BASE}
    />,
  );
};
