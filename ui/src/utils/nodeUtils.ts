import type { NodeMetadata, UIComponent } from "../types";

// height constants for different UI input controls (excluding label and margin)
const UI_COMPONENT_HEIGHTS: Record<string, number> = {
  Text: 30,
  TextArea: 60,
  Number: 30,
  Checkbox: 20,
  Password: 30,
  Select: 30,
  DynamicSelect: 30,
  AudioRecorder: 80, // Canvas (40px) + controls (40px)
  BooleanSelect: 30,
  ListEditor: 140, // editable: ~3–4 rows + add-row + gaps
};

// when a list input is wired from an upstream output, the control renders in
// its compact read-only form (no edit buttons, no add row). give it a smaller
// minimum so connected list ports don't bloat the node.
const LIST_EDITOR_CONNECTED_HEIGHT = 60;

// mirrors the Rust Auto resolution in src/node.rs: pick the default control
// for a given DataType so Auto-typed inputs contribute the right height.
const AUTO_HEIGHT_BY_TYPE: Record<string, number> = {
  string: UI_COMPONENT_HEIGHTS.Text,
  integer: UI_COMPONENT_HEIGHTS.Number,
  float: UI_COMPONENT_HEIGHTS.Number,
  boolean: UI_COMPONENT_HEIGHTS.BooleanSelect,
  list: UI_COMPONENT_HEIGHTS.ListEditor,
  object: UI_COMPONENT_HEIGHTS.TextArea,
  any: UI_COMPONENT_HEIGHTS.Text,
  file: UI_COMPONENT_HEIGHTS.Text,
};

// display content heights based on input data type (for Display* nodes)
const DISPLAY_CONTENT_HEIGHTS: Record<string, number> = {
  file: 160, // Image or audio player with waveform + controls
  string: 100, // Markdown content area
  object: 100, // JSON viewer
};

function getUIComponentHeight(
  uiComponent: UIComponent,
  inputType: string,
  connected: boolean,
): number {
  if (!uiComponent || typeof uiComponent !== "object") return 30;

  for (const key of Object.keys(uiComponent)) {
    if (key === "Auto") {
      const resolved = AUTO_HEIGHT_BY_TYPE[inputType.toLowerCase()] ?? 30;
      if (connected && resolved === UI_COMPONENT_HEIGHTS.ListEditor) {
        return LIST_EDITOR_CONNECTED_HEIGHT;
      }
      return resolved;
    }
    if (key === "ListEditor" && connected) {
      return LIST_EDITOR_CONNECTED_HEIGHT;
    }
    if (key in UI_COMPONENT_HEIGHTS) {
      return UI_COMPONENT_HEIGHTS[key];
    }
  }
  return 30;
}

// calculate minimum height based on node inputs and outputs.
// `connectedInputs` lets us shrink list controls that render in their
// read-only form because they're wired from an upstream output.
export function calculateNodeMinHeight(
  meta: NodeMetadata | undefined,
  connectedInputs?: Record<string, boolean>,
): number {
  if (!meta) return 150;

  let height = 40 + 24; // header + padding

  // calculate input heights based on UI component types
  // each input field has: label (~20px) + control + marginBottom (12px)
  const INPUT_MARGIN = 12;
  const LABEL_HEIGHT = 20;
  for (const input of meta.inputs) {
    const connected = !!connectedInputs?.[input.name];
    height +=
      LABEL_HEIGHT +
      getUIComponentHeight(input.ui_component, input.type, connected) +
      INPUT_MARGIN;
  }

  // for display nodes, add height for the display content area based on input type
  if (meta.name.startsWith("Display") && meta.inputs.length > 0) {
    const primaryInputType = meta.inputs[0].type.toLowerCase();
    const displayHeight = DISPLAY_CONTENT_HEIGHTS[primaryInputType] || 100;
    height += displayHeight;
  }

  // calculate output section height (border-top + outputs)
  const outputCount = meta.outputs?.length || 0;
  if (outputCount > 0) {
    height += 16 + outputCount * 24; // border/padding + each output row
  }

  return Math.max(100, height);
}
