use crate::node::{
    DataType, InputSpec, Node, NodeContext, OutputSpec, SelectOption, UIComponent,
};
use crate::nodes::declarative::DeclarativeNode;
use crate::nodes::script::ScriptDefinedNode;
use crate::value::Value;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::collections::BTreeMap;

pub const LANG_DECLARATIVE: &str = "declarative";
pub const LANG_RHAI: &str = "rhai";
pub const LANG_PYTHON: &str = "py";
pub const LANG_TYPESCRIPT: &str = "ts";

pub struct DynamicUserNode;

impl DynamicUserNode {
    fn language_options() -> Vec<SelectOption> {
        vec![
            SelectOption { value: LANG_DECLARATIVE.into(), label: Some("declarative".into()) },
            SelectOption { value: LANG_RHAI.into(), label: Some("rhai".into()) },
            SelectOption { value: LANG_PYTHON.into(), label: Some("python".into()) },
            SelectOption { value: LANG_TYPESCRIPT.into(), label: Some("typescript".into()) },
        ]
    }

    fn extract_code_lang(inputs: &BTreeMap<String, Value>) -> Option<(String, String)> {
        let code = match inputs.get("code") {
            Some(Value::String(s)) if !s.trim().is_empty() => s.clone(),
            _ => return None,
        };
        let language = match inputs.get("language") {
            Some(Value::String(s)) if !s.trim().is_empty() => s.clone(),
            _ => LANG_DECLARATIVE.to_string(),
        };
        Some((code, language))
    }

    fn synthetic_filename(language: &str) -> String {
        format!("inline.{}", language)
    }

    /// build the inner node implementation from the given code + language.
    /// returns either a DeclarativeNode or a ScriptDefinedNode wrapped as a
    /// trait object.
    fn build_inner(code: &str, language: &str) -> Result<Box<dyn Node>> {
        if language == LANG_DECLARATIVE {
            let node = DeclarativeNode::new(code)?;
            Ok(Box::new(node))
        } else {
            let filename = Self::synthetic_filename(language);
            let node = ScriptDefinedNode::new(code, &filename)?;
            Ok(Box::new(node))
        }
    }
}

#[async_trait]
impl Node for DynamicUserNode {
    fn name(&self) -> &str {
        "DynamicUserNode"
    }

    fn title(&self) -> &str {
        "Dynamic User Node"
    }

    fn category(&self) -> &str {
        "Scripting"
    }

    fn description(&self) -> &str {
        "Defines a node from user-supplied source code at runtime. The code and \
         language inputs are parsed to derive the node's actual input/output ports, \
         which then appear as additional handles on this node."
    }

    fn inputs(&self) -> Vec<InputSpec> {
        vec![
            InputSpec {
                name: "code".to_string(),
                r#type: DataType::String,
                ui_component: UIComponent::TextArea {},
                required: true,
                description: Some(
                    "source code defining the node. for 'declarative' this is JSON; \
                     for script languages it follows the same convention as files in \
                     user_nodes/."
                        .to_string(),
                ),
                ..Default::default()
            },
            InputSpec {
                name: "language".to_string(),
                r#type: DataType::String,
                ui_component: UIComponent::Select {
                    options: Self::language_options(),
                },
                default: Some(Value::String(LANG_DECLARATIVE.to_string())),
                required: true,
                description: Some("language of the code input".to_string()),
                ..Default::default()
            },
        ]
    }

    fn outputs(&self) -> Vec<OutputSpec> {
        vec![]
    }

    fn has_dynamic_spec(&self) -> bool {
        true
    }

    fn dynamic_spec(
        &self,
        resolved_inputs: &BTreeMap<String, Value>,
    ) -> Option<(Vec<InputSpec>, Vec<OutputSpec>)> {
        let (code, language) = Self::extract_code_lang(resolved_inputs)?;
        let inner = Self::build_inner(&code, &language).ok()?;
        Some((inner.inputs(), inner.outputs()))
    }

    async fn execute(
        &self,
        inputs: BTreeMap<String, Value>,
        ctx: NodeContext,
    ) -> Result<BTreeMap<String, Value>> {
        let (code, language) = Self::extract_code_lang(&inputs)
            .ok_or_else(|| anyhow!("DynamicUserNode: 'code' input is empty"))?;

        let inner = Self::build_inner(&code, &language)?;

        // strip the control inputs before delegating to the inner node
        let mut inner_inputs = inputs;
        inner_inputs.remove("code");
        inner_inputs.remove("language");

        inner.execute(inner_inputs, ctx).await
    }
}
