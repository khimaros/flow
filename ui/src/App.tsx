import { useState, useCallback, useRef, useEffect, useMemo } from "react";
import { v4 as uuidv4 } from "uuid";
import ReactFlow, {
  type Node,
  type Edge,
  type NodeChange,
  type NodeDimensionChange,
  addEdge,
  Background,
  Controls,
  type Connection,
  useNodesState,
  useEdgesState,
  reconnectEdge,
  ControlButton,
  useStore,
  ReactFlowProvider,
  type ReactFlowInstance,
  type OnConnectStartParams,
  type NodeTypes,
  useUpdateNodeInternals,
} from "reactflow";
import "reactflow/dist/style.css";
import {
  Play,
  Moon,
  Sun,
  Monitor,
  Save,
  FilePlus,
  ChevronDown,
  PanelLeftClose,
  PanelLeft,
  Eye,
  EyeOff,
  Layers,
  Wifi,
  WifiOff,
  Keyboard,
  X,
} from "lucide-react";

import { useTheme } from "./hooks/useTheme";
import { useExecution } from "./hooks/useExecution";
import { useExecutionQueue } from "./hooks/useExecutionQueue";
import { useKeyboardShortcuts } from "./hooks/useKeyboardShortcuts";
import { useToast } from "./hooks/useToast";
import { ToastProvider } from "./context/ToastContext";
import { GenericNode } from "./components/Nodes";
import { Sidebar } from "./components/Sidebar";
import { ContextMenu, NodeSelector } from "./components/Overlays";
import { calculateNodeMinHeight } from "./utils/nodeUtils";
import {
  generateWorkflow,
  generateState,
  resolveOutputs,
  type WorkflowState,
} from "./utils/workflowUtils";
import type { NodeMetadata, WorkflowNode, WorkflowEdge } from "./types";

// helper to create a ReactFlow node from workflow data
interface CreateNodeParams {
  nodeData: WorkflowNode;
  meta: NodeMetadata | undefined;
  gridSize: number;
  updateNodeData: (nodeId: string, field: string, value: unknown) => void;
  deleteNode: (id: string) => void;
  submitJob: (nodeId?: string, force?: boolean) => void;
  selectNode: (nodeId: string) => void;
  toggleSourceView: (nodeId: string) => void;
  workflowName: string | null;
  getNodes: () => Node[];
  getEdges: () => Edge[];
}

const createReactFlowNode = (params: CreateNodeParams): Node => {
  const {
    nodeData: n,
    meta,
    gridSize,
    updateNodeData,
    deleteNode,
    submitJob,
    selectNode,
    toggleSourceView,
    workflowName,
    getNodes,
    getEdges,
  } = params;

  const minHeight =
    Math.ceil(calculateNodeMinHeight(meta) / gridSize) * gridSize;
  const width = n.size?.width || 300;
  const height = n.size?.height || minHeight;

  return {
    id: n.id,
    type: n.type,
    position: n.position || { x: 0, y: 0 },
    style: {
      width: Math.max(300, Math.round(width / gridSize) * gridSize),
      height: Math.max(minHeight, Math.round(height / gridSize) * gridSize),
    },
    data: {
      ...n.inputs,
      skip_cache: n.skipCache,
      isBypassed: n.bypassed,
      metadata: meta,
      onChange: (f: string, v: unknown) => updateNodeData(n.id, f, v),
      onDelete: deleteNode,
      onRun: (nodeId: string, force?: boolean) =>
        submitJob(nodeId, force ?? false),
      onSelect: selectNode,
      onToggleSource: toggleSourceView,
      workflowName,
      getNodes,
      getEdges,
    },
  };
};

function AppInner() {
  const [showRunMenu, setShowRunMenu] = useState(false);
  const [runMode, setRunMode] = useState<"default" | "force">("default");
  const [reactFlowInstance, setReactFlowInstance] =
    useState<ReactFlowInstance | null>(null);
  const [edgeMode, setEdgeMode] = useState<"hidden" | "behind" | "above">(
    "above",
  );
  const reactFlowWrapper = useRef<HTMLDivElement>(null);
  const isInteractive = useStore((state) => state.nodesDraggable);
  const { theme, setTheme, resolvedTheme } = useTheme();
  const { addToast } = useToast();

  const [contextMenu, setContextMenu] = useState<{
    x: number;
    y: number;
    options: { label: string; onClick: () => void }[];
  } | null>(null);
  const [nodeSelector, setNodeSelector] = useState<{
    x: number;
    y: number;
    params: {
      position: { x: number; y: number };
    } & Partial<OnConnectStartParams>;
    filterType?: { type: string; direction: "input" | "output" };
  } | null>(null);
  const [showShortcuts, setShowShortcuts] = useState(false);

  // signal to close sidebar context menu when App opens its own menus
  const [sidebarCloseMenuSignal, setSidebarCloseMenuSignal] = useState(0);

  const sidebarVisibleState = useState(true);
  const [sidebarVisible, setSidebarVisible] = sidebarVisibleState;
  const connectionStartRef = useRef<OnConnectStartParams | null>(null);
  const connectionMadeRef = useRef(false);

  // workflow State

  const [workflows, setWorkflows] = useState<string[]>([]);
  const [currentWorkflow, setCurrentWorkflow] = useState<string | null>(() => {
    // restore from autosave, or generate a temp name for unsaved workflows
    const saved = localStorage.getItem("flow_autosave");
    if (saved) {
      try {
        const { currentWorkflow: savedWorkflow } = JSON.parse(saved);
        if (savedWorkflow) return savedWorkflow;
      } catch {
        // fall through
      }
    }
    const newName = `.temp_${uuidv4()}`;
    return newName;
  });

  // node Metadata State
  const [nodeDefs, setNodeDefs] = useState<NodeMetadata[]>([]);

  const [nodes, setNodes, onNodesChangeBase] = useNodesState([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState([]);

  // track unsaved changes
  const [isDirty, setIsDirty] = useState(false);

  const GRID_SIZE = 15;

  // track which nodes are currently being resized
  const resizingNodesRef = useRef<Set<string>>(new Set());

  // function definitions moved up to avoid "used before defined" errors
  const fetchNodeMetadata = async () => {
    try {
      const res = await fetch("/api/nodes");
      const data = await res.json();
      setNodeDefs(data);
    } catch (e) {
      console.error("Failed to load node metadata", e);
    }
  };

  const refreshWorkflows = useCallback(async () => {
    try {
      const res = await fetch("/api/workflows");
      const data = await res.json();
      setWorkflows(data);
    } catch (e) {
      console.error("Failed to load workflows", e);
    }
  }, []);

  const deleteNode = useCallback(
    (id: string) => {
      setNodes((nds) => nds.filter((n) => n.id !== id));
      setEdges((eds) => eds.filter((e) => e.source !== id && e.target !== id));
    },
    [setNodes, setEdges],
  );

  const updateNodeData = useCallback(
    (nodeId: string, field: string, value: unknown) => {
      setIsDirty(true);
      setNodes((nds) =>
        nds.map((node) => {
          if (node.id === nodeId) {
            return {
              ...node,
              data: {
                ...node.data,
                [field]: value,
              },
            };
          }
          return node;
        }),
      );
    },
    [setNodes],
  );

  const selectNode = useCallback(
    (nodeId: string) => {
      setNodes((nds) =>
        nds.map((node) => ({
          ...node,
          selected: node.id === nodeId,
        })),
      );
    },
    [setNodes],
  );

  const toggleSourceView = useCallback(
    (nodeId: string) => {
      setNodes((nds) =>
        nds.map((node) => {
          if (node.id === nodeId) {
            return {
              ...node,
              data: {
                ...node.data,
                showSource: !node.data.showSource,
              },
            };
          }
          return node;
        }),
      );
    },
    [setNodes],
  );

  const constPaneContextMenu = useCallback(() => setContextMenu(null), []);

  // called by Sidebar when it opens its context menu - close App's menus
  const handleSidebarContextMenuOpen = useCallback(() => {
    setContextMenu(null);
    setNodeSelector(null);
  }, []);

  const { submitJob: submitJobBase } = useExecution(
    nodes,
    edges,
    currentWorkflow,
  );

  // wrapper that shows a toast when job is queued
  const submitJob = useCallback(
    async (targetNodeId?: string, force: boolean = false) => {
      // reset node UI state before submitting (scoped to this client's submission
      // rather than onJobStarted, which fires for ALL jobs on the server and can
      // race with parallel test/user workflows). preserve isRunning on nodes
      // from a still-in-flight prior job so their blue border isn't cleared.
      setNodes((nds) =>
        nds.map((n) =>
          n.data.isRunning
            ? n
            : {
                ...n,
                data: {
                  ...n.data,
                  isRunning: false,
                  progress: 0,
                  progressMessage: undefined,
                  error: undefined,
                  isCached: false,
                },
              },
        ),
      );
      const result = await submitJobBase(targetNodeId, force);
      if (result) {
        const message = targetNodeId
          ? `Running node: ${targetNodeId}`
          : `Running workflow: ${currentWorkflow || "Untitled"}`;
        addToast("info", "Job Queued", message, "var(--primary-color)");
      }
      return result;
    },
    [submitJobBase, addToast, currentWorkflow, setNodes],
  );

  // refs for accessing current nodes/edges in callbacks
  const nodesRef = useRef(nodes);
  const edgesRef = useRef(edges);
  useEffect(() => {
    nodesRef.current = nodes;
    edgesRef.current = edges;
  }, [nodes, edges]);

  const getNodes = useCallback(() => nodesRef.current, []);
  const getEdges = useCallback(() => edgesRef.current, []);

  // helper to determine edge style based on source node output type
  const getEdgeStyle = useCallback(
    (sourceId: string, sourceHandle: string | null) => {
      if (!reactFlowInstance) return { strokeWidth: 2 };

      const sourceNode = reactFlowInstance.getNode(sourceId);
      if (!sourceNode) return { strokeWidth: 2 };

      const meta = nodeDefs.find((n) => n.name === sourceNode.type);
      if (!meta) return { strokeWidth: 2 };

      let type = "string"; // default
      if (sourceHandle) {
        const output = meta.outputs.find((o) => o.name === sourceHandle);
        if (output) type = output.type;
      } else if (meta.outputs.length > 0) {
        type = meta.outputs[0].type;
      }

      const typeLower = type.toLowerCase();

      return {
        stroke: `var(--type-${typeLower}, #94a3b8)`,
        strokeWidth: 2,
      };
    },
    [reactFlowInstance, nodeDefs],
  );

  const getHandleType = useCallback(
    (
      nodeId: string,
      handleId: string,
      handleType: "source" | "target",
    ): string | null => {
      const node = nodes.find((n) => n.id === nodeId);
      if (!node) return null;
      const meta = nodeDefs.find((d) => d.name === node.type);
      if (!meta) return null;

      if (handleType === "source") {
        const output = meta.outputs.find((o) => o.name === handleId);
        return output?.type || null;
      } else {
        const input = meta.inputs.find((i) => i.name === handleId);
        return input?.type || null;
      }
    },
    [nodes, nodeDefs],
  );

  const addNode = useCallback(
    (
      type: string,
      position?: { x: number; y: number },
      connectTo?: Partial<OnConnectStartParams>,
    ) => {
      const meta = nodeDefs.find((n) => n.name === type);
      if (!meta) {
        console.error("Unknown node type:", type);
        return;
      }

      const id = `${type.toLowerCase()}_${uuidv4().slice(0, 8)}`;

      const initialData: Record<string, unknown> = {
        onChange: (f: string, v: unknown) => updateNodeData(id, f, v),
        onDelete: deleteNode,
        onRun: (nodeId: string, force?: boolean) =>
          submitJob(nodeId, force ?? false),
        onSelect: selectNode,
        onToggleSource: toggleSourceView,
        isPinned: false,
        isBypassed: false,
        isCached: false,
        skip_cache: false,
        showSource: false,
        metadata: meta,
        workflowName: currentWorkflow,
        getNodes,
        getEdges,
      };

      const minHeight = calculateNodeMinHeight(meta);
      const initialHeight = Math.ceil(minHeight / GRID_SIZE) * GRID_SIZE;

      const rawPos = position || {
        x: Math.random() * 400 + 50,
        y: Math.random() * 400 + 50,
      };
      const snappedPos = {
        x: Math.round(rawPos.x / GRID_SIZE) * GRID_SIZE,
        y: Math.round(rawPos.y / GRID_SIZE) * GRID_SIZE,
      };

      const newNode: Node = {
        id,
        type,
        position: snappedPos,
        data: initialData,
        style: { width: 300, height: initialHeight },
      };

      setNodes((nds) => nds.concat(newNode));

      if (connectTo) {
        const { nodeId, handleId, handleType } = connectTo;
        setTimeout(() => {
          setEdges((eds) => {
            let source: string | null = null;
            let sourceHandle: string | null = null;
            let target: string | null = null;
            let targetHandle: string | null = null;

            if (handleType === "source") {
              source = nodeId ?? null;
              sourceHandle = handleId ?? null;
              target = id;

              const sourceType =
                nodeId && handleId
                  ? getHandleType(nodeId, handleId, "source")
                  : null;

              const matchingInput = sourceType
                ? meta.inputs.find((i) => {
                    const it = (i.type || "string").toLowerCase();
                    const st = sourceType.toLowerCase();
                    return it === "any" || st === "any" || it === st;
                  })
                : null;

              targetHandle = matchingInput
                ? matchingInput.name
                : meta.inputs.length > 0
                  ? meta.inputs[0].name
                  : null;
            } else {
              source = id;

              const targetType =
                nodeId && handleId
                  ? getHandleType(nodeId, handleId, "target")
                  : null;

              const matchingOutput = targetType
                ? meta.outputs.find((o) => {
                    const ot = (o.type || "string").toLowerCase();
                    const tt = targetType.toLowerCase();
                    return ot === "any" || tt === "any" || ot === tt;
                  })
                : null;

              sourceHandle = matchingOutput
                ? matchingOutput.name
                : meta.outputs.length > 0
                  ? meta.outputs[0].name
                  : null;
              target = nodeId ?? null;
              targetHandle = handleId ?? null;
            }

            if (source && target) {
              const newEdge: Edge = {
                id: `e${uuidv4()}`,
                source,
                sourceHandle,
                target,
                targetHandle,
                style: getEdgeStyle(source, sourceHandle),
              };
              return eds.concat(newEdge);
            }
            return eds;
          });
        }, 0);
      }
    },
    [
      nodeDefs,
      updateNodeData,
      deleteNode,
      submitJob,
      selectNode,
      toggleSourceView,
      currentWorkflow,
      getNodes,
      getEdges,
      setNodes,
      setEdges,
      getEdgeStyle,
      getHandleType,
      GRID_SIZE,
    ],
  );

  const onNodesChange = useCallback(
    (changes: NodeChange[]) => {
      // track resizing state
      for (const change of changes) {
        if (change.type === "dimensions") {
          if (change.resizing) {
            resizingNodesRef.current.add(change.id);
          } else {
            resizingNodesRef.current.delete(change.id);
          }
        }
      }

      const snappedChanges = changes.map((change) => {
        // snap dimensions during resize
        if (change.type === "dimensions") {
          const dimChange = change as NodeDimensionChange;
          if (dimChange.dimensions) {
            // find the node to get its min height
            const node = nodes.find((n) => n.id === dimChange.id);
            const meta = node
              ? nodeDefs.find((d) => d.name === node.type)
              : undefined;
            const minHeight =
              Math.ceil(calculateNodeMinHeight(meta) / GRID_SIZE) * GRID_SIZE;

            // snap to grid
            const snappedWidth =
              Math.round(dimChange.dimensions.width / GRID_SIZE) * GRID_SIZE;
            const snappedHeight =
              Math.round(dimChange.dimensions.height / GRID_SIZE) * GRID_SIZE;

            return {
              ...dimChange,
              dimensions: {
                width: Math.max(300, snappedWidth),
                height: Math.max(minHeight, snappedHeight),
              },
            };
          }
        }
        // snap position changes that occur during resize (from top/left edge resizing)
        if (
          change.type === "position" &&
          !change.dragging &&
          resizingNodesRef.current.has(change.id)
        ) {
          if (change.position) {
            return {
              ...change,
              position: {
                x: Math.round(change.position.x / GRID_SIZE) * GRID_SIZE,
                y: Math.round(change.position.y / GRID_SIZE) * GRID_SIZE,
              },
            };
          }
        }
        return change;
      });
      onNodesChangeBase(snappedChanges);
    },
    [onNodesChangeBase, nodes, nodeDefs],
  );

  const nodeEventCallbacks = useMemo(
    () => ({
      onNodeStarted: (nodeId: string) => {
        console.log("[App] onNodeStarted:", nodeId);
        setNodes((nds) => {
          const nodeExists = nds.some((n) => n.id === nodeId);
          console.log(
            "[App] Node exists in current workflow:",
            nodeExists,
            "nodeId:",
            nodeId,
          );
          return nds.map((n) =>
            n.id === nodeId
              ? {
                  ...n,
                  data: {
                    ...n.data,
                    isRunning: true,
                    progress: 0,
                    error: undefined,
                  },
                }
              : n,
          );
        });
      },
      onNodeProgress: (nodeId: string, progress: number, message?: string) => {
        setNodes((nds) =>
          nds.map((n) =>
            n.id === nodeId
              ? {
                  ...n,
                  data: { ...n.data, progress, progressMessage: message },
                }
              : n,
          ),
        );
      },
      onNodePartialOutput: (
        nodeId: string,
        outputName: string,
        _delta: unknown,
        accumulated: unknown,
      ) => {
        const currentEdges = edgesRef.current;
        setNodes((nds) => {
          const outgoingEdges = currentEdges.filter((e) => e.source === nodeId);
          return nds.map((n) => {
            // update the producing node's outputs
            if (n.id === nodeId) {
              return {
                ...n,
                data: {
                  ...n.data,
                  outputs: {
                    ...(n.data.outputs || {}),
                    [outputName]: accumulated,
                  },
                },
              };
            }
            // propagate to directly connected downstream nodes (write to data[targetHandle]
            // to match onNodeFinished, which is what NodeInputRenderer reads)
            const edge = outgoingEdges.find(
              (e) => e.sourceHandle === outputName && e.target === n.id,
            );
            if (edge && edge.targetHandle) {
              return {
                ...n,
                data: {
                  ...n.data,
                  [edge.targetHandle]: accumulated,
                },
              };
            }
            return n;
          });
        });
      },
      onNodeFinished: (nodeId: string, result: unknown, cached: boolean) => {
        console.log("[App] onNodeFinished:", nodeId, "cached:", cached);
        const currentEdges = edgesRef.current;

        // use functional form to ensure we have the latest state
        // this prevents race conditions when NodeStarted and NodeFinished arrive rapidly
        setNodes((currentNodes) => {
          let inputChanged = false;

          const nodeExists = currentNodes.some((n) => n.id === nodeId);
          console.log(
            "[App] onNodeFinished - Node exists:",
            nodeExists,
            "total nodes:",
            currentNodes.length,
          );

          const outgoingEdges = currentEdges.filter((e) => e.source === nodeId);
          const targetNodeIds = new Set(outgoingEdges.map((e) => e.target));

          const newNodes = currentNodes.map((node) => {
            if (node.id === nodeId) {
              return {
                ...node,
                data: {
                  ...node.data,
                  isRunning: false,
                  justFinished: true,
                  isCached: cached,
                  progress: 100,
                  outputs: result,
                  error: undefined,
                },
              };
            }

            if (targetNodeIds.has(node.id)) {
              const newData = { ...node.data };
              let changed = false;

              const relevantEdges = outgoingEdges.filter(
                (e) => e.target === node.id,
              );

              relevantEdges.forEach((e) => {
                const sourceHandle = e.sourceHandle || "output";
                const targetHandle = e.targetHandle;

                if (targetHandle) {
                  let value = null;
                  if (
                    result &&
                    typeof result === "object" &&
                    result !== null &&
                    sourceHandle in result
                  ) {
                    value = (result as Record<string, unknown>)[sourceHandle];
                  } else if (typeof result !== "object" || result === null) {
                    value = result;
                  }

                  if (value !== null && value !== undefined) {
                    const oldVal = newData[targetHandle];
                    if (oldVal !== value) {
                      newData[targetHandle] = value;
                      changed = true;
                    }
                  }
                }
              });

              if (changed) {
                inputChanged = true;
                // clear stale outputs so display components read fresh inputs
                delete newData.outputs;
                return { ...node, data: newData };
              }
            }

            return node;
          });

          const updatedNode = newNodes.find((n) => n.id === nodeId);
          console.log("[App] onNodeFinished - Setting node state:", nodeId, {
            isRunning: updatedNode?.data?.isRunning,
            justFinished: updatedNode?.data?.justFinished,
            isCached: updatedNode?.data?.isCached,
          });

          if (inputChanged) {
            // schedule setIsDirty to run after this state update
            setTimeout(() => setIsDirty(true), 0);
          }

          return newNodes;
        });

        console.log(
          "[App] Scheduling justFinished=false for:",
          nodeId,
          "in 1500ms",
        );
        setTimeout(() => {
          console.log("[App] Clearing justFinished for:", nodeId);
          setNodes((nds) =>
            nds.map((n) =>
              n.id === nodeId
                ? { ...n, data: { ...n.data, justFinished: false } }
                : n,
            ),
          );
        }, 1500);
      },
      onNodeError: (nodeId: string, error: string) => {
        setNodes((nds) => {
          const node = nds.find((n) => n.id === nodeId);
          const nodeName = node?.data?.metadata?.title || node?.type || nodeId;
          addToast("error", `Error: ${nodeName}`, error);
          return nds.map((n) =>
            n.id === nodeId
              ? { ...n, data: { ...n.data, isRunning: false, error } }
              : n,
          );
        });
      },
      onJobStarted: () => {
        // node state reset is done in submitJob (scoped to this client)
      },
      onJobCompleted: (_jobId: string, workflowName: string | null) => {
        addToast(
          "info",
          "Job Completed",
          `Workflow: ${workflowName || "Untitled"}`,
          "#22c55e",
        );
      },
    }),
    [setNodes, addToast],
  );

  const {
    jobs,
    connected: queueConnected,
    clearCompletedJobs,
    cancelJob,
  } = useExecutionQueue(nodeEventCallbacks);

  const nodeTypes = useMemo(() => {
    const types: NodeTypes = {};
    nodeDefs.forEach((def) => {
      types[def.name] = GenericNode;
    });
    if (Object.keys(types).length === 0) {
      return {
        Echo: GenericNode,
        HttpRequest: GenericNode,
        ShellCommand: GenericNode,
      };
    }
    return types;
  }, [nodeDefs]);

  const filteredNodeOptions = useMemo(() => {
    if (!nodeSelector?.filterType) return nodeDefs;

    const { type, direction } = nodeSelector.filterType;
    const sourceType = type.toLowerCase();

    return nodeDefs.filter((def) => {
      if (direction === "output") {
        return def.inputs.some((input) => {
          const inputType = (input.type || "string").toLowerCase();
          return (
            inputType === "any" ||
            sourceType === "any" ||
            inputType === sourceType
          );
        });
      } else {
        return def.outputs.some((output) => {
          const outputType = (output.type || "string").toLowerCase();
          return (
            outputType === "any" ||
            sourceType === "any" ||
            outputType === sourceType
          );
        });
      }
    });
  }, [nodeDefs, nodeSelector]);

  useEffect(() => {
    if (showRunMenu) {
      const handleGlobalClick = () => setShowRunMenu(false);
      setTimeout(() => window.addEventListener("click", handleGlobalClick), 0);
      return () => window.removeEventListener("click", handleGlobalClick);
    }
  }, [showRunMenu]);

  // for nodes whose type advertises has_dynamic_spec, fetch the per-instance
  // metadata when the values feeding the spec change. the fetched metadata
  // overrides the node's data.metadata so dynamic ports render alongside the
  // static control inputs. lastSpecKeyRef prevents refetch loops: we only
  // call the endpoint when the relevant input values actually changed.
  const updateNodeInternals = useUpdateNodeInternals();
  const lastSpecKeyRef = useRef<Map<string, string>>(new Map());
  useEffect(() => {
    if (!currentWorkflow) return;
    let cancelled = false;

    const work = async () => {
      for (const node of nodes) {
        const baseMeta = nodeDefs.find((d) => d.name === node.type);
        if (!baseMeta?.has_dynamic_spec) continue;

        // build the input map we'll send to the spec endpoint: live values
        // from node.data (which the user is editing) override anything else.
        const liveInputs: Record<string, unknown> = {};
        for (const spec of baseMeta.inputs) {
          const v = (node.data as Record<string, unknown>)[spec.name];
          if (v !== undefined) {
            liveInputs[spec.name] = v;
          }
        }
        const key = JSON.stringify(liveInputs);
        if (lastSpecKeyRef.current.get(node.id) === key) continue;

        try {
          const res = await fetch(
            `/api/workflows/${currentWorkflow}/nodes/${node.id}/spec`,
            {
              method: "POST",
              headers: { "Content-Type": "application/json" },
              body: JSON.stringify({
                inputs: liveInputs,
                node_type: node.type,
              }),
            },
          );
          if (!res.ok) continue;
          const fetched: NodeMetadata = await res.json();
          if (cancelled) return;
          setNodes((nds) =>
            nds.map((n) =>
              n.id === node.id
                ? {
                    ...n,
                    data: {
                      ...n.data,
                      metadata: fetched,
                    },
                  }
                : n,
            ),
          );
          // only record the cache key once the metadata has actually been
          // applied — otherwise a cancelled effect (common during workflow
          // load when nodes/state churn) would record the key without
          // updating data.metadata, permanently blocking the refetch.
          lastSpecKeyRef.current.set(node.id, key);
          // tell reactflow to recompute handle positions; without this,
          // newly-appeared output handles render but aren't connectable.
          updateNodeInternals(node.id);
        } catch (e) {
          console.error(`failed to fetch spec for ${node.id}`, e);
        }
      }
    };
    // debounce so editor keystrokes don't fire a request per character
    const handle = setTimeout(work, 300);
    return () => {
      cancelled = true;
      clearTimeout(handle);
    };
  }, [nodes, nodeDefs, currentWorkflow, setNodes, updateNodeInternals]);

  useEffect(() => {
    console.log("App Version: Dynamic-Node-System-" + Date.now());

    setTimeout(() => {
      fetchNodeMetadata();
    }, 0);
  }, []);

  useEffect(() => {
    setTimeout(() => {
      refreshWorkflows();
    }, 0);
  }, [refreshWorkflows]);

  const [restoreState, setRestoreState] = useState<
    "initial" | "restoring" | "pending-edges" | "pending-fitview" | "ready"
  >("initial");
  const pendingRestoreRef = useRef<{
    edges: Edge[];
    isDirty: boolean;
    fitView: boolean;
  } | null>(null);

  const restoreVersionRef = useRef(0);
  const lastProcessedVersionRef = useRef(0);
  const lastSeenNodesHashRef = useRef<string>("");
  const lastSeenEdgesHashRef = useRef<string>("");

  useEffect(() => {
    if (restoreState !== "ready") return;

    const workflow = generateWorkflow(nodes, edges);
    const state = generateState(nodes);
    try {
      localStorage.setItem(
        "flow_autosave",
        JSON.stringify({
          workflow,
          state,
          currentWorkflow,
          sidebarVisible,
          isDirty,
        }),
      );
    } catch {
      // quota exceeded -- retry without outputs to avoid crashing
      try {
        localStorage.setItem(
          "flow_autosave",
          JSON.stringify({
            workflow,
            state: {},
            currentWorkflow,
            sidebarVisible,
            isDirty,
          }),
        );
      } catch {
        // still too large, skip autosave entirely
      }
    }
  }, [nodes, edges, currentWorkflow, sidebarVisible, isDirty, restoreState]);

  useEffect(() => {
    if (restoreState !== "pending-edges" || !pendingRestoreRef.current) return;
    const { edges: pendingEdges, isDirty: pendingIsDirty } =
      pendingRestoreRef.current;
    const styledEdges: Edge[] = pendingEdges.map((e: Edge) => ({
      id: e.id,
      source: e.source,
      sourceHandle: e.sourceHandle,
      target: e.target,
      targetHandle: e.targetHandle,
      style: getEdgeStyle(e.source, e.sourceHandle || null),
    }));
    setEdges(styledEdges);
    setIsDirty(pendingIsDirty);
    setRestoreState("pending-fitview");
  }, [restoreState, getEdgeStyle, setEdges]);

  useEffect(() => {
    if (restoreState !== "pending-fitview") return;

    if (pendingRestoreRef.current?.fitView && !reactFlowInstance) {
      return;
    }

    const shouldFitView =
      pendingRestoreRef.current?.fitView && reactFlowInstance;
    pendingRestoreRef.current = null;

    const newVersion = restoreVersionRef.current + 1;
    restoreVersionRef.current = newVersion;

    setTimeout(() => setRestoreState("ready"), 0);

    if (shouldFitView) {
      requestAnimationFrame(() => {
        requestAnimationFrame(() => {
          if (reactFlowInstance) {
            reactFlowInstance.fitView({ padding: 0.1 });
          }
        });
      });
    }
  }, [restoreState, reactFlowInstance, nodes.length, edges.length]);

  useEffect(() => {
    if (nodeDefs.length === 0) return;
    if (restoreState !== "initial") return;

    const saved = localStorage.getItem("flow_autosave");
    if (!saved) {
      const newVersion = restoreVersionRef.current + 1;
      restoreVersionRef.current = newVersion;
      setTimeout(() => setRestoreState("ready"), 0);
      return;
    }

    try {
      const {
        workflow,
        state: savedState,
        currentWorkflow: savedWorkflow,
        sidebarVisible: savedSidebarVisible,
        isDirty: savedIsDirty,
      } = JSON.parse(saved);

      setTimeout(() => {
        setSidebarVisible(
          typeof savedSidebarVisible === "boolean" ? savedSidebarVisible : true,
        );
      }, 0);

      if (!workflow?.nodes || workflow.nodes.length === 0) {
        const newVersion = restoreVersionRef.current + 1;
        setTimeout(() => {
          setIsDirty(savedIsDirty === true);
          setRestoreState("ready");
        }, 0);
        restoreVersionRef.current = newVersion;
        return;
      }

      const savedEdges: WorkflowEdge[] = workflow.edges || [];
      resolveOutputs(workflow.nodes, savedEdges, savedState || {});

      const stateMap: WorkflowState = savedState || {};
      const restoredNodes: Node[] = workflow.nodes.map((n: WorkflowNode) => {
        const node = createReactFlowNode({
          nodeData: n,
          meta: nodeDefs.find((d) => d.name === n.type),
          gridSize: GRID_SIZE,
          updateNodeData,
          deleteNode,
          submitJob,
          selectNode,
          toggleSourceView,
          workflowName: savedWorkflow,
          getNodes,
          getEdges,
        });
        if (stateMap[n.id]) {
          node.data.outputs = stateMap[n.id];
        }
        return node;
      });

      setTimeout(() => {
        setRestoreState("restoring");
        setNodes(restoredNodes);
        setCurrentWorkflow(savedWorkflow || `.temp_${uuidv4()}`);
        setRestoreState("pending-edges");
      }, 0);

      pendingRestoreRef.current = {
        edges: savedEdges,
        isDirty: savedIsDirty === true,
        fitView: true,
      };
    } catch (e) {
      console.error("Failed to restore workflow state:", e);
      restoreVersionRef.current += 1;
      setTimeout(() => setRestoreState("ready"), 0);
    }
  }, [
    nodeDefs,
    restoreState,
    deleteNode,
    updateNodeData,
    submitJob,
    selectNode,
    toggleSourceView,
    getNodes,
    getEdges,
    setSidebarVisible,
    setNodes,
    setCurrentWorkflow,
  ]);

  // restore running node visual state from active jobs after page refresh
  const hasRestoredRunningRef = useRef(false);
  useEffect(() => {
    if (restoreState !== "ready" || hasRestoredRunningRef.current) return;
    if (jobs.length === 0) return;
    hasRestoredRunningRef.current = true;
    const runningJob = jobs.find((j) => j.status === "running");
    if (!runningJob) return;
    setNodes((nds) =>
      nds.map((n) => {
        if (!runningJob.active_nodes.includes(n.id)) return n;
        const np = runningJob.node_progress[n.id];
        return {
          ...n,
          data: {
            ...n.data,
            isRunning: true,
            progress: np?.progress ?? 0,
            progressMessage: np?.message,
            error: undefined,
          },
        };
      }),
    );
  }, [restoreState, jobs, setNodes]);

  useEffect(() => {
    const nodesHash = nodes
      .map(
        (n) =>
          `${n.id}:${n.position?.x}:${n.position?.y}:${n.style?.width}:${n.style?.height}`,
      )
      .join("|");
    const edgesHash = edges
      .map((e) => `${e.id}:${e.source}:${e.target}`)
      .join("|");

    if (restoreState !== "ready") {
      return;
    }

    if (restoreVersionRef.current !== lastProcessedVersionRef.current) {
      lastProcessedVersionRef.current = restoreVersionRef.current;
      lastSeenNodesHashRef.current = nodesHash;
      lastSeenEdgesHashRef.current = edgesHash;
      return;
    }

    if (
      lastSeenNodesHashRef.current === nodesHash &&
      lastSeenEdgesHashRef.current === edgesHash
    ) {
      return;
    }

    lastSeenNodesHashRef.current = nodesHash;
    lastSeenEdgesHashRef.current = edgesHash;

    setTimeout(() => setIsDirty(true), 0);
  }, [nodes, edges, restoreState]);

  const deleteWorkflow = async (name: string) => {
    try {
      const res = await fetch(`/api/workflows/${name}/delete`, {
        method: "POST",
      });
      if (res.ok) {
        refreshWorkflows();
        if (currentWorkflow === name) {
          setCurrentWorkflow(null);
          setNodes([]);
          setEdges([]);
        }
      } else {
        alert("Failed to delete workflow");
      }
    } catch (e) {
      // using e to avoid unused var
      console.error(e);
      alert("Failed to delete workflow");
    }
  };

  const renameWorkflow = async (oldName: string, newName: string) => {
    try {
      const res = await fetch(`/api/workflows/${oldName}/rename`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ new_name: newName }),
      });
      if (res.ok) {
        const data = await res.json();
        refreshWorkflows();
        if (currentWorkflow === oldName) {
          setCurrentWorkflow(data.new_name);
        }
      } else if (res.status === 409) {
        alert("A workflow with that name already exists");
      } else {
        alert("Failed to rename workflow");
      }
    } catch (e) {
      console.error(e);
      alert("Failed to rename workflow");
    }
  };

  const loadWorkflow = async (name: string) => {
    if (isDirty) {
      if (!confirm("You have unsaved changes. Load anyway?")) {
        return;
      }
    }

    try {
      setRestoreState("restoring");
      // wipe the dynamic-spec dedup cache: the new workflow may reuse node
      // ids whose inputs are unchanged, in which case the cached key would
      // cause the spec effect to skip the refetch and leave the freshly
      // remounted nodes with their base metadata (no dynamic ports, no
      // script_source for the View Source button).
      lastSpecKeyRef.current.clear();
      const [wfRes, stateRes] = await Promise.all([
        fetch(`/api/workflows/${name}`),
        fetch(`/api/workflows/${name}/state`),
      ]);
      const wf = await wfRes.json();
      const wfState: WorkflowState = stateRes.ok ? await stateRes.json() : {};
      if (wf.nodes) {
        const wfEdges: WorkflowEdge[] = wf.edges || [];
        resolveOutputs(wf.nodes, wfEdges, wfState);

        const restoredNodes: Node[] = wf.nodes.map((n: WorkflowNode) =>
          createReactFlowNode({
            nodeData: n,
            meta: nodeDefs.find((d) => d.name === n.type),
            gridSize: GRID_SIZE,
            updateNodeData,
            deleteNode,
            submitJob,
            selectNode,
            toggleSourceView,
            workflowName: name,
            getNodes,
            getEdges,
          }),
        );

        // inject outputs from state into node data for display
        for (const node of restoredNodes) {
          if (wfState[node.id]) {
            node.data.outputs = wfState[node.id];
          }
        }

        pendingRestoreRef.current = {
          edges: wfEdges,
          isDirty: false,
          fitView: true,
        };

        setNodes(restoredNodes);
        setCurrentWorkflow(name);
        setRestoreState("pending-edges");
      } else {
        setRestoreState("ready");
        alert("Invalid workflow format");
      }
    } catch (e) {
      console.error(e);
      setRestoreState("ready");
      alert("Failed to load workflow");
    }
  };

  // core save logic shared by saveWorkflow and saveWorkflowAs
  const saveWorkflowWithName = async (name: string) => {
    try {
      const workflow = generateWorkflow(nodes, edges);
      const state = generateState(nodes);
      // save definition and state in parallel
      await Promise.all([
        fetch(`/api/workflows/${name}`, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(workflow),
        }),
        fetch(`/api/workflows/${name}/state`, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(state),
        }),
      ]);
      // clean up temp workflow files when saving under a real name
      if (currentWorkflow && currentWorkflow.startsWith(".temp_") && name !== currentWorkflow) {
        fetch(`/api/workflows/${currentWorkflow}/delete`, { method: "POST" }).catch(() => {});
      }
      setCurrentWorkflow(name);
      setIsDirty(false);
      refreshWorkflows();
      addToast("info", "Workflow saved", `Saved as "${name}"`);
    } catch (e) {
      console.error(e);
      alert("Failed to save workflow");
    }
  };

  const saveWorkflow = async () => {
    const isTemp = !currentWorkflow || currentWorkflow.startsWith(".temp_");
    const name = isTemp ? prompt("Enter workflow name:") : currentWorkflow;
    if (name) await saveWorkflowWithName(name);
  };

  const saveWorkflowAs = async () => {
    const name = prompt("Enter new workflow name:");
    if (name) await saveWorkflowWithName(name);
  };

  const getConnectedInputs = useCallback(
    (nodeId: string, currentEdges: Edge[]) => {
      const connected: Record<string, unknown> = {};
      currentEdges.forEach((edge) => {
        if (edge.target === nodeId) {
          if (edge.targetHandle)
            connected[edge.targetHandle] = { connected: true };
        }
      });
      return connected;
    },
    [],
  );

  useEffect(() => {
    setNodes((nds) =>
      nds.map((node) => {
        const connectedInputs = getConnectedInputs(node.id, edges);
        if (
          JSON.stringify(node.data.inputs) !== JSON.stringify(connectedInputs)
        ) {
          return { ...node, data: { ...node.data, inputs: connectedInputs } };
        }
        return node;
      }),
    );
  }, [edges, getConnectedInputs, setNodes]);

  // placeholder for future connection validation logic
  const isValidConnection = useCallback(() => true, []);

  const newWorkflow = useCallback(() => {
    if (isDirty) {
      if (!confirm("You have unsaved changes. Start a new workflow anyway?")) {
        return;
      }
    }
    const newVersion = restoreVersionRef.current + 1;
    restoreVersionRef.current = newVersion;
    setNodes([]);
    setEdges([]);
    setCurrentWorkflow(`.temp_${uuidv4()}`);
    setIsDirty(false);
  }, [isDirty, setNodes, setEdges]);

  // use the new hook for keyboard shortcuts
  useKeyboardShortcuts({
    nodes,
    edges,
    reactFlowInstance,
    showShortcuts,
    submitJob,
    saveWorkflow,
    deleteNode,
    selectNode,
    setEdgeMode,
    setShowShortcuts,
    toggleSidebar: () => setSidebarVisible((v) => !v),
    setNodeSelector,
    setContextMenu,
  });

  // custom wheel handler for Cardinal/VCVRack/Firefox-style controls:
  // - Scroll: vertical pan
  // - Shift+Scroll: horizontal pan
  // - Ctrl+Scroll: zoom
  useEffect(() => {
    const wrapper = reactFlowWrapper.current;
    if (!wrapper || !reactFlowInstance) return;

    const handleWheel = (e: WheelEvent) => {
      // only handle if the event is within the ReactFlow canvas
      const target = e.target as Element;
      if (!wrapper.contains(target)) return;

      e.preventDefault();

      const viewport = reactFlowInstance.getViewport();
      const hasModifier = e.ctrlKey || e.metaKey;

      if (hasModifier) {
        // ctrl+Scroll: Zoom towards mouse position
        const zoomSensitivity = 0.002;
        const zoomDelta = -e.deltaY * zoomSensitivity;
        const newZoom = Math.min(
          Math.max(viewport.zoom * (1 + zoomDelta), 0.1),
          4,
        );

        const rect = wrapper.getBoundingClientRect();
        const mouseX = e.clientX - rect.left;
        const mouseY = e.clientY - rect.top;

        // calculate flow coordinate under mouse and new viewport to keep it there
        const pointX = (mouseX - viewport.x) / viewport.zoom;
        const pointY = (mouseY - viewport.y) / viewport.zoom;

        reactFlowInstance.setViewport({
          x: mouseX - pointX * newZoom,
          y: mouseY - pointY * newZoom,
          zoom: newZoom,
        });
      } else {
        // pan: Use both deltaX and deltaY for natural two-finger scrolling
        // this allows diagonal movement when scrolling diagonally on touchpad
        // shift+scroll converts vertical scroll to horizontal (traditional behavior)
        const panX = e.shiftKey ? -e.deltaY : -e.deltaX;
        const panY = e.shiftKey ? 0 : -e.deltaY;
        reactFlowInstance.setViewport({
          x: viewport.x + panX,
          y: viewport.y + panY,
          zoom: viewport.zoom,
        });
      }
    };

    wrapper.addEventListener("wheel", handleWheel, { passive: false });
    return () => wrapper.removeEventListener("wheel", handleWheel);
  }, [reactFlowInstance]);

  const onPaneContextMenu = useCallback(
    (event: React.MouseEvent) => {
      event.preventDefault();
      // close any existing menus (including sidebar)
      setContextMenu(null);
      setSidebarCloseMenuSignal((s) => s + 1);

      const clientX = event.clientX;
      const clientY = event.clientY;

      if (reactFlowInstance) {
        const position = reactFlowInstance.screenToFlowPosition({
          x: clientX,
          y: clientY,
        });
        setNodeSelector({
          x: clientX,
          y: clientY,
          params: { position },
        });
      }
    },
    [reactFlowInstance],
  );

  const onConnect = useCallback(
    (params: Connection) => {
      connectionMadeRef.current = true;

      if (isValidConnection()) {
        setEdges((eds) => {
          const filteredEdges = eds.filter(
            (e) =>
              !(
                e.target === params.target &&
                e.targetHandle === params.targetHandle
              ),
          );
          return addEdge(
            {
              ...params,
              style: getEdgeStyle(
                params.source || "",
                params.sourceHandle || null,
              ),
            },
            filteredEdges,
          );
        });
      } else {
        alert("Type Mismatch: Cannot connect incompatible types.");
      }
    },
    [setEdges, isValidConnection, getEdgeStyle],
  );

  const onConnectStart = useCallback(
    (_: React.MouseEvent | React.TouchEvent, params: OnConnectStartParams) => {
      connectionStartRef.current = params;
      connectionMadeRef.current = false;
    },
    [],
  );

  const onConnectEnd = useCallback(
    (event: MouseEvent | TouchEvent) => {
      const target = event.target;
      const startParams = connectionStartRef.current;

      const handleEl = (target as Element).closest?.(".react-flow__handle");
      const isTargetHandle = !!handleEl;

      if (!connectionMadeRef.current && startParams && !isTargetHandle) {
        const clientX =
          "clientX" in event
            ? event.clientX
            : (event as TouchEvent).changedTouches?.[0]?.clientX;
        const clientY =
          "clientY" in event
            ? event.clientY
            : (event as TouchEvent).changedTouches?.[0]?.clientY;

        if (
          clientX &&
          clientY &&
          reactFlowInstance &&
          startParams.nodeId &&
          startParams.handleId &&
          startParams.handleType
        ) {
          const position = reactFlowInstance.screenToFlowPosition({
            x: clientX,
            y: clientY,
          });

          const handleType = getHandleType(
            startParams.nodeId,
            startParams.handleId,
            startParams.handleType,
          );
          const direction =
            startParams.handleType === "source" ? "output" : "input";

          setNodeSelector({
            x: clientX,
            y: clientY,
            params: { ...startParams, position },
            filterType: handleType
              ? { type: handleType, direction }
              : undefined,
          });
        }
      }

      connectionStartRef.current = null;
    },
    [reactFlowInstance, getHandleType],
  );

  const onDragOver = useCallback((event: React.DragEvent) => {
    event.preventDefault();
    event.dataTransfer.dropEffect = "move";
  }, []);

  const onDrop = useCallback(
    (event: React.DragEvent) => {
      event.preventDefault();

      const type = event.dataTransfer.getData("application/reactflow");
      if (typeof type === "undefined" || !type) {
        return;
      }

      if (!reactFlowInstance) return;

      const position = reactFlowInstance.screenToFlowPosition({
        x: event.clientX,
        y: event.clientY,
      });

      position.x -= 100;
      position.y -= 40;
      addNode(type, position);
    },
    [reactFlowInstance, addNode],
  );

  const onNodeContextMenu = useCallback(
    (event: React.MouseEvent, node: Node) => {
      event.preventDefault();
      // close any existing menus (including sidebar)
      constPaneContextMenu();
      setNodeSelector(null);
      setSidebarCloseMenuSignal((s) => s + 1);

      const hasSource = !!node.data.metadata?.script_source;
      const options = [
        {
          label: node.data.isPinned ? "Unpin Node" : "Pin Node",
          onClick: () => {
            updateNodeData(node.id, "isPinned", !node.data.isPinned);
            setNodes((nds) =>
              nds.map((n) =>
                n.id === node.id ? { ...n, draggable: node.data.isPinned } : n,
              ),
            );
            setContextMenu(null);
          },
        },
        {
          label: node.data.isBypassed ? "Enable Node" : "Bypass Node",
          onClick: () => {
            updateNodeData(node.id, "isBypassed", !node.data.isBypassed);
            setContextMenu(null);
          },
        },
        {
          label: node.data.skip_cache ? "Enable Cache" : "Skip Cache",
          onClick: () => {
            updateNodeData(node.id, "skip_cache", !node.data.skip_cache);
            setContextMenu(null);
          },
        },
        {
          label: "Delete Node",
          onClick: () => {
            deleteNode(node.id);
            setContextMenu(null);
          },
        },
      ];

      if (hasSource) {
        options.unshift({
          label: node.data.showSource ? "Hide Source" : "View Source",
          onClick: () => {
            toggleSourceView(node.id);
            setContextMenu(null);
          },
        });
      }

      setContextMenu({
        x: event.clientX,
        y: event.clientY,
        options,
      });
    },
    [
      setNodes,
      deleteNode,
      toggleSourceView,
      updateNodeData,
      constPaneContextMenu,
    ],
  );

  return (
    <div
      style={{
        width: "100vw",
        height: "100vh",
        display: "flex",
        flexDirection: "column",
      }}
    >
      <div
        style={{
          padding: "10px",
          borderBottom: "1px solid var(--panel-border)",
          display: "flex",
          gap: "10px",
          alignItems: "center",
          background: "var(--panel-bg)",
        }}
      >
        <button
          onClick={() => setSidebarVisible(!sidebarVisible)}
          className="toolbar-btn"
          title={sidebarVisible ? "Hide Sidebar" : "Show Sidebar"}
          style={{ padding: "8px" }}
        >
          {sidebarVisible ? (
            <PanelLeftClose size={16} />
          ) : (
            <PanelLeft size={16} />
          )}
        </button>
        <div
          style={{
            width: "1px",
            height: "20px",
            background: "var(--panel-border)",
            margin: "0 5px",
          }}
        ></div>
        <div style={{ display: "flex", gap: "0", position: "relative" }}>
          <button
            onClick={() => submitJob(undefined, runMode === "force")}
            className="toolbar-btn"
            style={{ borderTopRightRadius: 0, borderBottomRightRadius: 0 }}
          >
            <Play size={16} />{" "}
            {runMode === "force" ? "Run Workflow (force)" : "Run Workflow"}
          </button>
          <button
            className="toolbar-btn"
            style={{
              borderTopLeftRadius: 0,
              borderBottomLeftRadius: 0,
              borderLeft: "none",
              padding: "8px 4px",
            }}
            onClick={() => setShowRunMenu(!showRunMenu)}
          >
            <ChevronDown size={14} />
          </button>
          {showRunMenu && (
            <div
              style={{
                position: "absolute",
                top: "100%",
                left: 0,
                marginTop: "4px",
                background: "var(--panel-bg)",
                border: "1px solid var(--panel-border)",
                borderRadius: "6px",
                boxShadow: "0 4px 6px -1px rgba(0,0,0,0.1)",
                zIndex: 100,
                minWidth: "180px",
                display: "flex",
                flexDirection: "column",
              }}
            >
              <div
                onClick={() => {
                  setRunMode("default");
                  setShowRunMenu(false);
                }}
                style={{
                  padding: "8px 12px",
                  cursor: "pointer",
                  fontSize: "13px",
                  display: "flex",
                  alignItems: "center",
                  gap: "8px",
                  color: "var(--text-color)",
                }}
                className="context-menu-item"
              >
                <Play size={14} /> Run Workflow
              </div>
              <div
                onClick={() => {
                  setRunMode("force");
                  setShowRunMenu(false);
                }}
                style={{
                  padding: "8px 12px",
                  cursor: "pointer",
                  fontSize: "13px",
                  display: "flex",
                  alignItems: "center",
                  gap: "8px",
                  color: "var(--text-color)",
                }}
                className="context-menu-item"
              >
                <Play size={14} /> Run Workflow (force)
              </div>
            </div>
          )}
        </div>
        <button onClick={saveWorkflow} className="toolbar-btn">
          <Save size={16} /> Save
        </button>
        <button onClick={saveWorkflowAs} className="toolbar-btn">
          <Save size={16} /> Save As
        </button>
        <div
          style={{
            width: "1px",
            height: "20px",
            background: "var(--panel-border)",
            margin: "0 5px",
          }}
        ></div>
        <button onClick={newWorkflow} className="toolbar-btn">
          <FilePlus size={16} /> New Workflow
        </button>
        {currentWorkflow && (
          <>
            <div
              style={{
                width: "1px",
                height: "20px",
                background: "var(--panel-border)",
                margin: "0 5px",
              }}
            ></div>
            <span
              style={{
                fontSize: "13px",
                color: "var(--text-color)",
                opacity: 0.7,
              }}
            >
              {currentWorkflow.startsWith(".temp_") ? "Untitled" : currentWorkflow}
            </span>
          </>
        )}
        <div style={{ flex: 1 }} />
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: "4px",
            fontSize: "12px",
            color: queueConnected ? "#22c55e" : "var(--danger-color)",
          }}
        >
          <span>{queueConnected ? "Connected" : "Disconnected"}</span>
          {queueConnected ? <Wifi size={14} /> : <WifiOff size={14} />}
        </div>
      </div>
      <div
        style={{
          flex: 1,
          display: "flex",
          overflow: "hidden",
          position: "relative",
        }}
      >
        {sidebarVisible && (
          <Sidebar
            workflows={workflows}
            onLoad={loadWorkflow}
            currentWorkflow={currentWorkflow}
            onDeleteWorkflow={deleteWorkflow}
            onRenameWorkflow={renameWorkflow}
            onAddNode={addNode}
            nodeTypes={nodeDefs}
            jobs={jobs}
            onClearCompletedJobs={clearCompletedJobs}
            onCancelJob={cancelJob}
            onContextMenuOpen={handleSidebarContextMenuOpen}
            closeMenuSignal={sidebarCloseMenuSignal}
          />
        )}
        <div
          style={{ flex: 1, position: "relative" }}
          ref={reactFlowWrapper}
          onDrop={onDrop}
          onDragOver={onDragOver}
        >
          <ReactFlow
            nodes={nodes}
            edges={
              edgeMode === "hidden"
                ? edges.map((e) => ({ ...e, hidden: true }))
                : edges
            }
            className={`${edgeMode === "above" ? "edges-above-nodes" : ""} ${!isInteractive ? "edges-locked" : ""}`}
            nodeTypes={nodeTypes}
            onNodesChange={onNodesChange}
            onEdgesChange={onEdgesChange}
            onConnect={onConnect}
            onReconnect={
              isInteractive
                ? (oldEdge, newConn) => {
                    connectionMadeRef.current = true;
                    setEdges((els) => reconnectEdge(oldEdge, newConn, els));
                  }
                : undefined
            }
            onReconnectEnd={
              isInteractive
                ? (_, edge) => {
                    const event = _ as MouseEvent | TouchEvent;
                    const target = event.target as Element;
                    if (
                      target.classList.contains("react-flow__pane") ||
                      target.tagName === "svg"
                    ) {
                      setEdges((eds) => eds.filter((e) => e.id !== edge.id));
                    }
                  }
                : undefined
            }
            onConnectStart={onConnectStart}
            onConnectEnd={onConnectEnd}
            onInit={setReactFlowInstance}
            onNodeContextMenu={onNodeContextMenu}
            onPaneContextMenu={onPaneContextMenu}
            onMoveStart={() => {
              setContextMenu(null);
              setNodeSelector(null);
            }}
            snapToGrid
            snapGrid={[15, 15]}
            defaultEdgeOptions={{ interactionWidth: isInteractive ? 40 : 0 }}
            edgesUpdatable={isInteractive}
            edgesFocusable={isInteractive}
            connectionRadius={20}
            panOnDrag={[1]}
            zoomOnScroll={false}
            panOnScroll={false}
          >
            <Controls position="bottom-right" className="controls-large">
              <ControlButton
                onClick={() =>
                  setEdgeMode((mode) =>
                    mode === "behind"
                      ? "above"
                      : mode === "above"
                        ? "hidden"
                        : "behind",
                  )
                }
                title={
                  edgeMode === "behind"
                    ? "Edges: Behind Nodes"
                    : edgeMode === "above"
                      ? "Edges: Above Nodes"
                      : "Edges: Hidden"
                }
              >
                {edgeMode === "behind" ? (
                  <Eye />
                ) : edgeMode === "above" ? (
                  <Layers />
                ) : (
                  <EyeOff />
                )}
              </ControlButton>
              <ControlButton
                onClick={() =>
                  setTheme(
                    theme === "light"
                      ? "dark"
                      : theme === "dark"
                        ? "system"
                        : "light",
                  )
                }
                title={
                  theme === "light"
                    ? "Theme: Light"
                    : theme === "dark"
                      ? "Theme: Dark"
                      : "Theme: System"
                }
              >
                {theme === "light" ? (
                  <Sun />
                ) : theme === "dark" ? (
                  <Moon />
                ) : (
                  <Monitor />
                )}
              </ControlButton>
            </Controls>
            <Background
              color={resolvedTheme === "dark" ? "#555" : "#888"}
              gap={15}
            />
          </ReactFlow>
        </div>
        {contextMenu && (
          <ContextMenu {...contextMenu} onClose={() => setContextMenu(null)} />
        )}
        {nodeSelector && (
          <NodeSelector
            x={nodeSelector.x}
            y={nodeSelector.y}
            options={filteredNodeOptions}
            onClose={() => {
              setNodeSelector(null);
              connectionStartRef.current = null;
            }}
            onSelect={(type) => {
              const params = nodeSelector.params;
              addNode(type, params?.position, params);
              setNodeSelector(null);
              connectionStartRef.current = null;
            }}
          />
        )}
        {showShortcuts && (
          <div
            style={{
              position: "fixed",
              inset: 0,
              background: "rgba(0, 0, 0, 0.5)",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              zIndex: 2000,
            }}
            onClick={() => setShowShortcuts(false)}
          >
            <div
              style={{
                background: "var(--panel-bg)",
                border: "1px solid var(--panel-border)",
                borderRadius: "12px",
                padding: "24px",
                maxWidth: "500px",
                width: "90%",
                maxHeight: "90vh",
                overflow: "auto",
                boxShadow: "0 8px 32px rgba(0, 0, 0, 0.3)",
              }}
              onClick={(e) => e.stopPropagation()}
            >
              <div
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "center",
                  marginBottom: "20px",
                }}
              >
                <div
                  style={{ display: "flex", alignItems: "center", gap: "10px" }}
                >
                  <Keyboard size={20} />
                  <h2 style={{ margin: 0, fontSize: "18px", fontWeight: 600 }}>
                    Keyboard Shortcuts
                  </h2>
                </div>
                <button
                  onClick={() => setShowShortcuts(false)}
                  style={{
                    background: "transparent",
                    border: "none",
                    cursor: "pointer",
                    padding: "4px",
                    color: "var(--text-color)",
                    display: "flex",
                  }}
                >
                  <X size={20} />
                </button>
              </div>
              <div
                style={{ display: "flex", flexDirection: "column", gap: "8px" }}
              >
                {[
                  {
                    keys: "Scroll",
                    desc: "Pan canvas (two-finger for both axes)",
                  },
                  { keys: "Shift+Scroll", desc: "Pan canvas horizontally" },
                  { keys: "Ctrl+Scroll", desc: "Zoom in/out" },
                  { keys: "Middle-click drag", desc: "Pan canvas" },
                  {
                    keys: "Arrow Keys",
                    desc: "Pan canvas / Move selected node",
                  },
                  { keys: "Shift+Arrow", desc: "Pan/Move faster" },
                  { keys: "Ctrl+Arrow", desc: "Pan/Move slower (precise)" },
                  { keys: "Page Up / Page Down", desc: "Zoom in / out" },
                  { keys: "F", desc: "Fit view" },
                  { keys: "Escape", desc: "Deselect node, close menus" },
                  { keys: "Ctrl+Shift+Enter", desc: "Run entire workflow" },
                  { keys: "Ctrl+Enter", desc: "Run selected node" },
                  { keys: "Ctrl+S", desc: "Save workflow" },
                  { keys: "Delete / Backspace", desc: "Delete selected node" },
                  { keys: "[ / ]", desc: "Select input / output node" },
                  { keys: "{ / }", desc: "Select prev / next sibling" },
                  { keys: "B", desc: "Toggle sidebar" },
                  { keys: "E", desc: "Cycle edge mode (behind/above/hidden)" },
                  { keys: "H", desc: "Toggle interactivity (lock)" },
                  { keys: "Right-click canvas", desc: "Add node" },
                  { keys: "Right-click node", desc: "Node options" },
                  { keys: "?", desc: "Toggle this help" },
                ].map(({ keys, desc }) => (
                  <div
                    key={keys}
                    style={{
                      display: "flex",
                      justifyContent: "space-between",
                      alignItems: "center",
                      padding: "8px 0",
                      borderBottom: "1px solid var(--panel-border)",
                    }}
                  >
                    <span
                      style={{ color: "var(--text-muted)", fontSize: "13px" }}
                    >
                      {desc}
                    </span>
                    <kbd
                      style={{
                        background: "var(--input-bg)",
                        border: "1px solid var(--panel-border)",
                        borderRadius: "4px",
                        padding: "4px 8px",
                        fontSize: "12px",
                        fontFamily: "monospace",
                        color: "var(--text-color)",
                      }}
                    >
                      {keys}
                    </kbd>
                  </div>
                ))}
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

export default function App() {
  return (
    <ToastProvider>
      <ReactFlowProvider>
        <AppInner />
      </ReactFlowProvider>
    </ToastProvider>
  );
}
