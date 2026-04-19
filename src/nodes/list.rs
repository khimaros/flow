use crate::node::{DataType, InputSpec, Node, NodeContext, OutputSpec, UIComponent};
use crate::value::Value;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::collections::BTreeMap;

pub struct FlattenNode;

#[async_trait]
impl Node for FlattenNode {
    fn name(&self) -> &str {
        "Flatten"
    }

    fn title(&self) -> &str {
        "Flatten"
    }

    fn category(&self) -> &str {
        "Data"
    }

    fn description(&self) -> &str {
        "Concatenate a list of lists into a single flat list (one level deep)"
    }

    fn inputs(&self) -> Vec<InputSpec> {
        vec![InputSpec {
            name: "input".to_string(),
            r#type: DataType::List,
            ui_component: UIComponent::Auto {},
            required: true,
            description: Some("list of lists to flatten one level".to_string()),
            ..Default::default()
        }]
    }

    fn outputs(&self) -> Vec<OutputSpec> {
        vec![
            OutputSpec {
                name: "output".to_string(),
                r#type: DataType::List,
                description: Some("flattened list".to_string()),
            },
            OutputSpec {
                name: "count".to_string(),
                r#type: DataType::Integer,
                description: Some("total number of items after flattening".to_string()),
            },
        ]
    }

    async fn execute(
        &self,
        inputs: BTreeMap<String, Value>,
        _ctx: NodeContext,
    ) -> Result<BTreeMap<String, Value>> {
        let items = match inputs.get("input") {
            Some(Value::Array(arr)) => arr,
            Some(Value::Null) | None => {
                let mut out = BTreeMap::new();
                out.insert("output".to_string(), Value::Array(vec![]));
                out.insert("count".to_string(), Value::Integer(0));
                return Ok(out);
            }
            Some(other) => return Err(anyhow!("Flatten: 'input' must be a list, got {:?}", other)),
        };

        let mut flat = Vec::new();
        for item in items {
            match item {
                Value::Array(inner) => flat.extend(inner.iter().cloned()),
                other => flat.push(other.clone()),
            }
        }

        let mut out = BTreeMap::new();
        let count = flat.len() as i64;
        out.insert("output".to_string(), Value::Array(flat));
        out.insert("count".to_string(), Value::Integer(count));
        Ok(out)
    }
}

/// pairs elements from two lists into {a, b} objects by index
pub struct ZipNode;

#[async_trait]
impl Node for ZipNode {
    fn name(&self) -> &str {
        "Zip"
    }

    fn title(&self) -> &str {
        "Zip"
    }

    fn category(&self) -> &str {
        "Data"
    }

    fn description(&self) -> &str {
        "Pair elements from two lists by index into {a, b} objects"
    }

    fn inputs(&self) -> Vec<InputSpec> {
        vec![
            InputSpec {
                name: "a".to_string(),
                r#type: DataType::List,
                ui_component: UIComponent::Auto {},
                required: true,
                description: Some("first list".to_string()),
                ..Default::default()
            },
            InputSpec {
                name: "b".to_string(),
                r#type: DataType::List,
                ui_component: UIComponent::Auto {},
                required: true,
                description: Some("second list".to_string()),
                ..Default::default()
            },
        ]
    }

    fn outputs(&self) -> Vec<OutputSpec> {
        vec![
            OutputSpec {
                name: "output".to_string(),
                r#type: DataType::List,
                description: Some("list of {a, b} pairs".to_string()),
            },
            OutputSpec {
                name: "count".to_string(),
                r#type: DataType::Integer,
                description: Some("number of pairs (length of shorter list)".to_string()),
            },
        ]
    }

    async fn execute(
        &self,
        inputs: BTreeMap<String, Value>,
        _ctx: NodeContext,
    ) -> Result<BTreeMap<String, Value>> {
        let list_a = match inputs.get("a") {
            Some(Value::Array(arr)) => arr.clone(),
            Some(Value::Null) | None => vec![],
            Some(other) => return Err(anyhow!("Zip: 'a' must be a list, got {:?}", other)),
        };
        let list_b = match inputs.get("b") {
            Some(Value::Array(arr)) => arr.clone(),
            Some(Value::Null) | None => vec![],
            Some(other) => return Err(anyhow!("Zip: 'b' must be a list, got {:?}", other)),
        };

        let pairs: Vec<Value> = list_a
            .into_iter()
            .zip(list_b)
            .map(|(a, b)| {
                let mut obj = BTreeMap::new();
                obj.insert("a".to_string(), a);
                obj.insert("b".to_string(), b);
                Value::Object(obj)
            })
            .collect();

        let mut out = BTreeMap::new();
        let count = pairs.len() as i64;
        out.insert("output".to_string(), Value::Array(pairs));
        out.insert("count".to_string(), Value::Integer(count));
        Ok(out)
    }
}
