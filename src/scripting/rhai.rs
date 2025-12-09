use super::*;
use ::rhai::{Dynamic, Engine, EvalAltResult, ImmutableString, Map, Module, Scope};
use base64::Engine as _;
use std::time::Duration;
use tracing::{debug, info};

/// poll the cancellation flag, returning when it becomes true.
/// used as the cancellation arm of tokio::select! around HTTP futures so
/// in-flight requests abort within ~50ms of the job being cancelled instead
/// of blocking the worker thread until the reqwest timeout elapses.
async fn poll_cancel(cancelled: Arc<AtomicBool>) {
    while !cancelled.load(Ordering::Relaxed) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// walk the std::error::Error source chain to build a readable message
fn error_chain(err: &dyn std::error::Error) -> String {
    let mut msg = err.to_string();
    let mut source = err.source();
    while let Some(cause) = source {
        msg = format!("{}: {}", msg, cause);
        source = cause.source();
    }
    msg
}

/// extract detailed error information from a Rhai EvalAltResult
fn extract_rhai_error_details(err: &EvalAltResult) -> (String, String) {
    match err {
        EvalAltResult::ErrorRuntime(value, _) => {
            // this is a thrown error - extract the actual value
            let detail = if value.is_string() {
                value
                    .clone()
                    .into_string()
                    .unwrap_or_else(|_| "Unknown".into())
            } else if value.is_map() {
                // try to convert map to readable format
                match ::rhai::serde::from_dynamic::<JsonValue>(value) {
                    Ok(json) => {
                        serde_json::to_string_pretty(&json).unwrap_or_else(|_| value.to_string())
                    }
                    Err(_) => value.to_string(),
                }
            } else {
                value.to_string()
            };
            ("RuntimeError".to_string(), detail.to_string())
        }
        EvalAltResult::ErrorFunctionNotFound(name, _) => (
            "FunctionNotFound".to_string(),
            format!("function '{}' not found", name),
        ),
        EvalAltResult::ErrorVariableNotFound(name, _) => (
            "VariableNotFound".to_string(),
            format!("variable '{}' not found", name),
        ),
        EvalAltResult::ErrorMismatchDataType(expected, got, _) => (
            "TypeMismatch".to_string(),
            format!("expected {}, got {}", expected, got),
        ),
        EvalAltResult::ErrorArithmetic(msg, _) => ("ArithmeticError".to_string(), msg.to_string()),
        EvalAltResult::ErrorArrayBounds(len, index, _) => (
            "ArrayBoundsError".to_string(),
            format!("index {} out of bounds for array of length {}", index, len),
        ),
        EvalAltResult::ErrorStringBounds(len, index, _) => (
            "StringBoundsError".to_string(),
            format!("index {} out of bounds for string of length {}", index, len),
        ),
        EvalAltResult::ErrorIndexingType(type_name, _) => (
            "IndexingError".to_string(),
            format!("cannot index into type '{}'", type_name),
        ),
        EvalAltResult::ErrorFor(_) => ("ForLoopError".to_string(), "invalid for loop".to_string()),
        EvalAltResult::ErrorTooManyOperations(_) => (
            "TooManyOperations".to_string(),
            "script exceeded operation limit".to_string(),
        ),
        EvalAltResult::ErrorTerminated(value, _) => {
            let detail = value
                .clone()
                .into_string()
                .unwrap_or_else(|_| "script terminated".into());
            ("Terminated".to_string(), detail.to_string())
        }
        EvalAltResult::ErrorSystem(msg, err) => {
            ("SystemError".to_string(), format!("{}: {}", msg, err))
        }
        // for other error types, use the default string representation
        other => {
            let full = other.to_string();
            ("Error".to_string(), full)
        }
    }
}

pub struct RhaiEngine;

impl Default for RhaiEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl RhaiEngine {
    pub fn new() -> Self {
        Self
    }

    /// create a base engine with common functions.
    /// `cancelled` is polled by HTTP helpers so in-flight requests abort
    /// promptly on job cancellation; pass a perma-false flag for contexts
    /// that don't have one (parse_spec, get_options).
    fn create_base_engine(cancelled: Arc<AtomicBool>) -> Engine {
        let mut engine = Engine::new();
        engine.set_max_expr_depths(256, 256);

        // create a custom module for standard library functions
        let mut module = Module::new();

        // HTTP request (JSON/text body)
        let http_cancelled = cancelled.clone();
        module.set_native_fn(
            "http_request",
            move |method: &str,
                  url: &str,
                  body: Map,
                  headers: Map,
                  options: Map|
                  -> Result<Dynamic, Box<::rhai::EvalAltResult>> {
                let timeout_secs = options
                    .get("timeout")
                    .and_then(|v| v.as_float().ok().map(|f| f as u64))
                    .or_else(|| {
                        options
                            .get("timeout")
                            .and_then(|v| v.as_int().ok().map(|i| i as u64))
                    })
                    .unwrap_or(60);

                let retries = options
                    .get("retries")
                    .and_then(|v| v.as_int().ok())
                    .unwrap_or(0);

                let client = reqwest::Client::builder()
                    .timeout(Duration::from_secs(timeout_secs))
                    .build()
                    .map_err(|e| format!("failed to create HTTP client: {}", e))?;

                let handle = tokio::runtime::Handle::current();
                let cancelled = http_cancelled.clone();
                let method = method.to_string();
                let url = url.to_string();

                handle.block_on(async move {
                    let mut last_error = String::new();
                    for attempt in 0..=retries {
                        if cancelled.load(Ordering::Relaxed) {
                            return Err::<Dynamic, Box<::rhai::EvalAltResult>>(
                                "HTTP request cancelled".into(),
                            );
                        }

                        let mut request = match method.to_uppercase().as_str() {
                            "GET" => client.get(&url),
                            "POST" => client.post(&url),
                            "PUT" => client.put(&url),
                            "PATCH" => client.patch(&url),
                            "DELETE" => client.delete(&url),
                            "HEAD" => client.head(&url),
                            _ => {
                                return Err(
                                    format!("unsupported HTTP method: {}", method).into()
                                )
                            }
                        };

                        for (k, v) in &headers {
                            if let Ok(val) = v.clone().into_string() {
                                request = request.header(k.as_str(), val);
                            }
                        }

                        if !body.is_empty()
                            && !matches!(method.to_uppercase().as_str(), "GET" | "HEAD")
                        {
                            let body_json: JsonValue =
                                ::rhai::serde::from_dynamic(&Dynamic::from(body.clone()))
                                    .map_err(|e| e.to_string())?;
                            request = request.json(&body_json);
                        }

                        let send_result = tokio::select! {
                            res = request.send() => res,
                            _ = poll_cancel(cancelled.clone()) => {
                                return Err("HTTP request cancelled".into());
                            }
                        };

                        match send_result {
                            Ok(response) => {
                                let status = response.status().as_u16();
                                let resp_text = tokio::select! {
                                    t = response.text() => t.unwrap_or_default(),
                                    _ = poll_cancel(cancelled.clone()) => {
                                        return Err("HTTP request cancelled".into());
                                    }
                                };

                                if status >= 400 {
                                    let preview = if resp_text.len() > 500 {
                                        format!("{}...(truncated)", &resp_text[..500])
                                    } else {
                                        resp_text.clone()
                                    };
                                    tracing::debug!(
                                        "[rhai http_request] {} {} returned status {}: {}",
                                        method,
                                        url,
                                        status,
                                        preview
                                    );
                                }

                                let body_dynamic = if let Ok(json_val) =
                                    serde_json::from_str::<JsonValue>(&resp_text)
                                {
                                    ::rhai::serde::to_dynamic(&json_val)
                                        .map_err(|e| e.to_string())?
                                } else {
                                    Dynamic::from(resp_text)
                                };

                                let mut result = Map::new();
                                result.insert("status".into(), Dynamic::from(status as i64));
                                result.insert("body".into(), body_dynamic);
                                return Ok(Dynamic::from(result));
                            }
                            Err(e) => {
                                last_error = error_chain(&e);
                                if attempt < retries {
                                    tracing::warn!(
                                        "HTTP request to {} failed (attempt {}/{}): {}. Retrying...",
                                        url,
                                        attempt + 1,
                                        retries + 1,
                                        last_error
                                    );
                                    tokio::select! {
                                        _ = tokio::time::sleep(Duration::from_secs(2)) => {},
                                        _ = poll_cancel(cancelled.clone()) => {
                                            return Err("HTTP request cancelled".into());
                                        }
                                    }
                                }
                            }
                        }
                    }

                    Err(format!(
                        "HTTP {} request to '{}' failed after {} attempts. Last error: {}",
                        method,
                        url,
                        retries + 1,
                        last_error
                    )
                    .into())
                })
            },
        );

        // environment variable access
        module.set_native_fn(
            "get_env",
            |name: &str| -> Result<ImmutableString, Box<::rhai::EvalAltResult>> {
                Ok(std::env::var(name).unwrap_or_default().into())
            },
        );

        // logging
        module.set_native_fn("log", |msg: ImmutableString| {
            tracing::info!("[rhai script] {}", msg);
            Ok(())
        });

        // UUID
        module.set_native_fn(
            "uuid_v4",
            || -> Result<ImmutableString, Box<::rhai::EvalAltResult>> {
                Ok(uuid::Uuid::new_v4().to_string().into())
            },
        );

        // save Asset (Base64)
        module.set_native_fn(
            "save_asset_base64",
            |filename: &str,
             content_b64: &str|
             -> Result<ImmutableString, Box<::rhai::EvalAltResult>> {
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(content_b64)
                    .map_err(|e| format!("failed to decode base64: {}", e))?;

                let path = std::path::Path::new("generated_assets").join(filename);
                std::fs::write(&path, bytes).map_err(|e| format!("failed to write file: {}", e))?;

                Ok(path.to_string_lossy().to_string().into())
            },
        );

        // read file as Base64
        module.set_native_fn(
            "read_file_base64",
            |file_path: &str| -> Result<ImmutableString, Box<::rhai::EvalAltResult>> {
                let bytes = std::fs::read(file_path)
                    .map_err(|e| format!("failed to read file '{}': {}", file_path, e))?;
                let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                Ok(b64.into())
            },
        );

        // decode base64 to UTF-8 text (for reading error bodies from binary responses)
        module.set_native_fn(
            "decode_base64_text",
            |b64: &str| -> Result<ImmutableString, Box<::rhai::EvalAltResult>> {
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(b64)
                    .map_err(|e| format!("failed to decode base64: {}", e))?;
                let text = String::from_utf8_lossy(&bytes).into_owned();
                Ok(text.into())
            },
        );

        // HTTP request with binary response (returns base64-encoded body)
        let http_bin_cancelled = cancelled.clone();
        module.set_native_fn(
            "http_request_binary",
            move |method: &str,
                  url: &str,
                  body: Map,
                  headers: Map,
                  options: Map|
                  -> Result<Dynamic, Box<::rhai::EvalAltResult>> {
                let timeout_secs = options
                    .get("timeout")
                    .and_then(|v| v.as_float().ok().map(|f| f as u64))
                    .or_else(|| {
                        options
                            .get("timeout")
                            .and_then(|v| v.as_int().ok().map(|i| i as u64))
                    })
                    .unwrap_or(60);

                let retries = options
                    .get("retries")
                    .and_then(|v| v.as_int().ok())
                    .unwrap_or(0);

                let client = reqwest::Client::builder()
                    .timeout(Duration::from_secs(timeout_secs))
                    .build()
                    .map_err(|e| format!("failed to create HTTP client: {}", e))?;

                let handle = tokio::runtime::Handle::current();
                let cancelled = http_bin_cancelled.clone();
                let method = method.to_string();
                let url = url.to_string();

                handle.block_on(async move {
                    let mut last_error = String::new();
                    for attempt in 0..=retries {
                        if cancelled.load(Ordering::Relaxed) {
                            return Err::<Dynamic, Box<::rhai::EvalAltResult>>(
                                "HTTP request cancelled".into(),
                            );
                        }

                        let mut request = match method.to_uppercase().as_str() {
                            "GET" => client.get(&url),
                            "POST" => client.post(&url),
                            "PUT" => client.put(&url),
                            "PATCH" => client.patch(&url),
                            "DELETE" => client.delete(&url),
                            "HEAD" => client.head(&url),
                            _ => {
                                return Err(
                                    format!("unsupported HTTP method: {}", method).into()
                                )
                            }
                        };

                        for (k, v) in &headers {
                            if let Ok(val) = v.clone().into_string() {
                                request = request.header(k.as_str(), val);
                            }
                        }

                        if !body.is_empty()
                            && !matches!(method.to_uppercase().as_str(), "GET" | "HEAD")
                        {
                            let body_json: JsonValue =
                                ::rhai::serde::from_dynamic(&Dynamic::from(body.clone()))
                                    .map_err(|e| e.to_string())?;
                            request = request.json(&body_json);
                        }

                        let send_result = tokio::select! {
                            res = request.send() => res,
                            _ = poll_cancel(cancelled.clone()) => {
                                return Err("HTTP request cancelled".into());
                            }
                        };

                        match send_result {
                            Ok(response) => {
                                let status = response.status().as_u16();
                                let content_type = response
                                    .headers()
                                    .get("content-type")
                                    .and_then(|v| v.to_str().ok())
                                    .unwrap_or("")
                                    .to_string();
                                let bytes_result = tokio::select! {
                                    b = response.bytes() => b,
                                    _ = poll_cancel(cancelled.clone()) => {
                                        return Err("HTTP request cancelled".into());
                                    }
                                };
                                let bytes = bytes_result.map_err(|e| {
                                    format!("failed to read response body: {}", e)
                                })?;
                                let b64_body = base64::engine::general_purpose::STANDARD
                                    .encode(&bytes);

                                let mut result = Map::new();
                                result.insert("status".into(), Dynamic::from(status as i64));
                                result.insert("body_base64".into(), Dynamic::from(b64_body));
                                result.insert(
                                    "content_type".into(),
                                    Dynamic::from(content_type),
                                );
                                return Ok(Dynamic::from(result));
                            }
                            Err(e) => {
                                last_error = error_chain(&e);
                                if attempt < retries {
                                    tracing::warn!(
                                        "HTTP request to {} failed (attempt {}/{}): {}. Retrying...",
                                        url,
                                        attempt + 1,
                                        retries + 1,
                                        last_error
                                    );
                                    tokio::select! {
                                        _ = tokio::time::sleep(Duration::from_secs(2)) => {},
                                        _ = poll_cancel(cancelled.clone()) => {
                                            return Err("HTTP request cancelled".into());
                                        }
                                    }
                                }
                            }
                        }
                    }

                    Err(format!(
                        "HTTP {} request to '{}' failed after {} attempts. Last error: {}",
                        method,
                        url,
                        retries + 1,
                        last_error
                    )
                    .into())
                })
            },
        );

        // HTTP multipart POST request (for file uploads)
        let http_mp_cancelled = cancelled.clone();
        module.set_native_fn(
            "http_request_multipart",
            move |url: &str,
                  fields: Map,
                  headers: Map,
                  options: Map|
                  -> Result<Dynamic, Box<::rhai::EvalAltResult>> {
                let timeout_secs = options
                    .get("timeout")
                    .and_then(|v| v.as_float().ok().map(|f| f as u64))
                    .or_else(|| {
                        options
                            .get("timeout")
                            .and_then(|v| v.as_int().ok().map(|i| i as u64))
                    })
                    .unwrap_or(120);

                let client = reqwest::Client::builder()
                    .timeout(Duration::from_secs(timeout_secs))
                    .build()
                    .map_err(|e| format!("failed to create HTTP client: {}", e))?;

                let mut form = reqwest::multipart::Form::new();

                // process fields - each field can be:
                // - A string (text field)
                // - A map with "file_base64", "filename", and optionally "mime_type" (file field)
                for (key, value) in &fields {
                    if let Ok(text) = value.clone().into_string() {
                        form = form.text(key.to_string(), text.to_string());
                    } else if value.is_map() {
                        let field_map = value.clone().cast::<Map>();
                        if let Some(file_b64) = field_map.get("file_base64") {
                            let b64_str = file_b64
                                .clone()
                                .into_string()
                                .map_err(|_| "file_base64 must be a string")?;
                            let bytes = base64::engine::general_purpose::STANDARD
                                .decode(b64_str.as_str())
                                .map_err(|e| format!("failed to decode file_base64: {}", e))?;

                            let filename = field_map
                                .get("filename")
                                .and_then(|v| v.clone().into_string().ok())
                                .unwrap_or_else(|| "file".into())
                                .to_string();

                            let mime_type = field_map
                                .get("mime_type")
                                .and_then(|v| v.clone().into_string().ok())
                                .unwrap_or_else(|| "application/octet-stream".into())
                                .to_string();

                            let part = reqwest::multipart::Part::bytes(bytes)
                                .file_name(filename)
                                .mime_str(&mime_type)
                                .map_err(|e| format!("invalid mime type: {}", e))?;

                            form = form.part(key.to_string(), part);
                        }
                    }
                }

                let mut request = client.post(url).multipart(form);

                for (k, v) in &headers {
                    if let Ok(val) = v.clone().into_string() {
                        request = request.header(k.as_str(), val);
                    }
                }

                let handle = tokio::runtime::Handle::current();
                let cancelled = http_mp_cancelled.clone();

                handle.block_on(async move {
                    if cancelled.load(Ordering::Relaxed) {
                        return Err::<Dynamic, Box<::rhai::EvalAltResult>>(
                            "HTTP request cancelled".into(),
                        );
                    }

                    let response = tokio::select! {
                        res = request.send() => res.map_err(|e| {
                            format!("HTTP multipart request failed: {}", error_chain(&e))
                        })?,
                        _ = poll_cancel(cancelled.clone()) => {
                            return Err("HTTP request cancelled".into());
                        }
                    };

                    let status = response.status().as_u16();
                    let resp_text = tokio::select! {
                        t = response.text() => t.unwrap_or_default(),
                        _ = poll_cancel(cancelled.clone()) => {
                            return Err("HTTP request cancelled".into());
                        }
                    };

                    let body_dynamic =
                        if let Ok(json_val) = serde_json::from_str::<JsonValue>(&resp_text) {
                            ::rhai::serde::to_dynamic(&json_val)
                                .unwrap_or(Dynamic::from(resp_text.clone()))
                        } else {
                            Dynamic::from(resp_text)
                        };

                    let mut result = Map::new();
                    result.insert("status".into(), Dynamic::from(status as i64));
                    result.insert("body".into(), body_dynamic);
                    Ok(Dynamic::from(result))
                })
            },
        );

        engine.register_global_module(module.into());
        engine
    }

    /// create an engine with cancellation and progress support for execution
    fn create_execution_engine(ctx: Arc<ScriptContext>) -> Engine {
        let mut engine = Self::create_base_engine(ctx.cancelled.clone());

        // set up progress callback to check cancellation periodically
        let cancelled = ctx.cancelled.clone();
        engine.on_progress(move |_ops| {
            if cancelled.load(Ordering::Relaxed) {
                info!("[rhai] script cancellation triggered via progress callback");
                Some("script execution cancelled".into())
            } else {
                None
            }
        });

        // register is_cancelled() function
        let cancelled_for_fn = ctx.cancelled.clone();
        engine.register_fn("is_cancelled", move || -> bool {
            cancelled_for_fn.load(Ordering::Relaxed)
        });

        // register report_progress(progress, message) function
        let progress_fn = ctx.report_progress.clone();
        engine.register_fn(
            "report_progress",
            move |progress: f64, message: ImmutableString| {
                progress_fn(progress as f32, Some(message.to_string()));
            },
        );

        // register report_progress(progress) without message
        let progress_fn2 = ctx.report_progress.clone();
        engine.register_fn("report_progress", move |progress: f64| {
            progress_fn2(progress as f32, None);
        });

        // accumulated state for emit_output (per output name)
        let accumulated_state: Arc<std::sync::Mutex<HashMap<String, String>>> =
            Arc::new(std::sync::Mutex::new(HashMap::new()));

        // register emit_output(output_name, delta_text) — appends delta and emits
        let emit_fn = ctx.emit_partial_output.clone();
        let acc = accumulated_state.clone();
        engine.register_fn(
            "emit_output",
            move |output_name: ImmutableString, delta: ImmutableString| {
                let mut state = acc.lock().unwrap();
                let accumulated = state.entry(output_name.to_string()).or_default();
                accumulated.push_str(&delta);
                let acc_val = accumulated.clone();
                emit_fn(
                    output_name.to_string(),
                    crate::value::Value::String(delta.to_string()),
                    crate::value::Value::String(acc_val),
                );
            },
        );

        // register sleep(ms) for testing and pacing
        engine.register_fn("sleep", |ms: i64| {
            std::thread::sleep(std::time::Duration::from_millis(ms as u64));
        });

        // streaming HTTP request that parses SSE and emits partial outputs
        let stream_emit_fn = ctx.emit_partial_output.clone();
        let stream_cancelled = ctx.cancelled.clone();
        engine.register_fn(
            "http_request_streaming",
            move |method: ImmutableString,
                  url: ImmutableString,
                  body: Map,
                  headers: Map,
                  options: Map|
                  -> Result<Dynamic, Box<::rhai::EvalAltResult>> {
                let output_name = options
                    .get("output_name")
                    .and_then(|v| v.clone().into_string().ok())
                    .unwrap_or_else(|| "text".into());

                let timeout_secs = options
                    .get("timeout")
                    .and_then(|v| v.as_int().ok().map(|i| i as u64))
                    .unwrap_or(600);

                let client = reqwest::blocking::Client::builder()
                    .timeout(std::time::Duration::from_secs(timeout_secs))
                    .build()
                    .map_err(|e| format!("failed to create HTTP client: {}", e))?;

                let mut request = match method.to_uppercase().as_str() {
                    "POST" => client.post(url.as_str()),
                    "GET" => client.get(url.as_str()),
                    _ => {
                        return Err(format!(
                            "http_request_streaming: unsupported method {}",
                            method
                        )
                        .into())
                    }
                };

                for (k, v) in &headers {
                    if let Ok(val) = v.clone().into_string() {
                        request = request.header(k.as_str(), val);
                    }
                }

                if !body.is_empty() {
                    let body_json: JsonValue = ::rhai::serde::from_dynamic(&Dynamic::from(body))
                        .map_err(|e| e.to_string())?;
                    request = request.json(&body_json);
                }

                let response = request.send().map_err(|e| {
                    format!("streaming request to '{}' failed: {}", url, error_chain(&e))
                })?;

                let status = response.status().as_u16();
                if status >= 400 {
                    let text = response.text().unwrap_or_default();
                    let mut result = Map::new();
                    result.insert("status".into(), Dynamic::from(status as i64));
                    let body_dynamic =
                        if let Ok(json_val) = serde_json::from_str::<JsonValue>(&text) {
                            ::rhai::serde::to_dynamic(&json_val).map_err(|e| e.to_string())?
                        } else {
                            Dynamic::from(text)
                        };
                    result.insert("body".into(), body_dynamic);
                    return Ok(Dynamic::from(result));
                }

                // read SSE stream line by line
                let reader = std::io::BufRead::lines(std::io::BufReader::new(response));
                let mut accumulated_content = String::new();
                let mut accumulated_thinking = String::new();
                let mut usage_val: Option<JsonValue> = None;

                for line_result in reader {
                    if stream_cancelled.load(Ordering::Relaxed) {
                        break;
                    }

                    let line =
                        line_result.map_err(|e| format!("error reading SSE stream: {}", e))?;

                    if !line.starts_with("data: ") {
                        continue;
                    }

                    let data = &line[6..];
                    if data == "[DONE]" {
                        break;
                    }

                    if let Ok(chunk) = serde_json::from_str::<JsonValue>(data) {
                        // extract delta content from OpenAI format
                        if let Some(choices) = chunk.get("choices").and_then(|c| c.as_array()) {
                            if let Some(choice) = choices.first() {
                                if let Some(delta) = choice.get("delta") {
                                    if let Some(content) =
                                        delta.get("content").and_then(|c| c.as_str())
                                    {
                                        accumulated_content.push_str(content);
                                        stream_emit_fn(
                                            output_name.to_string(),
                                            crate::value::Value::String(content.to_string()),
                                            crate::value::Value::String(
                                                accumulated_content.clone(),
                                            ),
                                        );
                                    }
                                    if let Some(reasoning) =
                                        delta.get("reasoning_content").and_then(|c| c.as_str())
                                    {
                                        accumulated_thinking.push_str(reasoning);
                                        stream_emit_fn(
                                            "thinking".to_string(),
                                            crate::value::Value::String(reasoning.to_string()),
                                            crate::value::Value::String(
                                                accumulated_thinking.clone(),
                                            ),
                                        );
                                    }
                                }
                            }
                        }
                        // capture usage from the final chunk
                        if let Some(usage) = chunk.get("usage") {
                            if !usage.is_null() {
                                usage_val = Some(usage.clone());
                            }
                        }
                    }
                }

                // build result matching non-streaming OpenAI response shape
                let mut body_map = Map::new();
                let choice_message = serde_json::json!({
                    "message": {
                        "content": accumulated_content,
                        "reasoning_content": accumulated_thinking,
                    }
                });
                let choices_dynamic =
                    ::rhai::serde::to_dynamic(serde_json::json!([choice_message]))
                        .map_err(|e| e.to_string())?;
                body_map.insert("choices".into(), choices_dynamic);

                if let Some(usage) = usage_val {
                    let usage_dynamic =
                        ::rhai::serde::to_dynamic(&usage).map_err(|e| e.to_string())?;
                    body_map.insert("usage".into(), usage_dynamic);
                }

                let mut result = Map::new();
                result.insert("status".into(), Dynamic::from(status as i64));
                result.insert("body".into(), Dynamic::from(body_map));
                Ok(Dynamic::from(result))
            },
        );

        engine
    }
}

impl ScriptEngine for RhaiEngine {
    fn language(&self) -> &str {
        "rhai"
    }

    fn parse_spec(&self, script: &str, filename: &str) -> Result<ScriptSpec> {
        let engine = Self::create_base_engine(Arc::new(AtomicBool::new(false)));
        let ast = engine.compile(script).map_err(|e| {
            let position = e.position();
            let err_msg = e.to_string();
            if position.is_none() {
                anyhow!("failed to compile script {}: {}", filename, err_msg)
            } else {
                let line = position.line().unwrap_or(0);
                let pos = position.position().unwrap_or(0);
                anyhow!(
                    "failed to compile script {}:{}:{}: {}",
                    filename,
                    line,
                    pos,
                    err_msg
                )
            }
        })?;

        let mut scope = Scope::new();
        let spec_result_dynamic: Dynamic =
            engine.call_fn(&mut scope, &ast, "spec", ()).map_err(|e| {
                let position = e.position();
                let err_msg = e.to_string();
                if position.is_none() {
                    anyhow!("failed to call spec() in {}: {}", filename, err_msg)
                } else {
                    let line = position.line().unwrap_or(0);
                    let pos = position.position().unwrap_or(0);
                    anyhow!(
                        "failed to call spec() in {}:{}:{}: {}",
                        filename,
                        line,
                        pos,
                        err_msg
                    )
                }
            })?;

        // convert to JSON and use common parser
        let json_val: JsonValue = ::rhai::serde::from_dynamic(&spec_result_dynamic)
            .map_err(|e| anyhow!("failed to convert spec result to JSON: {}", e))?;

        parse_spec_from_json(json_val)
    }

    fn execute(
        &self,
        script: &str,
        inputs: HashMap<String, JsonValue>,
        ctx: Arc<ScriptContext>,
    ) -> Result<HashMap<String, JsonValue>> {
        // check if already cancelled before starting
        if ctx.is_cancelled() {
            info!("[rhai] script execution skipped - already cancelled");
            return Err(anyhow!("script execution cancelled"));
        }

        debug!("[rhai] starting script execution");
        let engine = Self::create_execution_engine(ctx.clone());
        let ast = engine.compile(script).map_err(|e| {
            let position = e.position();
            let err_msg = e.to_string();
            if position.is_none() {
                anyhow!("failed to compile script: {}", err_msg)
            } else {
                let line = position.line().unwrap_or(0);
                let pos = position.position().unwrap_or(0);
                anyhow!("failed to compile script at {}:{}: {}", line, pos, err_msg)
            }
        })?;
        let mut scope = Scope::new();

        // convert inputs to Rhai map
        let mut inputs_map = Map::new();
        for (k, v) in inputs {
            let rhai_val = ::rhai::serde::to_dynamic(&v).map_err(|e| anyhow!(e.to_string()))?;
            inputs_map.insert(k.into(), rhai_val);
        }

        let result: Dynamic = engine
            .call_fn(&mut scope, &ast, "execute", (inputs_map,))
            .map_err(|e| {
                let position = e.position();

                // extract detailed error information
                let (err_type, err_detail) = extract_rhai_error_details(&e);

                // check if this was a cancellation
                if err_detail.contains("script execution cancelled") {
                    info!("[rhai] script execution cancelled");
                    return anyhow!("script execution cancelled");
                }

                let location = if position.is_none() {
                    String::new()
                } else {
                    format!(
                        " at line {}, column {}",
                        position.line().unwrap_or(0),
                        position.position().unwrap_or(0)
                    )
                };

                anyhow!("script error{}: [{}] {}", location, err_type, err_detail)
            })?;

        // convert result back to JSON
        let json_val: JsonValue = ::rhai::serde::from_dynamic(&result)?;

        // expected to be a map of outputs
        let result_map = json_val
            .as_object()
            .ok_or_else(|| anyhow!("script execution must return an object"))?;

        Ok(result_map
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect())
    }

    fn get_options(
        &self,
        script: &str,
        input_name: &str,
        inputs: HashMap<String, JsonValue>,
    ) -> Result<Vec<SelectOption>> {
        let engine = Self::create_base_engine(Arc::new(AtomicBool::new(false)));
        let ast = engine
            .compile(script)
            .map_err(|e| anyhow!("failed to compile script for options: {}", e))?;
        let mut scope = Scope::new();

        // check if get_options function exists
        let has_fn = ast.iter_functions().any(|f| f.name == "get_options");
        if !has_fn {
            return Ok(vec![]);
        }

        // convert inputs to Rhai map
        let mut inputs_map = Map::new();
        for (k, v) in inputs {
            let rhai_val = ::rhai::serde::to_dynamic(&v).map_err(|e| anyhow!(e.to_string()))?;
            inputs_map.insert(k.into(), rhai_val);
        }

        let result: Dynamic = engine
            .call_fn(
                &mut scope,
                &ast,
                "get_options",
                (input_name.to_string(), inputs_map),
            )
            .map_err(|e| anyhow!("failed to call get_options(): {}", e))?;

        // convert to JSON and parse
        let json_val: JsonValue = ::rhai::serde::from_dynamic(&result)
            .map_err(|e| anyhow!("failed to convert get_options result to JSON: {}", e))?;

        let arr = json_val
            .as_array()
            .ok_or_else(|| anyhow!("get_options must return an array"))?;

        parse_select_options_list(arr)
    }
}
