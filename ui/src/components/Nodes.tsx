import { useState, useEffect } from "react";
import type { NodeProps, Node, Edge } from "reactflow";
import { NodeContainer, InputField, OutputHandle } from "./NodeComponents";
import { calculateNodeMinHeight } from "../utils/nodeUtils";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import {
  oneDark,
  oneLight,
} from "react-syntax-highlighter/dist/esm/styles/prism";
import { renderInputControl } from "./NodeInputRenderer";
import {
  DisplayAudioNode,
  DisplayImageNode,
  DisplayMarkdownNode,
  DisplayJsonNode,
} from "./NodeDisplays";
import type { NodeMetadata } from "../types";

// hook to get current theme from document attribute
const useCurrentTheme = () => {
  const [isDark, setIsDark] = useState(() => {
    return document.documentElement.getAttribute("data-theme") === "dark";
  });

  useEffect(() => {
    const observer = new MutationObserver(() => {
      setIsDark(document.documentElement.getAttribute("data-theme") === "dark");
    });
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["data-theme"],
    });
    return () => observer.disconnect();
  }, []);

  return isDark;
};

interface CustomNodeData {
  [key: string]: unknown;
  metadata: NodeMetadata;
  onChange: (field: string, value: unknown) => void;
  onDelete: (nodeId: string) => void;
  onRun: (nodeId: string, force?: boolean) => void;
  onSelect: (nodeId: string) => void;
  onToggleSource: (nodeId: string) => void;
  isPinned: boolean;
  isBypassed: boolean;
  isCached: boolean;
  skip_cache: boolean;
  showSource: boolean;
  workflowName: string | null;
  inputs?: Record<string, { connected: boolean }>;
  outputs?: unknown;
  isRunning?: boolean;
  justFinished?: boolean;
  error?: string;
  getNodes: () => Node[];
  getEdges: () => Edge[];
}

// map language names to Prism language identifiers
const languageMap: Record<string, string> = {
  rhai: "rust", // Rhai is similar to Rust syntax
  python: "python",
  lua: "lua",
  typescript: "typescript",
  javascript: "javascript",
};

export const GenericNode = ({
  id,
  data,
  selected,
}: NodeProps<CustomNodeData>) => {
  const meta = data.metadata;
  const showSource = data.showSource;
  const hasSource = !!meta.script_source;
  const isDark = useCurrentTheme();

  const mapType = (t: string) => {
    return t.toLowerCase();
  };

  const GRID_SIZE = 15;
  const connectedInputs = Object.fromEntries(
    Object.entries(data.inputs ?? {}).map(([name, s]) => [
      name,
      !!s?.connected,
    ]),
  );
  const minHeight =
    Math.ceil(calculateNodeMinHeight(meta, connectedInputs) / GRID_SIZE) *
    GRID_SIZE;

  // front face - normal node view. when showSource is true, we still render
  // this structure (so the input/output Handle elements stay mounted and
  // connected edges remain anchored) but overlay the source code on top of
  // the controls. handles live outside the content padding box, so they
  // remain visible alongside the overlay.
  const showSourceOverlay = showSource && hasSource;
  const frontFace = (
    <NodeContainer
      label={meta.title || meta.name}
      selected={selected}
      isBypassed={data.isBypassed}
      onRun={(force) => data.onRun(id, force)}
      isRunning={data.isRunning}
      justFinished={data.justFinished}
      isCached={data.isCached}
      error={data.error}
      minHeight={minHeight}
      hasSource={hasSource}
      showSource={showSource}
      onToggleSource={() => data.onToggleSource?.(id)}
    >
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          flex: 1,
          flexShrink: 1,
          minHeight: 0,
          maxHeight: "100%",
          minWidth: 0, // Allow shrinking in flex container
          boxSizing: "border-box",
        }}
      >
        {meta.inputs.map((input) => (
          <InputField
            key={input.name}
            label={input.name}
            id={input.name}
            type={mapType(input.type)}
            connected={data.inputs?.[input.name]?.connected}
            required={input.required}
            description={input.description}
            compactRow={"DynamicMultiSelect" in input.ui_component}
          >
            {/* Cast data to satisfy RenderInputData if needed */}
            {renderInputControl(input, data, id)}
          </InputField>
        ))}

        {meta.name === "DisplayAudio" && <DisplayAudioNode data={data} />}
        {meta.name === "DisplayImage" && <DisplayImageNode data={data} />}
        {meta.name === "DisplayMarkdown" && <DisplayMarkdownNode data={data} />}
        {meta.name === "DisplayJson" && (
          <DisplayJsonNode data={data} isDark={isDark} />
        )}
      </div>

      <div
        style={{
          marginTop: "auto",
          paddingTop: "8px",
          borderTop: "1px solid var(--node-border)",
          display: "flex",
          flexDirection: "column",
          alignItems: "flex-end",
          gap: "4px",
          flexShrink: 0,
        }}
      >
        {meta.outputs.map((output) => (
          <OutputHandle
            key={output.name}
            id={output.name}
            label={output.name}
            type={mapType(output.type)}
            description={output.description}
          />
        ))}
      </div>

      {/* source code overlay: rendered on top of the front face so the
          underlying input/output Handle elements stay mounted (and visible
          at the node edges, since they sit outside the content padding box).
          the overlay's opaque syntax-highlighter background covers the
          control visuals beneath it. */}
      {showSourceOverlay && (
        <div
          className="nodrag"
          style={{
            position: "absolute",
            top: 0,
            left: 0,
            right: 0,
            bottom: 0,
            overflow: "auto",
            borderRadius: "4px",
            fontSize: "11px",
            lineHeight: "1.4",
          }}
        >
          <SyntaxHighlighter
            language={languageMap[meta.script_source!.language] || "javascript"}
            style={isDark ? oneDark : oneLight}
            customStyle={{
              margin: 0,
              padding: "8px",
              borderRadius: "4px",
              fontSize: "11px",
              minHeight: "100%",
              whiteSpace: "pre-wrap",
              wordBreak: "break-word",
              overflowWrap: "anywhere",
            }}
            codeTagProps={{
              style: {
                whiteSpace: "pre-wrap",
                wordBreak: "break-word",
                overflowWrap: "anywhere",
              },
            }}
            wrapLines
            wrapLongLines
          >
            {meta.script_source!.source}
          </SyntaxHighlighter>
        </div>
      )}
    </NodeContainer>
  );

  return frontFace;
};
