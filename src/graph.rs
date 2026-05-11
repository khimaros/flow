use crate::engine::{NodeRegistry, ResultMap};
use crate::node::UIComponent;
use crate::value::Value;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowNode {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: String,
    #[serde(default)]
    pub position: Option<Position>,
    #[serde(default)]
    pub size: Option<Size>,
    #[serde(default)]
    pub inputs: BTreeMap<String, Value>,
    #[serde(default, rename = "skipCache")]
    pub skip_cache: bool,
    #[serde(default)]
    pub bypassed: bool,
}

fn generate_edge_id() -> String {
    format!("e{}", Uuid::new_v4())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowEdge {
    #[serde(default = "generate_edge_id")]
    pub id: String,
    pub source: String,
    #[serde(rename = "sourceHandle")]
    pub source_handle: String,
    pub target: String,
    #[serde(rename = "targetHandle")]
    pub target_handle: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub nodes: Vec<WorkflowNode>,
    #[serde(default)]
    pub edges: Vec<WorkflowEdge>,
    #[serde(default, rename = "forceRun")]
    pub force_run: bool,
    #[serde(default, rename = "targetNodeId")]
    pub target_node_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ExecutionNode {
    pub id: String,
    pub node_type: String,
    pub inputs: BTreeMap<String, Value>,
    pub skip_cache: bool,
    pub bypassed: bool,
}

#[derive(Debug, Clone)]
pub struct ExecutionWorkflow {
    pub nodes: Vec<ExecutionNode>,
    pub force_run: bool,
    pub target_node_id: Option<String>,
}

impl Workflow {
    pub fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .map_err(|e| anyhow!("failed to read workflow file '{}': {}", path.display(), e))?;
        let workflow: Workflow = serde_json::from_str(&content)
            .map_err(|e| anyhow!("failed to parse workflow: {}", e))?;
        Ok(workflow)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| anyhow!("failed to serialize workflow: {}", e))?;
        fs::write(path, content)
            .map_err(|e| anyhow!("failed to write workflow file '{}': {}", path.display(), e))?;
        Ok(())
    }

    /// path to the sibling .state/ file for a given workflow path
    pub fn state_path(workflow_path: &Path) -> Option<PathBuf> {
        let parent = workflow_path.parent()?;
        let filename = workflow_path.file_name()?;
        Some(parent.join(".state").join(filename))
    }

    /// build the "resolved inputs" map used to derive a node's dynamic spec
    /// (effective_inputs/outputs, dynamic_script_source). merges literal
    /// inputs on the workflow node with wired inputs resolved from upstream
    /// nodes' saved state, so dynamic ports can be discovered even when the
    /// inputs that drive them are wired from another node rather than set
    /// literally. ports that depend on inputs the saved state doesn't cover
    /// will simply not appear -- callers needing strict validation must
    /// account for that.
    pub fn resolve_spec_inputs(
        &self,
        node: &WorkflowNode,
        state: &ResultMap,
    ) -> BTreeMap<String, Value> {
        let mut resolved: BTreeMap<String, Value> = node
            .inputs
            .iter()
            .filter_map(|(k, v)| match v {
                Value::Object(o) if o.contains_key("$node") => None,
                _ => Some((k.clone(), v.clone())),
            })
            .collect();
        for e in &self.edges {
            if e.target != node.id {
                continue;
            }
            if let Some(outputs) = state.get(&e.source) {
                if let Some(v) = outputs.get(&e.source_handle) {
                    resolved.insert(e.target_handle.clone(), v.clone());
                }
            }
        }
        resolved
    }

    /// load saved execution state from the .state/ directory
    pub fn load_state(workflow_path: &Path) -> ResultMap {
        Self::state_path(workflow_path)
            .and_then(|p| fs::read_to_string(&p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// save execution state to the .state/ directory
    pub fn save_state(workflow_path: &Path, results: &ResultMap) -> Result<()> {
        let state_path = Self::state_path(workflow_path)
            .ok_or_else(|| anyhow!("cannot determine state path for '{}'", workflow_path.display()))?;
        if let Some(dir) = state_path.parent() {
            fs::create_dir_all(dir)
                .map_err(|e| anyhow!("failed to create state directory: {}", e))?;
        }
        let content = serde_json::to_string_pretty(results)
            .map_err(|e| anyhow!("failed to serialize state: {}", e))?;
        fs::write(&state_path, content)
            .map_err(|e| anyhow!("failed to write state file '{}': {}", state_path.display(), e))?;
        Ok(())
    }

    /// normalize workflow inputs: fill first option for Select UI inputs.
    /// other defaults are applied at execution time (engine::apply_defaults)
    /// so the UI can show them as placeholder text.
    /// note: we don't strip unknown inputs since they may be injected (e.g. Read node's "input").
    pub fn normalize(&mut self, registry: &NodeRegistry) {
        for node in &mut self.nodes {
            if let Some(node_impl) = registry.create(&node.node_type) {
                for spec in &node_impl.inputs() {
                    if !node.inputs.contains_key(&spec.name) {
                        if let UIComponent::Select { options } = &spec.ui_component {
                            if !options.is_empty() {
                                node.inputs.insert(
                                    spec.name.clone(),
                                    Value::String(options[0].value.clone()),
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn to_execution(&self) -> ExecutionWorkflow {
        let bypassed_ids: HashSet<&str> = self
            .nodes
            .iter()
            .filter(|n| n.bypassed)
            .map(|n| n.id.as_str())
            .collect();

        let nodes: Vec<ExecutionNode> = self
            .nodes
            .iter()
            .map(|node| {
                let mut inputs = node.inputs.clone();

                for edge in &self.edges {
                    if edge.target == node.id && !bypassed_ids.contains(edge.source.as_str()) {
                        let connection = Value::Object(
                            [
                                ("$node".to_string(), Value::String(edge.source.clone())),
                                (
                                    "$output".to_string(),
                                    Value::String(edge.source_handle.clone()),
                                ),
                            ]
                            .into_iter()
                            .collect(),
                        );
                        inputs.insert(edge.target_handle.clone(), connection);
                    }
                }

                ExecutionNode {
                    id: node.id.clone(),
                    node_type: node.node_type.clone(),
                    inputs,
                    skip_cache: node.skip_cache,
                    bypassed: node.bypassed,
                }
            })
            .collect();

        ExecutionWorkflow {
            nodes,
            force_run: self.force_run,
            target_node_id: self.target_node_id.clone(),
        }
    }
}
