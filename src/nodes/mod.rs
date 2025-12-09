pub mod common;
pub mod declarative;
pub mod display;
pub mod http;
pub mod json;
pub mod list;
pub mod loop_node;
pub mod process;
pub mod random;
pub mod script;
pub mod string;
pub mod web;

use crate::engine::NodeRegistry;
use crate::node::Node;
use std::fs;
use std::path::Path;
use tracing::{info, warn};

pub fn register_all(registry: &mut NodeRegistry, quiet: bool) {
    // built-in nodes
    registry.register("Echo", || Box::new(common::EchoNode));
    registry.register("Read", || Box::new(common::ReadNode));
    registry.register("ShellCommand", || Box::new(process::ShellCommandNode));
    registry.register("HttpRequest", || Box::new(http::HttpRequestNode));
    registry.register("JsonQuery", || Box::new(json::JsonQueryNode));
    registry.register("RandomInteger", || Box::new(random::RandomIntegerNode));
    registry.register("AudioInput", || Box::new(display::AudioInputNode));
    registry.register("DisplayImage", || Box::new(display::DisplayImageNode));
    registry.register("DisplayAudio", || Box::new(display::DisplayAudioNode));
    registry.register("DisplayMarkdown", || Box::new(display::DisplayMarkdownNode));
    registry.register("DisplayJson", || Box::new(display::DisplayJsonNode));
    registry.register("List", || Box::new(display::DisplayListNode));
    registry.register("WebFetch", || Box::new(web::WebFetchNode));
    registry.register("HtmlToMarkdown", || Box::new(web::HtmlToMarkdownNode));
    registry.register("WebSearch", || Box::new(web::WebSearchNode));
    registry.register("Templatize", || Box::new(string::TemplatizeNode));
    registry.register("Join", || Box::new(string::JoinNode));
    registry.register("Split", || Box::new(string::SplitNode));
    registry.register("ListToJson", || Box::new(string::ListToJsonNode));
    registry.register("RegexpExtract", || Box::new(string::RegexpExtractNode));
    registry.register("Loop", || Box::new(loop_node::LoopNode));
    registry.register("Flatten", || Box::new(list::FlattenNode));
    registry.register("Zip", || Box::new(list::ZipNode));

    // dynamic Script Nodes from user_nodes/ directory
    // supports: .rhai, .py, .lua, .ts, .json (declarative)
    let nodes_dir = Path::new("user_nodes");
    if nodes_dir.exists() && nodes_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(nodes_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let ext = path.extension().map(|e| e.to_string_lossy().to_string());
                let filename = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();

                if ext.as_deref() == Some("json") {
                    // declarative Node
                    match fs::read_to_string(&path) {
                        Ok(content) => match declarative::DeclarativeNode::new(&content) {
                            Ok(node) => {
                                if !quiet {
                                    info!("registered {} ({})", node.name(), filename);
                                }
                                let node_clone = node.clone();
                                registry
                                    .register(node.name(), move || Box::new(node_clone.clone()));
                            }
                            Err(e) => {
                                if !quiet {
                                    warn!(
                                        file = %filename,
                                        error = %e,
                                        "failed to load declarative node"
                                    );
                                }
                            }
                        },
                        Err(e) => {
                            warn!(path = ?path, error = %e, "failed to read file");
                        }
                    }
                    continue;
                }

                // check for supported script extensions
                let is_supported = matches!(
                    ext.as_deref(),
                    Some("rhai") | Some("py") | Some("lua") | Some("ts")
                );

                if is_supported {
                    match fs::read_to_string(&path) {
                        Ok(content) => {
                            let filename = path
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string();
                            match script::ScriptDefinedNode::new(&content, &filename) {
                                Ok(node) => {
                                    if !quiet {
                                        info!("registered {} ({})", node.name(), filename);
                                    }
                                    let node_clone = node.clone();
                                    registry.register(node.name(), move || {
                                        Box::new(node_clone.clone())
                                    });
                                }
                                Err(e) => {
                                    warn!(file = %filename, error = %e, "failed to load script node");
                                }
                            }
                        }
                        Err(e) => {
                            warn!(path = ?path, error = %e, "failed to read file");
                        }
                    }
                }
            }
        }
    }
}
