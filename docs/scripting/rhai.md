# rhai user nodes

a rhai user node is a `.rhai` source file defining two top-level functions: `spec()` and `execute(inputs)`. optional: `get_options(input_name, inputs)`.

rhai is a sandboxed scripting language baked into the rust binary — no external runtime, no filesystem or network access except via the host functions listed below.

## llm output contract

emit only raw rhai source. no markdown fences, no commentary, no preamble. first character must be rhai source.

## minimal example

```rhai
fn spec() {
    return #{
        name: "Reverse",
        title: "Reverse String",
        category: "Utility",
        description: "return the input string reversed",
        inputs: [
            #{ name: "text", "type": "string", required: true }
        ],
        outputs: [
            #{ name: "reversed", "type": "string" }
        ]
    };
}

fn execute(inputs) {
    let text = inputs.text ?? "";
    let chars = text.split("");
    chars.reverse();
    return #{ reversed: chars.join("") };
}
```

note: `#{ ... }` is rhai's object map literal. `type` is a reserved word, so quote it as `"type"` inside maps.

## spec()

pure, deterministic, no IO. returns an object map:

- `name` (string): camelcase identifier, no spaces or punctuation
- `title` (string): display name
- `category` (string): label, e.g. `"Generative"`, `"Data"`, `"I/O"`, `"Network"`, `"Utility"`, `"Flow Control"`
- `description` (string): one short sentence
- `inputs` (array): input specs (may be empty)
- `outputs` (array): output specs (may be empty)

### input spec

- `name` (string, required): valid identifier
- `"type"` (string, required): `string`, `integer`, `float`, `boolean`, `list`, `object`, `any`, `file`
- `ui` (string, optional): `text`, `textarea`, `number`, `checkbox`, `boolean_select`, `password`, `select`, `dynamic_select`, `dynamic_multi_select`, `list_editor`, `audio_recorder`, `auto` (default)
- `options` (array, with `ui: "select"`): list of strings or `#{value, label}` maps
- `depends_on` (array, with `ui: "dynamic_select"`): names of inputs feeding `get_options`
- `required` (bool, default false)
- `default` (any): used when input is missing/empty
- `description` (string)
- `env_var` (string): env var that fills the input when unset; auto `FLOW_<NODE>_<INPUT>` is also checked

### output spec

- `name` (string, required)
- `"type"` (string, required): same set as input `"type"`
- `description` (string)

## execute(inputs)

receives `inputs` as an object map keyed by declared input names. values arrive as native rhai types (string, i64, f64, bool, array, map). returns an object map keyed by declared output names; missing keys become null.

rules:

- `spec()` is pure; `execute()` may do IO via host fns
- use `inputs.name ?? default` (null-coalesce) rather than throwing on missing input
- top-level code is limited to function definitions

## get_options(input_name, inputs) — optional

required if any input uses `ui: "dynamic_select"` or `"dynamic_multi_select"`. returns an array of strings or `#{value, label}` maps based on `input_name` and the (possibly partial) `inputs` map.

## host functions

logging and control flow:

- `log(msg)`: write to server log
- `is_cancelled() -> bool`: poll in long loops; return early when true
- `report_progress(progress)` / `report_progress(progress, message)`: progress in `0.0..1.0`
- `sleep(ms)`: blocking sleep (integer ms)
- `uuid_v4() -> string`: generate UUID v4

streaming partial output (engine consumers see live updates):

- `emit_output(output_name, delta)`: append `delta` to a per-name string accumulator and emit
- `emit_output_value(output_name, accumulated)`: emit a structured (non-string) partial value; pass the full current accumulated value (delta is set to the same)

http (returns `#{status, body}`; body is parsed JSON if valid, else text):

- `http_request(method, url, body, headers, options)`: JSON/text body; methods `GET POST PUT PATCH DELETE HEAD`
- `http_request_binary(method, url, body, headers, options)`: returns `#{status, body_base64, content_type}`
- `http_request_multipart(url, fields, headers, options)`: POST with multipart fields. each field value is either a string (text) or `#{file_base64, filename, mime_type}` (file)
- `http_request_sse(method, url, body, headers, options)`: server-sent events. returns `#{status, events}` where `events` is an array of `#{event, data}`. pass `options.emit = [#{event, path, output}, ...]` to stream matching event fields (json-pointer `path`) into `emit_output(output, ...)` as they arrive

`options` map (all optional): `timeout` (seconds, int/float), `retries` (int, http_request only).

assets and files:

- `save_asset_base64(filename, content_b64) -> string`: decodes base64 and writes under `generated_assets/`; returns the saved path
- `read_file_base64(file_path) -> string`: reads any file as base64
- `decode_base64_text(b64) -> string`: decodes base64 to UTF-8 text (useful for reading error bodies from `http_request_binary`)

environment:

- `get_env(name) -> string`: returns the env var, or empty string

registry introspection (available in spec/get_options/execute):

- `list_nodes() -> array`: array of `#{name, title, category, description}` for every registered node type
- `node_spec(name) -> object`: full metadata for one node type
- `node_to_openai_tool(name) -> object`: build an OpenAI tools-API entry from a node's input spec

dispatching other nodes (execute only):

- `dispatch_node(name, inputs) -> map`: invoke another registered node by type name. inputs is a map of input name to value; returns the node's outputs as a map. cancellation and partial-output emission propagate through the parent context.

## file inputs

`type: "file"` inputs arrive as `#{path, url, mime_type}`. read `path` directly with `read_file_base64` if needed.
