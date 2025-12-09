use crate::node::{DataType, InputSpec, Node, NodeContext, OutputSpec, SelectOption, UIComponent};
use crate::value::Value;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::collections::BTreeMap;

pub struct HttpRequestNode;

#[async_trait]
impl Node for HttpRequestNode {
    fn name(&self) -> &str {
        "HttpRequest"
    }

    fn title(&self) -> &str {
        "HTTP Request"
    }

    fn category(&self) -> &str {
        "Network"
    }

    fn description(&self) -> &str {
        "Performs an HTTP request."
    }

    fn inputs(&self) -> Vec<InputSpec> {
        vec![
            InputSpec {
                name: "method".to_string(),
                r#type: DataType::String,
                ui_component: UIComponent::Select {
                    options: vec![
                        SelectOption {
                            value: "GET".to_string(),
                            label: Some("GET".to_string()),
                        },
                        SelectOption {
                            value: "POST".to_string(),
                            label: Some("POST".to_string()),
                        },
                        SelectOption {
                            value: "PUT".to_string(),
                            label: Some("PUT".to_string()),
                        },
                        SelectOption {
                            value: "DELETE".to_string(),
                            label: Some("DELETE".to_string()),
                        },
                    ],
                },
                default: Some(Value::String("GET".to_string())),
                required: true,
                description: Some("the HTTP method.".to_string()),
                ..Default::default()
            },
            InputSpec {
                name: "url".to_string(),
                r#type: DataType::String,
                ui_component: UIComponent::Auto {},
                default: Some(Value::String("https://dummyjson.com/test".to_string())),
                required: true,
                description: Some("the target URL.".to_string()),
                ..Default::default()
            },
        ]
    }

    fn outputs(&self) -> Vec<OutputSpec> {
        vec![
            OutputSpec {
                name: "status".to_string(),
                r#type: DataType::Integer,
                description: Some("the HTTP status code.".to_string()),
            },
            OutputSpec {
                name: "body".to_string(),
                r#type: DataType::Object,
                description: Some("the response body.".to_string()),
            },
        ]
    }

    async fn execute(
        &self,
        inputs: BTreeMap<String, Value>,
        _ctx: NodeContext,
    ) -> Result<BTreeMap<String, Value>> {
        let url = inputs
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("missing or invalid 'url' input"))?;

        let method = inputs
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("GET");

        let client = reqwest::Client::new();
        let builder = match method.to_uppercase().as_str() {
            "GET" => client.get(url),
            "POST" => client.post(url),
            "PUT" => client.put(url),
            "DELETE" => client.delete(url),
            _ => return Err(anyhow!("unsupported HTTP method: {}", method)),
        };

        let response = builder.send().await?;

        let status = response.status().as_u16() as i64;

        // try to parse body as JSON, otherwise string
        let body_text = response.text().await?;
        let body_val = if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&body_text) {
            match serde_json::from_value(json_val) {
                Ok(v) => v,
                Err(_) => Value::String(body_text),
            }
        } else {
            Value::String(body_text)
        };

        let mut outputs = BTreeMap::new();
        outputs.insert("status".to_string(), Value::Integer(status));
        outputs.insert("body".to_string(), body_val);

        Ok(outputs)
    }
}
