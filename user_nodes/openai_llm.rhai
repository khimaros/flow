fn spec() {
    return #{
        name: "OpenAI_LLM",
        title: "OpenAI Text Generation",
        category: "Generative",
        description: "Query an OpenAI-compatible LLM endpoint, optionally with tool calling",
        inputs: [
            #{
                name: "api_base",
                "type": "string",
                "default": "https://api.openai.com/v1",
                description: "API base URL",
                env_var: "OPENAI_API_BASE"
            },
            #{
                name: "api_key",
                "type": "string",
                ui: "password",
                description: "API key",
                env_var: "OPENAI_API_KEY"
            },
            #{
                name: "model",
                "type": "string",
                ui: "dynamic_select",
                "default": "gpt-3.5-turbo",
                description: "model ID",
                depends_on: ["api_base", "api_key"]
            },
            #{
                name: "system_prompt",
                "type": "string",
                ui: "textarea",
                "default": "You are a helpful assistant.",
                description: "system instruction"
            },
            #{
                name: "prompt",
                "type": "string",
                ui: "textarea",
                required: true,
                description: "user prompt"
            },
            #{
                name: "tools",
                "type": "list",
                ui: "dynamic_multi_select",
                depends_on: [],
                description: "node types to expose to the model as callable tools"
            },
            #{
                name: "json_schema",
                "type": "object",
                ui: "textarea",
                description: "JSON schema for structured output (llama.cpp compatible, leave empty for free-form text)"
            },
            #{
                name: "temperature",
                "type": "float",
                description: "sampling temperature"
            },
            #{
                name: "seed",
                "type": "integer",
                description: "random seed"
            },
            #{
                name: "max_turns",
                "type": "integer",
                "default": 10,
                description: "maximum conversation turns (0 = unlimited)"
            },
            #{
                name: "max_tokens",
                "type": "integer",
                description: "maximum tokens to generate"
            },
            #{
                name: "thinking_budget_tokens",
                "type": "integer",
                description: "max thinking tokens for reasoning models"
            }
        ],
        outputs: [
            #{ name: "text", "type": "string", description: "final-turn response text" },
            #{ name: "thinking", "type": "string", description: "final-turn thinking content (if supported)" },
            #{ name: "usage", "type": "object", description: "token usage statistics (summed across tool iterations)" },
            #{ name: "messages", "type": "list", description: "full conversation transcript: system/user/assistant/tool messages (tool calls are embedded on assistant turns and tool results follow as role:tool messages)" }
        ]
    };
}

// reassemble a single chat-completion turn from an SSE events array.
// returns #{ content, thinking, tool_calls, usage } where tool_calls is an
// array of #{ id, name, arguments } (arguments is a JSON string).
fn reassemble_turn(events) {
    let content = "";
    let thinking = "";
    let usage = #{};
    // tool_calls accumulator keyed by index; rebuilt into an ordered array at end.
    let tc_by_index = #{};

    for ev in events {
        if ev.event != "message" { continue; }
        let choices = ev.data.choices;
        if choices != () && type_of(choices) == "array" && choices.len > 0 {
            let delta = choices[0].delta;
            if delta != () {
                if delta.content != () { content += delta.content; }
                if delta.reasoning_content != () { thinking += delta.reasoning_content; }
                if delta.tool_calls != () && type_of(delta.tool_calls) == "array" {
                    for tc in delta.tool_calls {
                        // tool_calls deltas arrive as fragments keyed by `index`.
                        // first fragment carries id+name; subsequent ones append
                        // to `function.arguments`. servers vary, so be permissive.
                        let idx = if tc.index != () { tc.index } else { 0 };
                        let key = idx.to_string();
                        let acc = if tc_by_index[key] != () { tc_by_index[key] } else { #{ id: "", name: "", arguments: "" } };
                        if tc.id != () { acc.id = tc.id; }
                        if tc.function != () {
                            if tc.function.name != () && tc.function.name != "" { acc.name = tc.function.name; }
                            if tc.function.arguments != () { acc.arguments += tc.function.arguments; }
                        }
                        tc_by_index[key] = acc;
                    }
                }
            }
        }
        if ev.data.usage != () { usage = ev.data.usage; }
    }

    // ordered array by numeric index. assumes contiguous from 0; in practice
    // OpenAI emits them that way.
    let tool_calls = [];
    let idx = 0;
    loop {
        let key = idx.to_string();
        if tc_by_index[key] == () { break; }
        tool_calls.push(tc_by_index[key]);
        idx += 1;
    }

    return #{
        content: content,
        thinking: thinking,
        tool_calls: tool_calls,
        usage: usage
    };
}

// non-streaming fallback for one turn. returns the same shape as reassemble_turn.
fn fetch_turn_nonstream(url, upstream_url, body, headers) {
    let resp = http_request("POST", url, body, headers, #{});
    if resp.status != 200 {
        resp = http_request("POST", upstream_url, body, headers, #{});
    }
    if resp.status != 200 {
        throw "API Error " + resp.status + ": " + to_string(resp.body);
    }
    let content = "";
    let thinking = "";
    let tool_calls = [];
    let usage = #{};
    let choices = resp.body.choices;
    if choices != () && type_of(choices) == "array" && choices.len > 0 {
        let msg = choices[0].message;
        if msg != () {
            if msg.content != () && msg.content != "" { content = msg.content; }
            if msg.reasoning_content != () { thinking = msg.reasoning_content; }
            if msg.tool_calls != () && type_of(msg.tool_calls) == "array" {
                for tc in msg.tool_calls {
                    let entry = #{ id: "", name: "", arguments: "" };
                    if tc.id != () { entry.id = tc.id; }
                    if tc.function != () {
                        if tc.function.name != () { entry.name = tc.function.name; }
                        if tc.function.arguments != () { entry.arguments = tc.function.arguments; }
                    }
                    tool_calls.push(entry);
                }
            }
        }
    }
    if resp.body.usage != () { usage = resp.body.usage; }
    return #{ content: content, thinking: thinking, tool_calls: tool_calls, usage: usage };
}

// accumulate usage stats across tool-call iterations. mutates `acc` in place.
fn accumulate_usage(acc, turn_usage) {
    if turn_usage == () || type_of(turn_usage) != "map" { return; }
    for k in turn_usage.keys() {
        let v = turn_usage[k];
        if type_of(v) == "i64" || type_of(v) == "int" {
            let prev = if acc[k] != () { acc[k] } else { 0 };
            acc[k] = prev + v;
        } else if acc[k] == () {
            acc[k] = v;
        }
    }
}

fn execute(inputs) {
    let api_base = inputs.api_base;
    if api_base.len > 0 && api_base.sub_string(api_base.len - 1) == "/" {
        api_base = api_base.sub_string(0, api_base.len - 1);
    }

    let api_key = inputs.api_key;

    let headers = #{
        "Content-Type": "application/json",
        "Authorization": "Bearer " + api_key
    };

    let messages = [
        #{ role: "system", content: inputs.system_prompt },
        #{ role: "user", content: inputs.prompt }
    ];

    // build base request body with shared params
    let base_body = #{ model: inputs.model };
    if inputs.temperature != () { base_body.temperature = inputs.temperature; }
    if inputs.seed != () { base_body.seed = inputs.seed; }
    if inputs.max_tokens != () { base_body.max_tokens = inputs.max_tokens; }
    if inputs.thinking_budget_tokens != () { base_body.thinking_budget_tokens = inputs.thinking_budget_tokens; }
    if inputs.json_schema != () && inputs.json_schema != "" {
        let schema = inputs.json_schema;
        if type_of(schema) == "string" { schema = parse_json(schema); }
        base_body.response_format = #{ "type": "json_object", schema: schema };
    }

    // build tool definitions if any tools requested. an empty/missing `tools`
    // input degrades to the original (tool-free) chat-completion path.
    let tool_names = [];
    if inputs.tools != () && type_of(inputs.tools) == "array" {
        for t in inputs.tools {
            if type_of(t) == "string" && t != "" { tool_names.push(t); }
        }
    }
    let has_tools = tool_names.len > 0;
    if has_tools {
        let tool_defs = [];
        for tn in tool_names {
            tool_defs.push(node_to_openai_tool(tn));
        }
        base_body.tools = tool_defs;
        base_body.tool_choice = "auto";
        log("openai_llm: agent loop enabled with " + tool_names.len + " tool(s): " + to_string(tool_names));
    }

    let server_base = api_base;
    if api_base.len >= 3 && api_base.sub_string(api_base.len - 3) == "/v1" {
        server_base = api_base.sub_string(0, api_base.len - 3);
    }
    let url = api_base + "/chat/completions";
    let upstream_url = server_base + "/upstream/" + inputs.model + "/v1/chat/completions";

    let sse_options = #{
        emit: [
            #{ event: "message", path: "/choices/0/delta/content", output: "text" },
            #{ event: "message", path: "/choices/0/delta/reasoning_content", output: "thinking" }
        ]
    };

    let final_content = "";
    let final_thinking = "";
    let cumulative_usage = #{};
    let total_tool_calls = 0;
    // 0 (or unset) = unlimited turns. otherwise decrements each round and we
    // break out when it goes negative.
    let max_t = if inputs.max_turns != () { inputs.max_turns } else { 10 };
    let unlimited_turns = max_t <= 0;
    let budget = max_t;
    let iteration = 0;

    // truncate long values for log readability (preserves head + tail)
    let log_truncate = |s, n| {
        let str = to_string(s);
        if str.len <= n { return str; }
        return str.sub_string(0, n) + "...(" + (str.len - n) + " more)";
    };

    // emit the initial system + user messages before any LLM round-trip, so
    // downstream nodes show the prompt context immediately rather than only
    // after the first assistant turn arrives.
    emit_output_value("messages", messages);

    loop {
        iteration += 1;
        if has_tools {
            let budget_str = if unlimited_turns { "unlimited" } else { to_string(budget) };
            log("openai_llm: turn " + iteration + " (budget=" + budget_str + ") messages=" + messages.len);
        }
        // assemble per-iteration body. `messages` grows as the conversation does.
        let body = base_body;
        body.messages = messages;

        // streaming path: content + thinking emit live; tool_calls reassembled
        // from the events array after the stream completes (partial JSON
        // argument fragments would be useless to a UI listener).
        let sse_body = body;
        sse_body.stream = true;
        sse_body.stream_options = #{ include_usage: true };

        let response = http_request_sse("POST", url, sse_body, headers, sse_options);
        if response.status != 200 {
            log("sse request failed (" + response.status + "): " + to_string(response.body) + ", trying upstream sse: " + upstream_url);
            response = http_request_sse("POST", upstream_url, sse_body, headers, sse_options);
        }

        let turn = #{};
        if response.status == 200 {
            turn = reassemble_turn(response.events);
        } else {
            log("sse upstream also failed (" + response.status + "): " + to_string(response.body) + ", falling back to non-streaming");
            turn = fetch_turn_nonstream(url, upstream_url, body, headers);
        }

        accumulate_usage(cumulative_usage, turn.usage);

        // append assistant turn to the running transcript. content +
        // reasoning_content are both stored per-turn so the `messages` output
        // gives callers full intermediate context. tool_calls are stored
        // verbatim in the structure the server expects on the next request.
        let assistant_msg = #{ role: "assistant", content: turn.content };
        if turn.thinking != () && turn.thinking != "" {
            assistant_msg.reasoning_content = turn.thinking;
        }
        if turn.tool_calls.len > 0 {
            let oa_tool_calls = [];
            for tc in turn.tool_calls {
                oa_tool_calls.push(#{
                    id: tc.id,
                    "type": "function",
                    function: #{ name: tc.name, arguments: tc.arguments }
                });
            }
            assistant_msg.tool_calls = oa_tool_calls;
        }
        messages.push(assistant_msg);
        // stream the running transcript so downstream nodes see assistant
        // turns as they complete (mirrors how tool_calls streams below).
        emit_output_value("messages", messages);

        // terminal turn: no tool calls — this is the final answer.
        if turn.tool_calls.len == 0 {
            final_content = turn.content;
            final_thinking = turn.thinking;
            if has_tools {
                log("openai_llm: agent done after " + iteration + " turn(s), " + total_tool_calls + " tool call(s)");
            }
            break;
        }

        // summarize the model's tool-call request before dispatch
        let tc_names = [];
        for tc in turn.tool_calls { tc_names.push(tc.name); }
        log("openai_llm: model requested " + turn.tool_calls.len + " tool call(s): " + to_string(tc_names));

        if !unlimited_turns {
            budget -= 1;
            if budget < 0 {
                throw "openai_llm: max_turns (" + max_t + ") exhausted after " + iteration + " turn(s) with " + total_tool_calls + " tool call(s); model still requesting tools. raise `max_turns` (0 = unlimited) or simplify the task.";
            }
        }

        // dispatch each tool call, append the result as a `tool` message.
        let call_idx = 0;
        for tc in turn.tool_calls {
            call_idx += 1;
            let args = #{};
            if tc.arguments != () && tc.arguments != "" {
                try {
                    args = parse_json(tc.arguments);
                } catch (e) {
                    args = #{};
                    log("openai_llm: [turn " + iteration + "/call " + call_idx + "] failed to parse tool arguments for '" + tc.name + "': " + to_string(e) + " raw=" + log_truncate.call(tc.arguments, 200));
                }
            }
            log("openai_llm: [turn " + iteration + "/call " + call_idx + "] -> " + tc.name + "(" + log_truncate.call(args, 300) + ")");
            let result = ();
            let failed = false;
            try {
                result = dispatch_node(tc.name, args);
            } catch (e) {
                result = #{ error: to_string(e) };
                failed = true;
            }
            if failed {
                log("openai_llm: [turn " + iteration + "/call " + call_idx + "] <- " + tc.name + " FAILED: " + log_truncate.call(result.error, 300));
            } else {
                log("openai_llm: [turn " + iteration + "/call " + call_idx + "] <- " + tc.name + " ok result=" + log_truncate.call(result, 300));
            }
            total_tool_calls += 1;
            // tool result must be a string in the OpenAI protocol.
            let content_str = "";
            if type_of(result) == "string" {
                content_str = result;
            } else {
                content_str = to_string(result);
            }
            messages.push(#{
                role: "tool",
                tool_call_id: tc.id,
                content: content_str
            });
            emit_output_value("messages", messages);
        }
    }

    return #{
        text: final_content,
        thinking: final_thinking,
        usage: cumulative_usage,
        messages: messages
    };
}

fn get_options(input_name, inputs) {
    if input_name == "tools" {
        // expose every registered node as an eligible tool. excludes
        // OpenAI_LLM itself to avoid the agent calling itself recursively.
        let nodes = list_nodes();
        let options = [];
        for n in nodes {
            if n.name == "OpenAI_LLM" { continue; }
            let label = n.name;
            if n.title != () && n.title != "" && n.title != n.name {
                label = n.name + " — " + n.title;
            }
            options.push(#{ value: n.name, label: label });
        }
        return options;
    }

    if input_name == "model" {
        let api_base = inputs.api_base;
        let api_key = inputs.api_key;

        if api_base == "" {
            return [];
        }

        // Normalize URL
        if api_base.len > 0 && api_base.sub_string(api_base.len - 1) == "/" {
            api_base = api_base.sub_string(0, api_base.len - 1);
        }

        let headers = #{};
        if api_key != () && api_key != "" {
            headers["Authorization"] = "Bearer " + api_key;
        }

        let url = api_base + "/models";
        let response = http_request("GET", url, #{}, headers, #{});

        if response.status != 200 {
            return [];
        }

        let models_data = response.body.data;
        let options = [];

        for model in models_data {
            let include = true;
            if model.meta != () && model.meta.llamaswap != () {
                let ls = model.meta.llamaswap;
                if ls.modsi != () {
                    let modsi_list = ls.modsi.split(",");
                    if !modsi_list.contains("text") { include = false; }
                }
                if ls.modso != () {
                    let modso_list = ls.modso.split(",");
                    if !modso_list.contains("text") { include = false; }
                }
            }
            if include {
                options.push(#{ value: model.id, label: model.id });
            }
        }
        return options;
    }
    return [];
}
