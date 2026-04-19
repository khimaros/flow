use crate::node::{DataType, InputSpec, Node, NodeContext, OutputSpec, UIComponent};
use crate::value::Value;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use glob::glob;
use std::collections::BTreeMap;
use std::process::Stdio;
use tokio::process::Command;

pub struct ShellCommandNode;

#[async_trait]
impl Node for ShellCommandNode {
    fn name(&self) -> &str {
        "ShellCommand"
    }

    fn title(&self) -> &str {
        "Shell Command"
    }

    fn category(&self) -> &str {
        "System"
    }

    fn description(&self) -> &str {
        "Execute a shell command"
    }

    fn inputs(&self) -> Vec<InputSpec> {
        vec![
            InputSpec {
                name: "command".to_string(),
                r#type: DataType::String,
                ui_component: UIComponent::Auto {},
                default: None,
                required: false,
                description: Some(
                    "command to execute (e.g., 'echo')".to_string(),
                ),
                ..Default::default()
            },
            InputSpec {
                name: "args".to_string(),
                r#type: DataType::List,
                ui_component: UIComponent::Auto {},
                default: Some(Value::String("".to_string())),
                required: false,
                description: Some(
                    "arguments for the command (first arg used as command if command is empty)"
                        .to_string(),
                ),
                ..Default::default()
            },
            InputSpec {
                name: "stdin".to_string(),
                r#type: DataType::String,
                ui_component: UIComponent::TextArea {},
                default: Some(Value::String("".to_string())),
                required: false,
                description: Some("input to pipe to the command's stdin".to_string()),
                ..Default::default()
            },
        ]
    }

    fn outputs(&self) -> Vec<OutputSpec> {
        vec![
            OutputSpec {
                name: "stdout".to_string(),
                r#type: DataType::String,
                description: Some("standard output".to_string()),
            },
            OutputSpec {
                name: "stderr".to_string(),
                r#type: DataType::String,
                description: Some("standard error".to_string()),
            },
            OutputSpec {
                name: "exit_code".to_string(),
                r#type: DataType::Integer,
                description: Some("exit code".to_string()),
            },
        ]
    }

    async fn execute(
        &self,
        inputs: BTreeMap<String, Value>,
        ctx: NodeContext,
    ) -> Result<BTreeMap<String, Value>> {
        let cmd_input = inputs
            .get("command")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty());

        // handle args input as array, JSON string, or plain string
        let explicit_args: Vec<String> = match inputs.get("args") {
            Some(Value::Array(arr)) => arr
                .iter()
                .filter_map(|x| match x {
                    Value::String(s) if !s.is_empty() => Some(s.clone()),
                    Value::Integer(i) => Some(i.to_string()),
                    Value::Float(f) => Some(f.to_string()),
                    _ => None,
                })
                .collect(),
            Some(Value::String(s)) if !s.is_empty() => {
                // try parsing as JSON array first (from ListEditor)
                if let Ok(parsed) = serde_json::from_str::<Vec<String>>(s) {
                    parsed.into_iter().filter(|x| !x.is_empty()).collect()
                } else {
                    // treat as single argument
                    vec![s.clone()]
                }
            }
            _ => vec![],
        };

        // determine command and arguments
        let (cmd, args) = if let Some(cmd_str) = cmd_input {
            if explicit_args.is_empty() {
                // legacy behavior: split command string
                let parts: Vec<&str> = cmd_str.split_whitespace().collect();
                if parts.is_empty() {
                    return Err(anyhow!("empty command"));
                }
                let cmd = parts[0];
                let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
                (cmd.to_string(), expand_glob_args(args))
            } else {
                // command + Explicit Args
                (cmd_str.to_string(), expand_glob_args(explicit_args))
            }
        } else if !explicit_args.is_empty() {
            // no command input, treat first arg as command
            let cmd = explicit_args[0].clone();
            let args = explicit_args[1..].to_vec();
            (cmd, expand_glob_args(args))
        } else {
            return Err(anyhow!("either 'command' or 'args' must be provided"));
        };

        // get stdin input if provided
        let stdin_input = inputs
            .get("stdin")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let mut child = Command::new(&cmd)
            .args(&args)
            .stdin(if stdin_input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;

        // write stdin in background if provided
        if let Some(input) = stdin_input {
            if let Some(mut s) = child.stdin.take() {
                tokio::spawn(async move {
                    use tokio::io::AsyncWriteExt;
                    let _ = s.write_all(input.as_bytes()).await;
                });
            }
        }

        let cancel_token = ctx.cancel_token();

        // stream stdout line-by-line, emitting partial outputs
        let mut stdout_handle = child.stdout.take().expect("stdout was piped");
        let mut stderr_handle = child.stderr.take().expect("stderr was piped");

        let mut accumulated_stdout = String::new();
        let mut accumulated_stderr = String::new();

        use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

        let mut stdout_reader = BufReader::new(&mut stdout_handle).lines();

        // read stderr fully in background
        let stderr_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            let _ = stderr_handle.read_to_end(&mut buf).await;
            String::from_utf8_lossy(&buf).to_string()
        });

        loop {
            tokio::select! {
                line = stdout_reader.next_line() => {
                    match line {
                        Ok(Some(line)) => {
                            if !accumulated_stdout.is_empty() {
                                accumulated_stdout.push('\n');
                            }
                            accumulated_stdout.push_str(&line);
                            ctx.emit_partial_output(
                                "stdout",
                                Value::String(format!("{}\n", line)),
                                Value::String(accumulated_stdout.clone()),
                            );
                        }
                        Ok(None) => break,
                        Err(e) => {
                            accumulated_stderr.push_str(&format!("error reading stdout: {}\n", e));
                            break;
                        }
                    }
                }
                _ = cancel_token.cancelled() => {
                    return Err(anyhow!("command cancelled"));
                }
            }
        }

        let status = child.wait().await?;
        let stderr = stderr_task.await.unwrap_or_default();
        accumulated_stderr.push_str(&stderr);

        let exit_code = status.code().unwrap_or(-1);

        let mut outputs = BTreeMap::new();
        outputs.insert(
            "stdout".to_string(),
            Value::String(accumulated_stdout.clone()),
        );
        outputs.insert(
            "stderr".to_string(),
            Value::String(accumulated_stderr.clone()),
        );
        outputs.insert("exit_code".to_string(), Value::Integer(exit_code as i64));

        if exit_code != 0 {
            let error_msg = format!(
                "Command '{}' exited with code {}:\nSTDOUT:\n{}\nSTDERR:\n{}",
                cmd,
                exit_code,
                accumulated_stdout.trim(),
                accumulated_stderr.trim()
            );
            return Err(anyhow!(error_msg));
        }

        Ok(outputs)
    }
}

/// check if a string contains glob pattern characters
fn is_glob_pattern(s: &str) -> bool {
    s.contains('*') || s.contains('?') || s.contains('[')
}

/// expand glob patterns in arguments, similar to shell behavior.
/// if pattern matches files, returns expanded paths; otherwise keeps original.
fn expand_glob_args(args: Vec<String>) -> Vec<String> {
    args.into_iter()
        .flat_map(|arg| {
            if !is_glob_pattern(&arg) {
                return vec![arg];
            }
            match glob(&arg) {
                Ok(paths) => {
                    let matches: Vec<String> = paths
                        .filter_map(|p| p.ok())
                        .map(|p| p.to_string_lossy().to_string())
                        .collect();
                    if matches.is_empty() {
                        vec![arg]
                    } else {
                        matches
                    }
                }
                Err(_) => vec![arg],
            }
        })
        .collect()
}
