# declarative (JSON) user nodes

a declarative user node is a `.json` file that composes existing registered nodes into a single reusable node, without writing any code. inner nodes and edges are wired together; the outer node exposes a flat list of inputs/outputs that map onto specific inputs/outputs of the inner subgraph.

## llm output contract

emit only raw JSON. no markdown fences, no commentary, no preamble. first character must be `{`.

## minimal example

```json
{
  "name": "FetchArticle",
  "title": "Fetch Article",
  "category": "Network",
  "description": "Fetch a URL and convert HTML to markdown",
  "inputs": [
    { "name": "url", "mapping": "fetch.url" }
  ],
  "outputs": [
    { "name": "markdown", "mapping": "convert.markdown" }
  ],
  "implementation": {
    "nodes": [
      {
        "id": "fetch",
        "type": "WebFetch",
        "position": { "x": 0.0, "y": 0.0 },
        "size": { "width": 400.0, "height": 220.0 },
        "inputs": {},
        "skipCache": false,
        "bypassed": false
      },
      {
        "id": "convert",
        "type": "HtmlToMarkdown",
        "position": { "x": 450.0, "y": 0.0 },
        "size": { "width": 400.0, "height": 300.0 },
        "inputs": {},
        "skipCache": false,
        "bypassed": false
      }
    ],
    "edges": [
      {
        "source": "fetch",
        "sourceHandle": "content",
        "target": "convert",
        "targetHandle": "html"
      }
    ]
  }
}
```

## top-level fields

- `name` (string): camelcase identifier, no spaces or punctuation
- `title` (string): display name
- `category` (string): label, e.g. `"Generative"`, `"Data"`, `"I/O"`, `"Network"`, `"Utility"`, `"Flow Control"`
- `description` (string): one short sentence
- `inputs` (array): outer input specs (may be empty)
- `outputs` (array): outer output specs (may be empty)
- `implementation` (object): the inner workflow — `nodes` and `edges`

## input spec

each entry routes one outer input to one or more inner-node inputs.

- `name` (string, required): the outer input name
- `mapping`: how the value is forwarded inside. accepts several shapes:
  - `"node_id.input_name"` — single target
  - `["node_a.in1", "node_b.in2"]` — broadcast to multiple targets
  - `{ "node_id": "...", "input_name": "..." }` — single target, structured
  - `[{ "node_id": "...", "input_name": "..." }, ...]` — multiple targets, structured
- `type`, `ui_component`, `default`, `required`, `description`, `env_var` — all optional. when omitted, they are inferred from the first mapped inner input. specify them explicitly to override.

if you want a purely cosmetic input (e.g. for documentation or auto env-var binding) with no inner mapping, omit `mapping`.

## output spec

each entry exposes one inner-node output as an outer output.

- `name` (string, required): the outer output name
- `mapping` (string): `"node_id.output_name"` — only the simple dotted form is supported
- `type`, `description` — optional; inferred from the inner output when omitted

## implementation

an embedded workflow:

- `nodes` (array): each entry has
  - `id` (string): unique within this file; referenced by `mapping` and `edges`
  - `type` (string): the registered node type (e.g. `"WebFetch"`, `"OpenAI_LLM"`, or another user node's `name`)
  - `position`, `size`: layout hints (`{x, y}` and `{width, height}`); used by the UI
  - `inputs` (object): inline default values keyed by inner input name. use `{}` if all values come from edges or outer inputs
  - `skipCache` (bool): bypass the result cache for this inner node
  - `bypassed` (bool): pass-through (skip execution); useful for toggling sub-steps
- `edges` (array): each entry has
  - `source`, `sourceHandle`: producing node id and its output name
  - `target`, `targetHandle`: consuming node id and its input name

inner inputs receive values in this priority: explicit edge → outer input via `mapping` → inline `inputs` value → declared default.

## env vars

when the outer input doesn't declare `env_var`, the engine still tries the auto convention `FLOW_<OUTER_NODE>_<OUTER_INPUT>`, then falls back to whatever env binding the inner input would have resolved on its own (e.g. `FLOW_OPENAI_LLM_API_KEY`). this lets declarative wrappers inherit credential env vars from the nodes they wrap without restating them.

## referencing other user nodes

inner `type` may reference any registered node, including other user nodes (rhai/python/typescript/declarative). user nodes are loaded once at startup, so referenced files must already exist.
