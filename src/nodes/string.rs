use crate::node::{DataType, InputSpec, Node, NodeContext, OutputSpec, UIComponent};
use crate::value::Value;
use anyhow::{Context, Result};
use async_trait::async_trait;
use regex::Regex;
use std::collections::BTreeMap;
use tera::Tera;

pub struct TemplatizeNode;

#[async_trait]
impl Node for TemplatizeNode {
    fn name(&self) -> &str {
        "Templatize"
    }

    fn title(&self) -> &str {
        "Templatize"
    }

    fn category(&self) -> &str {
        "Data"
    }

    fn description(&self) -> &str {
        "Render a Jinja2/Tera template with the provided context values"
    }

    fn inputs(&self) -> Vec<InputSpec> {
        vec![
            InputSpec {
                name: "template".to_string(),
                r#type: DataType::String,
                ui_component: UIComponent::TextArea {},
                default: Some(Value::String("Hello, {{ name }}!".to_string())),
                required: true,
                description: Some(
                    "Jinja2/Tera template with {{ key }} placeholders, loops, conditionals"
                        .to_string(),
                ),
                ..Default::default()
            },
            InputSpec {
                name: "context".to_string(),
                r#type: DataType::Object,
                ui_component: UIComponent::TextArea {},
                default: Some(Value::String("{\"name\": \"world\"}".to_string())),
                required: true,
                description: Some("object with keys available in the template".to_string()),
                ..Default::default()
            },
        ]
    }

    fn outputs(&self) -> Vec<OutputSpec> {
        vec![OutputSpec {
            name: "output".to_string(),
            r#type: DataType::String,
            description: Some("rendered template".to_string()),
        }]
    }

    async fn execute(
        &self,
        inputs: BTreeMap<String, Value>,
        _ctx: NodeContext,
    ) -> Result<BTreeMap<String, Value>> {
        let template_str = inputs
            .get("template")
            .context("missing 'template' input")?
            .as_str()
            .context("'template' must be a string")?;

        let context_value = inputs.get("context").context("missing 'context' input")?;

        // convert Value to tera::Context
        let tera_context = value_to_tera_context(context_value)?;

        // render template
        let result = Tera::one_off(template_str, &tera_context, false)
            .context("failed to render template")?;

        let mut outputs = BTreeMap::new();
        outputs.insert("output".to_string(), Value::String(result));
        Ok(outputs)
    }
}

fn value_to_tera_context(value: &Value) -> Result<tera::Context> {
    let mut context = tera::Context::new();

    match value {
        Value::Object(map) => {
            for (key, val) in map {
                context.insert(key, &value_to_tera_value(val));
            }
        }
        Value::Array(_) => {
            // make array available as "input" for iteration
            context.insert("input", &value_to_tera_value(value));
        }
        Value::String(s) => {
            // try parsing as JSON
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(s) {
                match parsed {
                    serde_json::Value::Object(map) => {
                        for (key, val) in map {
                            context.insert(&key, &val);
                        }
                    }
                    serde_json::Value::Array(_) => {
                        // JSON array, make available as "input"
                        context.insert("input", &parsed);
                    }
                    _ => {
                        // other JSON value, wrap as "value"
                        context.insert("value", &parsed);
                    }
                }
            } else {
                // plain string, wrap as "value"
                context.insert("value", s);
            }
        }
        _ => {
            // other types, wrap as "value"
            context.insert("value", &value_to_tera_value(value));
        }
    }

    Ok(context)
}

fn value_to_tera_value(value: &Value) -> serde_json::Value {
    match value {
        Value::Null => serde_json::Value::Null,
        Value::Boolean(b) => serde_json::Value::Bool(*b),
        Value::Integer(i) => serde_json::Value::Number((*i).into()),
        Value::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(value_to_tera_value).collect())
        }
        Value::Object(map) => {
            let obj: serde_json::Map<String, serde_json::Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), value_to_tera_value(v)))
                .collect();
            serde_json::Value::Object(obj)
        }
        Value::File(f) => serde_json::Value::String(f.path.clone()),
    }
}

pub struct JoinNode;

#[async_trait]
impl Node for JoinNode {
    fn name(&self) -> &str {
        "Join"
    }

    fn title(&self) -> &str {
        "Join"
    }

    fn category(&self) -> &str {
        "Data"
    }

    fn description(&self) -> &str {
        "Join a list of values into a string with a separator"
    }

    fn inputs(&self) -> Vec<InputSpec> {
        vec![
            InputSpec {
                name: "list".to_string(),
                r#type: DataType::List,
                ui_component: UIComponent::ListEditor {},
                default: None,
                required: true,
                description: Some("list of values to join".to_string()),
                ..Default::default()
            },
            InputSpec {
                name: "separator".to_string(),
                r#type: DataType::String,
                ui_component: UIComponent::Auto {},
                default: Some(Value::String("\\n".to_string())),
                required: false,
                description: Some(
                    "separator between elements (supports \\n, \\t, \\r)".to_string(),
                ),
                ..Default::default()
            },
        ]
    }

    fn outputs(&self) -> Vec<OutputSpec> {
        vec![OutputSpec {
            name: "output".to_string(),
            r#type: DataType::String,
            description: Some("joined string".to_string()),
        }]
    }

    async fn execute(
        &self,
        inputs: BTreeMap<String, Value>,
        _ctx: NodeContext,
    ) -> Result<BTreeMap<String, Value>> {
        let list = inputs.get("list").context("missing 'list' input")?;
        let separator_raw = inputs
            .get("separator")
            .and_then(|v| v.as_str())
            .unwrap_or("\\n");

        // handle common escape sequences
        let separator = separator_raw
            .replace("\\n", "\n")
            .replace("\\t", "\t")
            .replace("\\r", "\r");

        let items: Vec<String> = match list {
            Value::Array(arr) => arr
                .iter()
                .map(|v| match v {
                    Value::String(s) => s.clone(),
                    Value::Integer(i) => i.to_string(),
                    Value::Float(f) => f.to_string(),
                    Value::Boolean(b) => b.to_string(),
                    Value::Null => "".to_string(),
                    other => serde_json::to_string(other).unwrap_or_default(),
                })
                .collect(),
            Value::String(s) => s.lines().map(|l| l.to_string()).collect(),
            _ => vec![],
        };

        let result = items.join(&separator);

        let mut outputs = BTreeMap::new();
        outputs.insert("output".to_string(), Value::String(result));
        Ok(outputs)
    }
}

pub struct SplitNode;

#[async_trait]
impl Node for SplitNode {
    fn name(&self) -> &str {
        "Split"
    }

    fn title(&self) -> &str {
        "Split"
    }

    fn category(&self) -> &str {
        "Data"
    }

    fn description(&self) -> &str {
        "Split a string into a list using a separator"
    }

    fn inputs(&self) -> Vec<InputSpec> {
        vec![
            InputSpec {
                name: "text".to_string(),
                r#type: DataType::String,
                ui_component: UIComponent::TextArea {},
                default: None,
                required: true,
                description: Some("string to split".to_string()),
                ..Default::default()
            },
            InputSpec {
                name: "separator".to_string(),
                r#type: DataType::String,
                ui_component: UIComponent::Auto {},
                default: Some(Value::String("\\n".to_string())),
                required: false,
                description: Some(
                    "separator to split on (supports \\n, \\t, \\r)".to_string(),
                ),
                ..Default::default()
            },
        ]
    }

    fn outputs(&self) -> Vec<OutputSpec> {
        vec![OutputSpec {
            name: "output".to_string(),
            r#type: DataType::List,
            description: Some("list of split parts".to_string()),
        }]
    }

    async fn execute(
        &self,
        inputs: BTreeMap<String, Value>,
        _ctx: NodeContext,
    ) -> Result<BTreeMap<String, Value>> {
        let text = inputs
            .get("text")
            .context("missing 'text' input")?
            .as_str()
            .context("'text' must be a string")?;

        let separator_raw = inputs
            .get("separator")
            .and_then(|v| v.as_str())
            .unwrap_or("\\n");

        // handle common escape sequences
        let separator = separator_raw
            .replace("\\n", "\n")
            .replace("\\t", "\t")
            .replace("\\r", "\r");

        let parts: Vec<Value> = text
            .split(&separator)
            .map(|s| Value::String(s.to_string()))
            .collect();

        let mut outputs = BTreeMap::new();
        outputs.insert("output".to_string(), Value::Array(parts));
        Ok(outputs)
    }
}

pub struct ListToJsonNode;

#[async_trait]
impl Node for ListToJsonNode {
    fn name(&self) -> &str {
        "ListToJson"
    }

    fn title(&self) -> &str {
        "List to JSON"
    }

    fn category(&self) -> &str {
        "Data"
    }

    fn description(&self) -> &str {
        "Convert a list to a JSON object with 'values' key for template iteration"
    }

    fn inputs(&self) -> Vec<InputSpec> {
        vec![InputSpec {
            name: "list".to_string(),
            r#type: DataType::List,
            ui_component: UIComponent::ListEditor {},
            default: None,
            required: true,
            description: Some("list to convert".to_string()),
            ..Default::default()
        }]
    }

    fn outputs(&self) -> Vec<OutputSpec> {
        vec![OutputSpec {
            name: "output".to_string(),
            r#type: DataType::Object,
            description: Some("JSON object with 'values' array".to_string()),
        }]
    }

    async fn execute(
        &self,
        inputs: BTreeMap<String, Value>,
        _ctx: NodeContext,
    ) -> Result<BTreeMap<String, Value>> {
        let list = inputs.get("list").context("missing 'list' input")?;

        // if it's a string, try to parse it as JSON first
        let list_value = if let Value::String(s) = list {
            if let Ok(parsed) = serde_json::from_str::<Value>(s) {
                parsed
            } else {
                // not JSON, maybe it's a newline-separated list?
                // for now, just wrap as is or handle like JoinNode
                list.clone()
            }
        } else {
            list.clone()
        };

        let mut obj = std::collections::BTreeMap::new();
        obj.insert("values".to_string(), list_value);

        let mut outputs = BTreeMap::new();
        outputs.insert("output".to_string(), Value::Object(obj));
        Ok(outputs)
    }
}

pub struct RegexpExtractNode;

#[async_trait]
impl Node for RegexpExtractNode {
    fn name(&self) -> &str {
        "RegexpExtract"
    }

    fn title(&self) -> &str {
        "Regexp Extract"
    }

    fn category(&self) -> &str {
        "Data"
    }

    fn description(&self) -> &str {
        "Extract matches, capture groups, and named groups from text using regular expressions"
    }

    fn inputs(&self) -> Vec<InputSpec> {
        vec![
            InputSpec {
                name: "text".to_string(),
                r#type: DataType::String,
                ui_component: UIComponent::TextArea {},
                default: None,
                required: true,
                description: Some("text to search".to_string()),
                ..Default::default()
            },
            InputSpec {
                name: "pattern".to_string(),
                r#type: DataType::String,
                ui_component: UIComponent::Auto {},
                default: Some(Value::String(r"\w+@\w+\.\w+".to_string())),
                required: true,
                description: Some(
                    "regular expression pattern with optional named groups (?P<name>...)"
                        .to_string(),
                ),
                ..Default::default()
            },
            InputSpec {
                name: "case_sensitive".to_string(),
                r#type: DataType::Boolean,
                ui_component: UIComponent::Auto {},
                default: Some(Value::Boolean(true)),
                required: false,
                description: Some(
                    "make the regex case sensitive (when false, matches are case insensitive)"
                        .to_string(),
                ),
                ..Default::default()
            },
        ]
    }

    fn outputs(&self) -> Vec<OutputSpec> {
        vec![
            OutputSpec {
                name: "matches".to_string(),
                r#type: DataType::List,
                description: Some("list of all matching strings".to_string()),
            },
            OutputSpec {
                name: "groups".to_string(),
                r#type: DataType::List,
                description: Some("list of capture groups for each match".to_string()),
            },
            OutputSpec {
                name: "named".to_string(),
                r#type: DataType::Object,
                description: Some(
                    "object mapping named group names to their matched values".to_string(),
                ),
            },
        ]
    }

    async fn execute(
        &self,
        inputs: BTreeMap<String, Value>,
        _ctx: NodeContext,
    ) -> Result<BTreeMap<String, Value>> {
        let text = inputs
            .get("text")
            .context("missing 'text' input")?
            .as_str()
            .context("'text' must be a string")?;

        let pattern = inputs
            .get("pattern")
            .context("missing 'pattern' input")?
            .as_str()
            .context("'pattern' must be a string")?;

        let case_sensitive = inputs
            .get("case_sensitive")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let re = if case_sensitive {
            Regex::new(pattern).context("invalid regex pattern")?
        } else {
            Regex::new(&format!("(?i){}", pattern)).context("invalid regex pattern")?
        };

        let mut all_matches = Vec::new();
        let mut all_groups = Vec::new();
        let mut named_groups: BTreeMap<String, Vec<Value>> = BTreeMap::new();

        for caps in re.captures_iter(text) {
            if let Some(mat) = caps.get(0) {
                all_matches.push(Value::String(mat.as_str().to_string()));
            }

            for i in 1..caps.len() {
                if let Some(g) = caps.get(i) {
                    all_groups.push(Value::String(g.as_str().to_string()));
                }
            }

            for name in re.capture_names().flatten() {
                if let Some(cap) = caps.name(name) {
                    named_groups
                        .entry(name.to_string())
                        .or_default()
                        .push(Value::String(cap.as_str().to_string()));
                }
            }
        }

        let named_output: BTreeMap<String, Value> = named_groups
            .into_iter()
            .map(|(k, v)| (k, Value::Array(v)))
            .collect();

        let mut outputs = BTreeMap::new();
        outputs.insert("matches".to_string(), Value::Array(all_matches));
        outputs.insert("groups".to_string(), Value::Array(all_groups));
        outputs.insert("named".to_string(), Value::Object(named_output));

        Ok(outputs)
    }
}
