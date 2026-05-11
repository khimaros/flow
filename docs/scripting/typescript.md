# typescript user nodes

a typescript user node is a `.ts` source file defining two top-level functions: `spec()` and `execute(inputs)`. optional: `get_options(input_name, inputs)`.

scripts run inside an embedded boa engine — pure ECMAScript with a tiny host surface. there is no node.js, no DOM, no fetch, no filesystem, no external module loader. despite the `.ts` extension, type annotations are not enforced; treat it as plain JavaScript.

## llm output contract

emit only raw source. no markdown fences, no commentary, no preamble. first character must be source code.

## minimal example

```javascript
function spec() {
  return {
    name: "Reverse",
    title: "Reverse String",
    category: "Utility",
    description: "return the input string reversed",
    inputs: [{ name: "text", type: "string", required: true }],
    outputs: [{ name: "reversed", type: "string" }]
  };
}

function execute(inputs) {
  const text = inputs.text || "";
  return { reversed: text.split("").reverse().join("") };
}
```

## spec()

pure, deterministic, no IO. returns an object:

- `name` (string): camelcase identifier, no spaces or punctuation
- `title` (string): display name
- `category` (string): label, e.g. `"Generative"`, `"Data"`, `"I/O"`, `"Network"`, `"Utility"`, `"Flow Control"`
- `description` (string): one short sentence
- `inputs` (array): input specs (may be empty)
- `outputs` (array): output specs (may be empty)

### input spec

- `name` (string, required): valid identifier
- `type` (string, required): `string`, `integer`, `float`, `boolean`, `list`, `object`, `any`, `file`
- `ui` (string, optional): `text`, `textarea`, `number`, `checkbox`, `boolean_select`, `password`, `select`, `dynamic_select`, `dynamic_multi_select`, `list_editor`, `audio_recorder`, `auto` (default)
- `options` (array, with `ui: "select"`): list of strings or `{value, label}` objects
- `depends_on` (array, with `ui: "dynamic_select"`): names of inputs feeding `get_options`
- `required` (bool, default false)
- `default` (any): used when input is missing/empty
- `description` (string)
- `env_var` (string): env var that fills the input when unset; auto `FLOW_<NODE>_<INPUT>` is also checked

### output spec

- `name` (string, required)
- `type` (string, required): same set as input `type`
- `description` (string)

## execute(inputs)

receives `inputs` object keyed by declared input names. values arrive as native JS types. returns an object keyed by declared output names; missing keys become `null`.

rules:

- `spec()` is pure; `execute()` may use the host functions below
- use `inputs.name ?? default` rather than throwing on missing input
- top-level code is limited to `function` declarations
- no `import`, no `require`, no async/await runtime — execute is synchronous

## get_options(input_name, inputs) — optional

required if any input uses `ui: "dynamic_select"` or `"dynamic_multi_select"`. returns an array of strings or `{value, label}` objects based on `input_name` and the (possibly partial) `inputs` object.

## host globals

- `console.log(msg)`: write to server log
- `crypto.randomUUID()`: generate UUID v4

no built-in HTTP, filesystem, or environment access from the typescript runtime — for those, prefer rhai or python user nodes.

## file inputs

`type: "file"` inputs arrive as `{path, url, mime_type}`. there is no built-in file reader in the typescript runtime; pass the path through to a node that can read it, or use a different language for the node body.
