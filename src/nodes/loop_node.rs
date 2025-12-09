use crate::engine::NodeRegistry;
use crate::node::{DataType, InputSpec, Node, NodeContext, OutputSpec, SelectOption, UIComponent};
use crate::nodes::register_all;
use crate::value::Value;
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use std::collections::BTreeMap;

const ON_FAILURE_FAIL: &str = "fail";
const ON_FAILURE_SKIP: &str = "skip";
const ON_FAILURE_NULL: &str = "null";

pub struct LoopNode;

impl LoopNode {
    fn fresh_registry() -> NodeRegistry {
        let mut registry = NodeRegistry::new();
        register_all(&mut registry, true);
        registry
    }

    fn parse_extra_inputs(val: Option<&Value>) -> Result<BTreeMap<String, Value>> {
        match val {
            Some(Value::Object(obj)) => Ok(obj.clone()),
            Some(Value::String(s)) if !s.trim().is_empty() => {
                let parsed: serde_json::Value = serde_json::from_str(s)
                    .context("Loop: failed to parse 'extra_inputs' as JSON")?;
                let val: Value = serde_json::from_value(parsed)?;
                match val {
                    Value::Object(obj) => Ok(obj),
                    _ => Err(anyhow!("Loop: 'extra_inputs' must be a JSON object")),
                }
            }
            _ => Ok(BTreeMap::new()),
        }
    }
}

#[async_trait]
impl Node for LoopNode {
    fn name(&self) -> &str {
        "Loop"
    }

    fn title(&self) -> &str {
        "Loop"
    }

    fn category(&self) -> &str {
        "Flow Control"
    }

    fn description(&self) -> &str {
        "Iterates over a list, invoking a target node once per item."
    }

    fn inputs(&self) -> Vec<InputSpec> {
        vec![
            InputSpec {
                name: "items".to_string(),
                r#type: DataType::List,
                ui_component: UIComponent::Auto {},
                required: true,
                description: Some("List of items to iterate over.".to_string()),
                ..Default::default()
            },
            InputSpec {
                name: "node_type".to_string(),
                r#type: DataType::String,
                ui_component: UIComponent::DynamicSelect {
                    depends_on: vec![],
                },
                required: true,
                description: Some("The node type to invoke once per item.".to_string()),
                ..Default::default()
            },
            InputSpec {
                name: "item_input".to_string(),
                r#type: DataType::String,
                ui_component: UIComponent::DynamicSelect {
                    depends_on: vec!["node_type".to_string()],
                },
                required: true,
                description: Some(
                    "Inner-node input that receives each item from the list.".to_string(),
                ),
                ..Default::default()
            },
            InputSpec {
                name: "item_output".to_string(),
                r#type: DataType::String,
                ui_component: UIComponent::DynamicSelect {
                    depends_on: vec!["node_type".to_string()],
                },
                required: false,
                description: Some(
                    "Inner-node output to collect. If omitted, the full output map is collected."
                        .to_string(),
                ),
                ..Default::default()
            },
            InputSpec {
                name: "item_path".to_string(),
                r#type: DataType::String,
                ui_component: UIComponent::Auto {},
                required: false,
                description: Some(
                    "Optional JSON pointer (e.g. '/link') to extract a field from each item before passing to the inner node. If empty, the whole item is passed."
                        .to_string(),
                ),
                ..Default::default()
            },
            InputSpec {
                name: "limit".to_string(),
                r#type: DataType::Integer,
                ui_component: UIComponent::Auto {},
                default: Some(Value::Integer(0)),
                required: false,
                description: Some(
                    "Maximum number of items to process (0 = no limit).".to_string(),
                ),
                ..Default::default()
            },
            InputSpec {
                name: "delay".to_string(),
                r#type: DataType::Integer,
                ui_component: UIComponent::Auto {},
                default: Some(Value::Integer(0)),
                required: false,
                description: Some(
                    "Delay in milliseconds between iterations for rate limiting (0 = no delay)."
                        .to_string(),
                ),
                ..Default::default()
            },
            InputSpec {
                name: "extra_inputs".to_string(),
                r#type: DataType::Object,
                ui_component: UIComponent::TextArea {},
                required: false,
                description: Some(
                    "Additional inputs (JSON object) passed to every inner-node invocation."
                        .to_string(),
                ),
                ..Default::default()
            },
            InputSpec {
                name: "include_item".to_string(),
                r#type: DataType::Boolean,
                ui_component: UIComponent::Auto {},
                default: Some(Value::Boolean(false)),
                required: false,
                description: Some(
                    "If true, each result is wrapped as {item, output} so the source item is preserved alongside the inner-node output."
                        .to_string(),
                ),
                ..Default::default()
            },
            InputSpec {
                name: "on_failure".to_string(),
                r#type: DataType::String,
                ui_component: UIComponent::Select {
                    options: vec![
                        SelectOption {
                            value: ON_FAILURE_FAIL.to_string(),
                            label: Some("fail".to_string()),
                        },
                        SelectOption {
                            value: ON_FAILURE_SKIP.to_string(),
                            label: Some("skip item".to_string()),
                        },
                        SelectOption {
                            value: ON_FAILURE_NULL.to_string(),
                            label: Some("null result".to_string()),
                        },
                    ],
                },
                default: Some(Value::String(ON_FAILURE_FAIL.to_string())),
                required: false,
                description: Some("How to handle inner-node failures.".to_string()),
                ..Default::default()
            },
        ]
    }

    fn outputs(&self) -> Vec<OutputSpec> {
        vec![
            OutputSpec {
                name: "results".to_string(),
                r#type: DataType::List,
                description: Some("List of results, one per surviving item.".to_string()),
            },
            OutputSpec {
                name: "count".to_string(),
                r#type: DataType::Integer,
                description: Some("Number of results produced.".to_string()),
            },
        ]
    }

    async fn get_options(
        &self,
        input_name: &str,
        inputs: BTreeMap<String, Value>,
    ) -> Result<Vec<SelectOption>> {
        let registry = Self::fresh_registry();

        if input_name == "node_type" {
            let mut meta = registry.list_metadata();
            // skip Loop itself to avoid surprise self-recursion
            meta.retain(|m| m.name != "Loop");
            return Ok(meta
                .into_iter()
                .map(|m| SelectOption {
                    value: m.name.clone(),
                    label: Some(format!("{} ({})", m.title, m.name)),
                })
                .collect());
        }

        let node_type = inputs
            .get("node_type")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if node_type.is_empty() {
            return Ok(vec![]);
        }

        let inner = match registry.create(node_type) {
            Some(n) => n,
            None => return Ok(vec![]),
        };

        match input_name {
            "item_input" => Ok(inner
                .inputs()
                .into_iter()
                .map(|spec| SelectOption {
                    value: spec.name.clone(),
                    label: Some(spec.name),
                })
                .collect()),
            "item_output" => Ok(inner
                .outputs()
                .into_iter()
                .map(|spec| SelectOption {
                    value: spec.name.clone(),
                    label: Some(spec.name),
                })
                .collect()),
            _ => Ok(vec![]),
        }
    }

    async fn execute(
        &self,
        inputs: BTreeMap<String, Value>,
        ctx: NodeContext,
    ) -> Result<BTreeMap<String, Value>> {
        let mut items = match inputs.get("items") {
            Some(Value::Array(arr)) => arr.clone(),
            Some(Value::Null) | None => vec![],
            Some(other) => return Err(anyhow!("Loop: 'items' must be a list, got {:?}", other)),
        };

        let limit = inputs.get("limit").and_then(|v| v.as_i64()).unwrap_or(0) as usize;
        if limit > 0 {
            items.truncate(limit);
        }

        let node_type = inputs
            .get("node_type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Loop: 'node_type' is required"))?
            .to_string();

        if node_type == "Loop" {
            return Err(anyhow!("Loop: cannot iterate over Loop itself"));
        }

        let item_input_name = inputs
            .get("item_input")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Loop: 'item_input' is required"))?
            .to_string();

        let item_output = inputs
            .get("item_output")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let item_path = inputs
            .get("item_path")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let include_item = inputs
            .get("include_item")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let on_failure = inputs
            .get("on_failure")
            .and_then(|v| v.as_str())
            .unwrap_or(ON_FAILURE_FAIL)
            .to_string();

        let delay_ms = inputs.get("delay").and_then(|v| v.as_i64()).unwrap_or(0) as u64;

        let extra_inputs = Self::parse_extra_inputs(inputs.get("extra_inputs"))?;

        let registry = Self::fresh_registry();
        if registry.create(&node_type).is_none() {
            return Err(anyhow!("Loop: unknown inner node type '{}'", node_type));
        }

        let total = items.len();
        let mut results = Vec::with_capacity(total);

        for (idx, item) in items.into_iter().enumerate() {
            if ctx.is_cancelled() {
                return Err(anyhow!("Loop: job cancelled"));
            }

            let progress = if total == 0 {
                1.0
            } else {
                idx as f32 / total as f32
            };
            ctx.report_progress(progress, Some(format!("iteration {}/{}", idx + 1, total)));

            let inner_node = registry.create(&node_type).unwrap();

            let item_value = match &item_path {
                Some(path) => {
                    let serde_val: serde_json::Value = serde_json::to_value(&item)?;
                    let plucked = serde_val
                        .pointer(path)
                        .cloned()
                        .unwrap_or(serde_json::Value::Null);
                    serde_json::from_value(plucked)?
                }
                None => item.clone(),
            };

            let mut inner_inputs: BTreeMap<String, Value> = extra_inputs
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            inner_inputs.insert(item_input_name.clone(), item_value);

            let inner_result = inner_node.execute(inner_inputs, ctx.clone()).await;

            let output_val = match inner_result {
                Ok(mut outputs) => match &item_output {
                    Some(name) => outputs.remove(name).unwrap_or(Value::Null),
                    None => {
                        let mut obj = BTreeMap::new();
                        for (k, v) in outputs {
                            obj.insert(k, v);
                        }
                        Value::Object(obj)
                    }
                },
                Err(e) => match on_failure.as_str() {
                    ON_FAILURE_SKIP => continue,
                    ON_FAILURE_NULL => Value::Null,
                    _ => return Err(anyhow!("Loop: iteration {} failed: {:#}", idx, e)),
                },
            };

            let entry = if include_item {
                let mut obj = BTreeMap::new();
                obj.insert("item".to_string(), item);
                obj.insert("output".to_string(), output_val);
                Value::Object(obj)
            } else {
                output_val
            };
            results.push(entry.clone());

            // stream the running list so downstream / UI can render incrementally
            ctx.emit_partial_output(
                "results",
                Value::Array(vec![entry]),
                Value::Array(results.clone()),
            );
            let count_val = Value::Integer(results.len() as i64);
            ctx.emit_partial_output("count", count_val.clone(), count_val);

            // rate limiting delay between iterations
            if delay_ms > 0 && idx + 1 < total {
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            }
        }

        ctx.report_progress(1.0, Some("done".to_string()));

        let mut out = BTreeMap::new();
        let count = results.len() as i64;
        out.insert("results".to_string(), Value::Array(results));
        out.insert("count".to_string(), Value::Integer(count));
        Ok(out)
    }
}
