use crate::node::{DataType, InputSpec, Node, NodeContext, OutputSpec, UIComponent};
use crate::value::Value;
use anyhow::Result;
use async_trait::async_trait;
use std::collections::BTreeMap;
use tracing::debug;

pub struct EchoNode;

#[async_trait]
impl Node for EchoNode {
    fn name(&self) -> &str {
        "Echo"
    }

    fn title(&self) -> &str {
        "Echo"
    }

    fn category(&self) -> &str {
        "I/O"
    }

    fn description(&self) -> &str {
        "Prints a message to the server logs and passes it to the output."
    }

    fn inputs(&self) -> Vec<InputSpec> {
        vec![InputSpec {
            name: "message".to_string(),
            r#type: DataType::Any,
            ui_component: UIComponent::TextArea {},
            default: Some(Value::String("Hello, world!".to_string())),
            required: false,
            description: Some("the value to echo.".to_string()),
            ..Default::default()
        }]
    }

    fn outputs(&self) -> Vec<OutputSpec> {
        vec![OutputSpec {
            name: "output".to_string(),
            r#type: DataType::String,
            description: Some("the echoed value.".to_string()),
        }]
    }

    fn is_stream_passthrough(&self) -> bool {
        true
    }

    async fn execute(
        &self,
        inputs: BTreeMap<String, Value>,
        _ctx: NodeContext,
    ) -> Result<BTreeMap<String, Value>> {
        let msg = inputs.get("message").cloned().unwrap_or(Value::Null);
        debug!(value = ?msg, "echo");
        let mut outputs = BTreeMap::new();
        outputs.insert("output".to_string(), msg);
        Ok(outputs)
    }
}

pub struct ReadNode;

#[async_trait]
impl Node for ReadNode {
    fn name(&self) -> &str {
        "Read"
    }

    fn title(&self) -> &str {
        "Read"
    }

    fn category(&self) -> &str {
        "I/O"
    }

    fn description(&self) -> &str {
        "Reads input from the environment (e.g. CLI stdin or webui prompt)."
    }

    fn inputs(&self) -> Vec<InputSpec> {
        vec![]
    }

    fn outputs(&self) -> Vec<OutputSpec> {
        vec![OutputSpec {
            name: "output".to_string(),
            r#type: DataType::String,
            description: Some("the content from stdin or user input.".to_string()),
        }]
    }

    async fn execute(
        &self,
        inputs: BTreeMap<String, Value>,
        _ctx: NodeContext,
    ) -> Result<BTreeMap<String, Value>> {
        // input value is injected by CLI or webui before execution
        let val = inputs
            .get("input")
            .cloned()
            .unwrap_or(Value::String("".to_string()));
        let mut outputs = BTreeMap::new();
        outputs.insert("output".to_string(), val);
        Ok(outputs)
    }
}
