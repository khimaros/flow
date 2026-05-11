use tracing::info;

use axum::{
    extract::{DefaultBodyLimit, Json, Multipart, Path, State},
    http::{header, StatusCode},
    response::{
        sse::{Event, Sse},
        IntoResponse,
    },
    routing::{get, post},
    Router,
};

// axum's default body limit is 2 MB, which is too small for workflows that
// embed cached node outputs (e.g. RSS items, article markdown, audio base64).
const MAX_BODY_BYTES: usize = 64 * 1024 * 1024;
use clap::Parser;
use flow_rs::engine::{Engine, NodeRegistry, ResultMap};
use flow_rs::graph::Workflow;
use flow_rs::node::NodeMetadata;
use flow_rs::nodes;
use flow_rs::queue::{Job, JobEvent, Queue};
use futures::stream::Stream;
use futures::StreamExt;
use rust_embed::RustEmbed;
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{fmt, EnvFilter};

#[derive(RustEmbed)]
#[folder = "ui/dist"]
struct Asset;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// address to listen on
    #[arg(short, long, default_value = "127.0.0.1:3000")]
    listen: SocketAddr,

    /// data directory for workflows and assets
    #[arg(short, long, default_value = ".")]
    data_dir: PathBuf,
}

struct AppState {
    registry: Arc<NodeRegistry>,
    queue: Arc<Queue>,
    data_dir: PathBuf,
}

fn main() {
    dotenvy::dotenv().ok();
    let args = Args::parse();

    fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse().unwrap()))
        .with_target(false)
        .init();

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async_main(args));
}

async fn async_main(args: Args) {
    // install signal handler BEFORE Python is initialized (nodes::register_all)
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        shutdown_signal().await;
        let _ = shutdown_tx.send(());
    });

    let mut registry = NodeRegistry::new();
    nodes::register_all(&mut registry, false);

    // log active env var overrides for discoverability (deduplicated)
    let mut seen_env = std::collections::HashSet::new();
    for meta in registry.list_metadata() {
        for input in &meta.inputs {
            if let (Some(env_name), Some(env_val)) = (&input.env_var, &input.env_value) {
                if seen_env.insert(env_name.clone()) {
                    tracing::info!("env override: {}={}", env_name, env_val);
                }
            }
        }
    }

    let registry = Arc::new(registry);
    let engine = Engine::new(
        registry.clone(),
        Some(args.data_dir.join(flow_rs::engine::DEFAULT_CACHE_FILE)),
    );
    let queue = Queue::new(engine);

    // ensure workflows and state directories exist
    let workflows_dir = args.data_dir.join("workflows");
    if !workflows_dir.exists() {
        std::fs::create_dir_all(&workflows_dir).expect("failed to create workflows directory");
    }
    let shared_state = Arc::new(AppState {
        registry,
        queue,
        data_dir: args.data_dir.clone(),
    });
    let shutdown_state = shared_state.clone();

    // we serve static files for any route not starting with /api
    let app = Router::new()
        .route("/api/nodes", get(list_nodes))
        .route("/api/workflows", get(list_workflows))
        .route(
            "/api/workflows/{name}",
            get(load_workflow).post(save_workflow),
        )
        .route(
            "/api/workflows/{name}/state",
            get(load_state).post(save_state),
        )
        // workflow management endpoints
        .route("/api/workflows/{name}/delete", post(delete_workflow))
        .route("/api/workflows/{name}/rename", post(rename_workflow))
        .route("/api/assets/list", get(list_assets))
        .route("/api/assets/upload", post(upload_asset))
        // job queue endpoints
        .route("/api/queue", get(list_jobs).delete(clear_completed_jobs))
        .route("/api/queue/submit", post(submit_job))
        .route("/api/queue/stream", get(job_stream))
        .route("/api/queue/{job_id}", get(get_job))
        .route("/api/queue/{job_id}/cancel", post(cancel_job))
        .route(
            "/api/workflows/{workflow_name}/nodes/{node_id}/options/{input_name}",
            post(get_node_options),
        )
        .route(
            "/api/workflows/{workflow_name}/nodes/{node_id}/spec",
            post(get_node_spec),
        )
        .nest_service(
            "/api/assets",
            tower_http::services::ServeDir::new(args.data_dir.join("generated_assets")),
        )
        .fallback(static_handler)
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(shared_state);

    let addr = args.listen;
    tracing::info!("server listening on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = shutdown_rx.await;

            // shutdown the queue (cancels jobs, stops processor, signals SSE clients)
            shutdown_state.queue.shutdown().await;

            // force exit after timeout if graceful shutdown hangs
            tokio::spawn(async {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                std::process::exit(0);
            });
        })
        .await
        .unwrap();
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

async fn static_handler(uri: axum::http::Uri) -> impl IntoResponse {
    let mut path = uri.path().trim_start_matches('/').to_string();

    if path.is_empty() {
        path = "index.html".to_string();
    }

    match Asset::get(&path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            ([(header::CONTENT_TYPE, mime.as_ref())], content.data).into_response()
        }
        None => {
            if path.contains('.') {
                return StatusCode::NOT_FOUND.into_response();
            }
            // fallback to index.html for SPA routing
            match Asset::get("index.html") {
                Some(content) => {
                    ([(header::CONTENT_TYPE, "text/html")], content.data).into_response()
                }
                None => StatusCode::NOT_FOUND.into_response(),
            }
        }
    }
}

async fn list_nodes(State(state): State<Arc<AppState>>) -> Json<Vec<NodeMetadata>> {
    info!("listing nodes");
    Json(state.registry.list_metadata())
}

async fn list_workflows(State(state): State<Arc<AppState>>) -> Json<Vec<String>> {
    let entries = match std::fs::read_dir(state.data_dir.join("workflows")) {
        Ok(entries) => entries,
        Err(_) => return Json(Vec::new()),
    };

    info!("listing workflows");
    let workflows: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            // skip if not a file
            if !entry.file_type().ok()?.is_file() {
                return None;
            }
            let name = entry.file_name().into_string().ok()?;
            let stripped = name.strip_suffix(".json")?;
            // skip temp files
            if stripped.starts_with(".temp_") {
                return None;
            }
            Some(stripped.to_string())
        })
        .collect();

    Json(workflows)
}

async fn list_assets(State(state): State<Arc<AppState>>) -> Json<Vec<String>> {
    info!("listing assets");
    let entries = match std::fs::read_dir(state.data_dir.join("generated_assets")) {
        Ok(entries) => entries,
        Err(_) => return Json(Vec::new()),
    };

    let assets: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            if !entry.file_type().ok()?.is_file() {
                return None;
            }
            entry.file_name().into_string().ok()
        })
        .collect();

    Json(assets)
}

async fn upload_asset(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<Json<Value>, StatusCode> {
    info!("uploading asset");
    if let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?
    {
        let file_name = field.file_name().unwrap_or("uploaded_file").to_string();
        let data = field
            .bytes()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let ext = std::path::Path::new(&file_name)
            .extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_else(|| "bin".to_string());

        let new_filename = format!("upload_{}.{}", uuid::Uuid::new_v4(), ext);
        let path = state.data_dir.join("generated_assets").join(&new_filename);

        std::fs::create_dir_all(state.data_dir.join("generated_assets"))
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        std::fs::write(&path, data).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        return Ok(Json(serde_json::json!({
            "status": "ok",
            "filename": new_filename,
            "url": format!("/api/assets/{}", new_filename)
        })));
    }

    Err(StatusCode::BAD_REQUEST)
}

async fn load_workflow(
    Path(name): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    info!(workflow = %name, "loading workflow");
    let path = state
        .data_dir
        .join("workflows")
        .join(format!("{}.json", name));
    if !path.exists() {
        return Err(StatusCode::NOT_FOUND);
    }

    // read the file directly as JSON to preserve UI-specific fields like edges, positions, etc.
    match std::fs::read_to_string(&path) {
        Ok(content) => match serde_json::from_str(&content) {
            Ok(json) => Ok(Json(json)),
            Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
        },
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn save_workflow(
    Path(name): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(workflow_value): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    info!(workflow = %name, "saving workflow");
    let path = state
        .data_dir
        .join("workflows")
        .join(format!("{}.json", name));

    // parse to Workflow, normalize inputs, then serialize
    let mut workflow: Workflow =
        serde_json::from_value(workflow_value).map_err(|_| StatusCode::BAD_REQUEST)?;
    workflow.normalize(&state.registry);

    let content =
        serde_json::to_string_pretty(&workflow).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    std::fs::write(&path, content).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({"status": "ok", "name": name})))
}

async fn load_state(
    Path(name): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Json<Value> {
    let workflow_path = state.data_dir.join("workflows").join(format!("{}.json", name));
    let results = Workflow::load_state(&workflow_path);
    if results.is_empty() {
        info!(workflow = %name, "no execution state found");
        Json(serde_json::json!({}))
    } else {
        info!(workflow = %name, "loaded execution state");
        Json(serde_json::to_value(results).unwrap_or_default())
    }
}

async fn save_state(
    Path(name): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let workflow_path = state.data_dir.join("workflows").join(format!("{}.json", name));
    let results: ResultMap = serde_json::from_value(body).map_err(|_| StatusCode::BAD_REQUEST)?;
    info!(workflow = %name, keys = ?results.keys().collect::<Vec<_>>(), "saving execution state");
    Workflow::save_state(&workflow_path, &results).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({"status": "ok"})))
}

// workflow Management Endpoints

async fn delete_workflow(
    Path(name): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    info!(workflow = %name, "deleting workflow");
    let path = state
        .data_dir
        .join("workflows")
        .join(format!("{}.json", name));
    if !path.exists() {
        return Err(StatusCode::NOT_FOUND);
    }

    std::fs::remove_file(&path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if let Some(state_path) = Workflow::state_path(&path) {
        let _ = std::fs::remove_file(state_path);
    }
    Ok(Json(serde_json::json!({"status": "ok"})))
}

#[derive(Deserialize)]
struct RenameRequest {
    new_name: String,
}

async fn rename_workflow(
    Path(name): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(request): Json<RenameRequest>,
) -> Result<Json<Value>, StatusCode> {
    info!(workflow = %name, "renaming workflow");
    let old_path = state
        .data_dir
        .join("workflows")
        .join(format!("{}.json", name));
    if !old_path.exists() {
        return Err(StatusCode::NOT_FOUND);
    }

    let new_name = request.new_name;
    let new_path = state
        .data_dir
        .join("workflows")
        .join(format!("{}.json", new_name));
    if new_path.exists() {
        return Err(StatusCode::CONFLICT);
    }

    std::fs::rename(&old_path, &new_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if let (Some(old_state), Some(new_state)) = (
        Workflow::state_path(&old_path),
        Workflow::state_path(&new_path),
    ) {
        if old_state.exists() {
            let _ = std::fs::rename(old_state, new_state);
        }
    }

    Ok(Json(
        serde_json::json!({"status": "ok", "new_name": new_name}),
    ))
}

async fn get_node_options(
    Path((workflow_name, node_id, input_name)): Path<(String, String, String)>,
    State(state): State<Arc<AppState>>,
    Json(request): Json<GetOptionsRequest>,
) -> Result<Json<Vec<flow_rs::node::SelectOption>>, StatusCode> {
    info!(workflow = %workflow_name, node = %node_id, input = %input_name, "fetching node options");
    let workflow_name_cleaned = workflow_name
        .strip_suffix(".json")
        .unwrap_or(&workflow_name)
        .to_string();

    // 1. Load the workflow
    let workflow_path = state
        .data_dir
        .join("workflows")
        .join(format!("{}.json", workflow_name_cleaned));
    let workflow = Workflow::load(&workflow_path).map_err(|e| {
        tracing::error!("failed to load workflow {}: {}", workflow_name, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // 2. Find the NodeInstance within the workflow
    let node_instance = workflow
        .nodes
        .iter()
        .find(|n| n.id == node_id)
        .ok_or(StatusCode::NOT_FOUND)?;

    // 3. Instantiate the actual Node trait object
    let node_type = &node_instance.node_type;
    let node = state.registry.create(node_type).ok_or_else(|| {
        tracing::error!("node type not found in registry: {}", node_type);
        StatusCode::NOT_FOUND
    })?;

    // 4. Call `get_options` on the instantiated Node, applying env var overrides
    let mut converted_inputs: BTreeMap<String, flow_rs::value::Value> = request
        .inputs
        .into_iter()
        .map(|(k, v)| {
            let flow_val: flow_rs::value::Value =
                serde_json::from_value(v).unwrap_or(flow_rs::value::Value::Null);
            (k, flow_val)
        })
        .collect();

    flow_rs::engine::Engine::apply_env_overrides(
        node.as_ref(),
        &converted_inputs.clone(),
        &mut converted_inputs,
    );

    let options = node
        .get_options(&input_name, converted_inputs)
        .await
        .map_err(|e| {
            tracing::error!("failed to get options for node {}: {}", node_id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // 5. Return the options as JSON
    Ok(Json(options))
}

#[derive(Deserialize)]
struct GetSpecRequest {
    #[serde(default)]
    inputs: BTreeMap<String, Value>,
    /// optional node type, used as a fallback when the workflow file is not
    /// (yet) persisted to disk or does not contain the requested node id.
    /// the editor sends this for unsaved workflows.
    #[serde(default)]
    node_type: Option<String>,
}

async fn get_node_spec(
    Path((workflow_name, node_id)): Path<(String, String)>,
    State(state): State<Arc<AppState>>,
    Json(request): Json<GetSpecRequest>,
) -> Result<Json<NodeMetadata>, StatusCode> {
    info!(workflow = %workflow_name, node = %node_id, "fetching node spec");
    let workflow_name_cleaned = workflow_name
        .strip_suffix(".json")
        .unwrap_or(&workflow_name)
        .to_string();

    let workflow_path = state
        .data_dir
        .join("workflows")
        .join(format!("{}.json", workflow_name_cleaned));

    // try to load the workflow + locate the node instance, but tolerate the
    // case where the workflow isn't on disk yet (new editor session) by
    // falling back to the node_type supplied in the request body.
    let workflow = Workflow::load(&workflow_path).ok();
    let saved_node = workflow
        .as_ref()
        .and_then(|wf| wf.nodes.iter().find(|n| n.id == node_id));

    let node_type = saved_node
        .map(|n| n.node_type.clone())
        .or_else(|| request.node_type.clone())
        .ok_or(StatusCode::BAD_REQUEST)?;

    let node = state.registry.create(&node_type).ok_or_else(|| {
        tracing::error!("node type not found in registry: {}", node_type);
        StatusCode::NOT_FOUND
    })?;

    // merge: start with saved literals on the workflow node (skipping wired
    // refs), then overlay any live editor values from the request.
    let mut resolved: BTreeMap<String, flow_rs::value::Value> = saved_node
        .map(|n| {
            n.inputs
                .iter()
                .filter_map(|(k, v)| match v {
                    flow_rs::value::Value::Object(o) if o.contains_key("$node") => None,
                    _ => Some((k.clone(), v.clone())),
                })
                .collect()
        })
        .unwrap_or_default();
    for (k, v) in request.inputs {
        let flow_val: flow_rs::value::Value =
            serde_json::from_value(v).unwrap_or(flow_rs::value::Value::Null);
        resolved.insert(k, flow_val);
    }

    // apply env/defaults using the static specs so the inputs needed to derive
    // dynamic ports are populated.
    flow_rs::engine::Engine::apply_env_overrides(
        node.as_ref(),
        &resolved.clone(),
        &mut resolved,
    );
    for spec in node.inputs() {
        let needs_default = match resolved.get(&spec.name) {
            None => true,
            Some(flow_rs::value::Value::Null) => true,
            Some(flow_rs::value::Value::String(s)) if s.is_empty() => true,
            _ => false,
        };
        if needs_default {
            if let Some(default) = &spec.default {
                resolved.insert(spec.name.clone(), default.clone());
            }
        }
    }

    let inputs = flow_rs::node::effective_inputs(node.as_ref(), &resolved);
    let outputs = flow_rs::node::effective_outputs(node.as_ref(), &resolved);

    Ok(Json(NodeMetadata {
        name: node.name().to_string(),
        title: node.title().to_string(),
        category: node.category().to_string(),
        description: node.description().to_string(),
        inputs,
        outputs,
        script_source: node.script_source(),
        has_dynamic_spec: node.has_dynamic_spec(),
    }))
}

// job Queue Endpoints

#[derive(Deserialize)]
struct GetOptionsRequest {
    inputs: BTreeMap<String, Value>,
}

#[derive(Deserialize)]
struct SubmitJobRequest {
    workflow: Workflow,
    workflow_name: Option<String>,
}

async fn list_jobs(State(state): State<Arc<AppState>>) -> Json<Vec<Job>> {
    info!("listing jobs");
    Json(state.queue.list_jobs().await)
}

async fn get_job(
    State(state): State<Arc<AppState>>,
    Path(job_id): Path<String>,
) -> Result<Json<Job>, StatusCode> {
    info!(job = %job_id, "getting job");
    state
        .queue
        .get_job(&job_id)
        .await
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn submit_job(
    State(state): State<Arc<AppState>>,
    Json(request): Json<SubmitJobRequest>,
) -> Json<Job> {
    info!("submitting job");
    let mut workflow = request.workflow;
    workflow.normalize(&state.registry);
    let job = state.queue.submit(workflow, request.workflow_name).await;
    Json(job)
}

async fn clear_completed_jobs(State(state): State<Arc<AppState>>) -> Json<Value> {
    info!("clearing completed jobs");
    state.queue.clear_completed().await;
    Json(serde_json::json!({"status": "ok"}))
}

async fn cancel_job(
    State(state): State<Arc<AppState>>,
    Path(job_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    info!(job = %job_id, "cancelling job");
    if state.queue.cancel_job(&job_id).await {
        Ok(Json(serde_json::json!({"status": "ok"})))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn job_stream(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, axum::Error>>> {
    info!("client connected to job stream");
    let mut rx = state.queue.subscribe();

    // first, send current job list as initial state
    let jobs = state.queue.list_jobs().await;

    let initial_stream = futures::stream::iter(jobs.into_iter().map(|job| {
        Ok(Event::default()
            .json_data(JobEvent::JobCreated { job })
            .unwrap())
    }));

    // then stream live updates
    let live_stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(JobEvent::Shutdown) => break,
                Ok(event) => {
                    yield Ok(Event::default().json_data(event).unwrap());
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    Sse::new(initial_stream.chain(live_stream))
        .keep_alive(axum::response::sse::KeepAlive::default())
}
