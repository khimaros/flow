use crate::engine::{Engine, NodeRegistry};
use crate::graph::Workflow;
use crate::node::{DataType, InputSpec, Node, NodeContext, OutputSpec, UIComponent};
use crate::nodes::register_all;
use crate::value::Value;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InnerInput {
    pub node_id: String,
    pub input_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum InputMappingTarget {
    SimpleList(Vec<String>),
    Simple(String),
    Multiple(Vec<InnerInput>),
    Single(InnerInput),
}

impl InputMappingTarget {
    fn targets(&self) -> Vec<InnerInput> {
        match self {
            InputMappingTarget::Single(target) => vec![target.clone()],
            InputMappingTarget::Multiple(targets) => targets.clone(),
            InputMappingTarget::Simple(s) => {
                if let Some((node, input)) = s.split_once('.') {
                    vec![InnerInput {
                        node_id: node.to_string(),
                        input_name: input.to_string(),
                    }]
                } else {
                    vec![]
                }
            }
            InputMappingTarget::SimpleList(list) => list
                .iter()
                .filter_map(|s| {
                    s.split_once('.').map(|(node, input)| InnerInput {
                        node_id: node.to_string(),
                        input_name: input.to_string(),
                    })
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeclarativeInputSpec {
    pub name: String,
    pub mapping: Option<InputMappingTarget>,
    #[serde(default)]
    pub r#type: Option<DataType>,
    #[serde(default)]
    pub ui_component: Option<UIComponent>,
    #[serde(default)]
    pub default: Option<Value>,
    #[serde(default)]
    pub required: Option<bool>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub env_var: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeclarativeOutputSpec {
    pub name: String,
    pub mapping: Option<String>,
    #[serde(default)]
    pub r#type: Option<DataType>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeclarativeNodeDefinition {
    pub name: String,
    pub title: String,
    pub category: String,
    pub description: String,
    #[serde(default)]
    pub inputs: Vec<DeclarativeInputSpec>,
    #[serde(default)]
    pub outputs: Vec<DeclarativeOutputSpec>,
    pub implementation: Workflow,
}

#[derive(Clone)]
pub struct DeclarativeNode {
    definition: DeclarativeNodeDefinition,
    inferred_inputs: Arc<Mutex<Option<Vec<InputSpec>>>>,
    inferred_outputs: Arc<Mutex<Option<Vec<OutputSpec>>>>,
}

impl DeclarativeNode {
    pub fn new(content: &str) -> Result<Self> {
        let definition: DeclarativeNodeDefinition = serde_json::from_str(content)
            .map_err(|e| anyhow!("failed to parse declarative node definition: {}", e))?;
        Ok(Self {
            definition,
            inferred_inputs: Arc::new(Mutex::new(None)),
            inferred_outputs: Arc::new(Mutex::new(None)),
        })
    }

    fn resolve_inputs(&self) -> Vec<InputSpec> {
        if let Ok(guard) = self.inferred_inputs.lock() {
            if let Some(cached) = &*guard {
                return cached.clone();
            }
        }

        let mut registry = NodeRegistry::new();
        register_all(&mut registry, true);

        // build cache of inner nodes
        let mut inner_nodes = HashMap::new();
        for node_def in &self.definition.implementation.nodes {
            if let Some(node) = registry.create(&node_def.node_type) {
                inner_nodes.insert(node_def.id.clone(), node);
            }
        }

        // build reverse mapping for DynamicSelect: (node_id, inner_input) -> outer_input
        let mut reverse_map = HashMap::new();
        for spec in &self.definition.inputs {
            if let Some(target) = &spec.mapping {
                for t in target.targets() {
                    reverse_map
                        .insert((t.node_id.clone(), t.input_name.clone()), spec.name.clone());
                }
            }
        }

        let mut final_inputs = Vec::new();

        for spec in &self.definition.inputs {
            let mut resolved_spec = InputSpec {
                name: spec.name.clone(),
                r#type: spec.r#type.clone().unwrap_or(DataType::String),
                ui_component: spec.ui_component.clone().unwrap_or_else(|| {
                    UIComponent::resolve_auto(&spec.r#type.clone().unwrap_or(DataType::String))
                }),
                default: spec.default.clone(),
                required: spec.required.unwrap_or(false),
                description: spec.description.clone(),
                env_var: spec.env_var.clone(),
                env_value: None,
            };

            // if mapped, infer missing fields from the inner node
            if let Some(mapping) = &spec.mapping {
                let targets = mapping.targets();
                if let Some(first_target) = targets.first() {
                    if let Some(inner_node) = inner_nodes.get(&first_target.node_id) {
                        if let Some(inner_spec) = inner_node
                            .inputs()
                            .iter()
                            .find(|i| i.name == first_target.input_name)
                        {
                            if spec.r#type.is_none() {
                                resolved_spec.r#type = inner_spec.r#type.clone();
                            }
                            if spec.ui_component.is_none() {
                                resolved_spec.ui_component = inner_spec.ui_component.clone();

                                // handle DynamicSelect remapping
                                if let UIComponent::DynamicSelect { depends_on } =
                                    &resolved_spec.ui_component
                                {
                                    let new_deps: Vec<String> = depends_on
                                        .iter()
                                        .filter_map(|dep| {
                                            reverse_map
                                                .get(&(first_target.node_id.clone(), dep.clone()))
                                                .cloned()
                                        })
                                        .collect();
                                    resolved_spec.ui_component = UIComponent::DynamicSelect {
                                        depends_on: new_deps,
                                    };
                                }
                            }
                            if spec.default.is_none() {
                                resolved_spec.default = inner_spec.default.clone();
                            }
                            if spec.required.is_none() {
                                resolved_spec.required = inner_spec.required;
                            }
                            if spec.description.is_none() {
                                resolved_spec.description = inner_spec.description.clone();
                            }
                            if resolved_spec.env_var.is_none() {
                                resolved_spec.env_var = inner_spec.env_var.clone();
                            }
                            // inherit resolved env from the inner node as
                            // a fallback so FLOW_<INNER>_<INPUT> vars
                            // propagate through declarative wrappers
                            let inner_node_type = inner_nodes
                                .get(&first_target.node_id)
                                .map(|n| n.name().to_string())
                                .unwrap_or_default();
                            let (inner_env_var, inner_env_value) =
                                crate::node::resolve_env_for_input(
                                    &inner_node_type,
                                    inner_spec,
                                );
                            if resolved_spec.env_var.is_none()
                                || resolved_spec.env_var == inner_spec.env_var
                            {
                                // only override if outer resolution found nothing
                                let (outer_env_var, outer_env_value) =
                                    crate::node::resolve_env_for_input(
                                        &self.definition.name,
                                        &resolved_spec,
                                    );
                                if outer_env_value.is_none() && inner_env_value.is_some() {
                                    resolved_spec.env_var = inner_env_var;
                                    resolved_spec.env_value = inner_env_value;
                                } else {
                                    resolved_spec.env_var = outer_env_var;
                                    resolved_spec.env_value = outer_env_value;
                                }
                            }
                        }
                    }
                }
            }

            final_inputs.push(resolved_spec);
        }

        if let Ok(mut guard) = self.inferred_inputs.lock() {
            *guard = Some(final_inputs.clone());
        }

        final_inputs
    }

    fn resolve_outputs(&self) -> Vec<OutputSpec> {
        if let Ok(guard) = self.inferred_outputs.lock() {
            if let Some(cached) = &*guard {
                return cached.clone();
            }
        }

        let mut registry = NodeRegistry::new();
        register_all(&mut registry, true);

        let mut inner_nodes = HashMap::new();
        for node_def in &self.definition.implementation.nodes {
            if let Some(node) = registry.create(&node_def.node_type) {
                inner_nodes.insert(node_def.id.clone(), node);
            }
        }

        let mut final_outputs = Vec::new();

        for spec in &self.definition.outputs {
            let mut resolved_spec = OutputSpec {
                name: spec.name.clone(),
                r#type: spec.r#type.clone().unwrap_or(DataType::String),
                description: spec.description.clone(),
            };

            if let Some(mapping_str) = &spec.mapping {
                if let Some((node_id, output_name)) = mapping_str.split_once('.') {
                    if let Some(inner_node) = inner_nodes.get(node_id) {
                        if let Some(inner_spec) =
                            inner_node.outputs().iter().find(|o| o.name == output_name)
                        {
                            if spec.r#type.is_none() {
                                resolved_spec.r#type = inner_spec.r#type.clone();
                            }
                            if spec.description.is_none() {
                                resolved_spec.description = inner_spec.description.clone();
                            }
                        }
                    }
                }
            }
            final_outputs.push(resolved_spec);
        }

        if let Ok(mut guard) = self.inferred_outputs.lock() {
            *guard = Some(final_outputs.clone());
        }

        final_outputs
    }
}

#[async_trait]
impl Node for DeclarativeNode {
    fn name(&self) -> &str {
        &self.definition.name
    }

    fn title(&self) -> &str {
        &self.definition.title
    }

    fn category(&self) -> &str {
        &self.definition.category
    }

    fn description(&self) -> &str {
        &self.definition.description
    }

    fn inputs(&self) -> Vec<InputSpec> {
        self.resolve_inputs()
    }

    fn outputs(&self) -> Vec<OutputSpec> {
        self.resolve_outputs()
    }

    async fn get_options(
        &self,
        input_name: &str,
        inputs: BTreeMap<String, Value>,
    ) -> Result<Vec<crate::node::SelectOption>> {
        let input_spec = self
            .definition
            .inputs
            .iter()
            .find(|i| i.name == input_name)
            .ok_or_else(|| anyhow!("input '{}' not found in definition", input_name))?;

        let mapping_target = input_spec
            .mapping
            .as_ref()
            .ok_or_else(|| anyhow!("input '{}' is not mapped", input_name))?;

        // re-implementing logic using targets() to be safe and consistent
        let targets = mapping_target.targets();
        let inner_dest = targets
            .first()
            .ok_or_else(|| anyhow!("input '{}' mapping is empty", input_name))?;

        let inner_node_def = self
            .definition
            .implementation
            .nodes
            .iter()
            .find(|n| n.id == inner_dest.node_id)
            .ok_or_else(|| anyhow!("inner node '{}' not found", inner_dest.node_id))?;

        let mut registry = NodeRegistry::new();
        register_all(&mut registry, true);

        let inner_node = registry
            .create(&inner_node_def.node_type)
            .ok_or_else(|| anyhow!("inner node type '{}' not found", inner_node_def.node_type))?;

        let mut inner_inputs = inner_node_def.inputs.clone();

        // apply overrides from outer inputs
        for spec in &self.definition.inputs {
            if let Some(target) = &spec.mapping {
                for dest in target.targets() {
                    if dest.node_id == inner_dest.node_id {
                        if let Some(val) = inputs.get(&spec.name) {
                            inner_inputs.insert(dest.input_name.clone(), val.clone());
                        }
                    }
                }
            }
        }

        inner_node
            .get_options(&inner_dest.input_name, inner_inputs)
            .await
    }

    async fn execute(
        &self,
        inputs: BTreeMap<String, Value>,
        ctx: NodeContext,
    ) -> Result<BTreeMap<String, Value>> {
        let mut registry = NodeRegistry::new();
        register_all(&mut registry, true);
        let registry = Arc::new(registry);

        let mut engine = Engine::new(registry.clone(), None);

        let mut inner_workflow = self.definition.implementation.clone();

        // map Inputs — only forward non-empty values so the inner engine
        // can apply its own env var overrides and defaults for unmapped inputs
        for spec in &self.definition.inputs {
            if let Some(val) = inputs.get(&spec.name) {
                let is_empty = match val {
                    Value::Null => true,
                    Value::String(s) if s.is_empty() => true,
                    _ => false,
                };
                if is_empty {
                    continue;
                }
                if let Some(target) = &spec.mapping {
                    for inner_dest in target.targets() {
                        if let Some(node) = inner_workflow
                            .nodes
                            .iter_mut()
                            .find(|n| n.id == inner_dest.node_id)
                        {
                            node.inputs
                                .insert(inner_dest.input_name.clone(), val.clone());
                        }
                    }
                }
            }
        }

        inner_workflow.normalize(&registry);

        let outcome = engine
            .execute(&inner_workflow, None, ctx.cancel_token(), ctx.is_terse())
            .await;
        if let Some(err) = outcome.error {
            return Err(anyhow::anyhow!("{}", err));
        }
        let results = outcome.results;

        // map Outputs
        let mut outputs = BTreeMap::new();
        for spec in &self.definition.outputs {
            if let Some(mapping_str) = &spec.mapping {
                if let Some((node_id, output_name)) = mapping_str.split_once('.') {
                    if let Some(node_results) = results.get(node_id) {
                        if let Some(val) = node_results.get(output_name) {
                            outputs.insert(spec.name.clone(), val.clone());
                        }
                    }
                }
            }
        }

        Ok(outputs)
    }
}
