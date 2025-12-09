import {
  Play,
  CheckCircle,
  XCircle,
  Clock,
  Loader2,
  Trash2,
  Ban,
  X,
} from "lucide-react";
import type { Job } from "../hooks/useExecutionQueue";

interface ExecutionQueueProps {
  jobs: Job[];
  onClearCompleted: () => void;
  onCancelJob: (jobId: string) => void;
}

const formatDuration = (startMs: number, endMs?: number) => {
  const duration = (endMs || Date.now()) - startMs;
  if (duration < 1000) return `${duration}ms`;
  if (duration < 60000) return `${(duration / 1000).toFixed(1)}s`;
  return `${Math.floor(duration / 60000)}m ${Math.floor((duration % 60000) / 1000)}s`;
};

const formatTime = (timestamp: number) => {
  return new Date(timestamp).toLocaleTimeString();
};

const JobStatusIcon = ({ status }: { status: Job["status"] }) => {
  switch (status) {
    case "queued":
      return (
        <Clock size={14} style={{ color: "var(--text-color)", opacity: 0.5 }} />
      );
    case "running":
      return (
        <Loader2
          size={14}
          className="spin"
          style={{ color: "var(--primary-color)" }}
        />
      );
    case "completed":
      return <CheckCircle size={14} style={{ color: "#22c55e" }} />;
    case "error":
      return <XCircle size={14} style={{ color: "var(--danger-color)" }} />;
    case "cancelled":
      return (
        <Ban size={14} style={{ color: "var(--text-color)", opacity: 0.5 }} />
      );
  }
};

const ProgressBar = ({
  progress,
  total,
}: {
  progress: number;
  total: number;
}) => {
  const percentage = total > 0 ? (progress / total) * 100 : 0;
  return (
    <div className="progress-bar-container">
      <div className="progress-bar-fill" style={{ width: `${percentage}%` }} />
    </div>
  );
};

const JobItem = ({
  job,
  onCancel,
}: {
  job: Job;
  onCancel?: (jobId: string) => void;
}) => {
  const isActive = job.status === "running";
  const canCancel = job.status === "running" || job.status === "queued";
  const progressPercent =
    job.total_nodes > 0
      ? Math.round((job.completed_nodes / job.total_nodes) * 100)
      : 0;

  return (
    <div className={`queue-item ${job.status}`}>
      <div className="queue-item-header">
        <JobStatusIcon status={job.status} />
        <span className="queue-item-name">
          {job.workflow_name || "Untitled Workflow"}
          {job.force_run && <span className="queue-item-badge">force</span>}
        </span>
        {canCancel && onCancel && (
          <button
            onClick={() => onCancel(job.id)}
            className="queue-cancel-btn"
            title="Cancel job"
          >
            <X size={12} />
          </button>
        )}
        <span className="queue-item-time">
          {job.started_at && formatDuration(job.started_at, job.completed_at)}
        </span>
      </div>

      {isActive && (
        <div className="queue-item-progress">
          <ProgressBar progress={job.completed_nodes} total={job.total_nodes} />
          <div className="queue-item-progress-text">
            <span>
              {job.completed_nodes}/{job.total_nodes} nodes
            </span>
            <span>{progressPercent}%</span>
          </div>
          {job.current_node && (
            <div className="queue-item-current-node">
              <Play size={10} /> {job.current_node}
            </div>
          )}
        </div>
      )}

      {job.status === "error" && job.error && (
        <div className="queue-item-error">{job.error}</div>
      )}

      <div className="queue-item-footer">
        <span className="queue-item-id">{job.id.slice(0, 20)}...</span>
        <span>{formatTime(job.created_at)}</span>
      </div>
    </div>
  );
};

export const ExecutionQueue = ({
  jobs,
  onClearCompleted,
  onCancelJob,
}: ExecutionQueueProps) => {
  const runningJobs = jobs.filter((j) => j.status === "running");
  const queuedJobs = jobs.filter((j) => j.status === "queued");
  const completedJobs = jobs.filter(
    (j) =>
      j.status === "completed" ||
      j.status === "error" ||
      j.status === "cancelled",
  );

  const activeJob = runningJobs[0];
  const overallProgress = activeJob
    ? Math.round((activeJob.completed_nodes / activeJob.total_nodes) * 100)
    : 0;

  return (
    <div className="execution-queue">
      {/* Active Job Progress */}
      {activeJob && (
        <div className="queue-active-job">
          <div className="queue-active-header">
            <Loader2
              size={14}
              className="spin"
              style={{ color: "var(--primary-color)" }}
            />
            <span>Running</span>
            <span className="queue-active-percent">{overallProgress}%</span>
            <button
              onClick={() => onCancelJob(activeJob.id)}
              className="queue-cancel-btn"
              title="Cancel job"
            >
              <X size={14} />
            </button>
          </div>
          <ProgressBar
            progress={activeJob.completed_nodes}
            total={activeJob.total_nodes}
          />
          <div className="queue-active-details">
            <span>{activeJob.workflow_name || "Untitled"}</span>
            <span>
              {activeJob.completed_nodes}/{activeJob.total_nodes} nodes
            </span>
          </div>
          {activeJob.current_node && (
            <div className="queue-active-node">
              <Play size={10} /> {activeJob.current_node}
            </div>
          )}
        </div>
      )}

      {/* Job Lists */}
      <div className="queue-sections">
        {/* Running */}
        {runningJobs.length > 0 && (
          <div className="queue-section">
            <div className="queue-section-header">
              <span>Running ({runningJobs.length})</span>
            </div>
            {runningJobs.map((job) => (
              <JobItem key={job.id} job={job} onCancel={onCancelJob} />
            ))}
          </div>
        )}

        {/* Queued */}
        {queuedJobs.length > 0 && (
          <div className="queue-section">
            <div className="queue-section-header">
              <span>Queued ({queuedJobs.length})</span>
            </div>
            {queuedJobs.map((job) => (
              <JobItem key={job.id} job={job} onCancel={onCancelJob} />
            ))}
          </div>
        )}

        {/* Completed */}
        {completedJobs.length > 0 && (
          <div className="queue-section">
            <div className="queue-section-header">
              <span>Completed ({completedJobs.length})</span>
              <button
                onClick={onClearCompleted}
                className="queue-clear-btn"
                title="Clear completed jobs"
              >
                <Trash2 size={12} />
              </button>
            </div>
            {completedJobs.slice(0, 10).map((job) => (
              <JobItem key={job.id} job={job} />
            ))}
            {completedJobs.length > 10 && (
              <div className="queue-more">
                +{completedJobs.length - 10} more
              </div>
            )}
          </div>
        )}

        {jobs.length === 0 && (
          <div className="queue-empty">No jobs in queue</div>
        )}
      </div>
    </div>
  );
};
