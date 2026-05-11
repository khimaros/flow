// workflow schema types - matches backend graph.rs

export interface Position {
  x: number;
  y: number;
}

export interface Size {
  width: number;
  height: number;
}

export interface WorkflowNode {
  id: string;
  type: string;
  position?: Position;
  size?: Size;
  inputs: Record<string, unknown>;
  skipCache?: boolean;
  bypassed?: boolean;
}

export interface WorkflowEdge {
  id: string;
  source: string;
  sourceHandle: string;
  target: string;
  targetHandle: string;
}

export interface FileValue {
  path: string;
  url: string;
  mime_type: string;
}

export interface Workflow {
  nodes: WorkflowNode[];
  edges: WorkflowEdge[];
  forceRun?: boolean;
  targetNodeId?: string;
}

// UI-only transient state (not persisted)

export interface NodeUIState {
  isRunning: boolean;
  progress: number;
  progressMessage?: string;
  error?: string;
  outputs?: Record<string, unknown>;
  isCached?: boolean;
  justFinished?: boolean;
}

export type NodeUIStateMap = Map<string, NodeUIState>;

// node metadata from backend

export interface ScriptSource {
  language: string;
  source: string;
}

export interface SelectOption {
  value: string;
  label?: string;
}

export type UIComponent =
  | { Text: Record<string, never> }
  | { TextArea: Record<string, never> }
  | { Number: Record<string, never> }
  | { Checkbox: Record<string, never> }
  | { BooleanSelect: Record<string, never> }
  | { Password: Record<string, never> }
  | { Select: { options: SelectOption[] } }
  | { DynamicSelect: { depends_on: string[] } }
  | { DynamicMultiSelect: { depends_on: string[] } }
  | { AudioRecorder: Record<string, never> }
  | { ListEditor: Record<string, never> }
  | { Auto: Record<string, never> };

export interface InputSpec {
  name: string;
  type: string;
  ui_component: UIComponent;
  default?: unknown;
  required: boolean;
  description?: string;
  env_var?: string;
  env_value?: string;
}

// narrowed InputSpec for DynamicSelect inputs only
export type DynamicSelectInputSpec = Omit<InputSpec, "ui_component"> & {
  ui_component: Extract<UIComponent, { DynamicSelect: unknown }>;
};

// narrowed InputSpec for DynamicMultiSelect inputs only
export type DynamicMultiSelectInputSpec = Omit<InputSpec, "ui_component"> & {
  ui_component: Extract<UIComponent, { DynamicMultiSelect: unknown }>;
};

export interface OutputSpec {
  name: string;
  type: string;
  description?: string;
}

export interface NodeMetadata {
  name: string;
  title: string;
  category: string;
  description: string;
  inputs: InputSpec[];
  outputs: OutputSpec[];
  script_source?: ScriptSource;
  /** if true, this node type's ports depend on its input values and the
   *  frontend should fetch per-instance ports via the spec endpoint */
  has_dynamic_spec?: boolean;
}

// helper to create default UI state
export const defaultNodeUIState = (): NodeUIState => ({
  isRunning: false,
  progress: 0,
});

// toast Types
export type ToastType = "error" | "info";

export interface Toast {
  id: string;
  type: ToastType;
  title: string;
  message: string;
  removing?: boolean;
  borderColor?: string;
}
