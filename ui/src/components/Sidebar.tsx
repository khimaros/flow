import { useState, useRef, useEffect, useCallback, useMemo } from "react";
import {
  Database,
  FileText,
  MoreVertical,
  Trash2,
  Edit2,
  ChevronDown,
  ChevronRight,
  Search,
} from "lucide-react";
import { ExecutionQueue } from "./ExecutionQueue";
import { useClampedMenuPosition } from "../hooks/useClampedMenuPosition";
import type { Job } from "../hooks/useExecutionQueue";
import type { NodeMetadata } from "../types";

export const Sidebar = ({
  workflows,
  onLoad,
  currentWorkflow,
  onDeleteWorkflow,
  onRenameWorkflow,
  nodeTypes = [],
  jobs = [],
  onClearCompletedJobs,
  onCancelJob,
  onContextMenuOpen,
  closeMenuSignal,
}: {
  workflows: string[];
  onLoad: (name: string) => void;
  currentWorkflow: string | null;
  onDeleteWorkflow?: (name: string) => void;
  onRenameWorkflow?: (oldName: string, newName: string) => void;
  onAddNode?: (type: string) => void;
  nodeTypes?: NodeMetadata[];
  jobs?: Job[];
  onClearCompletedJobs?: () => void;
  onCancelJob?: (jobId: string) => void;
  onContextMenuOpen?: () => void;
  closeMenuSignal?: number;
}) => {
  const [activeTab, setActiveTab] = useState<"nodes" | "workflows" | "queue">(
    "workflows",
  );
  const [contextMenu, setContextMenu] = useState<{
    workflow: string;
    x: number;
    y: number;
  } | null>(null);
  const [renameDialog, setRenameDialog] = useState<{
    workflow: string;
    newName: string;
  } | null>(null);
  const [collapsedCategories, setCollapsedCategories] = useState<Set<string>>(
    new Set(),
  );
  const [nodeSearch, setNodeSearch] = useState("");

  const filteredNodes = useMemo(() => {
    if (!nodeSearch.trim()) return nodeTypes;
    const query = nodeSearch.toLowerCase();
    return nodeTypes.filter(
      (node) =>
        node.title.toLowerCase().includes(query) ||
        node.description.toLowerCase().includes(query) ||
        node.name.toLowerCase().includes(query),
    );
  }, [nodeTypes, nodeSearch]);

  // group nodes by category
  const nodesByCategory = useMemo(() => {
    const groups: Record<string, NodeMetadata[]> = {};
    filteredNodes.forEach((node) => {
      const category = node.category || "General";
      if (!groups[category]) {
        groups[category] = [];
      }
      groups[category].push(node);
    });
    return groups;
  }, [filteredNodes]);

  const categories = useMemo(
    () => Object.keys(nodesByCategory).sort(),
    [nodesByCategory],
  );

  const toggleCategory = (category: string) => {
    setCollapsedCategories((prev) => {
      const next = new Set(prev);
      if (next.has(category)) {
        next.delete(category);
      } else {
        next.add(category);
      }
      return next;
    });
  };

  // sidebar resizing state
  const [width, setWidth] = useState(() => {
    const saved = localStorage.getItem("flow_sidebar_width");
    return saved ? parseInt(saved, 10) : 280;
  });
  const [isResizing, setIsResizing] = useState(false);
  const sidebarRef = useRef<HTMLDivElement>(null);

  const { ref: contextMenuRef, pos: contextMenuPos } = useClampedMenuPosition(
    contextMenu?.x ?? 0,
    contextMenu?.y ?? 0,
  );
  const prevCloseMenuSignalRef = useRef(closeMenuSignal);

  const startResizing = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    setIsResizing(true);
  }, []);

  useEffect(() => {
    if (!isResizing) return;

    const handleMouseMove = (e: MouseEvent) => {
      // min width 240px to accommodate tabs, max 800px
      const newWidth = Math.max(240, Math.min(e.clientX, 800));
      setWidth(newWidth);
    };

    const handleMouseUp = () => {
      setIsResizing(false);
    };

    document.addEventListener("mousemove", handleMouseMove);
    document.addEventListener("mouseup", handleMouseUp);

    return () => {
      document.removeEventListener("mousemove", handleMouseMove);
      document.removeEventListener("mouseup", handleMouseUp);
    };
  }, [isResizing]);

  // save width to localStorage when it changes
  useEffect(() => {
    localStorage.setItem("flow_sidebar_width", width.toString());
  }, [width]);

  const onDragStart = (event: React.DragEvent, nodeType: string) => {
    event.dataTransfer.setData("application/reactflow", nodeType);
    event.dataTransfer.effectAllowed = "move";
  };

  const runningJobs = jobs.filter((j) => j.status === "running");
  const queuedJobs = jobs.filter((j) => j.status === "queued");
  const hasActiveJobs = runningJobs.length > 0 || queuedJobs.length > 0;

  // close context menu when clicking outside
  useEffect(() => {
    const handleClickOutside = (e: MouseEvent) => {
      if (
        contextMenuRef.current &&
        !contextMenuRef.current.contains(e.target as Node)
      ) {
        setContextMenu(null);
      }
    };
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [contextMenuRef]);

  // close context menu when parent signals (e.g., when App opens a different menu)
  useEffect(() => {
    // skip initial render (when ref value matches the prop)
    if (prevCloseMenuSignalRef.current === closeMenuSignal) {
      return;
    }
    prevCloseMenuSignalRef.current = closeMenuSignal;
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setContextMenu(null);
  }, [closeMenuSignal]);

  const handleContextMenu = (e: React.MouseEvent, workflow: string) => {
    e.preventDefault();
    e.stopPropagation();
    onContextMenuOpen?.();
    setContextMenu({ workflow, x: e.clientX, y: e.clientY });
  };

  const handleMoreClick = (e: React.MouseEvent, workflow: string) => {
    e.stopPropagation();
    onContextMenuOpen?.();
    const rect = (e.target as HTMLElement).getBoundingClientRect();
    setContextMenu({ workflow, x: rect.right, y: rect.top });
  };

  const handleDelete = () => {
    if (contextMenu && onDeleteWorkflow) {
      if (
        confirm(
          `Delete workflow "${contextMenu.workflow.replace(".json", "")}"?`,
        )
      ) {
        onDeleteWorkflow(contextMenu.workflow);
      }
    }
    setContextMenu(null);
  };

  const handleRenameStart = () => {
    if (contextMenu) {
      setRenameDialog({
        workflow: contextMenu.workflow,
        newName: contextMenu.workflow.replace(".json", ""),
      });
    }
    setContextMenu(null);
  };

  const handleRenameSubmit = () => {
    if (renameDialog && onRenameWorkflow && renameDialog.newName.trim()) {
      const newName = renameDialog.newName.trim();
      if (newName !== renameDialog.workflow.replace(".json", "")) {
        onRenameWorkflow(renameDialog.workflow, newName);
      }
    }
    setRenameDialog(null);
  };

  return (
    <div
      ref={sidebarRef}
      style={{
        width: `${width}px`,
        position: "absolute",
        left: 0,
        top: 0,
        bottom: 0,
        borderRight: "1px solid var(--panel-border)",
        background: "var(--panel-bg)",
        display: "flex",
        flexDirection: "column",
        zIndex: 10,
      }}
    >
      {/* Resize Handle */}
      <div
        onMouseDown={startResizing}
        style={{
          position: "absolute",
          top: 0,
          right: -5,
          width: "10px",
          height: "100%",
          cursor: "col-resize",
          zIndex: 100,
          background: "transparent",
        }}
      />

      <div
        style={{
          display: "flex",
          borderBottom: "1px solid var(--panel-border)",
        }}
      >
        <div
          className={`sidebar-tab ${activeTab === "workflows" ? "active" : ""}`}
          onClick={() => setActiveTab("workflows")}
          title="Workflows"
        >
          Workflows
        </div>
        <div
          className={`sidebar-tab ${activeTab === "nodes" ? "active" : ""}`}
          onClick={() => setActiveTab("nodes")}
          title="Nodes"
        >
          Nodes
        </div>
        <div
          className={`sidebar-tab ${activeTab === "queue" ? "active" : ""}`}
          onClick={() => setActiveTab("queue")}
          title="Queue"
          style={{ position: "relative" }}
        >
          Queue
          {hasActiveJobs && (
            <span
              style={{
                position: "absolute",
                top: "6px",
                right: "6px",
                width: "8px",
                height: "8px",
                borderRadius: "50%",
                background: "var(--primary-color)",
                animation: "pulse 1.5s infinite",
              }}
            />
          )}
        </div>
      </div>

      {activeTab === "nodes" && (
        <div
          style={{
            padding: "15px",
            display: "flex",
            flexDirection: "column",
            gap: "8px",
            overflow: "auto",
            flex: 1,
          }}
        >
          <div style={{ position: "relative" }}>
            <Search
              size={14}
              style={{
                position: "absolute",
                left: "10px",
                top: "50%",
                transform: "translateY(-50%)",
                opacity: 0.5,
                pointerEvents: "none",
              }}
            />
            <input
              type="text"
              placeholder="search nodes..."
              value={nodeSearch}
              onChange={(e) => setNodeSearch(e.target.value)}
              style={{
                width: "100%",
                padding: "8px 10px 8px 32px",
                fontSize: "13px",
                border: "1px solid var(--node-border)",
                borderRadius: "6px",
                background: "var(--node-bg)",
                color: "var(--text-color)",
                outline: "none",
                boxSizing: "border-box",
              }}
            />
          </div>
          {nodeTypes.length > 0 ? (
            categories.map((category) => (
              <div key={category}>
                <div
                  onClick={() => toggleCategory(category)}
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: "8px",
                    padding: "8px",
                    cursor: "pointer",
                    fontSize: "12px",
                    fontWeight: 600,
                    color: "var(--text-color)",
                    opacity: 0.8,
                    userSelect: "none",
                  }}
                >
                  {collapsedCategories.has(category) ? (
                    <ChevronRight size={14} />
                  ) : (
                    <ChevronDown size={14} />
                  )}
                  {category}
                </div>
                {!collapsedCategories.has(category) && (
                  <div
                    style={{
                      display: "flex",
                      flexDirection: "column",
                      gap: "8px",
                      paddingLeft: "8px",
                    }}
                  >
                    {nodesByCategory[category].map((node) => (
                      <div
                        key={node.name}
                        className="sidebar-node-item"
                        data-node-type={node.name}
                        draggable
                        onDragStart={(e) => onDragStart(e, node.name)}
                        style={{
                          display: "flex",
                          alignItems: "center",
                          gap: "10px",
                          padding: "12px",
                          background: "var(--node-bg)",
                          border: "1px solid var(--node-border)",
                          borderRadius: "6px",
                          cursor: "grab",
                          boxShadow: "0 1px 2px rgba(0,0,0,0.05)",
                        }}
                        title={node.description}
                      >
                        <Database size={16} style={{ flexShrink: 0 }} />
                        <div style={{ minWidth: 0, flex: 1 }}>
                          <div
                            style={{
                              fontWeight: 500,
                              fontSize: "13px",
                              whiteSpace: "nowrap",
                              overflow: "hidden",
                              textOverflow: "ellipsis",
                            }}
                          >
                            {node.title}
                          </div>
                          <div
                            style={{
                              fontSize: "11px",
                              opacity: 0.7,
                              whiteSpace: "nowrap",
                              overflow: "hidden",
                              textOverflow: "ellipsis",
                            }}
                          >
                            {node.description}
                          </div>
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            ))
          ) : (
            <div
              style={{
                padding: "20px",
                textAlign: "center",
                opacity: 0.5,
                fontSize: "13px",
              }}
            >
              {nodeTypes.length > 0 ? "no matching nodes" : "Loading nodes..."}
            </div>
          )}
        </div>
      )}

      {activeTab === "workflows" && (
        <div
          style={{
            flex: 1,
            display: "flex",
            flexDirection: "column",
            minHeight: 0,
          }}
        >
          <div style={{ flex: 1, overflow: "auto", minHeight: 0 }}>
            {workflows.map((name) => (
              <div
                key={name}
                onClick={() => onLoad(name)}
                onContextMenu={(e) => handleContextMenu(e, name)}
                style={{
                  padding: "10px 15px",
                  cursor: "pointer",
                  background:
                    currentWorkflow === name
                      ? "rgba(59, 130, 246, 0.1)"
                      : "transparent",
                  display: "flex",
                  alignItems: "center",
                  gap: "8px",
                  fontSize: "13px",
                  borderLeft:
                    currentWorkflow === name
                      ? "2px solid var(--primary-color)"
                      : "2px solid transparent",
                }}
              >
                <FileText size={14} style={{ flexShrink: 0 }} />
                <span
                  style={{
                    flex: 1,
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                  }}
                >
                  {name.replace(".json", "")}
                </span>
                <div
                  onClick={(e) => handleMoreClick(e, name)}
                  style={{
                    padding: "2px",
                    borderRadius: "4px",
                    display: "flex",
                    alignItems: "center",
                    opacity: 0.5,
                  }}
                  className="workflow-more-btn"
                >
                  <MoreVertical size={14} style={{ flexShrink: 0 }} />
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      {activeTab === "queue" && (
        <div style={{ flex: 1, overflow: "auto" }}>
          <ExecutionQueue
            jobs={jobs}
            onClearCompleted={onClearCompletedJobs || (() => {})}
            onCancelJob={onCancelJob || (() => {})}
          />
        </div>
      )}

      {/* Context Menu */}
      {contextMenu && (
        <div
          ref={contextMenuRef}
          style={{
            position: "fixed",
            left: contextMenuPos.x,
            top: contextMenuPos.y,
            background: "var(--panel-bg)",
            border: "1px solid var(--panel-border)",
            borderRadius: "6px",
            boxShadow: "0 4px 12px rgba(0,0,0,0.15)",
            zIndex: 1000,
            minWidth: "140px",
            overflow: "hidden",
          }}
        >
          <div
            onClick={handleRenameStart}
            style={{
              padding: "8px 12px",
              cursor: "pointer",
              display: "flex",
              alignItems: "center",
              gap: "8px",
              fontSize: "13px",
              color: "var(--text-color)",
            }}
            className="context-menu-item"
          >
            <Edit2 size={14} /> Rename
          </div>
          <div
            onClick={handleDelete}
            style={{
              padding: "8px 12px",
              cursor: "pointer",
              display: "flex",
              alignItems: "center",
              gap: "8px",
              fontSize: "13px",
              color: "var(--danger-color)",
            }}
            className="context-menu-item"
          >
            <Trash2 size={14} /> Delete
          </div>
        </div>
      )}

      {/* Rename Dialog */}
      {renameDialog && (
        <div
          style={{
            position: "fixed",
            inset: 0,
            background: "rgba(0,0,0,0.5)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            zIndex: 1001,
          }}
        >
          <div
            style={{
              background: "var(--panel-bg)",
              border: "1px solid var(--panel-border)",
              borderRadius: "8px",
              padding: "20px",
              minWidth: "300px",
              boxShadow: "0 8px 24px rgba(0,0,0,0.2)",
            }}
          >
            <div style={{ marginBottom: "12px", fontWeight: 600 }}>
              Rename Workflow
            </div>
            <input
              type="text"
              value={renameDialog.newName}
              onChange={(e) =>
                setRenameDialog({ ...renameDialog, newName: e.target.value })
              }
              onKeyDown={(e) => {
                if (e.key === "Enter") handleRenameSubmit();
                if (e.key === "Escape") setRenameDialog(null);
              }}
              autoFocus
              style={{
                width: "100%",
                padding: "8px 12px",
                borderRadius: "4px",
                border: "1px solid var(--panel-border)",
                background: "var(--node-bg)",
                color: "var(--text-color)",
                fontSize: "14px",
                boxSizing: "border-box",
              }}
            />
            <div
              style={{
                marginTop: "16px",
                display: "flex",
                gap: "8px",
                justifyContent: "flex-end",
              }}
            >
              <button
                onClick={() => setRenameDialog(null)}
                style={{
                  padding: "6px 12px",
                  borderRadius: "4px",
                  border: "1px solid var(--panel-border)",
                  background: "transparent",
                  color: "var(--text-color)",
                  cursor: "pointer",
                }}
              >
                Cancel
              </button>
              <button
                onClick={handleRenameSubmit}
                style={{
                  padding: "6px 12px",
                  borderRadius: "4px",
                  border: "none",
                  background: "var(--primary-color)",
                  color: "white",
                  cursor: "pointer",
                }}
              >
                Rename
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
