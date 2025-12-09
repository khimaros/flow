import { useState, useCallback, useEffect, useRef } from "react";

export interface NodeProgress {
  node_id: string;
  progress: number;
  message?: string;
}

export interface Job {
  id: string;
  workflow_name: string | null;
  status: "queued" | "running" | "completed" | "error" | "cancelled";
  created_at: number;
  started_at?: number;
  completed_at?: number;
  total_nodes: number;
  completed_nodes: number;
  active_nodes: string[];
  current_node?: string;
  node_progress: Record<string, NodeProgress>;
  error?: string;
  force_run: boolean;
}

export type JobEvent =
  | { type: "JobCreated"; data: { job: Job } }
  | { type: "JobStarted"; data: { job_id: string } }
  | { type: "NodeStarted"; data: { job_id: string; node_id: string } }
  | {
      type: "NodeProgress";
      data: {
        job_id: string;
        node_id: string;
        progress: number;
        message?: string;
      };
    }
  | {
      type: "NodeFinished";
      data: {
        job_id: string;
        node_id: string;
        result: unknown;
        cached: boolean;
      };
    }
  | {
      type: "NodePartialOutput";
      data: {
        job_id: string;
        node_id: string;
        output_name: string;
        delta: unknown;
        accumulated: unknown;
      };
    }
  | {
      type: "NodeError";
      data: { job_id: string; node_id: string; error: string };
    }
  | { type: "JobCompleted"; data: { job_id: string } }
  | { type: "JobError"; data: { job_id: string; error: string } }
  | { type: "JobCancelled"; data: { job_id: string } };

export interface NodeEventCallback {
  onNodeStarted?: (nodeId: string) => void;
  onNodeProgress?: (nodeId: string, progress: number, message?: string) => void;
  onNodePartialOutput?: (
    nodeId: string,
    outputName: string,
    delta: unknown,
    accumulated: unknown,
  ) => void;
  onNodeFinished?: (nodeId: string, result: unknown, cached: boolean) => void;
  onNodeError?: (nodeId: string, error: string) => void;
  onJobStarted?: () => void;
  onJobCompleted?: (jobId: string, workflowName: string | null) => void;
}

export const useExecutionQueue = (callbacks?: NodeEventCallback) => {
  const [jobs, setJobs] = useState<Job[]>([]);
  const [connected, setConnected] = useState(false);
  const eventSourceRef = useRef<EventSource | null>(null);
  const callbacksRef = useRef(callbacks);
  const jobsRef = useRef(jobs);

  // keep refs up to date
  useEffect(() => {
    callbacksRef.current = callbacks;
  }, [callbacks]);

  useEffect(() => {
    jobsRef.current = jobs;
  }, [jobs]);

  const handleJobEvent = useCallback((event: JobEvent) => {
    const cbs = callbacksRef.current;

    console.log("[Queue] Received event:", event.type, event.data);

    switch (event.type) {
      case "JobCreated":
        setJobs((prev) => {
          // check if job already exists (for reconnection scenarios)
          const exists = prev.some((j) => j.id === event.data.job.id);
          if (exists) {
            return prev.map((j) =>
              j.id === event.data.job.id ? event.data.job : j,
            );
          }
          return [event.data.job, ...prev];
        });
        break;

      case "JobStarted":
        setJobs((prev) =>
          prev.map((job) =>
            job.id === event.data.job_id
              ? { ...job, status: "running" as const, started_at: Date.now() }
              : job,
          ),
        );
        cbs?.onJobStarted?.();
        break;

      case "NodeStarted":
        setJobs((prev) =>
          prev.map((job) =>
            job.id === event.data.job_id
              ? {
                  ...job,
                  current_node: event.data.node_id,
                  active_nodes: [
                    ...job.active_nodes.filter(
                      (id) => id !== event.data.node_id,
                    ),
                    event.data.node_id,
                  ],
                  node_progress: {
                    ...job.node_progress,
                    [event.data.node_id]: {
                      node_id: event.data.node_id,
                      progress: 0,
                    },
                  },
                }
              : job,
          ),
        );
        // call the callback to update the node on canvas
        console.log(
          "[Queue] Calling onNodeStarted callback for:",
          event.data.node_id,
        );
        cbs?.onNodeStarted?.(event.data.node_id);
        break;

      case "NodeProgress":
        setJobs((prev) =>
          prev.map((job) =>
            job.id === event.data.job_id
              ? {
                  ...job,
                  node_progress: {
                    ...job.node_progress,
                    [event.data.node_id]: {
                      node_id: event.data.node_id,
                      progress: event.data.progress,
                      message: event.data.message,
                    },
                  },
                }
              : job,
          ),
        );
        cbs?.onNodeProgress?.(
          event.data.node_id,
          event.data.progress,
          event.data.message,
        );
        break;

      case "NodePartialOutput":
        cbs?.onNodePartialOutput?.(
          event.data.node_id,
          event.data.output_name,
          event.data.delta,
          event.data.accumulated,
        );
        break;

      case "NodeFinished":
        setJobs((prev) =>
          prev.map((job) => {
            if (job.id !== event.data.job_id) return job;
            const newActiveNodes = job.active_nodes.filter(
              (id) => id !== event.data.node_id,
            );
            return {
              ...job,
              completed_nodes: job.completed_nodes + 1,
              active_nodes: newActiveNodes,
              current_node:
                newActiveNodes.length > 0 ? newActiveNodes[0] : undefined,
              node_progress: {
                ...job.node_progress,
                [event.data.node_id]: {
                  node_id: event.data.node_id,
                  progress: 100,
                },
              },
            };
          }),
        );
        // call the callback to update the node on canvas with results
        console.log(
          "[Queue] Calling onNodeFinished callback for:",
          event.data.node_id,
          "cached:",
          event.data.cached,
        );
        cbs?.onNodeFinished?.(
          event.data.node_id,
          event.data.result,
          event.data.cached,
        );
        break;

      case "NodeError":
        cbs?.onNodeError?.(event.data.node_id, event.data.error);
        break;

      case "JobCompleted": {
        setJobs((prev) =>
          prev.map((job) =>
            job.id === event.data.job_id
              ? {
                  ...job,
                  status: "completed" as const,
                  completed_at: Date.now(),
                  completed_nodes: job.total_nodes,
                  active_nodes: [],
                  current_node: undefined,
                }
              : job,
          ),
        );
        // find the job to get its workflow name
        const completedJob = jobsRef.current.find(
          (j) => j.id === event.data.job_id,
        );
        cbs?.onJobCompleted?.(
          event.data.job_id,
          completedJob?.workflow_name || null,
        );
        break;
      }

      case "JobError":
        setJobs((prev) =>
          prev.map((job) =>
            job.id === event.data.job_id
              ? {
                  ...job,
                  status: "error" as const,
                  completed_at: Date.now(),
                  error: event.data.error,
                  active_nodes: [],
                  current_node: undefined,
                }
              : job,
          ),
        );
        break;

      case "JobCancelled":
        setJobs((prev) =>
          prev.map((job) =>
            job.id === event.data.job_id
              ? {
                  ...job,
                  status: "cancelled" as const,
                  completed_at: Date.now(),
                  active_nodes: [],
                  current_node: undefined,
                }
              : job,
          ),
        );
        break;
    }
  }, []);

  // connect to the server-side job stream
  useEffect(() => {
    const connect = () => {
      const eventSource = new EventSource("/api/queue/stream");
      eventSourceRef.current = eventSource;

      eventSource.onopen = () => {
        setConnected(true);
      };

      eventSource.onmessage = (event) => {
        const jobEvent: JobEvent = JSON.parse(event.data);
        handleJobEvent(jobEvent);
      };

      eventSource.onerror = () => {
        setConnected(false);
        eventSource.close();
        // reconnect after 2 seconds
        setTimeout(connect, 2000);
      };
    };

    connect();

    return () => {
      if (eventSourceRef.current) {
        eventSourceRef.current.close();
      }
    };
  }, [handleJobEvent]);

  const cancelJob = useCallback(async (jobId: string) => {
    try {
      const response = await fetch(`/api/queue/${jobId}/cancel`, {
        method: "POST",
      });
      return response.ok;
    } catch (e) {
      console.error("Failed to cancel job:", e);
      return false;
    }
  }, []);

  const clearCompletedJobs = useCallback(async () => {
    try {
      await fetch("/api/queue", { method: "DELETE" });
      setJobs((prev) =>
        prev.filter((j) => j.status === "queued" || j.status === "running"),
      );
    } catch (e) {
      console.error("Failed to clear completed jobs:", e);
    }
  }, []);

  const getActiveJob = useCallback((): Job | undefined => {
    return jobs.find((job) => job.status === "running");
  }, [jobs]);

  const getQueuedJobs = useCallback((): Job[] => {
    return jobs.filter((job) => job.status === "queued");
  }, [jobs]);

  const getCompletedJobs = useCallback((): Job[] => {
    return jobs.filter(
      (job) => job.status === "completed" || job.status === "error",
    );
  }, [jobs]);

  return {
    jobs,
    connected,
    cancelJob,
    clearCompletedJobs,
    getActiveJob,
    getQueuedJobs,
    getCompletedJobs,
  };
};
