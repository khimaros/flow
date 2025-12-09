use crate::node::{DataType, InputSpec, Node, NodeContext, OutputSpec, UIComponent};
use crate::value::Value;
use anyhow::Result;
use async_trait::async_trait;
use std::collections::BTreeMap;

pub struct RandomIntegerNode;

#[async_trait]
impl Node for RandomIntegerNode {
    fn name(&self) -> &str {
        "RandomInteger"
    }

    fn title(&self) -> &str {
        "Random Integer"
    }

    fn category(&self) -> &str {
        "Math"
    }

    fn description(&self) -> &str {
        "Generates a random number."
    }

    fn inputs(&self) -> Vec<InputSpec> {
        vec![
            InputSpec {
                name: "min".to_string(),
                r#type: DataType::Integer,
                ui_component: UIComponent::Auto {},
                default: Some(Value::Integer(0)),
                required: false,
                description: Some("Minimum value (inclusive, default 0)".to_string()),
                ..Default::default()
            },
            InputSpec {
                name: "max".to_string(),
                r#type: DataType::Integer,
                ui_component: UIComponent::Auto {},
                default: Some(Value::Integer(100)),
                required: false,
                description: Some("Maximum value (inclusive, default 100)".to_string()),
                ..Default::default()
            },
        ]
    }

    fn outputs(&self) -> Vec<OutputSpec> {
        vec![OutputSpec {
            name: "output".to_string(),
            r#type: DataType::Integer,
            description: Some("the generated random number".to_string()),
        }]
    }

    async fn execute(
        &self,
        inputs: BTreeMap<String, Value>,
        _ctx: NodeContext,
    ) -> Result<BTreeMap<String, Value>> {
        let min_val = inputs
            .get("min")
            .and_then(|v| {
                v.as_i64()
                    .or_else(|| v.as_str().and_then(|s| s.parse::<i64>().ok()))
            })
            .unwrap_or(0);
        let max_val = inputs
            .get("max")
            .and_then(|v| {
                v.as_i64()
                    .or_else(|| v.as_str().and_then(|s| s.parse::<i64>().ok()))
            })
            .unwrap_or(100);
        let mut rng = rand::thread_rng();

        use rand::Rng;
        let rand_int = rng.gen_range(min_val..=max_val);
        let output_value = Value::Integer(rand_int);

        let mut outputs = BTreeMap::new();
        outputs.insert("output".to_string(), output_value);
        Ok(outputs)
    }
}
