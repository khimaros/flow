use crate::value::Value;
use anyhow::Result;
use async_trait::async_trait;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;

use serde::{Deserialize, Serialize};

/// progress event sent from nodes during execution
#[derive(Debug, Clone)]
pub struct ProgressUpdate {
    pub progress: f32,
    pub message: Option<String>,
}

/// partial output event sent from nodes during streaming execution
#[derive(Debug, Clone)]
pub struct PartialOutputUpdate {
    pub output_name: String,
    pub delta: Value,
    pub accumulated: Value,
}

/// context passed to node execution providing cancellation and progress reporting
#[derive(Clone)]
pub struct NodeContext {
    cancel_token: CancellationToken,
    cancelled: Arc<AtomicBool>,
    progress_tx: Option<Sender<ProgressUpdate>>,
    partial_output_tx: Option<Sender<PartialOutputUpdate>>,
    terse: bool,
}

impl NodeContext {
    pub fn new(
        cancel_token: CancellationToken,
        progress_tx: Option<Sender<ProgressUpdate>>,
        partial_output_tx: Option<Sender<PartialOutputUpdate>>,
        terse: bool,
    ) -> Self {
        let cancelled = Arc::new(AtomicBool::new(false));

        // spawn a task to update the atomic flag when cancellation is triggered
        let cancelled_clone = cancelled.clone();
        let token_clone = cancel_token.clone();
        tokio::spawn(async move {
            token_clone.cancelled().await;
            cancelled_clone.store(true, Ordering::SeqCst);
        });

        Self {
            cancel_token,
            cancelled,
            progress_tx,
            partial_output_tx,
            terse,
        }
    }

    pub fn is_terse(&self) -> bool {
        self.terse
    }

    /// check if execution has been cancelled
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed) || self.cancel_token.is_cancelled()
    }

    /// get the atomic cancelled flag (for sharing with blocking threads)
    pub fn cancelled_flag(&self) -> Arc<AtomicBool> {
        self.cancelled.clone()
    }

    /// get the cancellation token (for async select! patterns)
    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel_token.clone()
    }

    /// report progress (0.0 to 1.0) with optional message
    pub fn report_progress(&self, progress: f32, message: Option<String>) {
        if let Some(tx) = &self.progress_tx {
            let _ = tx.try_send(ProgressUpdate {
                progress: progress.clamp(0.0, 1.0),
                message,
            });
        }
    }

    /// emit a partial output value during streaming execution
    pub fn emit_partial_output(&self, output_name: &str, delta: Value, accumulated: Value) {
        if let Some(tx) = &self.partial_output_tx {
            let _ = tx.try_send(PartialOutputUpdate {
                output_name: output_name.to_string(),
                delta,
                accumulated,
            });
        }
    }

    /// get the partial output sender (for sharing with blocking threads)
    pub fn partial_output_tx(&self) -> Option<Sender<PartialOutputUpdate>> {
        self.partial_output_tx.clone()
    }
}

/// option for dynamic select inputs
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SelectOption {
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DataType {
    String,
    Integer,
    Float,
    Boolean,
    List,
    Object,
    Any,
    File,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UIComponent {
    Text {},
    TextArea {},
    Select {
        options: Vec<SelectOption>,
    },
    /// dynamic select that fetches options via get_options() function
    /// depends_on lists the input names whose values are needed to fetch options
    DynamicSelect {
        depends_on: Vec<String>,
    },
    /// like DynamicSelect, but for `list`-typed inputs. renders as a
    /// checkbox/chip multi-select; value is a list of selected strings.
    /// options are fetched via get_options() — same mechanism as DynamicSelect.
    DynamicMultiSelect {
        depends_on: Vec<String>,
    },
    Number {},
    Checkbox {},
    BooleanSelect {},
    Password {},
    /// audio recorder/uploader UI component
    AudioRecorder {},
    /// list editor with add/delete/reorder functionality
    ListEditor {},
    /// auto-selects the appropriate UI component based on the DataType
    Auto {},
}

impl UIComponent {
    /// resolves UIComponent::Auto to the appropriate UI component based on DataType
    pub fn resolve_auto(r#type: &DataType) -> Self {
        match r#type {
            DataType::String => UIComponent::Text {},
            DataType::Integer | DataType::Float => UIComponent::Number {},
            DataType::Boolean => UIComponent::BooleanSelect {},
            DataType::List => UIComponent::ListEditor {},
            DataType::Object => UIComponent::TextArea {},
            DataType::Any => UIComponent::Text {},
            DataType::File => UIComponent::Text {},
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputSpec {
    pub name: String,
    pub r#type: DataType,
    pub ui_component: UIComponent,
    pub default: Option<Value>,
    pub required: bool,
    pub description: Option<String>,
    /// optional environment variable name. If set and non-empty at the time the
    /// spec is read, env_value will be populated and the value overrides the
    /// user-provided input at execution time.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub env_var: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub env_value: Option<String>,
}

/// generate the auto-convention env var name: FLOW_<NODE_TYPE>_<INPUT_NAME>
/// (all uppercase, non-alphanumeric chars replaced with underscores)
pub fn auto_env_var_name(node_type: &str, input_name: &str) -> String {
    format!(
        "FLOW_{}_{}",
        node_type.to_uppercase().replace(|c: char| !c.is_ascii_alphanumeric(), "_"),
        input_name.to_uppercase().replace(|c: char| !c.is_ascii_alphanumeric(), "_"),
    )
}

/// resolve the effective env var name and value for an input spec.
/// priority: auto FLOW_<NODE>_<INPUT> (most specific) > explicit env_var alias.
pub fn resolve_env_for_input(node_type: &str, spec: &InputSpec) -> (Option<String>, Option<String>) {
    // try auto-convention first (most specific to this node+input)
    let auto_name = auto_env_var_name(node_type, &spec.name);
    let auto_val = std::env::var(&auto_name).ok().filter(|s| !s.is_empty());
    if auto_val.is_some() {
        return (Some(auto_name), auto_val);
    }

    // fall back to explicit env_var alias (shared across nodes)
    if let Some(env_name) = &spec.env_var {
        let val = std::env::var(env_name).ok().filter(|s| !s.is_empty());
        if val.is_some() {
            return (Some(env_name.clone()), val);
        }
    }

    // return explicit env_var with no value if specified
    (spec.env_var.clone(), None)
}

impl Default for InputSpec {
    fn default() -> Self {
        Self {
            name: String::new(),
            r#type: DataType::Any,
            ui_component: UIComponent::Auto {},
            default: None,
            required: false,
            description: None,
            env_var: None,
            env_value: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputSpec {
    pub name: String,
    pub r#type: DataType,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptSource {
    pub language: String,
    pub source: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NodeMetadata {
    pub name: String,
    pub title: String,
    pub category: String,
    pub description: String,
    pub inputs: Vec<InputSpec>,
    pub outputs: Vec<OutputSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub script_source: Option<ScriptSource>,
    /// when true, the frontend should fetch per-instance ports via the
    /// /api/workflows/{name}/nodes/{id}/spec endpoint after the user supplies
    /// values for this node's control inputs, because additional ports are
    /// derived from those inputs at runtime.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub has_dynamic_spec: bool,
}

/// returns the effective input specs for a node given its currently-resolved
/// inputs: the static inputs() followed by any dynamic inputs derived by the
/// node's dynamic_spec() (None for nodes with static-only ports).
pub fn effective_inputs(node: &dyn Node, resolved: &BTreeMap<String, Value>) -> Vec<InputSpec> {
    let mut specs = node.inputs();
    if let Some((dyn_in, _)) = node.dynamic_spec(resolved) {
        let existing: std::collections::HashSet<String> =
            specs.iter().map(|s| s.name.clone()).collect();
        for s in dyn_in {
            if !existing.contains(&s.name) {
                specs.push(s);
            }
        }
    }
    specs
}

/// returns the effective output specs for a node given its currently-resolved
/// inputs.
pub fn effective_outputs(node: &dyn Node, resolved: &BTreeMap<String, Value>) -> Vec<OutputSpec> {
    let mut specs = node.outputs();
    if let Some((_, dyn_out)) = node.dynamic_spec(resolved) {
        let existing: std::collections::HashSet<String> =
            specs.iter().map(|s| s.name.clone()).collect();
        for s in dyn_out {
            if !existing.contains(&s.name) {
                specs.push(s);
            }
        }
    }
    specs
}

#[async_trait]
pub trait Node: Send + Sync {
    /// the unique name/identifier of the node type
    fn name(&self) -> &str;

    /// the user-friendly name of the node
    fn title(&self) -> &str;

    /// the category of the node (default: "General")
    fn category(&self) -> &str {
        "General"
    }

    /// a description of what the node does
    fn description(&self) -> &str;

    /// expected inputs and their configuration
    fn inputs(&self) -> Vec<InputSpec>;

    /// expected outputs and their types
    fn outputs(&self) -> Vec<OutputSpec>;

    /// for nodes whose ports depend on their resolved input values (e.g. a
    /// node that derives its ports by parsing a user-supplied script), return
    /// the dynamic input/output specs given the currently-resolved inputs.
    /// the returned inputs/outputs REPLACE the ones from inputs()/outputs()
    /// after the base (control) inputs are preserved by the caller.
    /// default: None (node has static ports).
    fn dynamic_spec(
        &self,
        _resolved_inputs: &BTreeMap<String, Value>,
    ) -> Option<(Vec<InputSpec>, Vec<OutputSpec>)> {
        None
    }

    /// hint to surface in NodeMetadata so the frontend knows this node type
    /// may expose additional ports beyond inputs()/outputs(), and should fetch
    /// per-instance specs via the spec endpoint.
    fn has_dynamic_spec(&self) -> bool {
        false
    }

    /// whether this node passes streaming inputs directly to outputs
    fn is_stream_passthrough(&self) -> bool {
        false
    }

    /// get the script source for scripted nodes (default: None)
    fn script_source(&self) -> Option<ScriptSource> {
        None
    }

    /// get dynamic options for a specific input (default: empty vec)
    async fn get_options(
        &self,
        _input_name: &str,
        _inputs: BTreeMap<String, Value>,
    ) -> Result<Vec<SelectOption>> {
        Ok(vec![])
    }

    /// execute the node logic
    async fn execute(
        &self,
        inputs: BTreeMap<String, Value>,
        ctx: NodeContext,
    ) -> Result<BTreeMap<String, Value>>;
}
