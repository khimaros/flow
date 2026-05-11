use crate::node::{NodeContext, SelectOption, UIComponent};
use crate::value::Value;
use anyhow::{anyhow, Result};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub mod lua;
#[cfg(feature = "python")]
pub mod python;
pub mod rhai;
#[cfg(feature = "typescript")]
pub mod typescript;

/// context for script execution providing cancellation and progress reporting
pub struct ScriptContext {
    /// flag to check if execution should be cancelled
    pub cancelled: Arc<AtomicBool>,
    /// callback to report progress (0.0 to 1.0) with optional message
    pub report_progress: Arc<dyn Fn(f32, Option<String>) + Send + Sync>,
    /// callback to emit partial output during streaming execution
    pub emit_partial_output: Arc<dyn Fn(String, Value, Value) + Send + Sync>,
    /// the parent NodeContext, when execution was kicked off from a Node.
    /// available to host fns (e.g. `dispatch_node`) that need to invoke other
    /// nodes with propagated cancellation and partial-output forwarding.
    /// None in non-node call sites (parse_spec, get_options, tests).
    pub node_ctx: Option<NodeContext>,
}

impl ScriptContext {
    pub fn new(
        cancelled: Arc<AtomicBool>,
        report_progress: impl Fn(f32, Option<String>) + Send + Sync + 'static,
        emit_partial_output: impl Fn(String, Value, Value) + Send + Sync + 'static,
    ) -> Self {
        Self {
            cancelled,
            report_progress: Arc::new(report_progress),
            emit_partial_output: Arc::new(emit_partial_output),
            node_ctx: None,
        }
    }

    /// attach a NodeContext (builder pattern). enables `dispatch_node` in
    /// the rhai engine to invoke other nodes with proper context forwarding.
    pub fn with_node_ctx(mut self, node_ctx: NodeContext) -> Self {
        self.node_ctx = Some(node_ctx);
        self
    }

    /// create a no-op context for testing or when progress isn't needed
    pub fn noop() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            report_progress: Arc::new(|_, _| {}),
            emit_partial_output: Arc::new(|_, _, _| {}),
            node_ctx: None,
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

/// parsed script specification containing node metadata
#[derive(Debug, Clone)]
pub struct ScriptSpec {
    pub name: String,
    pub title: String,
    pub category: String,
    pub description: String,
    pub inputs: Vec<ScriptInputSpec>,
    pub outputs: Vec<ScriptOutputSpec>,
}

#[derive(Debug, Clone)]
pub struct ScriptInputSpec {
    pub name: String,
    pub data_type: String,
    pub ui_component: UIComponent,
    pub default: Option<JsonValue>,
    pub required: bool,
    pub description: Option<String>,
    pub env_var: Option<String>,
    pub env_value: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ScriptOutputSpec {
    pub name: String,
    pub data_type: String,
    pub description: Option<String>,
}

/// trait for script engines that can execute user-defined scripts
pub trait ScriptEngine: Send + Sync {
    /// get the language name for this engine
    fn language(&self) -> &str;

    /// parse the script and extract the spec metadata
    fn parse_spec(&self, script: &str, filename: &str) -> Result<ScriptSpec>;

    /// execute the script with given inputs and return outputs
    /// the context provides cancellation checking and progress reporting
    fn execute(
        &self,
        script: &str,
        inputs: HashMap<String, JsonValue>,
        ctx: Arc<ScriptContext>,
    ) -> Result<HashMap<String, JsonValue>>;

    /// get options for a dynamic select input
    /// returns a list of (value, label) pairs, or just values if labels match
    fn get_options(
        &self,
        script: &str,
        input_name: &str,
        inputs: HashMap<String, JsonValue>,
    ) -> Result<Vec<SelectOption>>;
}

/// factory function to create the appropriate script engine based on file extension
pub fn create_engine_for_file(filename: &str) -> Result<Box<dyn ScriptEngine>> {
    if filename.ends_with(".rhai") {
        Ok(Box::new(rhai::RhaiEngine::new()))
    } else if filename.ends_with(".py") {
        #[cfg(feature = "python")]
        {
            Ok(Box::new(python::PythonEngine::new()))
        }
        #[cfg(not(feature = "python"))]
        {
            Err(anyhow!(
                "Python support is not enabled. Build with --features python"
            ))
        }
    } else if filename.ends_with(".lua") {
        Ok(Box::new(lua::LuaEngine::new()))
    } else if filename.ends_with(".ts") {
        #[cfg(feature = "typescript")]
        {
            Ok(Box::new(typescript::TypeScriptEngine::new()))
        }
        #[cfg(not(feature = "typescript"))]
        {
            Err(anyhow!(
                "TypeScript support is not enabled. Build with --features typescript"
            ))
        }
    } else {
        Err(anyhow!("unsupported script file extension: {}", filename))
    }
}

/// get the language identifier from filename
pub fn language_from_filename(filename: &str) -> &'static str {
    if filename.ends_with(".rhai") {
        "rhai"
    } else if filename.ends_with(".py") {
        "python"
    } else if filename.ends_with(".lua") {
        "lua"
    } else if filename.ends_with(".ts") {
        #[cfg(feature = "typescript")]
        {
            "typescript"
        }
        #[cfg(not(feature = "typescript"))]
        {
            "unknown"
        }
    } else {
        "unknown"
    }
}

// ============================================================================
// common Parsing Logic (DRY)
// ============================================================================

pub(super) fn parse_ui_component(
    ui_type: &str,
    map: &serde_json::Map<String, JsonValue>,
) -> Result<UIComponent> {
    match ui_type {
        "text" => Ok(UIComponent::Text {}),
        "textarea" => Ok(UIComponent::TextArea {}),
        "number" => Ok(UIComponent::Number {}),
        "checkbox" => Ok(UIComponent::Checkbox {}),
        "boolean_select" => Ok(UIComponent::BooleanSelect {}),
        "password" => Ok(UIComponent::Password {}),
        "select" => {
            let options_val = map
                .get("options")
                .ok_or_else(|| anyhow!("'select' UI type requires 'options' array"))?;

            let options_arr = options_val
                .as_array()
                .ok_or_else(|| anyhow!("'options' for 'select' must be an array"))?;

            let select_options = parse_select_options_list(options_arr)?;
            Ok(UIComponent::Select {
                options: select_options,
            })
        }
        "dynamic_select" => {
            let depends_on_val = map
                .get("depends_on")
                .ok_or_else(|| anyhow!("'dynamic_select' UI type requires 'depends_on' array"))?;

            let depends_on_arr = depends_on_val
                .as_array()
                .ok_or_else(|| anyhow!("'depends_on' for 'dynamic_select' must be an array"))?;

            let depends_on = depends_on_arr
                .iter()
                .map(|v| v.as_str().unwrap_or_default().to_string())
                .collect();
            Ok(UIComponent::DynamicSelect { depends_on })
        }
        "dynamic_multi_select" => {
            // depends_on is optional for multi-select (often used for a fixed
            // registry-wide list with no per-input dependencies).
            let depends_on = map
                .get("depends_on")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .map(|v| v.as_str().unwrap_or_default().to_string())
                        .collect()
                })
                .unwrap_or_default();
            Ok(UIComponent::DynamicMultiSelect { depends_on })
        }
        "list_editor" => Ok(UIComponent::ListEditor {}),
        "audio_recorder" => Ok(UIComponent::AudioRecorder {}),
        "auto" => Ok(UIComponent::Auto {}),
        _ => Ok(UIComponent::Auto {}), // default to Auto for unknown UI types
    }
}

pub(super) fn parse_select_options_list(arr: &[JsonValue]) -> Result<Vec<SelectOption>> {
    arr.iter()
        .map(|item| {
            if let Some(s) = item.as_str() {
                Ok(SelectOption {
                    value: s.to_string(),
                    label: None,
                })
            } else if let Some(map) = item.as_object() {
                let value = map
                    .get("value")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow!("select option must have 'value'"))?
                    .to_string();
                let label = map
                    .get("label")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                Ok(SelectOption { value, label })
            } else {
                Err(anyhow!(
                    "Select options must be strings or objects with value/label"
                ))
            }
        })
        .collect()
}

pub(super) fn parse_input_spec_from_json(val: &JsonValue) -> Result<ScriptInputSpec> {
    let map = val
        .as_object()
        .ok_or_else(|| anyhow!("input spec must be an object"))?;

    let name = map
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("missing input name"))?
        .to_string();

    let data_type = map
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("missing input type"))?
        .to_string();

    // when `ui` is omitted, fall back to "auto" so the component is inferred
    // from the data type (List -> ListEditor, Integer -> Number, etc.)
    let ui_component_str = map.get("ui").and_then(|v| v.as_str()).unwrap_or("auto");

    // pass the whole map to parse_ui_component so it can find 'options' or 'depends_on'
    let ui_component = parse_ui_component(ui_component_str, map)?;

    let required = map
        .get("required")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let description = map
        .get("description")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let default = map.get("default").cloned();

    let env_var = map
        .get("env_var")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Ok(ScriptInputSpec {
        name,
        data_type,
        ui_component,
        default,
        required,
        description,
        env_var,
        env_value: None, // resolved later with auto-convention support
    })
}

pub(super) fn parse_output_spec_from_json(val: &JsonValue) -> Result<ScriptOutputSpec> {
    let map = val
        .as_object()
        .ok_or_else(|| anyhow!("output spec must be an object"))?;

    let name = map
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("missing output name"))?
        .to_string();

    let data_type = map
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("missing output type"))?
        .to_string();

    let description = map
        .get("description")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Ok(ScriptOutputSpec {
        name,
        data_type,
        description,
    })
}

pub(super) fn parse_spec_from_json(value: JsonValue) -> Result<ScriptSpec> {
    let map = value
        .as_object()
        .ok_or_else(|| anyhow!("spec must return an object"))?;

    let name = map
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("missing 'name' in spec"))?
        .to_string();

    let title = map
        .get("title")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("missing 'title' in spec"))?
        .to_string();

    let category = map
        .get("category")
        .and_then(|v| v.as_str())
        .unwrap_or("User")
        .to_string();

    let description = map
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let inputs_arr = map
        .get("inputs")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("missing 'inputs' array in spec"))?;

    let outputs_arr = map
        .get("outputs")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("missing 'outputs' array in spec"))?;

    let inputs = inputs_arr
        .iter()
        .map(parse_input_spec_from_json)
        .collect::<Result<Vec<_>>>()?;

    let outputs = outputs_arr
        .iter()
        .map(parse_output_spec_from_json)
        .collect::<Result<Vec<_>>>()?;

    Ok(ScriptSpec {
        name,
        title,
        category,
        description,
        inputs,
        outputs,
    })
}
