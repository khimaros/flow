use crate::node::{DataType, InputSpec, Node, NodeContext, OutputSpec, ScriptSource, SelectOption};
use crate::scripting::{
    create_engine_for_file, language_from_filename, ScriptContext, ScriptEngine,
};
use crate::value::Value;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde_json;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

#[derive(Clone)]
pub struct ScriptDefinedNode {
    id: String,
    title: String,
    category: String,
    description: String,
    inputs: Vec<InputSpec>,
    outputs: Vec<OutputSpec>,
    script_src: String,
    script_language: String,
    engine: Arc<dyn ScriptEngine>,
}

impl ScriptDefinedNode {
    pub fn new(script_content: &str, filename: &str) -> Result<Self> {
        let engine = create_engine_for_file(filename)?;
        let spec = engine.parse_spec(script_content, filename)?;

        let inputs = spec
            .inputs
            .into_iter()
            .map(convert_input_spec)
            .collect::<Result<Vec<_>>>()?;
        let outputs = spec
            .outputs
            .into_iter()
            .map(convert_output_spec)
            .collect::<Result<Vec<_>>>()?;

        let script_language = language_from_filename(filename).to_string();

        Ok(Self {
            id: spec.name,
            title: spec.title,
            category: spec.category,
            description: spec.description,
            inputs,
            outputs,
            script_src: script_content.to_string(),
            script_language,
            engine: Arc::from(engine),
        })
    }

    /// get options for a dynamic select input
    pub fn get_options(
        &self,
        input_name: &str,
        inputs: BTreeMap<String, Value>,
    ) -> Result<Vec<SelectOption>> {
        // convert inputs to JSON
        let json_inputs: HashMap<String, serde_json::Value> = inputs
            .into_iter()
            .map(|(k, v)| {
                let json_val = serde_json::to_value(&v).unwrap_or(serde_json::Value::Null);
                (k, json_val)
            })
            .collect();

        self.engine
            .get_options(&self.script_src, input_name, json_inputs)
    }
}

fn parse_data_type(s: &str) -> Result<DataType> {
    match s {
        "string" => Ok(DataType::String),
        "integer" => Ok(DataType::Integer),
        "float" => Ok(DataType::Float),
        "boolean" => Ok(DataType::Boolean),
        "object" => Ok(DataType::Object),
        "list" => Ok(DataType::List),
        "any" => Ok(DataType::Any),
        "file" => Ok(DataType::File),
        _ => Err(anyhow!("unknown data type: {}", s)),
    }
}

fn convert_input_spec(spec: crate::scripting::ScriptInputSpec) -> Result<InputSpec> {
    let default = spec
        .default
        .map(|v| serde_json::from_value::<Value>(v).unwrap_or(Value::Null));

    Ok(InputSpec {
        name: spec.name.clone(),
        r#type: parse_data_type(&spec.data_type)?,
        ui_component: spec.ui_component,
        default,
        required: spec.required,
        description: spec.description,
        env_var: spec.env_var,
        env_value: spec.env_value,
    })
}

fn convert_output_spec(spec: crate::scripting::ScriptOutputSpec) -> Result<OutputSpec> {
    Ok(OutputSpec {
        name: spec.name,
        r#type: parse_data_type(&spec.data_type)?,
        description: spec.description,
    })
}

#[async_trait]
impl Node for ScriptDefinedNode {
    fn name(&self) -> &str {
        &self.id
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn category(&self) -> &str {
        &self.category
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn inputs(&self) -> Vec<InputSpec> {
        self.inputs.clone()
    }

    fn outputs(&self) -> Vec<OutputSpec> {
        self.outputs.clone()
    }

    fn script_source(&self) -> Option<ScriptSource> {
        Some(ScriptSource {
            language: self.script_language.clone(),
            source: self.script_src.clone(),
        })
    }

    async fn get_options(
        &self,
        input_name: &str,
        inputs: BTreeMap<String, Value>,
    ) -> Result<Vec<SelectOption>> {
        let node_self = self.clone(); // Clone self for the blocking task
        let input_name = input_name.to_string();

        let result =
            tokio::task::spawn_blocking(move || node_self.get_options(&input_name, inputs))
                .await??;

        Ok(result)
    }

    async fn execute(
        &self,
        inputs: BTreeMap<String, Value>,
        ctx: NodeContext,
    ) -> Result<BTreeMap<String, Value>> {
        // convert inputs to JSON
        let json_inputs: HashMap<String, serde_json::Value> = inputs
            .into_iter()
            .map(|(k, v)| {
                let json_val = serde_json::to_value(&v).unwrap_or(serde_json::Value::Null);
                (k, json_val)
            })
            .collect();

        // clone what we need for the blocking task
        let engine = self.engine.clone();
        let script_src = self.script_src.clone();

        // create ScriptContext from NodeContext
        let cancelled = ctx.cancelled_flag();
        let po_tx = ctx.partial_output_tx();
        let ctx_clone = ctx.clone();
        let script_ctx = Arc::new(ScriptContext::new(
            cancelled,
            move |progress, message| {
                ctx_clone.report_progress(progress, message);
            },
            move |output_name, delta, accumulated| {
                if let Some(tx) = &po_tx {
                    let _ = tx.try_send(crate::node::PartialOutputUpdate {
                        output_name,
                        delta,
                        accumulated,
                    });
                }
            },
        ));

        // execute on a blocking thread and race it against the cancellation
        // token. if the job is cancelled, we return immediately — the blocking
        // thread cannot be aborted, but the cancelled flag is shared via
        // script_ctx so cancellation-aware native fns (e.g. rhai http_request)
        // will also observe it and abort their in-flight work.
        let handle = tokio::task::spawn_blocking(move || {
            engine.execute(&script_src, json_inputs, script_ctx)
        });
        let cancel_token = ctx.cancel_token();
        let result = tokio::select! {
            joined = handle => joined??,
            _ = cancel_token.cancelled() => {
                return Err(anyhow!("script execution cancelled"));
            }
        };

        // convert outputs back to Value
        let outputs: BTreeMap<String, Value> = result
            .into_iter()
            .map(|(k, v)| {
                let val: Value = serde_json::from_value(v).unwrap_or(Value::Null);
                (k, val)
            })
            .collect();

        Ok(outputs)
    }
}
