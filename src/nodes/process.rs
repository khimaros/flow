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
        "Execute a shell command line (via `<shell> -c`, default sh) or run a program directly with explicit argv. Provide exactly one of `command` or `argv`."
    }

    fn inputs(&self) -> Vec<InputSpec> {
        vec![
            InputSpec {
                name: "command".to_string(),
                r#type: DataType::String,
                ui_component: UIComponent::TextArea {},
                default: None,
                required: false,
                description: Some(
                    "shell command line, run via the chosen `shell` with `-c` (default \
                     `sh`). Pipes, redirects, quoting, globs, and env-expansion all \
                     work (e.g., \"du -sh * | sort -h\"). Mutually exclusive with \
                     `argv` — set exactly one."
                        .to_string(),
                ),
                ..Default::default()
            },
            InputSpec {
                name: "shell".to_string(),
                r#type: DataType::String,
                ui_component: UIComponent::Auto {},
                default: Some(Value::String("sh".to_string())),
                required: false,
                description: Some(
                    "shell binary used to interpret `command` (default `sh`). Only \
                     applies when `command` is set; ignored when `argv` is used."
                        .to_string(),
                ),
                ..Default::default()
            },
            InputSpec {
                name: "argv".to_string(),
                r#type: DataType::List,
                ui_component: UIComponent::Auto {},
                default: Some(Value::String("".to_string())),
                required: false,
                description: Some(
                    "explicit argument vector for direct execution (no shell). First \
                     element is the program, rest are its arguments — e.g., \
                     [\"ls\", \"-l\", \"/tmp\"]. No shell metacharacter interpretation: \
                     pipes, redirects, quoting and env-expansion are NOT interpreted. \
                     Use `command` if you need any of those. Mutually exclusive with \
                     `command` — set exactly one."
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
        let command_input = inputs
            .get("command")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty());

        let shell_bin = inputs
            .get("shell")
            .and_then(|v| v.as_str())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .unwrap_or("sh")
            .to_string();

        let argv: Vec<String> = match inputs.get("argv") {
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
                // ListEditor serializes to a JSON-encoded string
                if let Ok(parsed) = serde_json::from_str::<Vec<String>>(s) {
                    parsed.into_iter().filter(|x| !x.is_empty()).collect()
                } else {
                    vec![s.clone()]
                }
            }
            _ => vec![],
        };

        let (cmd, args) = match (command_input, argv.is_empty()) {
            (Some(s), true) => (shell_bin, vec!["-c".to_string(), s.to_string()]),
            (None, false) => {
                let mut it = argv.into_iter();
                let cmd = it.next().expect("argv non-empty");
                (cmd, expand_glob_args(it.collect()))
            }
            (Some(_), false) => {
                return Err(anyhow!(
                    "`command` and `argv` are mutually exclusive — set exactly one"
                ))
            }
            (None, true) => {
                return Err(anyhow!("either `command` or `argv` must be provided"))
            }
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
