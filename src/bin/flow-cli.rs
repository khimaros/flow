use clap::Parser;
use flow_rs::engine::{Engine, ExecutionEvent, NodeRegistry};
use flow_rs::graph::Workflow;
use flow_rs::nodes;
use flow_rs::value::Value;
use std::collections::{HashMap, HashSet};
use std::io::{IsTerminal, Read};
use std::path::PathBuf;
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
        Ok(vec![PipeTarget {
            node_id: node_pattern.to_string(),
            handle: handle.to_string(),
        }])
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
struct Cli {
    /// path to the workflow JSON file (required unless using --cache-stats or --cache-gc)
    #[arg(value_name = "WORKFLOW_FILE")]
    workflow_file: Option<PathBuf>,

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

    /// specific node to execute
    #[arg(value_name = "NODE", num_args = 0..=1)]
    node: Option<String>,

    /// list all nodes in the workflow file
    #[arg(short, long)]
    list_nodes: bool,

    /// list inputs and outputs for nodes matching a pattern (supports * and ? globs)
    #[arg(long, value_name = "NODE_PATTERN")]
    list_handles: Option<String>,

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

    /// list all available env vars for all node types
    #[arg(long)]
    list_env: bool,

    /// emit a .env template with every known env var, commented out, with
    /// per-var descriptions pulled from the node input metadata
    #[arg(long)]
    env_example: bool,

    /// show cache statistics (entry count, file size)
    #[arg(long)]
    cache_stats: bool,

    /// prune cache: remove entries not referenced by workflows in the given
    /// directory (default: workflows/)
    #[arg(long, default_missing_value = "workflows", num_args = 0..=1)]
    cache_prune: Option<PathBuf>,
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
        let input_names: Vec<String> = node_impl.inputs().iter().map(|s| s.name.clone()).collect();
        let output_names: Vec<String> = node_impl.outputs().iter().map(|s| s.name.clone()).collect();
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

fn run_cache_stats(cache_file: &PathBuf) -> anyhow::Result<()> {
    let metadata = std::fs::metadata(cache_file);
    let size_bytes = metadata.as_ref().map(|m| m.len()).unwrap_or(0);

    let entry_count = if cache_file.exists() {
        let content = std::fs::read_to_string(cache_file)?;
        let cache: HashMap<String, serde_json::Value> = serde_json::from_str(&content)?;
        cache.len()
    } else {
        0
    };

    let (size, unit) = if size_bytes >= 1024 * 1024 {
        (size_bytes as f64 / (1024.0 * 1024.0), "MB")
    } else if size_bytes >= 1024 {
        (size_bytes as f64 / 1024.0, "KB")
    } else {
        (size_bytes as f64, "B")
    };

    eprintln!("cache file: {:?}", cache_file);
    eprintln!("entries:    {}", entry_count);
    eprintln!("size:       {:.1} {}", size, unit);
    Ok(())
}

/// emit a .env-style template keyed by every env var the registry knows about.
/// each entry has a comment with the node/input it binds to and the input's
/// description, then a commented-out KEY= line ready to uncomment.
fn run_env_example() -> anyhow::Result<()> {
    let mut registry = NodeRegistry::new();
    nodes::register_all(&mut registry, true);
    let metadata = registry.list_metadata();

    // env var -> list of (node, input, description) bindings
    let mut entries: std::collections::BTreeMap<
        String,
        Vec<(String, String, Option<String>)>,
    > = std::collections::BTreeMap::new();

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

    println!("# generated by flow-cli --env-example");
    println!("# uncomment and set values to override node inputs via environment");
    println!();
    for (env_name, bindings) in &entries {
        // prefer the first binding that has a description; fall back to the first
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

fn run_list_env() -> anyhow::Result<()> {
    let mut registry = NodeRegistry::new();
    nodes::register_all(&mut registry, true);

    // collect rows: one per env var name (auto-convention + alias on separate rows)
    let metadata = registry.list_metadata();
    let mut rows: Vec<(String, String, String)> = Vec::new();
    for meta in &metadata {
        let instance = registry.create(&meta.name);
        let original_specs = instance.as_ref().map(|n| n.inputs()).unwrap_or_default();

        for (i, input) in meta.inputs.iter().enumerate() {
            let auto_name =
                flow_rs::node::auto_env_var_name(&meta.name, &input.name);
            let mut seen = std::collections::HashSet::new();
            // 1. auto-convention (highest priority)
            seen.insert(auto_name.clone());
            rows.push((auto_name.clone(), meta.name.clone(), input.name.clone()));
            // 2. explicit alias from spec
            if let Some(alias) = original_specs.get(i).and_then(|s| s.env_var.as_deref()) {
                if seen.insert(alias.to_string()) {
                    rows.push((alias.to_string(), meta.name.clone(), input.name.clone()));
                }
            }
            // 3. resolved inner env var (for declarative nodes)
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

fn run_cache_prune(cache_file: &PathBuf, workflows_dir: &PathBuf) -> anyhow::Result<()> {
    if !cache_file.exists() {
        eprintln!("cache file {:?} does not exist, nothing to prune", cache_file);
        return Ok(());
    }
    if !workflows_dir.exists() || !workflows_dir.is_dir() {
        anyhow::bail!("workflows directory {:?} does not exist", workflows_dir);
    }

    let mut registry = NodeRegistry::new();
    nodes::register_all(&mut registry, true);
    let registry = Arc::new(registry);

    // collect cache keys referenced by workflows
    let mut live_keys: HashSet<String> = HashSet::new();
    let entries = std::fs::read_dir(workflows_dir)?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map(|e| e == "json").unwrap_or(false) {
            match Workflow::load(&path) {
                Ok(mut workflow) => {
                    workflow.normalize(&registry);
                    let exec = workflow.to_execution();
                    for node in &exec.nodes {
                        // only compute keys for nodes with purely static inputs
                        let all_static = node.inputs.values().all(is_static_input);
                        if all_static {
                            let key = Engine::calculate_hash_static(
                                &node.node_type,
                                &node.inputs,
                            );
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

    // load cache and remove stale entries
    let content = std::fs::read_to_string(cache_file)?;
    let mut cache: HashMap<String, serde_json::Value> = serde_json::from_str(&content)?;
    let total = cache.len();
    cache.retain(|key, _| live_keys.contains(key));
    let pruned = total - cache.len();

    // write back
    let new_content = serde_json::to_string_pretty(&cache)?;
    std::fs::write(cache_file, new_content)?;

    eprintln!(
        "pruned {} of {} cache entries ({} retained from {} workflows)",
        pruned,
        total,
        cache.len(),
        live_keys.len()
    );
    Ok(())
}

async fn run() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let cli = Cli::parse();

    // handle cache operations (no workflow file required)
    if cli.cache_stats {
        return run_cache_stats(&cli.cache_file);
    }
    if let Some(ref workflows_dir) = cli.cache_prune {
        return run_cache_prune(&cli.cache_file, workflows_dir);
    }

    if cli.list_env {
        return run_list_env();
    }

    if cli.env_example {
        return run_env_example();
    }

    let workflow_file = cli.workflow_file.as_ref().ok_or_else(|| {
        anyhow::anyhow!("WORKFLOW_FILE is required (unless using --list-env, --env-example, --cache-stats, or --cache-prune)")
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
        let node = workflow
            .nodes
            .iter_mut()
            .find(|n| n.id == node_id)
            .ok_or_else(|| anyhow::anyhow!("node '{}' not found in workflow", node_id))?;
        let parsed: Value =
            serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_string()));
        node.inputs.insert(input_name.to_string(), parsed);
        set_targets.insert(format!("{}/{}", node_id, input_name));
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

    // warn about unfilled Read nodes when explicit --stdin bypasses them
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

    if cli.list_nodes {
        let max_term = total_inner_width();
        let col_id = workflow.nodes.iter()
            .map(|n| n.id.len())
            .max().unwrap_or(0)
            .max("NODE ID".len());
        let col_type = workflow.nodes.iter()
            .map(|n| n.node_type.len())
            .max().unwrap_or(0)
            .max("TYPE".len());
        // cap total to terminal width (3 = separator between columns)
        let needed = col_id + col_type + 3;
        let cols = if needed > max_term {
            let shrunk_type = max_term.saturating_sub(col_id + 3);
            [col_id, shrunk_type]
        } else {
            [col_id, col_type]
        };
        eprintln!(
            "\nworkflow: {}\n",
            workflow_file.file_stem().unwrap_or_default().to_string_lossy()
        );
        eprintln!("{}", table_separator(&cols));
        eprintln!("{}", table_header(&["NODE ID", "TYPE"], &cols));
        eprintln!("{}", table_separator(&cols));
        for node in &workflow.nodes {
            eprintln!("{}", table_row(&[&node.id, &node.node_type], &cols));
            eprintln!("{}", table_separator(&cols));
        }
        return Ok(());
    }

    if let Some(ref pattern) = cli.list_handles {
        let max_term = total_inner_width();
        let matched: Vec<_> = workflow
            .nodes
            .iter()
            .filter(|n| glob_match(pattern, &n.id))
            .collect();
        if matched.is_empty() {
            anyhow::bail!("no nodes matched pattern '{}'", pattern);
        }
        let saved_state = Workflow::load_state(workflow_file);
        let col_dir = 3;
        for node in &matched {
            let node_impl = registry.create(&node.node_type);
            let input_specs = node_impl.as_ref().map(|n| n.inputs()).unwrap_or_default();
            let output_specs = node_impl.as_ref().map(|n| n.outputs()).unwrap_or_default();

            // pre-compute cell data for this node
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

            // measure columns from data
            let col_name = rows.iter().map(|r| display_width(&r.name)).max().unwrap_or(0)
                .max("NAME".len());
            let col_type = rows.iter().map(|r| display_width(&r.type_str)).max().unwrap_or(0)
                .max("TYPE".len());

            // value column: fit longest single-line value, capped so total <= terminal
            // chrome per column: " content " = width + 2, plus "|" separators = ncols + 1
            let fixed = col_dir + col_name + col_type + 12; // 4 cols * 2 padding + 5 separators
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
        return Ok(());
    }

    // validate stdin handles are actual inputs
    if !cli.stdin_targets.is_empty() {
        validate_pipe_handles(&stdin_targets, &workflow, &registry, "input")?;
    }

    // apply CLI arguments to the workflow
    if cli.force {
        workflow.force_run = true;
    }
    if let Some(node_id) = cli.node {
        workflow.target_node_id = Some(node_id);
    }

    // resolve stdout targets
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

    // validate stdout handles are actual outputs
    if !cli.stdout_targets.is_empty() {
        validate_pipe_handles(&stdout_targets, &workflow, &registry, "output")?;
    }

    // build lookup set for stdout targets: "node_id/output_name"
    let stdout_keys: HashSet<String> = stdout_targets
        .iter()
        .map(|t| format!("{}/{}", t.node_id, t.handle))
        .collect();
    let stdout_node_ids: HashSet<String> =
        stdout_targets.iter().map(|t| t.node_id.clone()).collect();

    let mut engine = Engine::new(registry, Some(cli.cache_file));
    let cancel_token = CancellationToken::new();

    let (tx, mut rx) = mpsc::channel(100);
    let verbose = cli.verbose;
    let quiet = cli.quiet;

    let cached_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let total_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let cached_count_clone = cached_count.clone();
    let total_count_clone = total_count.clone();

    let event_handler = tokio::spawn(async move {
        // track which stdout targets have received streaming output,
        // so we don't double-print on Finished
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
                    // write stdout targets that didn't stream
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
        // reset runtime-only fields before saving
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
