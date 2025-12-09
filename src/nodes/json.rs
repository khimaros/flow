use crate::node::{DataType, InputSpec, Node, NodeContext, OutputSpec, UIComponent};
use crate::value::Value;
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::collections::BTreeMap;

pub struct JsonQueryNode;

#[async_trait]
impl Node for JsonQueryNode {
    fn name(&self) -> &str {
        "JsonQuery"
    }

    fn title(&self) -> &str {
        "JSON Query"
    }

    fn category(&self) -> &str {
        "Data"
    }

    fn description(&self) -> &str {
        "Queries a JSON object using RFC 6901 JSON Pointer syntax."
    }

    fn inputs(&self) -> Vec<InputSpec> {
        vec![
            InputSpec {
                name: "json".to_string(),
                r#type: DataType::Object,
                ui_component: UIComponent::TextArea {},
                default: None,
                required: true,
                description: Some("the JSON object to query.".to_string()),
                ..Default::default()
            },
            InputSpec {
                name: "path".to_string(),
                r#type: DataType::String,
                ui_component: UIComponent::Auto {},
                default: Some(Value::String("/status".to_string())),
                required: true,
                description: Some("the JSON pointer path (e.g., /key/0/subkey).".to_string()),
                ..Default::default()
            },
        ]
    }

    fn outputs(&self) -> Vec<OutputSpec> {
        vec![OutputSpec {
            name: "result".to_string(),
            r#type: DataType::Any,
            description: Some("the query result.".to_string()),
        }]
    }

    async fn execute(
        &self,
        inputs: BTreeMap<String, Value>,
        _ctx: NodeContext,
    ) -> Result<BTreeMap<String, Value>> {
        let json_input = inputs.get("json").context("missing 'json' input")?;
        let path_input = inputs.get("path").context("missing 'path' input")?;

        let path = path_input.as_str().context("'path' must be a string")?;

        // convert internal Value to serde_json::Value
        let mut serde_val: serde_json::Value = serde_json::to_value(json_input)?;

        // if the input was a string, it might be a JSON string that needs parsing
        if let Value::String(s) = json_input {
            if let Ok(parsed) = serde_json::from_str(s) {
                serde_val = parsed;
            }
        }

        let result_val = serde_val.pointer(path).unwrap_or(&serde_json::Value::Null);

        // convert back to internal Value
        let result: Value = serde_json::from_value(result_val.clone())?;

        let mut outputs = BTreeMap::new();
        outputs.insert("result".to_string(), result);
        Ok(outputs)
    }
}
