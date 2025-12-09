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
  ListEditor: 120, // Item list + add input
  Auto: 30, // Will be resolved to another component at runtime
};

// display content heights based on input data type (for Display* nodes)
const DISPLAY_CONTENT_HEIGHTS: Record<string, number> = {
  file: 160, // Image or audio player with waveform + controls
  string: 100, // Markdown content area
  object: 100, // JSON viewer
};

function getUIComponentHeight(uiComponent: UIComponent): number {
  if (!uiComponent || typeof uiComponent !== "object") return 30;

  // find which key exists in the component
  for (const key of Object.keys(uiComponent)) {
    if (key in UI_COMPONENT_HEIGHTS) {
      return UI_COMPONENT_HEIGHTS[key];
    }
  }
  return 30; // Default height for unknown control types
}

// calculate minimum height based on node inputs and outputs
export function calculateNodeMinHeight(meta: NodeMetadata | undefined): number {
  if (!meta) return 150;

  let height = 40 + 24; // header + padding

  // calculate input heights based on UI component types
  // each input field has: label (~20px) + control + marginBottom (12px)
  const INPUT_MARGIN = 12;
  const LABEL_HEIGHT = 20;
  for (const input of meta.inputs) {
    height +=
      LABEL_HEIGHT + getUIComponentHeight(input.ui_component) + INPUT_MARGIN;
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
