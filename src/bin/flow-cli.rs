use clap::{Args, Parser, Subcommand};
use flow_rs::engine::{Engine, ExecutionEvent, NodeRegistry};
use flow_rs::graph::{Workflow, WorkflowEdge, WorkflowNode};
use flow_rs::nodes;
use flow_rs::value::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{IsTerminal, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// a node_id/handle pair parsed from --stdin or --stdout flags
#[derive(Clone, Debug)]
struct PipeTarget {
    node_id: String,
    handle: String,
}

/// expand a pipe target pattern (possibly with globs) against workflow node IDs.
/// eg. "echo_*/output" matches all nodes whose ID starts with "echo_".
fn expand_pipe_targets(pattern: &str, workflow: &Workflow) -> anyhow::Result<Vec<PipeTarget>> {
    let (node_pattern, handle) = pattern.split_once('/').ok_or_else(|| {
        anyhow::anyhow!("invalid pipe target '{}', expected node_id/handle", pattern)
    })?;
    if node_pattern.contains('*') || node_pattern.contains('?') {
        let matched: Vec<PipeTarget> = workflow
            .nodes
            .iter()
            .filter(|n| glob_match(node_pattern, &n.id))
            .map(|n| PipeTarget {
                node_id: n.id.clone(),
                handle: handle.to_string(),
            })
            .collect();
        if matched.is_empty() {
            anyhow::bail!("no nodes matched pattern '{}'", node_pattern);
        }
        Ok(matched)
    } else {
        let resolved = resolve_node_id(node_pattern, workflow)?.to_string();
        Ok(vec![PipeTarget {
            node_id: resolved,
            handle: handle.to_string(),
        }])
    }
}

/// resolve a node id, accepting any unambiguous prefix. exact matches win
/// over prefix matches; ambiguous prefixes report all candidates.
fn resolve_node_id<'a>(needle: &str, workflow: &'a Workflow) -> anyhow::Result<&'a str> {
    if let Some(n) = workflow.nodes.iter().find(|n| n.id == needle) {
        return Ok(&n.id);
    }
    let matches: Vec<&str> = workflow
        .nodes
        .iter()
        .filter(|n| n.id.starts_with(needle))
        .map(|n| n.id.as_str())
        .collect();
    match matches.len() {
        0 => anyhow::bail!("node '{}' not found in workflow", needle),
        1 => Ok(matches[0]),
        _ => anyhow::bail!(
            "node prefix '{}' is ambiguous, matches: {}",
            needle,
            matches.join(", ")
        ),
    }
}

/// simple glob matching supporting * and ? wildcards
fn glob_match(pattern: &str, text: &str) -> bool {
    let mut pi = pattern.chars().peekable();
    let mut ti = text.chars().peekable();
    let mut star_p = None;
    let mut star_t = None;

    loop {
        match (pi.peek(), ti.peek()) {
            (Some('*'), _) => {
                pi.next();
                star_p = Some(pi.clone());
                star_t = Some(ti.clone());
            }
            (Some('?'), Some(_)) => {
                pi.next();
                ti.next();
            }
            (Some(&pc), Some(&tc)) if pc == tc => {
                pi.next();
                ti.next();
            }
            (None, None) => return true,
            _ => {
                if let (Some(sp), Some(mut st)) = (star_p.clone(), star_t.clone()) {
                    st.next();
                    if st.peek().is_none() && pi.peek().is_none() {
                        return true;
                    }
                    star_t = Some(st.clone());
                    pi = sp;
                    ti = st;
                } else {
                    return false;
                }
            }
        }
    }
}

fn trim_trailing_newline(s: &mut String) {
    if s.ends_with('\n') {
        s.pop();
    }
    if s.ends_with('\r') {
        s.pop();
    }
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(args_conflicts_with_subcommands = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// `flow-cli <WORKFLOW>` is shorthand for `flow-cli run <WORKFLOW>`
    #[command(flatten)]
    run: RunArgs,
}

#[derive(Subcommand)]
enum Command {
    /// execute a workflow (default when no subcommand is given)
    Run(RunArgs),
    /// lint workflows for naming + edge inconsistencies; --fix rewrites them
    Lint(LintArgs),
    /// list nodes in a workflow
    Nodes(WorkflowArg),
    /// list inputs and outputs for nodes matching a pattern (default: '*')
    Handles(HandlesArgs),
    /// list env-var bindings (--defaults emits a .env template)
    Env(EnvArgs),
    /// inspect or maintain the on-disk cache
    Cache {
        #[command(subcommand)]
        action: CacheCommand,
    },
}

#[derive(Subcommand)]
enum CacheCommand {
    /// show cache entry count and size; with WORKFLOWS_DIR, show live vs. stale
    Stats(CacheScopeArgs),
    /// remove cache entries not referenced by workflows in WORKFLOWS_DIR
    Prune(CacheScopeArgs),
}

#[derive(Args, Default, Clone)]
struct RunArgs {
    /// path to the workflow JSON file
    #[arg(value_name = "WORKFLOW_FILE")]
    workflow_file: Option<PathBuf>,

    /// specific node to execute (alone or alongside the workflow path)
    #[arg(value_name = "NODE", num_args = 0..=1)]
    node: Option<String>,

    /// path to the cache file
    #[arg(long = "cache-file", default_value = flow_rs::engine::DEFAULT_CACHE_FILE)]
    cache_file: PathBuf,

    /// verbose output (full input/output details per node)
    #[arg(short, long)]
    verbose: bool,

    /// suppress all diagnostic output on stderr
    #[arg(short, long)]
    quiet: bool,

    /// force run, ignoring cache
    #[arg(short, long)]
    force: bool,

    /// save execution state to .state/ and persist --set/--stdin values to workflow
    #[arg(long)]
    save: bool,

    /// set node input values (format: node_id/input_name=value)
    #[arg(short, long = "set", value_name = "NODE_ID/INPUT=VALUE")]
    set_values: Vec<String>,

    /// route stdin to specific node inputs (format: node_id/input_name).
    /// defaults to all Read nodes in the workflow.
    #[arg(long = "stdin", value_name = "NODE_ID/INPUT")]
    stdin_targets: Vec<String>,

    /// route specific node outputs to stdout (format: node_id/output_name).
    /// defaults to terminal Echo nodes in the workflow.
    #[arg(long = "stdout", value_name = "NODE_ID/OUTPUT")]
    stdout_targets: Vec<String>,
}

#[derive(Args, Clone)]
struct LintArgs {
    /// workflow files to lint (default: workflows/*.json)
    #[arg(value_name = "WORKFLOW")]
    workflows: Vec<PathBuf>,

    /// rewrite files to fix issues in place
    #[arg(long)]
    fix: bool,
}

#[derive(Args, Clone)]
struct WorkflowArg {
    #[arg(value_name = "WORKFLOW_FILE")]
    workflow_file: PathBuf,
}

#[derive(Args, Clone)]
struct HandlesArgs {
    #[arg(value_name = "WORKFLOW_FILE")]
    workflow_file: PathBuf,
    /// node id pattern (supports * and ?), default '*'
    #[arg(value_name = "PATTERN", default_value = "*")]
    pattern: String,
}

#[derive(Args, Clone)]
struct EnvArgs {
    /// emit a .env-style template with every known env var, commented out
    #[arg(long)]
    defaults: bool,
}

#[derive(Args, Clone)]
struct CacheScopeArgs {
    /// workflows directory used to determine which cache entries are live
    #[arg(value_name = "WORKFLOWS_DIR", default_value = "workflows")]
    workflows_dir: PathBuf,
    /// path to the cache file
    #[arg(long = "cache-file", default_value = flow_rs::engine::DEFAULT_CACHE_FILE)]
    cache_file: PathBuf,
}

/// collect default stdin targets: all Read nodes without pre-filled input
fn default_stdin_targets(workflow: &Workflow) -> Vec<PipeTarget> {
    workflow
        .nodes
        .iter()
        .filter(|n| n.node_type == "Read" && !n.inputs.contains_key("input"))
        .map(|n| PipeTarget {
            node_id: n.id.clone(),
            handle: "input".to_string(),
        })
        .collect()
}

/// collect default stdout targets: terminal Echo nodes
fn default_stdout_targets(
    workflow: &Workflow,
    connected_sources: &HashSet<String>,
) -> Vec<PipeTarget> {
    workflow
        .nodes
        .iter()
        .filter(|n| n.node_type == "Echo" && !connected_sources.contains(&n.id))
        .map(|n| PipeTarget {
            node_id: n.id.clone(),
            handle: "output".to_string(),
        })
        .collect()
}

/// read stdin for pipe targets: piped mode reads all at once, interactive
/// mode prompts per target with ctrl-d to complete each input
fn read_stdin_for_targets(
    workflow: &mut Workflow,
    targets: &[PipeTarget],
    edge_context: &HashMap<String, (String, String)>,
) -> anyhow::Result<()> {
    if targets.is_empty() {
        return Ok(());
    }

    if !std::io::stdin().is_terminal() {
        let mut buffer = String::new();
        std::io::stdin().read_to_string(&mut buffer)?;
        trim_trailing_newline(&mut buffer);
        for target in targets {
            if let Some(node) = workflow.nodes.iter_mut().find(|n| n.id == target.node_id) {
                node.inputs
                    .insert(target.handle.clone(), Value::String(buffer.clone()));
            }
        }
    } else {
        use std::io::Write;
        for target in targets {
            let context = edge_context
                .get(&target.node_id)
                .map(|(node_type, handle)| format!("{}/{}", node_type, handle))
                .unwrap_or_else(|| "input".to_string());
            eprint!("[{}] {} (^D to complete): ", target.node_id, context);
            std::io::stderr().flush()?;

            let mut buffer = String::new();
            std::io::stdin().read_to_string(&mut buffer)?;
            trim_trailing_newline(&mut buffer);

            if let Some(node) = workflow.nodes.iter_mut().find(|n| n.id == target.node_id) {
                node.inputs
                    .insert(target.handle.clone(), Value::String(buffer));
            }
            // newline after ^D so the next prompt starts on a fresh line
            eprintln!();
        }
    }

    Ok(())
}

/// build a map from source node_id to (downstream_node_type, downstream_handle)
/// for generating interactive prompts
fn build_edge_context(workflow: &Workflow) -> HashMap<String, (String, String)> {
    let node_types: HashMap<&str, &str> = workflow
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.node_type.as_str()))
        .collect();

    workflow
        .edges
        .iter()
        .filter(|e| e.source_handle == "output")
        .filter_map(|e| {
            let target_type = node_types.get(e.target.as_str())?;
            Some((
                e.source.clone(),
                (target_type.to_string(), e.target_handle.clone()),
            ))
        })
        .collect()
}

/// validate that pipe targets reference the correct handle direction.
/// stdin targets must be inputs, stdout targets must be outputs.
/// when the handle is on the wrong side, suggest the connected edge.
fn validate_pipe_handles(
    targets: &[PipeTarget],
    workflow: &Workflow,
    state: &flow_rs::engine::ResultMap,
    registry: &NodeRegistry,
    direction: &str,
) -> anyhow::Result<()> {
    for target in targets {
        let node = match workflow.nodes.iter().find(|n| n.id == target.node_id) {
            Some(n) => n,
            None => continue,
        };
        let node_impl = match registry.create(&node.node_type) {
            Some(n) => n,
            None => continue,
        };
        // Read and Echo are special: Read accepts injected "input",
        // Echo is the default stdout target with "output"
        let is_io_node = node.node_type == "Read" || node.node_type == "Echo";
        if is_io_node {
            continue;
        }
        // for nodes with dynamic ports (e.g. DynamicUserNode) we resolve
        // inputs that drive the spec from the workflow's saved state when
        // they're wired rather than set literally, so dynamic ports show up.
        let resolved_for_spec = workflow.resolve_spec_inputs(node, state);
        let input_specs = flow_rs::node::effective_inputs(&*node_impl, &resolved_for_spec);
        let output_specs = flow_rs::node::effective_outputs(&*node_impl, &resolved_for_spec);
        let input_names: Vec<String> = input_specs.iter().map(|s| s.name.clone()).collect();
        let output_names: Vec<String> = output_specs.iter().map(|s| s.name.clone()).collect();
        if direction == "input" {
            if output_names.contains(&target.handle) {
                let suggestion = workflow.edges.iter()
                    .find(|e| e.source == target.node_id && e.source_handle == target.handle)
                    .map(|e| format!("\nconnected to {}/{}. did you mean: --stdin {}/{}", e.target, e.target_handle, e.target, e.target_handle));
                anyhow::bail!(
                    "'{}' is an output on node '{}', not an input. available inputs: {}{}",
                    target.handle, target.node_id, input_names.join(", "),
                    suggestion.unwrap_or_default()
                );
            }
            if !input_names.contains(&target.handle) {
                anyhow::bail!(
                    "node '{}' has no input '{}'. available inputs: {}",
                    target.node_id, target.handle, input_names.join(", ")
                );
            }
        } else {
            if input_names.contains(&target.handle) {
                let suggestion = workflow.edges.iter()
                    .find(|e| e.target == target.node_id && e.target_handle == target.handle)
                    .map(|e| format!("\nconnected from {}/{}. did you mean: --stdout {}/{}", e.source, e.source_handle, e.source, e.source_handle));
                anyhow::bail!(
                    "'{}' is an input on node '{}', not an output. available outputs: {}{}",
                    target.handle, target.node_id, output_names.join(", "),
                    suggestion.unwrap_or_default()
                );
            }
            if !output_names.contains(&target.handle) {
                anyhow::bail!(
                    "node '{}' has no output '{}'. available outputs: {}",
                    target.node_id, target.handle, output_names.join(", ")
                );
            }
        }
    }
    Ok(())
}

const BOX_MIN_INNER_WIDTH: usize = 48;
const BOX_DEFAULT_INNER_WIDTH: usize = 68;
// box chrome: "│ " + " │" = 4 chars
const BOX_CHROME: usize = 4;

fn terminal_width() -> usize {
    // try COLUMNS env first, then ioctl on stderr
    if let Ok(cols) = std::env::var("COLUMNS") {
        if let Ok(w) = cols.parse::<usize>() {
            return w;
        }
    }
    #[cfg(unix)]
    {
        use std::mem::MaybeUninit;
        unsafe {
            let mut ws = MaybeUninit::<libc::winsize>::zeroed().assume_init();
            if libc::ioctl(2, libc::TIOCGWINSZ, &mut ws) == 0 && ws.ws_col > 0 {
                return ws.ws_col as usize;
            }
        }
    }
    BOX_DEFAULT_INNER_WIDTH + BOX_CHROME
}

fn format_value(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Boolean(b) => b.to_string(),
        Value::Integer(n) => n.to_string(),
        Value::Float(f) => f.to_string(),
        Value::String(s) => s.clone(),
        Value::Array(_) | Value::Object(_) => {
            serde_json::to_string(value).unwrap_or_else(|_| format!("{:?}", value))
        }
        Value::File(f) => f.path.clone(),
    }
}

/// split on newlines, then wrap each line to max_width
fn wrap_value_for_table(s: &str, max_width: usize) -> Vec<String> {
    let result: Vec<String> = s.lines()
        .flat_map(|line| wrap_str(line, max_width))
        .collect();
    if result.is_empty() {
        vec![String::new()]
    } else {
        result
    }
}

fn wrap_str(s: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![s.to_string()];
    }
    let mut lines = Vec::new();
    let mut remaining = s;
    while display_width(remaining) > max_width {
        // find byte offset of the max_width-th char
        let end: usize = remaining
            .char_indices()
            .nth(max_width)
            .map(|(i, _)| i)
            .unwrap_or(remaining.len());
        lines.push(remaining[..end].to_string());
        remaining = &remaining[end..];
    }
    lines.push(remaining.to_string());
    lines
}

/// approximate display width (counts chars, not bytes)
fn display_width(s: &str) -> usize {
    s.chars().count()
}

fn total_inner_width() -> usize {
    let tw = terminal_width();
    if tw >= BOX_CHROME {
        (tw - BOX_CHROME).max(BOX_MIN_INNER_WIDTH)
    } else {
        BOX_DEFAULT_INNER_WIDTH
    }
}


fn table_separator(col_widths: &[usize]) -> String {
    let mut out = String::from("+");
    for &w in col_widths {
        out.push_str(&"-".repeat(w + 2));
        out.push('+');
    }
    out
}


fn table_row(cells: &[&str], col_widths: &[usize]) -> String {
    let mut out = String::from("|");
    for (i, &cell) in cells.iter().enumerate() {
        let w = col_widths.get(i).copied().unwrap_or(0);
        let pad = w.saturating_sub(display_width(cell));
        out.push_str(&format!(" {}{} ", cell, " ".repeat(pad)));
        out.push('|');
    }
    out
}

/// centered text in each cell
fn table_header(cells: &[&str], col_widths: &[usize]) -> String {
    let mut out = String::from("|");
    for (i, &cell) in cells.iter().enumerate() {
        let w = col_widths.get(i).copied().unwrap_or(0);
        let cell_len = display_width(cell);
        let left = w.saturating_sub(cell_len) / 2;
        let right = w.saturating_sub(cell_len + left);
        out.push_str(&format!(" {}{}{} ", " ".repeat(left), cell, " ".repeat(right)));
        out.push('|');
    }
    out
}


fn print_value_to_stdout(value: &Value) {
    print!("{}", format_value(value));
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install ctrl+c handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install sigterm handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
}

/// returns true if a value contains no $node/$output references (i.e. is fully static)
fn is_static_input(val: &Value) -> bool {
    match val {
        Value::Object(obj) => {
            if obj.contains_key("$node") && obj.contains_key("$output") {
                return false;
            }
            obj.values().all(is_static_input)
        }
        Value::Array(arr) => arr.iter().all(is_static_input),
        _ => true,
    }
}

/// classify cache entries as live (referenced by workflows in `workflows_dir`)
/// or stale. returns (live, stale) counts plus the live key set for prune.
fn classify_cache_entries(
    cache: &HashMap<String, serde_json::Value>,
    workflows_dir: &PathBuf,
) -> anyhow::Result<(HashSet<String>, usize, usize)> {
    if !workflows_dir.exists() || !workflows_dir.is_dir() {
        anyhow::bail!("workflows directory {:?} does not exist", workflows_dir);
    }

    let mut registry = NodeRegistry::new();
    nodes::register_all(&mut registry, true);
    let registry = Arc::new(registry);

    let mut live_keys: HashSet<String> = HashSet::new();
    for entry in std::fs::read_dir(workflows_dir)?.flatten() {
        let path = entry.path();
        if path.extension().map(|e| e == "json").unwrap_or(false) {
            match Workflow::load(&path) {
                Ok(mut workflow) => {
                    workflow.normalize(&registry);
                    let exec = workflow.to_execution();
                    for node in &exec.nodes {
                        let all_static = node.inputs.values().all(is_static_input);
                        if all_static {
                            let key = Engine::calculate_hash_static(&node.node_type, &node.inputs);
                            live_keys.insert(key);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("warning: skipping {:?}: {}", path, e);
                }
            }
        }
    }

    let live = cache.keys().filter(|k| live_keys.contains(*k)).count();
    let stale = cache.len() - live;
    Ok((live_keys, live, stale))
}

fn run_cache_stats(args: &CacheScopeArgs) -> anyhow::Result<()> {
    let metadata = std::fs::metadata(&args.cache_file);
    let size_bytes = metadata.as_ref().map(|m| m.len()).unwrap_or(0);

    let cache: HashMap<String, serde_json::Value> = if args.cache_file.exists() {
        let content = std::fs::read_to_string(&args.cache_file)?;
        serde_json::from_str(&content)?
    } else {
        HashMap::new()
    };

    let (size, unit) = if size_bytes >= 1024 * 1024 {
        (size_bytes as f64 / (1024.0 * 1024.0), "MB")
    } else if size_bytes >= 1024 {
        (size_bytes as f64 / 1024.0, "KB")
    } else {
        (size_bytes as f64, "B")
    };

    eprintln!("cache file: {:?}", args.cache_file);
    eprintln!("entries:    {}", cache.len());
    eprintln!("size:       {:.1} {}", size, unit);

    // if workflows dir was given (or default exists), show live/stale breakdown
    if args.workflows_dir.exists() {
        let (_keys, live, stale) = classify_cache_entries(&cache, &args.workflows_dir)?;
        eprintln!("live:       {} (referenced by {:?})", live, args.workflows_dir);
        eprintln!("stale:      {} (would be removed by `cache prune`)", stale);
    }
    Ok(())
}

fn run_cache_prune(args: &CacheScopeArgs) -> anyhow::Result<()> {
    if !args.cache_file.exists() {
        eprintln!("cache file {:?} does not exist, nothing to prune", args.cache_file);
        return Ok(());
    }

    let content = std::fs::read_to_string(&args.cache_file)?;
    let mut cache: HashMap<String, serde_json::Value> = serde_json::from_str(&content)?;
    let (live_keys, _live, _stale) = classify_cache_entries(&cache, &args.workflows_dir)?;
    let total = cache.len();
    cache.retain(|key, _| live_keys.contains(key));
    let pruned = total - cache.len();

    let new_content = serde_json::to_string_pretty(&cache)?;
    std::fs::write(&args.cache_file, new_content)?;

    eprintln!(
        "pruned {} of {} cache entries ({} retained from {} workflows)",
        pruned,
        total,
        cache.len(),
        live_keys.len()
    );
    Ok(())
}

/// emit a .env-style template keyed by every env var the registry knows about.
/// each entry has a comment with the node/input it binds to and the input's
/// description, then a commented-out KEY= line ready to uncomment.
fn run_env_defaults() -> anyhow::Result<()> {
    let mut registry = NodeRegistry::new();
    nodes::register_all(&mut registry, true);
    let metadata = registry.list_metadata();

    // env var -> list of (node, input, description) bindings
    let mut entries: BTreeMap<String, Vec<(String, String, Option<String>)>> = BTreeMap::new();

    for meta in &metadata {
        let instance = registry.create(&meta.name);
        let original_specs = instance.as_ref().map(|n| n.inputs()).unwrap_or_default();

        for (i, input) in meta.inputs.iter().enumerate() {
            let desc = input
                .description
                .clone()
                .or_else(|| original_specs.get(i).and_then(|s| s.description.clone()));
            let mut names: Vec<String> =
                vec![flow_rs::node::auto_env_var_name(&meta.name, &input.name)];
            if let Some(alias) = original_specs.get(i).and_then(|s| s.env_var.as_deref()) {
                if !names.iter().any(|n| n == alias) {
                    names.push(alias.to_string());
                }
            }
            if let Some(resolved) = &input.env_var {
                if !names.iter().any(|n| n == resolved) {
                    names.push(resolved.clone());
                }
            }
            for name in names {
                entries
                    .entry(name)
                    .or_default()
                    .push((meta.name.clone(), input.name.clone(), desc.clone()));
            }
        }
    }

    println!("# generated by `flow-cli env --defaults`");
    println!("# uncomment and set values to override node inputs via environment");
    println!();
    for (env_name, bindings) in &entries {
        let (node, input, desc) = bindings
            .iter()
            .find(|(_, _, d)| d.is_some())
            .unwrap_or(&bindings[0]);
        if let Some(d) = desc {
            println!("# {}", d);
        }
        if bindings.len() == 1 {
            println!("# binds: {}.{}", node, input);
        } else {
            let joined = bindings
                .iter()
                .map(|(n, i, _)| format!("{}.{}", n, i))
                .collect::<Vec<_>>()
                .join(", ");
            println!("# binds: {}", joined);
        }
        println!("#{}=", env_name);
        println!();
    }

    Ok(())
}

fn run_env_list() -> anyhow::Result<()> {
    let mut registry = NodeRegistry::new();
    nodes::register_all(&mut registry, true);

    let metadata = registry.list_metadata();
    let mut rows: Vec<(String, String, String)> = Vec::new();
    for meta in &metadata {
        let instance = registry.create(&meta.name);
        let original_specs = instance.as_ref().map(|n| n.inputs()).unwrap_or_default();

        for (i, input) in meta.inputs.iter().enumerate() {
            let auto_name = flow_rs::node::auto_env_var_name(&meta.name, &input.name);
            let mut seen = HashSet::new();
            seen.insert(auto_name.clone());
            rows.push((auto_name.clone(), meta.name.clone(), input.name.clone()));
            if let Some(alias) = original_specs.get(i).and_then(|s| s.env_var.as_deref()) {
                if seen.insert(alias.to_string()) {
                    rows.push((alias.to_string(), meta.name.clone(), input.name.clone()));
                }
            }
            if let Some(resolved) = &input.env_var {
                if seen.insert(resolved.clone()) {
                    rows.push((resolved.clone(), meta.name.clone(), input.name.clone()));
                }
            }
        }
    }

    let col_env = rows.iter().map(|r| display_width(&r.0)).max().unwrap_or(0)
        .max("ENV VAR".len());
    let col_node = rows.iter().map(|r| display_width(&r.1)).max().unwrap_or(0)
        .max("NODE".len());
    let col_input = rows.iter().map(|r| display_width(&r.2)).max().unwrap_or(0)
        .max("INPUT".len());
    let cols = [col_env, col_node, col_input];

    eprintln!("{}", table_separator(&cols));
    eprintln!("{}", table_header(&["ENV VAR", "NODE", "INPUT"], &cols));
    eprintln!("{}", table_separator(&cols));
    for (env_name, node, input) in &rows {
        eprintln!("{}", table_row(&[env_name, node, input], &cols));
    }
    eprintln!("{}", table_separator(&cols));
    eprintln!("\n{} env vars across {} node types", rows.len(), metadata.len());

    Ok(())
}

fn run_nodes(args: &WorkflowArg) -> anyhow::Result<()> {
    let workflow = Workflow::load(&args.workflow_file)?;
    let max_term = total_inner_width();
    let col_id = workflow.nodes.iter()
        .map(|n| n.id.len())
        .max().unwrap_or(0)
        .max("NODE ID".len());
    let col_type = workflow.nodes.iter()
        .map(|n| n.node_type.len())
        .max().unwrap_or(0)
        .max("TYPE".len());
    let needed = col_id + col_type + 3;
    let cols = if needed > max_term {
        let shrunk_type = max_term.saturating_sub(col_id + 3);
        [col_id, shrunk_type]
    } else {
        [col_id, col_type]
    };
    eprintln!(
        "\nworkflow: {}\n",
        args.workflow_file.file_stem().unwrap_or_default().to_string_lossy()
    );
    eprintln!("{}", table_separator(&cols));
    eprintln!("{}", table_header(&["NODE ID", "TYPE"], &cols));
    eprintln!("{}", table_separator(&cols));
    for node in &workflow.nodes {
        eprintln!("{}", table_row(&[&node.id, &node.node_type], &cols));
        eprintln!("{}", table_separator(&cols));
    }
    Ok(())
}

fn run_handles(args: &HandlesArgs) -> anyhow::Result<()> {
    let workflow = Workflow::load(&args.workflow_file)?;
    let mut registry = NodeRegistry::new();
    nodes::register_all(&mut registry, true);

    let max_term = total_inner_width();
    // pattern semantics: with * or ?, glob match. otherwise, accept any
    // unambiguous prefix (exact match wins) — same shape as --set / --stdin.
    let has_glob = args.pattern.contains('*') || args.pattern.contains('?');
    let matched: Vec<_> = if has_glob {
        workflow
            .nodes
            .iter()
            .filter(|n| glob_match(&args.pattern, &n.id))
            .collect()
    } else if let Some(exact) = workflow.nodes.iter().find(|n| n.id == args.pattern) {
        vec![exact]
    } else {
        workflow
            .nodes
            .iter()
            .filter(|n| n.id.starts_with(&args.pattern))
            .collect()
    };
    if matched.is_empty() {
        anyhow::bail!("no nodes matched '{}'", args.pattern);
    }
    let saved_state = Workflow::load_state(&args.workflow_file);
    let col_dir = 3;
    for node in &matched {
        let node_impl = registry.create(&node.node_type);
        let resolved_for_spec = workflow.resolve_spec_inputs(node, &saved_state);
        let input_specs = node_impl
            .as_ref()
            .map(|n| flow_rs::node::effective_inputs(&**n, &resolved_for_spec))
            .unwrap_or_default();
        let output_specs = node_impl
            .as_ref()
            .map(|n| flow_rs::node::effective_outputs(&**n, &resolved_for_spec))
            .unwrap_or_default();

        struct HandleRow {
            dir: &'static str,
            name: String,
            type_str: String,
            value: String,
        }
        let mut rows: Vec<HandleRow> = Vec::new();
        for spec in &input_specs {
            let req = if spec.required { "*" } else { "" };
            rows.push(HandleRow {
                dir: "in",
                name: format!("{}{}", req, spec.name),
                type_str: format!("{:?}", spec.r#type),
                value: node.inputs.get(&spec.name)
                    .map(format_value)
                    .unwrap_or_else(|| "-".to_string()),
            });
        }
        for spec in &output_specs {
            let value = saved_state
                .get(&node.id)
                .and_then(|outputs| outputs.get(&spec.name))
                .map(format_value)
                .unwrap_or_default();
            rows.push(HandleRow {
                dir: "out",
                name: spec.name.clone(),
                type_str: format!("{:?}", spec.r#type),
                value,
            });
        }

        let col_name = rows.iter().map(|r| display_width(&r.name)).max().unwrap_or(0)
            .max("NAME".len());
        let col_type = rows.iter().map(|r| display_width(&r.type_str)).max().unwrap_or(0)
            .max("TYPE".len());

        let fixed = col_dir + col_name + col_type + 12;
        let max_value_col = max_term.saturating_sub(fixed);
        let longest_value = rows.iter()
            .flat_map(|r| r.value.lines().map(display_width))
            .max().unwrap_or(0)
            .max("VALUE".len());
        let col_value = longest_value.min(max_value_col);
        let cols = [col_dir, col_name, col_type, col_value];

        eprintln!("\n{} ({})\n", node.id, node.node_type);
        eprintln!("{}", table_separator(&cols));
        eprintln!("{}", table_header(&["I/O", "NAME", "TYPE", "VALUE"], &cols));
        eprintln!("{}", table_separator(&cols));
        for row in &rows {
            let chunks = wrap_value_for_table(&row.value, col_value);
            for (i, chunk) in chunks.iter().enumerate() {
                if i == 0 {
                    eprintln!("{}", table_row(&[row.dir, &row.name, &row.type_str, chunk], &cols));
                } else {
                    eprintln!("{}", table_row(&["", "", "", chunk], &cols));
                }
            }
            eprintln!("{}", table_separator(&cols));
        }
    }
    Ok(())
}

// === lint ===

/// canonical node id: `{type.to_lowercase()}_{8 hex chars}`.
/// the 8-char suffix is preserved if the old id has one, else derived from
/// sha256(old_id) so re-runs are idempotent.
fn canonical_node_id(node_type: &str, old_id: &str) -> String {
    let prefix = node_type.to_lowercase();
    if let Some((_, suffix)) = old_id.rsplit_once('_') {
        if suffix.len() == 8 && suffix.chars().all(|c| c.is_ascii_hexdigit()) {
            return format!("{}_{}", prefix, suffix);
        }
    }
    let mut hasher = Sha256::new();
    hasher.update(old_id.as_bytes());
    let digest = hasher.finalize();
    let hex: String = digest.iter().take(4).map(|b| format!("{:02x}", b)).collect();
    format!("{}_{}", prefix, hex)
}

fn canonical_edge_id(edge: &WorkflowEdge) -> String {
    let src = if edge.source_handle.is_empty() { "output" } else { &edge.source_handle };
    let tgt = if edge.target_handle.is_empty() { "input" } else { &edge.target_handle };
    format!("e-{}-{}-{}-{}", edge.source, src, edge.target, tgt)
}

#[derive(Default)]
struct LintReport {
    issues: Vec<String>,
    modified: bool,
}

impl LintReport {
    fn issue(&mut self, msg: String) {
        self.issues.push(msg);
    }
}

fn lint_one(path: &Path, registry: &NodeRegistry, fix: bool) -> anyhow::Result<LintReport> {
    let mut workflow = Workflow::load(path)?;
    let saved_state = Workflow::load_state(path);
    let mut report = LintReport::default();

    // 1. plan node id renames
    let mut rename: HashMap<String, String> = HashMap::new();
    let mut used: HashSet<String> = HashSet::new();
    // first pass: keep ids that already match canonical form
    for n in &workflow.nodes {
        let canon = canonical_node_id(&n.node_type, &n.id);
        if canon == n.id {
            used.insert(n.id.clone());
        }
    }
    // second pass: assign new ids for mismatched, breaking collisions deterministically
    for n in &workflow.nodes {
        let canon = canonical_node_id(&n.node_type, &n.id);
        if canon == n.id {
            continue;
        }
        let mut candidate = canon.clone();
        let mut salt = 0usize;
        while used.contains(&candidate) {
            salt += 1;
            let mut hasher = Sha256::new();
            hasher.update(format!("{}#{}", n.id, salt).as_bytes());
            let digest = hasher.finalize();
            let hex: String =
                digest.iter().take(4).map(|b| format!("{:02x}", b)).collect();
            candidate = format!("{}_{}", n.node_type.to_lowercase(), hex);
        }
        used.insert(candidate.clone());
        report.issue(format!(
            "node id '{}' (type {}) -> '{}'",
            n.id, n.node_type, candidate
        ));
        rename.insert(n.id.clone(), candidate);
    }

    // apply node renames
    if !rename.is_empty() {
        for n in &mut workflow.nodes {
            if let Some(new) = rename.get(&n.id) {
                n.id = new.clone();
            }
        }
        if let Some(tgt) = workflow.target_node_id.as_mut() {
            if let Some(new) = rename.get(tgt) {
                *tgt = new.clone();
            }
        }
        report.modified = true;
    }

    // 2. node-id -> WorkflowNode lookup for handle validation
    let nodes_by_id: HashMap<String, &WorkflowNode> = workflow
        .nodes
        .iter()
        .map(|n| (n.id.clone(), n))
        .collect();

    // 3. detect dead inputs (keys in node.inputs not in spec). collect names per node first
    let mut dead_inputs_by_node: HashMap<String, Vec<String>> = HashMap::new();
    for n in &workflow.nodes {
        let node_impl = match registry.create(&n.node_type) {
            Some(impl_) => impl_,
            None => {
                report.issue(format!(
                    "node '{}' has unknown type '{}'",
                    n.id, n.node_type
                ));
                continue;
            }
        };
        let resolved = workflow.resolve_spec_inputs(n, &saved_state);
        // when a node has dynamic ports but we couldn't resolve the inputs
        // that drive them (e.g. wired control inputs with no saved state),
        // we can't tell which ports are valid -- skip dead-input detection
        // for this node rather than reporting false positives.
        let dynamic_unresolved =
            node_impl.has_dynamic_spec() && node_impl.dynamic_spec(&resolved).is_none();
        if dynamic_unresolved {
            continue;
        }
        let input_specs = flow_rs::node::effective_inputs(&*node_impl, &resolved);
        let input_names: HashSet<String> =
            input_specs.iter().map(|s| s.name.clone()).collect();
        let mut dead: Vec<String> = Vec::new();
        for key in n.inputs.keys() {
            if !input_names.contains(key) {
                dead.push(key.clone());
            }
        }
        if !dead.is_empty() {
            for k in &dead {
                report.issue(format!(
                    "node '{}' has dead input '{}' (not in spec for type {})",
                    n.id, k, n.node_type
                ));
            }
            dead_inputs_by_node.insert(n.id.clone(), dead);
        }
    }

    // 4. process edges: rewrite endpoints, drop dangling, canonicalize ids,
    //    detect unknown handles
    let mut new_edges: Vec<WorkflowEdge> = Vec::new();
    let original_edges = workflow.edges.clone();
    for e in &original_edges {
        let mut edge = e.clone();
        if let Some(new) = rename.get(&edge.source) {
            edge.source = new.clone();
        }
        if let Some(new) = rename.get(&edge.target) {
            edge.target = new.clone();
        }
        if !nodes_by_id.contains_key(&edge.source) {
            report.issue(format!(
                "dangling edge '{}': source '{}' not found",
                e.id, e.source
            ));
            continue;
        }
        if !nodes_by_id.contains_key(&edge.target) {
            report.issue(format!(
                "dangling edge '{}': target '{}' not found",
                e.id, e.target
            ));
            continue;
        }

        // check handles against spec. for nodes with unresolvable dynamic
        // ports (e.g. DynamicUserNode whose code input is wired but has no
        // saved state), skip validation rather than report false positives.
        let mut bad_handle = false;
        if let Some(src_node) = nodes_by_id.get(&edge.source) {
            if let Some(impl_) = registry.create(&src_node.node_type) {
                let resolved = workflow.resolve_spec_inputs(src_node, &saved_state);
                let dynamic_unresolved =
                    impl_.has_dynamic_spec() && impl_.dynamic_spec(&resolved).is_none();
                if !dynamic_unresolved {
                    let outputs = flow_rs::node::effective_outputs(&*impl_, &resolved);
                    if !outputs.iter().any(|s| s.name == edge.source_handle) {
                        report.issue(format!(
                            "edge '{}' references unknown output '{}' on node '{}' ({})",
                            e.id, edge.source_handle, edge.source, src_node.node_type
                        ));
                        bad_handle = true;
                    }
                }
            }
        }
        if let Some(tgt_node) = nodes_by_id.get(&edge.target) {
            if let Some(impl_) = registry.create(&tgt_node.node_type) {
                let resolved = workflow.resolve_spec_inputs(tgt_node, &saved_state);
                let dynamic_unresolved =
                    impl_.has_dynamic_spec() && impl_.dynamic_spec(&resolved).is_none();
                if !dynamic_unresolved {
                    let inputs = flow_rs::node::effective_inputs(&*impl_, &resolved);
                    if !inputs.iter().any(|s| s.name == edge.target_handle) {
                        report.issue(format!(
                            "edge '{}' references unknown input '{}' on node '{}' ({})",
                            e.id, edge.target_handle, edge.target, tgt_node.node_type
                        ));
                        bad_handle = true;
                    }
                }
            }
        }
        if bad_handle {
            // drop it on --fix; the edge cannot fire anyway
            continue;
        }

        let canonical = canonical_edge_id(&edge);
        if edge.id != canonical {
            report.issue(format!("edge id '{}' -> '{}'", edge.id, canonical));
            edge.id = canonical;
        }
        new_edges.push(edge);
    }

    if new_edges.len() != workflow.edges.len()
        || new_edges.iter().zip(workflow.edges.iter()).any(|(a, b)| a.id != b.id)
    {
        report.modified = true;
    }
    workflow.edges = new_edges;

    // remove dead inputs
    if !dead_inputs_by_node.is_empty() {
        for n in &mut workflow.nodes {
            if let Some(dead) = dead_inputs_by_node.get(&n.id) {
                for k in dead {
                    n.inputs.remove(k);
                }
            }
        }
        report.modified = true;
    }

    if fix && report.modified {
        workflow.save(path)?;
    }

    Ok(report)
}

fn run_lint(args: &LintArgs) -> anyhow::Result<()> {
    let paths: Vec<PathBuf> = if args.workflows.is_empty() {
        let mut p: Vec<PathBuf> = std::fs::read_dir("workflows")
            .map_err(|e| anyhow::anyhow!("cannot read workflows/: {}", e))?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().map(|e| e == "json").unwrap_or(false))
            .collect();
        p.sort();
        p
    } else {
        args.workflows.clone()
    };
    if paths.is_empty() {
        eprintln!("no workflows to lint");
        return Ok(());
    }

    let mut registry = NodeRegistry::new();
    nodes::register_all(&mut registry, true);

    let mut total_issues = 0usize;
    let mut total_fixed = 0usize;
    for p in &paths {
        let report = lint_one(p, &registry, args.fix)?;
        if report.issues.is_empty() {
            continue;
        }
        total_issues += report.issues.len();
        let tag = if args.fix && report.modified { "FIXED" } else { "WARN " };
        eprintln!("{} {}", tag, p.display());
        for i in &report.issues {
            eprintln!("  - {}", i);
        }
        if args.fix && report.modified {
            total_fixed += 1;
        }
    }

    eprintln!();
    if args.fix {
        eprintln!("{} issues across {} files fixed", total_issues, total_fixed);
        Ok(())
    } else if total_issues == 0 {
        eprintln!("no issues");
        Ok(())
    } else {
        eprintln!("{} issues found; re-run with --fix to apply", total_issues);
        std::process::exit(1);
    }
}

async fn run() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Run(args)) => run_workflow(args).await,
        Some(Command::Lint(args)) => run_lint(&args),
        Some(Command::Nodes(args)) => run_nodes(&args),
        Some(Command::Handles(args)) => run_handles(&args),
        Some(Command::Env(args)) => {
            if args.defaults {
                run_env_defaults()
            } else {
                run_env_list()
            }
        }
        Some(Command::Cache { action }) => match action {
            CacheCommand::Stats(args) => run_cache_stats(&args),
            CacheCommand::Prune(args) => run_cache_prune(&args),
        },
        None => run_workflow(cli.run).await,
    }
}

async fn run_workflow(cli: RunArgs) -> anyhow::Result<()> {
    let workflow_file = cli.workflow_file.as_ref().ok_or_else(|| {
        anyhow::anyhow!(
            "WORKFLOW_FILE is required. see `flow-cli --help` for subcommands."
        )
    })?;
    let mut workflow = Workflow::load(workflow_file)?;

    // apply --set overrides before stdin handling so Read nodes can be pre-filled
    let mut set_targets: HashSet<String> = HashSet::new();
    for set_arg in &cli.set_values {
        let (node_path, value) = set_arg.split_once('=').ok_or_else(|| {
            anyhow::anyhow!(
                "invalid --set format '{}', expected node_id/input=value",
                set_arg
            )
        })?;
        let (node_id, input_name) = node_path.split_once('/').ok_or_else(|| {
            anyhow::anyhow!(
                "invalid --set format '{}', expected node_id/input=value",
                set_arg
            )
        })?;
        let resolved_id = resolve_node_id(node_id, &workflow)?.to_string();
        let node = workflow
            .nodes
            .iter_mut()
            .find(|n| n.id == resolved_id)
            .expect("resolve_node_id returned an id not in workflow");
        let parsed: Value =
            serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_string()));
        node.inputs.insert(input_name.to_string(), parsed);
        set_targets.insert(format!("{}/{}", resolved_id, input_name));
    }

    // resolve stdin targets
    let stdin_targets: Vec<PipeTarget> = if cli.stdin_targets.is_empty() {
        default_stdin_targets(&workflow)
    } else {
        cli.stdin_targets
            .iter()
            .map(|s| expand_pipe_targets(s, &workflow))
            .collect::<anyhow::Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect()
    };

    // skip stdin targets already filled by --set
    let stdin_targets: Vec<PipeTarget> = stdin_targets
        .into_iter()
        .filter(|t| {
            let key = format!("{}/{}", t.node_id, t.handle);
            if set_targets.contains(&key) {
                eprintln!("warning: skipping --stdin target '{}' (already filled by --set)", key);
                false
            } else {
                true
            }
        })
        .collect();

    if !cli.stdin_targets.is_empty() {
        let stdin_node_ids: HashSet<&str> =
            stdin_targets.iter().map(|t| t.node_id.as_str()).collect();
        for node in &workflow.nodes {
            if node.node_type == "Read"
                && !node.inputs.contains_key("input")
                && !stdin_node_ids.contains(node.id.as_str())
            {
                eprintln!(
                    "warning: Read node '{}' has no input (not targeted by --stdin or --set)",
                    node.id
                );
            }
        }
    }

    let edge_context = build_edge_context(&workflow);
    read_stdin_for_targets(&mut workflow, &stdin_targets, &edge_context)?;

    let mut registry = NodeRegistry::new();
    nodes::register_all(&mut registry, cli.quiet);
    let registry = Arc::new(registry);

    let saved_state_for_specs = Workflow::load_state(workflow_file);
    if !cli.stdin_targets.is_empty() {
        validate_pipe_handles(&stdin_targets, &workflow, &saved_state_for_specs, &registry, "input")?;
    }

    if cli.force {
        workflow.force_run = true;
    }
    if let Some(node_id) = cli.node {
        workflow.target_node_id = Some(node_id);
    }

    let connected_sources: HashSet<String> =
        workflow.edges.iter().map(|e| e.source.clone()).collect();

    let stdout_targets: Vec<PipeTarget> = if cli.stdout_targets.is_empty() {
        default_stdout_targets(&workflow, &connected_sources)
    } else {
        cli.stdout_targets
            .iter()
            .map(|s| expand_pipe_targets(s, &workflow))
            .collect::<anyhow::Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect()
    };

    if !cli.stdout_targets.is_empty() {
        validate_pipe_handles(&stdout_targets, &workflow, &saved_state_for_specs, &registry, "output")?;
    }

    let stdout_keys: HashSet<String> = stdout_targets
        .iter()
        .map(|t| format!("{}/{}", t.node_id, t.handle))
        .collect();
    let stdout_node_ids: HashSet<String> =
        stdout_targets.iter().map(|t| t.node_id.clone()).collect();

    let mut engine = Engine::new(registry, Some(cli.cache_file));
    let cancel_token = CancellationToken::new();

    {
        let cancel_token = cancel_token.clone();
        let quiet = cli.quiet;
        tokio::spawn(async move {
            shutdown_signal().await;
            if !quiet {
                eprintln!("\nreceived shutdown signal, cancelling workflow...");
            }
            cancel_token.cancel();
        });
    }

    let (tx, mut rx) = mpsc::channel(100);
    let verbose = cli.verbose;
    let quiet = cli.quiet;

    let cached_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let total_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let cached_count_clone = cached_count.clone();
    let total_count_clone = total_count.clone();

    let event_handler = tokio::spawn(async move {
        let mut streamed_keys: HashSet<String> = HashSet::new();

        while let Some(event) = rx.recv().await {
            match event {
                ExecutionEvent::Started { node_id, inputs } => {
                    if !quiet {
                        if verbose {
                            eprintln!("node [{}] started with inputs: {:?}", node_id, inputs);
                        } else {
                            eprintln!("node [{}] started", node_id);
                        }
                    }
                }
                ExecutionEvent::Progress {
                    node_id,
                    progress,
                    message,
                } => {
                    if !quiet {
                        if let Some(msg) = message {
                            eprintln!("node [{}] progress: {}% - {}", node_id, progress, msg);
                        } else {
                            eprintln!("node [{}] progress: {}%", node_id, progress);
                        }
                    }
                }
                ExecutionEvent::PartialOutput {
                    node_id,
                    output_name,
                    delta,
                    accumulated,
                } => {
                    let key = format!("{}/{}", node_id, output_name);
                    if stdout_keys.contains(&key) {
                        print_value_to_stdout(&delta);
                        streamed_keys.insert(key);
                    }
                    if verbose {
                        eprintln!(
                            "node [{}] partial {}: {:?}",
                            node_id, output_name, accumulated
                        );
                    }
                }
                ExecutionEvent::Finished {
                    node_id,
                    result,
                    cached,
                } => {
                    total_count_clone.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if cached {
                        cached_count_clone.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    if stdout_node_ids.contains(&node_id) {
                        for target in &stdout_targets {
                            if target.node_id != node_id {
                                continue;
                            }
                            let key = format!("{}/{}", target.node_id, target.handle);
                            if streamed_keys.contains(&key) {
                                continue;
                            }
                            if let Some(value) = result.get(&target.handle) {
                                print_value_to_stdout(value);
                                println!();
                            }
                        }
                    }
                    if !quiet {
                        let status = if cached { "cached" } else { "finished" };
                        if verbose {
                            eprintln!("node [{}] {} -> {:?}", node_id, status, result);
                        } else {
                            eprintln!("node [{}] {}", node_id, status);
                        }
                    }
                }
                ExecutionEvent::Error { node_id, error } => {
                    if !quiet {
                        eprintln!("node [{}] error: {}", node_id, error);
                    }
                }
            }
        }
    });

    if !quiet {
        eprintln!("executing workflow from {:?}", workflow_file);
    }
    let save = cli.save;
    let workflow_file = workflow_file.clone();

    let outcome = engine
        .execute(&workflow, Some(tx), cancel_token, false)
        .await;
    let _ = event_handler.await;

    if save {
        workflow.force_run = false;
        workflow.target_node_id = None;
        workflow.save(&workflow_file)?;
        if !quiet {
            eprintln!("workflow saved to {:?}", workflow_file);
        }
        if !outcome.results.is_empty() {
            Workflow::save_state(&workflow_file, &outcome.results)?;
            if !quiet {
                if let Some(state_path) = Workflow::state_path(&workflow_file) {
                    eprintln!("state saved to {:?}", state_path);
                }
            }
        }
    }

    if let Some(err) = outcome.error {
        if !quiet {
            eprintln!("workflow execution failed: {:?}", err);
        }
        std::process::exit(1);
    }

    if !quiet {
        let cached = cached_count.load(std::sync::atomic::Ordering::Relaxed);
        let total = total_count.load(std::sync::atomic::Ordering::Relaxed);
        if cached > 0 {
            eprintln!(
                "workflow execution completed successfully. ({}/{} nodes cached, use -f to force re-run)",
                cached, total
            );
        } else {
            eprintln!("workflow execution completed successfully.");
        }
    }

    Ok(())
}
