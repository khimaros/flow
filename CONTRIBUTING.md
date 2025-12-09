# contributing

## build commands

```bash
# rust backend and CLI
make build

# UI (react + typescript + vite)
make build-ui
```

## testing

this project uses a mix of unit tests, integration tests, and end-to-end
(e2e) tests.

- **`make precommit`**: runs linting and all tests. recommended before pushing.
- **`make test`**: runs all e2e tests (API, CLI, UI).
- **`make test-api-e2e`**: runs HTTP API tests using `tests/run_e2e.sh` and `hurl`.
- **`make test-ui-e2e`**: runs UI tests using playwright (`ui/tests/`).
- **`make test-cli-e2e`**: runs CLI tests.

the server embeds the built UI using `rust-embed`, so run `npm run build`
in `ui/` before `cargo build` to include UI changes.

## architecture

this is a visual node-based workflow execution engine with a rust backend
and react frontend.

### core engine (rust)

- **`src/engine.rs`**: workflow execution engine with topological sort,
  result caching (SHA256 hash of inputs), and dependency resolution. the
  `NodeRegistry` manages node type registration.
- **`src/node.rs`**: the `Node` trait defines the interface all nodes
  implement: `name()`, `inputs()`, `outputs()`, and async `execute()`.
  nodes declare typed I/O via the `DataType` enum.
- **`src/graph.rs`**: `Workflow` and `NodeInstance` structs define the
  workflow graph format. node connections use
  `{"$node": "id", "$output": "field"}` syntax.
- **`src/value.rs`**: dynamic `Value` enum (Null, Boolean, Integer, Float,
  String, Array, Object) used throughout the system.
- **`src/nodes/`**: node implementations — add new nodes here and
  register in `mod.rs` via `register_all()`.

### binaries

- **`flow-server`** (`src/bin/flow-server.rs`): axum HTTP server with
  endpoints:
  - `GET /api/nodes` — list available node types
  - `POST /api/execute` — execute a workflow
  - `GET/POST /api/workflows/{name}` — load/save workflows (stored in
    the data dir's `workflows/` as JSON)
- **`flow-cli`** (`src/bin/flow-cli.rs`): CLI runner for saved workflow
  files.

### UI (react + reactflow)

- **`ui/src/App.tsx`**: main canvas component with drag-drop node
  creation, edge connections, and workflow execution.
- **`ui/src/components/Nodes.tsx`**: visual node components for each
  node type.
- workflows saved from the UI are JSON with `nodes` and `edges` arrays
  including visual layout.

## adding new node types

1. create a node struct in `src/nodes/` implementing the `Node` trait.
2. register it in `src/nodes/mod.rs` via
   `registry.register("NodeName", || Box::new(node::NodeType))`.
3. add a matching react component in `ui/src/components/Nodes.tsx` with
   input/output handles.
4. register the component in the `nodeTypes` map in `App.tsx`.

## scripting and user nodes

the engine supports dynamic node creation via files placed in the
`user_nodes/` directory. supported formats (all enabled by default):

- **rhai** (`.rhai`) — sandboxed scripting baked into the rust binary.
  fast and safe; the bundled `openai_*` nodes are written in rhai.
- **python** (`.py`) — full cpython via pyo3.
- **typescript** (`.ts`) — executed via the embedded boa engine.
- **declarative** (`.json`) — compose existing nodes into a reusable
  unit without writing any code.

files in `user_nodes/` are automatically loaded on server startup.
