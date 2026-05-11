use crate::graph::{ExecutionNode, Workflow};
use crate::node::{DataType, Node, NodeContext, PartialOutputUpdate, ProgressUpdate};
use crate::value::Value;
use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc::Sender, RwLock};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

pub type ResultMap = BTreeMap<String, BTreeMap<String, Value>>;

#[derive(Debug)]
pub enum EngineError {
    Cancelled,
    Anyhow(anyhow::Error),
}

/// execution outcome, always including whatever results were produced
pub struct ExecutionOutcome {
    pub results: ResultMap,
    pub error: Option<EngineError>,
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineError::Cancelled => write!(f, "job cancelled"),
            EngineError::Anyhow(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for EngineError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            EngineError::Cancelled => None,
            EngineError::Anyhow(e) => e.source(),
        }
    }
}

impl From<anyhow::Error> for EngineError {
    fn from(err: anyhow::Error) -> Self {
        EngineError::Anyhow(err)
    }
}

// registry to create nodes by type name
pub struct NodeRegistry {
    creators: HashMap<String, Box<dyn Fn() -> Box<dyn Node> + Send + Sync>>,
}

impl Default for NodeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl NodeRegistry {
    pub fn new() -> Self {
        Self {
            creators: HashMap::new(),
        }
    }

    pub fn register<F>(&mut self, name: &str, creator: F)
    where
        F: Fn() -> Box<dyn Node> + Send + Sync + 'static,
    {
        self.creators.insert(name.to_string(), Box::new(creator));
    }

    pub fn create(&self, name: &str) -> Option<Box<dyn Node>> {
        self.creators.get(name).map(|f| f())
    }

    pub fn list_metadata(&self) -> Vec<crate::node::NodeMetadata> {
        let mut metadata = Vec::new();
        for constructor in self.creators.values() {
            let instance = constructor();
            let node_name = instance.name().to_string();
            let mut inputs = instance.inputs();
            // resolve env vars (explicit + auto FLOW_<NODE>_<INPUT>) for UI.
            // if the node already resolved env vars (e.g. declarative nodes
            // inherit inner node env), only overwrite when outer resolution
            // finds an actual value
            for input in &mut inputs {
                let (env_var, env_value) =
                    crate::node::resolve_env_for_input(&node_name, input);
                if env_value.is_some() || input.env_value.is_none() {
                    input.env_var = env_var;
                    input.env_value = env_value;
                }
            }
            metadata.push(crate::node::NodeMetadata {
                name: node_name,
                title: instance.title().to_string(),
                category: instance.category().to_string(),
                description: instance.description().to_string(),
                inputs,
                outputs: instance.outputs(),
                script_source: instance.script_source(),
                has_dynamic_spec: instance.has_dynamic_spec(),
            });
        }
        metadata.sort_by(|a, b| a.name.cmp(&b.name));
        metadata
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum ExecutionEvent {
    Started {
        node_id: String,
        inputs: BTreeMap<String, Value>,
    },
    Progress {
        node_id: String,
        progress: f32,
        message: Option<String>,
    },
    Finished {
        node_id: String,
        result: BTreeMap<String, Value>,
        cached: bool,
    },
    PartialOutput {
        node_id: String,
        output_name: String,
        delta: Value,
        accumulated: Value,
    },
    Error {
        node_id: String,
        error: String,
    },
}

type AdjacencyMap<'a> = HashMap<&'a String, Vec<&'a String>>;
type InDegreeMap<'a> = HashMap<&'a String, usize>;

// internal context shared across async tasks
struct ExecutionContext {
    results: RwLock<BTreeMap<String, BTreeMap<String, Value>>>,
    cache: RwLock<BTreeMap<String, BTreeMap<String, Value>>>,
    node_map: HashMap<String, ExecutionNode>,
    adj: HashMap<String, Vec<String>>,
    remaining_deg: RwLock<HashMap<String, usize>>,
    completion_tx: tokio::sync::mpsc::Sender<Result<String>>,
    // wrapped in Mutex so we can drop it when execution finishes, ensuring
    // the event handler channel closes even if orphaned tasks hold Arc clones
    progress_sender: std::sync::Mutex<Option<Sender<ExecutionEvent>>>,
    registry: Arc<NodeRegistry>,
    running: AtomicUsize,
    force_run: bool,
    force_run_node_id: Option<String>,
    terse: bool,
    cancel_token: CancellationToken,
    // per-node forwarding task handles, aborted on completion so their
    // cloned senders are dropped promptly
    forwarding_handles: std::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>,
}

pub const DEFAULT_CACHE_FILE: &str = ".flow_cache.json";

pub struct Engine {
    registry: Arc<NodeRegistry>,
    cache: BTreeMap<String, BTreeMap<String, Value>>,
    cache_path: Option<PathBuf>,
}

impl Engine {
    pub fn new(registry: Arc<NodeRegistry>, cache_path: Option<PathBuf>) -> Self {
        let mut cache = BTreeMap::new();
        if let Some(path) = &cache_path {
            if path.exists() {
                info!("loading cache from {:?}", path);
                if let Ok(content) = fs::read_to_string(path) {
                    if let Ok(loaded) = serde_json::from_str(&content) {
                        cache = loaded;
                        info!("loaded cache from {:?}", path);
                    }
                }
            }
        }

        Self {
            registry,
            cache,
            cache_path,
        }
    }

    fn save_cache(&self) {
        if let Some(path) = &self.cache_path {
            if let Ok(content) = serde_json::to_string_pretty(&self.cache) {
                let _ = fs::write(path, content);
            }
        }
    }

    pub async fn execute(
        &mut self,
        workflow: &Workflow,
        progress_sender: Option<Sender<ExecutionEvent>>,
        cancel_token: CancellationToken,
        terse: bool,
    ) -> ExecutionOutcome {
        let exec_workflow = workflow.to_execution();

        // 1. Identify active nodes (handling target node dependencies)
        let (node_map, active_nodes_ids) = match self.build_active_nodes(&exec_workflow) {
            Ok(v) => v,
            Err(e) => return ExecutionOutcome { results: BTreeMap::new(), error: Some(EngineError::Anyhow(e)) },
        };

        // 2. Build dependency graph
        let (adj, initial_in_degree) = match self.build_dependency_graph(&node_map, &active_nodes_ids) {
            Ok(v) => v,
            Err(e) => return ExecutionOutcome { results: BTreeMap::new(), error: Some(EngineError::Anyhow(e)) },
        };

        // 3. Validate for cycles
        if let Err(e) = self.validate_graph(&active_nodes_ids, &adj, &initial_in_degree) {
            return ExecutionOutcome { results: BTreeMap::new(), error: Some(EngineError::Anyhow(e)) };
        }

        let (completion_tx, completion_rx) = tokio::sync::mpsc::channel::<Result<String>>(100);

        // 4. Initialize Execution Context
        let ctx = self.create_execution_context(
            &exec_workflow,
            node_map,
            adj,
            initial_in_degree,
            completion_tx,
            progress_sender,
            cancel_token,
            terse,
        );

        // 5. Start initial nodes (degree 0)
        self.spawn_ready_nodes(&ctx).await;

        // 6. Run Event Loop
        let outcome = self
            .run_event_loop(ctx.clone(), completion_rx, active_nodes_ids.len())
            .await;

        // 7. Close the progress channel so the caller's event handler exits
        // promptly, even if orphaned spawn_blocking threads are still alive
        for h in ctx.forwarding_handles.lock().unwrap().drain(..) {
            h.abort();
        }
        ctx.progress_sender.lock().unwrap().take();

        outcome
    }

    #[allow(clippy::too_many_arguments)]
    fn create_execution_context(
        &self,
        exec_workflow: &crate::graph::ExecutionWorkflow,
        node_map: HashMap<&String, &ExecutionNode>,
        adj: AdjacencyMap,
        initial_in_degree: InDegreeMap,
        completion_tx: tokio::sync::mpsc::Sender<Result<String>>,
        progress_sender: Option<Sender<ExecutionEvent>>,
        cancel_token: CancellationToken,
        terse: bool,
    ) -> Arc<ExecutionContext> {
        Arc::new(ExecutionContext {
            results: RwLock::new(BTreeMap::new()),
            cache: RwLock::new(self.cache.clone()),
            node_map: node_map
                .iter()
                .map(|(k, v)| ((*k).clone(), (*v).clone()))
                .collect(),
            adj: adj
                .iter()
                .map(|(k, v)| ((*k).clone(), v.iter().map(|s| (*s).clone()).collect()))
                .collect(),
            remaining_deg: RwLock::new(
                initial_in_degree
                    .iter()
                    .map(|(k, v)| ((*k).clone(), *v))
                    .collect(),
            ),
            completion_tx,
            progress_sender: std::sync::Mutex::new(progress_sender),
            registry: self.registry.clone(),
            running: AtomicUsize::new(0),
            force_run: exec_workflow.force_run,
            force_run_node_id: if exec_workflow.force_run {
                exec_workflow.target_node_id.clone()
            } else {
                None
            },
            terse,
            cancel_token,
            forwarding_handles: std::sync::Mutex::new(Vec::new()),
        })
    }

    async fn run_event_loop(
        &mut self,
        ctx: Arc<ExecutionContext>,
        mut completion_rx: tokio::sync::mpsc::Receiver<Result<String>>,
        total_nodes: usize,
    ) -> ExecutionOutcome {
        let mut completed_count = 0;
        let mut error = None;

        while completed_count < total_nodes {
            match completion_rx.recv().await {
                Some(Ok(node_id)) => {
                    completed_count += 1;
                    ctx.running.fetch_sub(1, Ordering::SeqCst);
                    info!(node_id = %node_id, "node finished, checking dependents");

                    self.spawn_ready_nodes(&ctx).await;
                }
                Some(Err(e)) => {
                    // save cache for nodes that succeeded before the error
                    let final_cache = ctx.cache.read().await;
                    self.cache = final_cache.clone();
                    self.save_cache();

                    if ctx.cancel_token.is_cancelled()
                        || e.to_string().to_lowercase().contains("cancelled")
                    {
                        warn!("engine execution cancelled");
                        error = Some(EngineError::Cancelled);
                    } else {
                        warn!(error = %e, "engine execution aborted due to error");
                        error = Some(EngineError::Anyhow(e));
                    }
                    break;
                }
                None => {
                    if ctx.cancel_token.is_cancelled() {
                        error = Some(EngineError::Cancelled);
                    } else {
                        warn!("completion channel closed unexpectedly");
                    }
                    break;
                }
            }
        }

        if error.is_none() && ctx.cancel_token.is_cancelled() {
            error = Some(EngineError::Cancelled);
        }

        if error.is_none() {
            info!("engine execution completed successfully");
            // update and save cache
            let final_cache = ctx.cache.read().await;
            self.cache = final_cache.clone();
            self.save_cache();
        }

        let final_results = ctx.results.read().await;
        ExecutionOutcome {
            results: final_results.clone(),
            error,
        }
    }

    // --- Graph Building Helpers ---

    fn build_active_nodes<'a>(
        &self,
        exec_workflow: &'a crate::graph::ExecutionWorkflow,
    ) -> Result<(HashMap<&'a String, &'a ExecutionNode>, HashSet<&'a String>)> {
        let mut node_map: HashMap<&String, &ExecutionNode> = HashMap::new();
        for node in &exec_workflow.nodes {
            node_map.insert(&node.id, node);
        }

        let mut active_nodes_ids: HashSet<&String> = HashSet::new();

        if let Some(target) = &exec_workflow.target_node_id {
            if !node_map.contains_key(target) {
                return Err(anyhow!("target node {} not found", target));
            }
            let mut stack = vec![target];
            active_nodes_ids.insert(target);

            while let Some(curr) = stack.pop() {
                if let Some(node) = node_map.get(curr) {
                    let mut deps = HashSet::new();
                    for input in node.inputs.values() {
                        Self::collect_dependencies(input, &mut deps);
                    }
                    for dep in deps {
                        if node_map.contains_key(dep) && active_nodes_ids.insert(dep) {
                            stack.push(dep);
                        }
                    }
                }
            }
        } else {
            active_nodes_ids = node_map.keys().cloned().collect();
        }

        let bypassed_ids: HashSet<&String> = node_map
            .iter()
            .filter(|(_, node)| node.bypassed)
            .map(|(id, _)| *id)
            .collect();

        active_nodes_ids.retain(|id| !bypassed_ids.contains(*id));

        Ok((node_map, active_nodes_ids))
    }

    fn build_dependency_graph<'a>(
        &self,
        node_map: &HashMap<&'a String, &'a ExecutionNode>,
        active_nodes_ids: &HashSet<&'a String>,
    ) -> Result<(AdjacencyMap<'a>, InDegreeMap<'a>)> {
        let mut adj: HashMap<&String, Vec<&String>> = HashMap::new();
        let mut in_degree: HashMap<&String, usize> = HashMap::new();

        for node_id in active_nodes_ids {
            let node = node_map.get(node_id).unwrap();
            in_degree.entry(&node.id).or_insert(0);

            let mut dependencies = HashSet::new();
            for input_val in node.inputs.values() {
                Self::collect_dependencies(input_val, &mut dependencies);
            }

            for dep in dependencies {
                if !active_nodes_ids.contains(dep) {
                    if !node_map.contains_key(dep) {
                        return Err(anyhow!("node {} depends on missing node {}", node.id, dep));
                    }
                    continue;
                }

                adj.entry(dep).or_default().push(&node.id);
                *in_degree.entry(&node.id).or_insert(0) += 1;
            }
        }
        Ok((adj, in_degree))
    }

    fn validate_graph(
        &self,
        active_nodes_ids: &HashSet<&String>,
        adj: &HashMap<&String, Vec<&String>>,
        in_degree: &HashMap<&String, usize>,
    ) -> Result<()> {
        let mut queue: Vec<&String> = in_degree
            .iter()
            .filter(|(_, deg)| **deg == 0)
            .map(|(id, _)| *id)
            .collect();

        // sort for deterministic validation order
        queue.sort();

        let mut processed_count = 0;
        let mut current_degree = in_degree
            .iter()
            .map(|(k, v)| (*k, *v))
            .collect::<HashMap<_, _>>();

        while let Some(node_id) = queue.pop() {
            processed_count += 1;
            if let Some(neighbors) = adj.get(node_id) {
                let mut sorted_neighbors = neighbors.clone();
                sorted_neighbors.sort();

                for neighbor in sorted_neighbors {
                    if let Some(d) = current_degree.get_mut(neighbor) {
                        *d -= 1;
                        if *d == 0 {
                            queue.push(neighbor);
                        }
                    }
                }
            }
        }

        if processed_count != active_nodes_ids.len() {
            return Err(anyhow!(
                "Cycle detected in workflow or disconnected nodes (processed {}/{})",
                processed_count,
                active_nodes_ids.len()
            ));
        }
        Ok(())
    }

    // --- Execution Helpers ---

    async fn spawn_ready_nodes(&self, ctx: &Arc<ExecutionContext>) {
        let ready_nodes: Vec<String> = {
            let mut deg = ctx.remaining_deg.write().await;
            let mut ready = Vec::new();

            // collect keys first to avoid immutable borrow while mutating
            let keys: Vec<String> = deg.keys().cloned().collect();

            for id in keys {
                if let Some(d) = deg.get(&id) {
                    if *d == 0 {
                        ready.push(id.clone());
                    }
                }
            }

            // remove them from map so they aren't picked up again
            for id in &ready {
                deg.remove(id);
            }
            ready
        };

        if !ready_nodes.is_empty() {
            info!(
                count = ready_nodes.len(),
                nodes = ?ready_nodes,
                "starting ready nodes"
            );
            for node_id in ready_nodes {
                self.spawn_node_task(ctx.clone(), node_id);
            }
        }
    }

    fn spawn_node_task(&self, ctx: Arc<ExecutionContext>, node_id: String) {
        ctx.running.fetch_add(1, Ordering::SeqCst);

        tokio::spawn(async move {
            Self::run_node_task(ctx, node_id).await;
        });
    }

    async fn run_node_task(ctx: Arc<ExecutionContext>, node_id: String) {
        if ctx.cancel_token.is_cancelled() {
            Self::report_error(&ctx, &node_id, anyhow!("job cancelled")).await;
            return;
        }

        // 1. Preparation
        let (node_instance, node_impl) = match Self::prepare_node(&ctx, &node_id) {
            Ok(v) => v,
            Err(e) => {
                Self::report_error(&ctx, &node_id, e).await;
                return;
            }
        };

        // 2. Input Resolution
        let resolved_inputs =
            match Self::prepare_inputs(&ctx, node_instance, &*node_impl, &node_id).await {
                Ok(v) => v,
                Err(e) => {
                    Self::report_error(&ctx, &node_id, e).await;
                    return;
                }
            };

        // 3. Send Started event (before cache check so UI always sees Started before Finished)
        let ps = ctx.progress_sender.lock().unwrap().clone();
        if let Some(tx) = ps {
            let _ = tx
                .send(ExecutionEvent::Started {
                    node_id: node_id.clone(),
                    inputs: resolved_inputs.clone(),
                })
                .await;
        }

        // 4. Caching
        let cache_key = Self::calculate_hash_static(&node_instance.node_type, &resolved_inputs);
        if let Some(cached_val) = Self::check_cache(&ctx, node_instance, &cache_key).await {
            info!(node_id = %node_id, "using cached result");
            Self::complete_node(&ctx, &node_id, cached_val, true).await;
            return;
        }

        // 5. Execution
        Self::execute_node_logic(
            &ctx,
            &node_id,
            node_instance,
            &*node_impl,
            resolved_inputs,
            cache_key,
        )
        .await;
    }

    fn prepare_node<'a>(
        ctx: &'a ExecutionContext,
        node_id: &str,
    ) -> Result<(&'a ExecutionNode, Box<dyn Node>)> {
        let node_instance = ctx
            .node_map
            .get(node_id)
            .ok_or_else(|| anyhow!("node not found in map"))?;

        info!(node_id = %node_id, node_type = %node_instance.node_type, "task started for node");

        let node_impl = ctx
            .registry
            .create(&node_instance.node_type)
            .ok_or_else(|| anyhow!("unknown node type: {}", node_instance.node_type))?;

        Ok((node_instance, node_impl))
    }

    async fn prepare_inputs(
        ctx: &ExecutionContext,
        node_instance: &ExecutionNode,
        node_impl: &dyn Node,
        node_id: &str,
    ) -> Result<BTreeMap<String, Value>> {
        let mut resolved_inputs = Self::resolve_inputs(ctx, &node_instance.inputs).await?;

        // first pass: apply env/defaults using the static input specs so that
        // any inputs needed to derive dynamic ports (e.g. code/language on a
        // DynamicUserNode) are populated before dynamic_spec() is consulted.
        let static_specs = node_impl.inputs();
        Self::apply_env_overrides_for(node_impl.name(), &static_specs, &node_instance.inputs, &mut resolved_inputs);
        Self::apply_defaults_for(&static_specs, &mut resolved_inputs);

        // compute the effective spec list, which may now include dynamic ports
        // resolved from the (partially) populated inputs.
        let effective_specs = crate::node::effective_inputs(node_impl, &resolved_inputs);

        // second pass: env/defaults over dynamic-only additions are picked up
        // here. iterations over static specs are idempotent.
        Self::apply_env_overrides_for(node_impl.name(), &effective_specs, &node_instance.inputs, &mut resolved_inputs);
        Self::apply_defaults_for(&effective_specs, &mut resolved_inputs);

        Self::validate_required_inputs_for(&effective_specs, node_id, &resolved_inputs)?;
        Ok(resolved_inputs)
    }

    /// fill empty inputs with env var values (explicit env_var or auto
    /// FLOW_<NODE>_<INPUT>). skips edge-connected and non-empty inputs.
    pub fn apply_env_overrides(
        node_impl: &dyn Node,
        original_inputs: &BTreeMap<String, Value>,
        resolved_inputs: &mut BTreeMap<String, Value>,
    ) {
        let specs = node_impl.inputs();
        Self::apply_env_overrides_for(node_impl.name(), &specs, original_inputs, resolved_inputs);
    }

    fn apply_env_overrides_for(
        node_type: &str,
        specs: &[crate::node::InputSpec],
        original_inputs: &BTreeMap<String, Value>,
        resolved_inputs: &mut BTreeMap<String, Value>,
    ) {
        for spec in specs {
            // skip inputs fed by edge connections
            if let Some(Value::Object(obj)) = original_inputs.get(&spec.name) {
                if obj.contains_key("$node") {
                    continue;
                }
            }
            // skip inputs that already have a non-empty value (user override wins)
            match resolved_inputs.get(&spec.name) {
                Some(Value::String(s)) if !s.is_empty() => continue,
                Some(Value::Null) | None => {}
                Some(Value::String(_)) => {} // empty string, fall through
                Some(_) => continue, // non-string non-null value present
            }
            let (_env_name, env_val) = crate::node::resolve_env_for_input(node_type, spec);
            if let Some(val) = env_val {
                let typed = Self::coerce_env_value(&val, &spec.r#type);
                resolved_inputs.insert(spec.name.clone(), typed);
            }
        }
    }

    /// fill missing or empty inputs with spec defaults
    #[allow(dead_code)]
    fn apply_defaults(
        node_impl: &dyn Node,
        resolved_inputs: &mut BTreeMap<String, Value>,
    ) {
        let specs = node_impl.inputs();
        Self::apply_defaults_for(&specs, resolved_inputs);
    }

    fn apply_defaults_for(
        specs: &[crate::node::InputSpec],
        resolved_inputs: &mut BTreeMap<String, Value>,
    ) {
        for spec in specs {
            let needs_default = match resolved_inputs.get(&spec.name) {
                None => true,
                Some(Value::Null) => true,
                Some(Value::String(s)) if s.is_empty() => true,
                _ => false,
            };
            if needs_default {
                if let Some(default) = &spec.default {
                    resolved_inputs.insert(spec.name.clone(), default.clone());
                }
            }
        }
    }

    /// parse an env var string into the appropriate Value type
    fn coerce_env_value(s: &str, data_type: &DataType) -> Value {
        match data_type {
            DataType::Integer => s.parse::<i64>().map(Value::Integer).unwrap_or(Value::String(s.to_string())),
            DataType::Float => s.parse::<f64>().map(Value::Float).unwrap_or(Value::String(s.to_string())),
            DataType::Boolean => match s {
                "true" | "1" | "yes" => Value::Boolean(true),
                "false" | "0" | "no" => Value::Boolean(false),
                _ => Value::String(s.to_string()),
            },
            _ => Value::String(s.to_string()),
        }
    }

    async fn check_cache(
        ctx: &ExecutionContext,
        node_instance: &ExecutionNode,
        cache_key: &str,
    ) -> Option<BTreeMap<String, Value>> {
        // when force_run targets a specific node, only that node skips cache
        let force_skip = match &ctx.force_run_node_id {
            Some(target_id) => target_id == &node_instance.id,
            None => ctx.force_run,
        };
        if !force_skip && !node_instance.skip_cache {
            let r = ctx.cache.read().await;
            if let Some(val) = r.get(cache_key) {
                return Some(val.clone());
            }
        }
        None
    }

    async fn execute_node_logic(
        ctx: &ExecutionContext,
        node_id: &str,
        node_instance: &ExecutionNode,
        node_impl: &dyn Node,
        resolved_inputs: BTreeMap<String, Value>,
        cache_key: String,
    ) {
        info!(node_id = %node_id, node_type = %node_instance.node_type, "executing node");

        // note: Started event is already sent in run_node_task before cache check
        let node_ctx = Self::create_node_context(ctx, node_id);
        let start_time = std::time::Instant::now();

        let result = node_impl
            .execute(resolved_inputs, node_ctx)
            .await
            .with_context(|| format!("failed to execute node {}", node_id));

        match result {
            Ok(output) => {
                let elapsed = start_time.elapsed();
                info!(
                    node_id = %node_id,
                    elapsed_ms = %elapsed.as_millis(),
                    "node completed successfully"
                );

                // update cache
                {
                    let mut w = ctx.cache.write().await;
                    w.insert(cache_key, output.clone());
                }

                Self::complete_node(ctx, node_id, output, false).await;
            }
            Err(e) => {
                Self::report_error(ctx, node_id, e).await;
            }
        }
    }

    async fn resolve_inputs(
        ctx: &ExecutionContext,
        inputs: &BTreeMap<String, Value>,
    ) -> Result<BTreeMap<String, Value>> {
        let mut resolved = BTreeMap::new();
        let results = ctx.results.read().await;
        for (name, val) in inputs {
            resolved.insert(name.clone(), Self::resolve_input_static(val, &results)?);
        }
        Ok(resolved)
    }

    #[allow(dead_code)]
    fn validate_required_inputs(
        node: &dyn Node,
        node_id: &str,
        resolved_inputs: &BTreeMap<String, Value>,
    ) -> Result<()> {
        let specs = node.inputs();
        Self::validate_required_inputs_for(&specs, node_id, resolved_inputs)
    }

    fn validate_required_inputs_for(
        specs: &[crate::node::InputSpec],
        node_id: &str,
        resolved_inputs: &BTreeMap<String, Value>,
    ) -> Result<()> {
        for input_spec in specs {
            if input_spec.required {
                let val = resolved_inputs.get(&input_spec.name);
                match val {
                    None => {
                        return Err(anyhow!(
                            "missing required input '{}' for node {}",
                            input_spec.name,
                            node_id
                        ))
                    }
                    Some(Value::String(s)) if s.is_empty() => {
                        return Err(anyhow!(
                            "Required input '{}' is empty for node {}",
                            input_spec.name,
                            node_id
                        ))
                    }
                    Some(Value::Null) => {
                        return Err(anyhow!(
                            "Required input '{}' is null for node {}",
                            input_spec.name,
                            node_id
                        ))
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn create_node_context(ctx: &ExecutionContext, node_id: &str) -> NodeContext {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<ProgressUpdate>(16);
        let progress_sender = ctx.progress_sender.lock().unwrap().clone();
        let nid = node_id.to_string();

        let h1 = tokio::spawn(async move {
            while let Some(update) = rx.recv().await {
                if let Some(ps) = &progress_sender {
                    let _ = ps
                        .send(ExecutionEvent::Progress {
                            node_id: nid.clone(),
                            progress: update.progress,
                            message: update.message,
                        })
                        .await;
                }
            }
        });

        let (po_tx, mut po_rx) = tokio::sync::mpsc::channel::<PartialOutputUpdate>(64);
        let po_sender = ctx.progress_sender.lock().unwrap().clone();
        let po_nid = node_id.to_string();
        let po_node_map = ctx.node_map.clone();
        let po_registry = ctx.registry.clone();

        let h2 = tokio::spawn(async move {
            while let Some(update) = po_rx.recv().await {
                if let Some(ps) = &po_sender {
                    // emit for the producing node
                    let _ = ps
                        .send(ExecutionEvent::PartialOutput {
                            node_id: po_nid.clone(),
                            output_name: update.output_name.clone(),
                            delta: update.delta.clone(),
                            accumulated: update.accumulated.clone(),
                        })
                        .await;

                    // propagate through downstream passthrough nodes
                    Self::propagate_partial_output(
                        ps,
                        &po_node_map,
                        &po_registry,
                        &po_nid,
                        &update.output_name,
                        &update.delta,
                        &update.accumulated,
                    )
                    .await;
                }
            }
        });

        ctx.forwarding_handles.lock().unwrap().extend([h1, h2]);

        NodeContext::new(ctx.cancel_token.clone(), Some(tx), Some(po_tx), ctx.terse)
    }

    async fn complete_node(
        ctx: &ExecutionContext,
        node_id: &str,
        result: BTreeMap<String, Value>,
        cached: bool,
    ) {
        // store result
        {
            let mut w = ctx.results.write().await;
            w.insert(node_id.to_string(), result.clone());
        }

        // send event
        let ps = ctx.progress_sender.lock().unwrap().clone();
        if let Some(tx) = ps {
            let _ = tx
                .send(ExecutionEvent::Finished {
                    node_id: node_id.to_string(),
                    result,
                    cached,
                })
                .await;
        }

        // update dependencies (neighbors)
        if let Some(neighbors) = ctx.adj.get(node_id) {
            let mut deg = ctx.remaining_deg.write().await;
            for neighbor in neighbors {
                if let Some(d) = deg.get_mut(neighbor) {
                    *d = d.saturating_sub(1);
                }
            }
        }

        let _ = ctx.completion_tx.send(Ok(node_id.to_string())).await;
    }

    async fn report_error(ctx: &ExecutionContext, node_id: &str, error: anyhow::Error) {
        let error_str = format!("{:?}", error);
        warn!(node_id = %node_id, error = %error_str, "node execution error");

        let ps = ctx.progress_sender.lock().unwrap().clone();
        if let Some(tx) = ps {
            let _ = tx
                .send(ExecutionEvent::Error {
                    node_id: node_id.to_string(),
                    error: error_str,
                })
                .await;
        }
        let _ = ctx.completion_tx.send(Err(error)).await;
    }

    // --- Static Logic ---

    fn resolve_input_static(
        val: &Value,
        results: &BTreeMap<String, BTreeMap<String, Value>>,
    ) -> Result<Value> {
        match val {
            Value::Object(obj) => {
                if let (Some(Value::String(node_id)), Some(Value::String(output_name))) =
                    (obj.get("$node"), obj.get("$output"))
                {
                    let source_outputs = results
                        .get(node_id)
                        .ok_or_else(|| anyhow!("missing output from node {}", node_id))?;
                    let val = source_outputs.get(output_name).ok_or_else(|| {
                        anyhow!(
                            "missing output field '{}' from node {}",
                            output_name,
                            node_id
                        )
                    })?;
                    return Ok(val.clone());
                }

                let mut new_obj = BTreeMap::new();
                for (k, v) in obj {
                    new_obj.insert(k.clone(), Self::resolve_input_static(v, results)?);
                }
                Ok(Value::Object(new_obj))
            }
            Value::Array(arr) => {
                let mut out = Vec::new();
                for item in arr {
                    out.push(Self::resolve_input_static(item, results)?);
                }
                Ok(Value::Array(out))
            }
            _ => Ok(val.clone()),
        }
    }

    pub fn calculate_hash_static(node_type: &str, inputs: &BTreeMap<String, Value>) -> String {
        let mut hasher = Sha256::new();
        hasher.update(node_type.as_bytes());

        let mut keys: Vec<&String> = inputs.keys().collect();
        keys.sort();

        for k in keys {
            hasher.update(k.as_bytes());
            let v = inputs.get(k).unwrap();
            if let Ok(json) = serde_json::to_string(v) {
                hasher.update(json.as_bytes());
            }
        }
        hex::encode(hasher.finalize())
    }

    fn collect_dependencies<'a>(val: &'a Value, deps: &mut HashSet<&'a String>) {
        match val {
            Value::Object(obj) => {
                if let (Some(Value::String(node_id)), Some(Value::String(_))) =
                    (obj.get("$node"), obj.get("$output"))
                {
                    deps.insert(node_id);
                }
                for v in obj.values() {
                    Self::collect_dependencies(v, deps);
                }
            }
            Value::Array(arr) => {
                for item in arr {
                    Self::collect_dependencies(item, deps);
                }
            }
            _ => {}
        }
    }

    /// propagate partial output through downstream passthrough nodes
    async fn propagate_partial_output(
        sender: &Sender<ExecutionEvent>,
        node_map: &HashMap<String, ExecutionNode>,
        registry: &NodeRegistry,
        source_node_id: &str,
        output_name: &str,
        delta: &Value,
        accumulated: &Value,
    ) {
        // find downstream nodes whose inputs reference this source output
        for (target_id, target_node) in node_map {
            for (input_name, input_val) in &target_node.inputs {
                if let Value::Object(obj) = input_val {
                    if let (Some(Value::String(ref_node)), Some(Value::String(ref_output))) =
                        (obj.get("$node"), obj.get("$output"))
                    {
                        if ref_node == source_node_id && ref_output == output_name {
                            if let Some(node_impl) = registry.create(&target_node.node_type) {
                                if node_impl.is_stream_passthrough() {
                                    // map to the node's first output name (may differ from input name)
                                    let outputs = node_impl.outputs();
                                    let target_output = outputs
                                        .first()
                                        .map(|o| o.name.clone())
                                        .unwrap_or_else(|| input_name.clone());

                                    let _ = sender
                                        .send(ExecutionEvent::PartialOutput {
                                            node_id: target_id.clone(),
                                            output_name: target_output.clone(),
                                            delta: delta.clone(),
                                            accumulated: accumulated.clone(),
                                        })
                                        .await;

                                    // recurse through further passthroughs
                                    Box::pin(Self::propagate_partial_output(
                                        sender,
                                        node_map,
                                        registry,
                                        target_id,
                                        &target_output,
                                        delta,
                                        accumulated,
                                    ))
                                    .await;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
