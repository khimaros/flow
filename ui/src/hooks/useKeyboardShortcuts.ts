import { useEffect, useRef } from "react";
import {
  useStoreApi,
  type Node,
  type Edge,
  type ReactFlowInstance,
} from "reactflow";
import {
  getUpstreamNodes,
  getDownstreamNodes,
  getSiblingNodes,
  getRootNodes,
  getLeafNodes,
} from "../utils/graphNavigation";

// check if focus is currently in an interactive input element
function isInputFocused(): boolean {
  const el = document.activeElement;
  if (!el) return false;
  return (
    el.tagName === "INPUT" ||
    el.tagName === "TEXTAREA" ||
    el.tagName === "SELECT" ||
    (el as HTMLElement).isContentEditable
  );
}

interface UseKeyboardShortcutsProps {
  nodes: Node[];
  edges: Edge[];
  reactFlowInstance: ReactFlowInstance | null;
  showShortcuts: boolean;
  submitJob: (nodeId?: string) => void;
  saveWorkflow: () => void;
  deleteNode: (id: string) => void;
  selectNode: (id: string) => void;
  setEdgeMode: React.Dispatch<
    React.SetStateAction<"hidden" | "behind" | "above">
  >;
  setShowShortcuts: React.Dispatch<React.SetStateAction<boolean>>;
  toggleSidebar: () => void;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  setNodeSelector: (val: any) => void;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  setContextMenu: (val: any) => void;
}

export function useKeyboardShortcuts({
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
  toggleSidebar,
  setNodeSelector,
  setContextMenu,
}: UseKeyboardShortcutsProps) {
  const store = useStoreApi();
  const nodesRef = useRef(nodes);
  const edgesRef = useRef(edges);

  useEffect(() => {
    nodesRef.current = nodes;
  }, [nodes]);

  useEffect(() => {
    edgesRef.current = edges;
  }, [edges]);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const currentNodes = nodesRef.current;
      const currentEdges = edgesRef.current;
      const hasModifier = e.ctrlKey || e.metaKey;
      const getSelectedNode = () => currentNodes.find((n) => n.selected);

      // ctrl+Shift+Enter: Run entire workflow (works even in inputs)
      if (hasModifier && e.shiftKey && e.key === "Enter") {
        e.preventDefault();
        e.stopPropagation();
        submitJob(undefined);
        return;
      }

      // ctrl+Enter: Run selected node (works even in inputs)
      if (hasModifier && e.key === "Enter") {
        const selectedNode = getSelectedNode();
        if (selectedNode) {
          e.preventDefault();
          e.stopPropagation();
          submitJob(selectedNode.id);
        }
        return;
      }

      // ctrl+S: Save workflow (works even in inputs)
      if (hasModifier && e.key === "s") {
        e.preventDefault();
        e.stopPropagation();
        saveWorkflow();
        return;
      }

      // escape: Close menus/overlays and deselect nodes (works even in inputs)
      if (e.key === "Escape") {
        e.preventDefault();
        if (showShortcuts) {
          setShowShortcuts(false);
        }
        setNodeSelector(null);
        setContextMenu(null);
        if (reactFlowInstance) {
          reactFlowInstance.setNodes((nds) =>
            nds.map((n) => ({ ...n, selected: false })),
          );
        }
        return;
      }

      // all remaining shortcuts require focus NOT in an input
      if (isInputFocused()) return;

      // delete/Backspace: Delete selected node
      if (e.key === "Delete" || e.key === "Backspace") {
        const selectedNode = getSelectedNode();
        if (selectedNode) {
          e.preventDefault();
          e.stopPropagation();
          deleteNode(selectedNode.id);
        }
        return;
      }

      // B: Toggle sidebar
      if (e.key === "b" || e.key === "B") {
        e.preventDefault();
        toggleSidebar();
        return;
      }

      // graph Navigation: [ and ]
      const selectedNode = getSelectedNode();

      // [ : Jump to input node (upstream) or deepest leaf if none selected
      if (e.key === "[" && !e.shiftKey) {
        e.preventDefault();
        if (selectedNode) {
          const sources = getUpstreamNodes(
            selectedNode.id,
            currentNodes,
            currentEdges,
          );
          if (sources.length > 0) selectNode(sources[0].id);
        } else {
          const leaves = getLeafNodes(currentNodes, currentEdges);
          if (leaves.length > 0) selectNode(leaves[leaves.length - 1].id);
        }
        return;
      }

      // ] : Jump to output node (downstream) or highest parent if none selected
      if (e.key === "]" && !e.shiftKey) {
        e.preventDefault();
        if (selectedNode) {
          const targets = getDownstreamNodes(
            selectedNode.id,
            currentNodes,
            currentEdges,
          );
          if (targets.length > 0) selectNode(targets[0].id);
        } else {
          const roots = getRootNodes(currentNodes, currentEdges);
          if (roots.length > 0) selectNode(roots[0].id);
        }
        return;
      }

      // { or } : Prev/Next Sibling
      const isSiblingNav =
        e.key === "{" ||
        e.key === "}" ||
        (e.shiftKey && (e.key === "[" || e.key === "]"));
      if (isSiblingNav) {
        e.preventDefault();
        const isNext = e.key === "}" || (e.shiftKey && e.key === "]");

        if (selectedNode) {
          const siblings = getSiblingNodes(
            selectedNode.id,
            currentNodes,
            currentEdges,
          );
          if (siblings.length > 0) {
            const currentIndex = siblings.findIndex(
              (n) => n.id === selectedNode.id,
            );
            const nextIndex =
              currentIndex === -1
                ? 0
                : isNext
                  ? (currentIndex + 1) % siblings.length
                  : (currentIndex - 1 + siblings.length) % siblings.length;
            selectNode(siblings[nextIndex].id);
          }
        } else {
          // no selection: behave like [ or ]
          if (isNext) {
            const roots = getRootNodes(currentNodes, currentEdges);
            if (roots.length > 0) selectNode(roots[0].id);
          } else {
            const leaves = getLeafNodes(currentNodes, currentEdges);
            if (leaves.length > 0) selectNode(leaves[leaves.length - 1].id);
          }
        }
        return;
      }

      // arrow keys: Move selected node OR Pan canvas
      if (["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"].includes(e.key)) {
        if (!reactFlowInstance) return;
        e.preventDefault();
        e.stopPropagation();

        // determine speed: Ctrl = slow, Shift = fast, default = normal
        const step = hasModifier ? 5 : e.shiftKey ? 45 : 15;

        if (selectedNode) {
          // move selected node(s)
          reactFlowInstance.setNodes((nds) =>
            nds.map((n) => {
              if (!n.selected) return n;
              const pos = { ...n.position };
              if (e.key === "ArrowUp") pos.y -= step;
              if (e.key === "ArrowDown") pos.y += step;
              if (e.key === "ArrowLeft") pos.x -= step;
              if (e.key === "ArrowRight") pos.x += step;
              return { ...n, position: pos };
            }),
          );
        } else {
          // pan canvas (inverted direction for natural feel)
          const viewport = reactFlowInstance.getViewport();
          const dx =
            e.key === "ArrowLeft" ? step : e.key === "ArrowRight" ? -step : 0;
          const dy =
            e.key === "ArrowUp" ? step : e.key === "ArrowDown" ? -step : 0;
          reactFlowInstance.setViewport({
            x: viewport.x + dx,
            y: viewport.y + dy,
            zoom: viewport.zoom,
          });
        }
        return;
      }

      // page Up/Down: Zoom
      if (e.key === "PageUp" || e.key === "PageDown") {
        if (!reactFlowInstance) return;
        e.preventDefault();
        const viewport = reactFlowInstance.getViewport();
        const zoomFactor = e.key === "PageUp" ? 1.2 : 0.8;
        const newZoom = Math.min(Math.max(viewport.zoom * zoomFactor, 0.1), 4);
        reactFlowInstance.setViewport({
          x: viewport.x,
          y: viewport.y,
          zoom: newZoom,
        });
        return;
      }

      // F: Fit view
      if ((e.key === "f" || e.key === "F") && reactFlowInstance) {
        e.preventDefault();
        reactFlowInstance.fitView({ padding: 0.1 });
        return;
      }

      // E: Cycle edge display mode
      if (e.key === "e" || e.key === "E") {
        e.preventDefault();
        setEdgeMode((mode) =>
          mode === "behind" ? "above" : mode === "above" ? "hidden" : "behind",
        );
        return;
      }

      // H: Toggle interactivity (lock)
      if (e.key === "h" || e.key === "H") {
        e.preventDefault();
        const { nodesDraggable, nodesConnectable, elementsSelectable } =
          store.getState();
        const isInteractive =
          nodesDraggable || nodesConnectable || elementsSelectable;
        store.setState({
          nodesDraggable: !isInteractive,
          nodesConnectable: !isInteractive,
          elementsSelectable: !isInteractive,
        });
        return;
      }

      // ?: Show keyboard shortcuts
      if (e.key === "?" || (e.shiftKey && e.key === "/")) {
        e.preventDefault();
        setShowShortcuts((prev) => !prev);
        return;
      }
    };

    window.addEventListener("keydown", handleKeyDown, true);
    return () => window.removeEventListener("keydown", handleKeyDown, true);
  }, [
    submitJob,
    saveWorkflow,
    deleteNode,
    selectNode,
    reactFlowInstance,
    showShortcuts,
    store,
    setEdgeMode,
    setShowShortcuts,
    setNodeSelector,
    setContextMenu,
    toggleSidebar,
  ]);
}
