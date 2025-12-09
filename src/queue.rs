use crate::engine::{Engine, EngineError, ExecutionEvent};
use crate::graph::Workflow;
use crate::value::Value;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, Mutex, RwLock};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Queued,
    Running,
    Completed,
    Error,
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
pub struct Job {
    pub id: String,
    pub workflow_name: Option<String>,
    pub status: JobStatus,
    pub created_at: u64,
    pub started_at: Option<u64>,
    pub completed_at: Option<u64>,
    pub total_nodes: usize,
    pub completed_nodes: usize,
    pub active_nodes: Vec<String>,
    pub current_node: Option<String>,
    pub node_progress: HashMap<String, NodeProgress>,
    pub error: Option<String>,
    pub force_run: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeProgress {
    pub node_id: String,
    pub progress: f32,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum JobEvent {
    JobCreated {
        job: Job,
    },
    JobStarted {
        job_id: String,
    },
    NodeStarted {
        job_id: String,
        node_id: String,
    },
    NodeProgress {
        job_id: String,
        node_id: String,
        progress: f32,
        message: Option<String>,
    },
    NodeFinished {
        job_id: String,
        node_id: String,
        result: BTreeMap<String, Value>,
        cached: bool,
    },
    NodePartialOutput {
        job_id: String,
        node_id: String,
        output_name: String,
        delta: Value,
        accumulated: Value,
    },
    NodeError {
        job_id: String,
        node_id: String,
        error: String,
    },
    JobCompleted {
        job_id: String,
    },
    JobError {
        job_id: String,
        error: String,
    },
    JobCancelled {
        job_id: String,
    },
    Shutdown,
}

pub struct Queue {
    jobs: Arc<RwLock<Vec<Job>>>,
    engine: Mutex<Engine>,
    event_tx: broadcast::Sender<JobEvent>,
    pending_tx: mpsc::Sender<(Job, Workflow, CancellationToken)>,
    cancellation_tokens: Arc<RwLock<HashMap<String, CancellationToken>>>,
    shutdown_token: CancellationToken,
}

impl Queue {
    pub fn new(engine: Engine) -> Arc<Self> {
        let (event_tx, _) = broadcast::channel(1000);
        let (pending_tx, pending_rx) = mpsc::channel(100);
        let shutdown_token = CancellationToken::new();

        let queue = Arc::new(Self {
            jobs: Arc::new(RwLock::new(Vec::new())),
            engine: Mutex::new(engine),
            event_tx,
            pending_tx,
            cancellation_tokens: Arc::new(RwLock::new(HashMap::new())),
            shutdown_token: shutdown_token.clone(),
        });

        // spawn the job processor
        let queue_clone = queue.clone();
        tokio::spawn(async move {
            queue_clone.process_jobs(pending_rx, shutdown_token).await;
        });

        queue
    }

    async fn process_jobs(
        self: Arc<Self>,
        mut pending_rx: mpsc::Receiver<(Job, Workflow, CancellationToken)>,
        shutdown_token: CancellationToken,
    ) {
        loop {
            tokio::select! {
                _ = shutdown_token.cancelled() => break,
                job_opt = pending_rx.recv() => {
                    match job_opt {
                        Some((job, workflow, cancel_token)) => {
                            self.run_job(job, workflow, cancel_token).await;
                        }
                        None => break,
                    }
                }
            }
        }
    }

    /// signal the queue to shut down and cancel any running jobs
    pub async fn shutdown(&self) {
        info!("queue shutdown initiated");

        // cancel all running jobs
        let tokens = self.cancellation_tokens.read().await;
        let job_count = tokens.len();
        for (job_id, token) in tokens.iter() {
            info!(job_id = %job_id, "cancelling job due to shutdown");
            token.cancel();
        }
        drop(tokens);

        info!(cancelled_jobs = job_count, "cancelled all running jobs");

        // signal the processor to stop
        self.shutdown_token.cancel();

        // notify SSE clients to disconnect
        let _ = self.event_tx.send(JobEvent::Shutdown);
        info!("queue shutdown complete");
    }

    async fn run_job(&self, job: Job, workflow: Workflow, cancel_token: CancellationToken) {
        let job_id = job.id.clone();
        let workflow_name = job
            .workflow_name
            .clone()
            .unwrap_or_else(|| "<unnamed>".to_string());

        // check if cancelled before starting
        if cancel_token.is_cancelled() {
            info!(job_id = %job_id, workflow = %workflow_name, "job cancelled before execution started");
            self.mark_job_cancelled(&job_id).await;
            return;
        }

        info!(job_id = %job_id, workflow = %workflow_name, total_nodes = job.total_nodes, "job starting");

        // mark job as started
        {
            let mut jobs = self.jobs.write().await;
            if let Some(j) = jobs.iter_mut().find(|j| j.id == job_id) {
                j.status = JobStatus::Running;
                j.started_at = Some(Self::now());
            }
        }
        let _ = self.event_tx.send(JobEvent::JobStarted {
            job_id: job_id.clone(),
        });

        // execute with progress tracking
        let (tx, mut rx) = mpsc::channel::<ExecutionEvent>(100);

        let job_id_clone = job_id.clone();
        let event_tx = self.event_tx.clone();
        let jobs = self.jobs.clone();

        // spawn a task to forward execution events to job events
        let event_handler = tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                match event {
                    ExecutionEvent::Started { node_id, .. } => {
                        // update job state
                        {
                            let mut jobs = jobs.write().await;
                            if let Some(j) = jobs.iter_mut().find(|j| j.id == job_id_clone) {
                                j.current_node = Some(node_id.clone());
                                if !j.active_nodes.contains(&node_id) {
                                    j.active_nodes.push(node_id.clone());
                                }
                                j.node_progress.insert(
                                    node_id.clone(),
                                    NodeProgress {
                                        node_id: node_id.clone(),
                                        progress: 0.0,
                                        message: None,
                                    },
                                );
                            }
                        }
                        let _ = event_tx.send(JobEvent::NodeStarted {
                            job_id: job_id_clone.clone(),
                            node_id,
                        });
                    }
                    ExecutionEvent::Progress {
                        node_id,
                        progress,
                        message,
                    } => {
                        // update node progress
                        {
                            let mut jobs = jobs.write().await;
                            if let Some(j) = jobs.iter_mut().find(|j| j.id == job_id_clone) {
                                j.node_progress.insert(
                                    node_id.clone(),
                                    NodeProgress {
                                        node_id: node_id.clone(),
                                        progress,
                                        message: message.clone(),
                                    },
                                );
                            }
                        }
                        let _ = event_tx.send(JobEvent::NodeProgress {
                            job_id: job_id_clone.clone(),
                            node_id,
                            progress,
                            message,
                        });
                    }
                    ExecutionEvent::Finished {
                        node_id,
                        result,
                        cached,
                    } => {
                        // update job state
                        {
                            let mut jobs = jobs.write().await;
                            if let Some(j) = jobs.iter_mut().find(|j| j.id == job_id_clone) {
                                j.completed_nodes += 1;
                                j.active_nodes.retain(|id| id != &node_id);
                                j.current_node = j.active_nodes.first().cloned();
                                j.node_progress.insert(
                                    node_id.clone(),
                                    NodeProgress {
                                        node_id: node_id.clone(),
                                        progress: 100.0,
                                        message: None,
                                    },
                                );
                            }
                        }
                        let _ = event_tx.send(JobEvent::NodeFinished {
                            job_id: job_id_clone.clone(),
                            node_id,
                            result,
                            cached,
                        });
                    }
                    ExecutionEvent::PartialOutput {
                        node_id,
                        output_name,
                        delta,
                        accumulated,
                    } => {
                        let _ = event_tx.send(JobEvent::NodePartialOutput {
                            job_id: job_id_clone.clone(),
                            node_id,
                            output_name,
                            delta,
                            accumulated,
                        });
                    }
                    ExecutionEvent::Error { node_id, error } => {
                        let _ = event_tx.send(JobEvent::NodeError {
                            job_id: job_id_clone.clone(),
                            node_id,
                            error,
                        });
                    }
                }
            }
        });

        // execute the workflow
        info!(job_id = %job_id, "executing workflow in engine");
        let outcome = {
            let mut engine = self.engine.lock().await;
            let outcome = engine
                .execute(&workflow, Some(tx), cancel_token.clone(), false)
                .await;
            info!(job_id = %job_id, "engine execution finished");
            outcome
        };

        // wait for event handler to finish
        info!(job_id = %job_id, "waiting for event handler to finish");
        let _ = event_handler.await;
        info!(job_id = %job_id, "event handler finished");

        // update final job status
        {
            let mut jobs = self.jobs.write().await;
            if let Some(j) = jobs.iter_mut().find(|j| j.id == job_id) {
                j.completed_at = Some(Self::now());
                j.active_nodes.clear();
                j.current_node = None;

                // don't overwrite if already cancelled via API
                if j.status != JobStatus::Cancelled {
                    match &outcome.error {
                        None => {
                            j.status = JobStatus::Completed;
                            j.completed_nodes = j.total_nodes;
                        }
                        Some(EngineError::Cancelled) => {
                            j.status = JobStatus::Cancelled;
                        }
                        Some(EngineError::Anyhow(e)) => {
                            j.status = JobStatus::Error;
                            j.error = Some(e.to_string());
                        }
                    }
                }
            }
        }

        match outcome.error {
            None => {
                info!(job_id = %job_id, workflow = %workflow_name, "job completed successfully");
                let _ = self.event_tx.send(JobEvent::JobCompleted {
                    job_id: job_id.clone(),
                });
            }
            Some(EngineError::Cancelled) => {
                info!(job_id = %job_id, workflow = %workflow_name, "job cancelled during execution");
                let _ = self.event_tx.send(JobEvent::JobCancelled {
                    job_id: job_id.clone(),
                });
            }
            Some(EngineError::Anyhow(e)) => {
                let error_str = e.to_string();
                warn!(job_id = %job_id, workflow = %workflow_name, error = %error_str, "job failed with error");
                let _ = self.event_tx.send(JobEvent::JobError {
                    job_id: job_id.clone(),
                    error: error_str,
                });
            }
        }

        // remove cancellation token now that job is finished
        {
            let mut tokens = self.cancellation_tokens.write().await;
            tokens.remove(&job_id);
        }
    }

    pub async fn submit(&self, workflow: Workflow, workflow_name: Option<String>) -> Job {
        let job_id = format!("job_{}_{}", Self::now(), Self::random_suffix());
        let wf_name = workflow_name
            .clone()
            .unwrap_or_else(|| "<unnamed>".to_string());

        info!(
            job_id = %job_id,
            workflow = %wf_name,
            total_nodes = workflow.nodes.len(),
            force_run = workflow.force_run,
            "Job submitted to queue"
        );

        let job = Job {
            id: job_id.clone(),
            workflow_name,
            status: JobStatus::Queued,
            created_at: Self::now(),
            started_at: None,
            completed_at: None,
            total_nodes: workflow.nodes.len(),
            completed_nodes: 0,
            active_nodes: Vec::new(),
            current_node: None,
            node_progress: HashMap::new(),
            error: None,
            force_run: workflow.force_run,
        };

        // create cancellation token for this job
        let cancel_token = CancellationToken::new();
        {
            let mut tokens = self.cancellation_tokens.write().await;
            tokens.insert(job_id.clone(), cancel_token.clone());
        }

        // add to job list
        {
            let mut jobs = self.jobs.write().await;
            jobs.insert(0, job.clone());

            // keep only last 100 jobs
            if jobs.len() > 100 {
                jobs.truncate(100);
            }
        }

        // send job created event
        let _ = self
            .event_tx
            .send(JobEvent::JobCreated { job: job.clone() });

        // queue for execution
        let _ = self
            .pending_tx
            .send((job.clone(), workflow, cancel_token))
            .await;

        job
    }

    pub async fn cancel_job(&self, job_id: &str) -> bool {
        info!(job_id = %job_id, "cancel requested for job");

        // trigger cancellation
        {
            let tokens = self.cancellation_tokens.read().await;
            if let Some(token) = tokens.get(job_id) {
                token.cancel();
            } else {
                warn!(job_id = %job_id, "cancel failed: job not found or already completed");
                return false;
            }
        }

        // mark job as cancelled
        self.mark_job_cancelled(job_id).await;
        true
    }

    async fn mark_job_cancelled(&self, job_id: &str) {
        {
            let mut jobs = self.jobs.write().await;
            if let Some(j) = jobs.iter_mut().find(|j| j.id == job_id) {
                j.status = JobStatus::Cancelled;
                j.completed_at = Some(Self::now());
                j.active_nodes.clear();
                j.current_node = None;
            }
        }

        // remove cancellation token
        {
            let mut tokens = self.cancellation_tokens.write().await;
            tokens.remove(job_id);
        }

        info!(job_id = %job_id, "job marked as cancelled");

        let _ = self.event_tx.send(JobEvent::JobCancelled {
            job_id: job_id.to_string(),
        });
    }

    pub async fn list_jobs(&self) -> Vec<Job> {
        self.jobs.read().await.clone()
    }

    pub async fn get_job(&self, job_id: &str) -> Option<Job> {
        self.jobs
            .read()
            .await
            .iter()
            .find(|j| j.id == job_id)
            .cloned()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<JobEvent> {
        self.event_tx.subscribe()
    }

    pub async fn clear_completed(&self) {
        let mut jobs = self.jobs.write().await;
        let before_count = jobs.len();
        jobs.retain(|j| j.status == JobStatus::Queued || j.status == JobStatus::Running);
        let removed = before_count - jobs.len();
        info!(
            removed_jobs = removed,
            remaining_jobs = jobs.len(),
            "Cleared completed jobs from queue"
        );
    }

    fn now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }

    fn random_suffix() -> String {
        use std::collections::hash_map::RandomState;
        use std::hash::{BuildHasher, Hasher};
        let s = RandomState::new();
        let mut hasher = s.build_hasher();
        hasher.write_u64(Self::now());
        format!("{:x}", hasher.finish())[..8].to_string()
    }
}
