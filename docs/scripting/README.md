# user node scripting

flow loads `user_nodes/*` at server startup and registers each file as a node, alongside the built-in nodes. the file extension picks the runtime:

| extension | runtime | doc |
|-----------|---------|-----|
| `.json`   | declarative (compose existing nodes) | [declarative.md](declarative.md) |
| `.rhai`   | rhai (sandboxed, baked into the binary) | [rhai.md](rhai.md) |
| `.py`     | python (cpython via pyo3) | [python.md](python.md) |
| `.ts`     | typescript / javascript (boa engine) | [typescript.md](typescript.md) |

every runtime exposes the same node-authoring contract: the file declares a spec (name, title, category, description, inputs, outputs) and an execute function that receives an inputs map/object and returns an outputs map/object. a third optional `get_options(input_name, inputs)` powers `dynamic_select` / `dynamic_multi_select` UI components.

inline / dynamic user nodes (created in the UI via the `DynamicUserNode` node) accept the same source string for any of these runtimes — pick the language from the dropdown.

## picking a runtime

- start with **declarative** when your node is just "wire these existing nodes together" — no code, no failure modes from script bugs.
- pick **rhai** when you need IO (HTTP, SSE streaming, dispatching other nodes, asset writes) without external dependencies. the bundled `openai_*` nodes are written in rhai.
- pick **python** when you need third-party libraries or richer string/data handling. python is the most permissive runtime.
- pick **typescript** for tiny pure-data transforms — the boa runtime has almost no host surface (no HTTP, no fs).

## llm-author tips

each language doc starts with a one-line "llm output contract" that specifies what an llm should emit (raw source only, no markdown fences). these are written so the docs can be handed to a code-generating LLM as a self-contained prompt.

## see also

- `user_nodes/` in this distribution — examples of each runtime
- `workflows/` — example workflows that wire user nodes together
