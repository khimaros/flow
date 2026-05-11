# python user nodes

a python user node is a source file (or string, for `DynamicUserNode`) defining two top-level functions: `spec()` and `execute(inputs)`. optional: `get_options(input_name, inputs)`.

## llm output contract

emit only raw python source. no markdown fences, no commentary, no preamble. first character must be python source.

## minimal example

```python
def spec():
    return {
        "name": "Reverse",
        "title": "Reverse String",
        "category": "Utility",
        "description": "return the input string reversed",
        "inputs": [{"name": "text", "type": "string", "required": True}],
        "outputs": [{"name": "reversed", "type": "string"}],
    }

def execute(inputs):
    return {"reversed": inputs.get("text", "")[::-1]}
```

## spec()

pure, deterministic, no IO. returns a dict:

- `name` (str): camelcase identifier, no spaces or punctuation
- `title` (str): display name
- `category` (str): label, e.g. `"Generative"`, `"Data"`, `"I/O"`, `"Network"`, `"Utility"`, `"Flow Control"`
- `description` (str): one short sentence
- `inputs` (list): input specs (may be empty)
- `outputs` (list): output specs (may be empty)

### input spec

- `name` (str, required): valid python identifier
- `type` (str, required): `string`, `integer`, `float`, `boolean`, `list`, `object`, `any`, `file`
- `ui` (str, optional): `text`, `textarea`, `number`, `checkbox`, `boolean_select`, `password`, `select`, `dynamic_select`, `list_editor`, `audio_recorder`, `auto` (default)
- `options` (list, with `ui=select`): list of strings or `{"value", "label"}` dicts
- `depends_on` (list[str], with `ui=dynamic_select`): names of inputs feeding `get_options`
- `required` (bool, default False)
- `default` (any): used when input is missing/empty
- `description` (str)
- `env_var` (str): env var that fills the input when unset; auto `FLOW_<NODE>_<INPUT>` is also checked

### output spec

- `name` (str, required)
- `type` (str, required): same set as input `type`
- `description` (str)

## execute(inputs)

receives `inputs` dict keyed by declared input names. values arrive as native python types. returns a dict keyed by declared output names; missing keys become `None`.

rules:

- `spec()` is pure; `execute()` may do IO
- stdlib only unless third-party is intentional; import third-party inside `execute()` so it can't break spec parsing
- use `inputs.get(name, default)` rather than raising on missing input
- self-contained: no relative imports
- top-level code is limited to imports and function definitions

## get_options(input_name, inputs) — optional

required if any input uses `ui=dynamic_select`. returns a list of strings or `{"value", "label"}` dicts based on `input_name` and the (possibly partial) `inputs` dict.

## engine globals

- `log(msg)`: write to server log (prefer over `print`)
- `is_cancelled() -> bool`: poll in long loops; return early when True
- `report_progress(progress, message=None)`: progress in `0.0..1.0`

no built-in http. use `urllib.request` from stdlib, or `requests`/`httpx` if imported.

## file inputs

`type=file` inputs arrive as `{"path", "url", "mime_type"}`. open `path` directly.
