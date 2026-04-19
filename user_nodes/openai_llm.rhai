fn spec() {
    return #{
        name: "OpenAI_LLM",
        title: "OpenAI Text Generation",
        category: "Generative",
        description: "Query an OpenAI-compatible LLM endpoint",
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
                name: "max_tokens",
                "type": "integer",
                description: "maximum tokens to generate"
            },
            #{
                name: "thinking_budget_tokens",
                "type": "integer",
                description: "max thinking tokens for reasoning models"
            },
            #{
                name: "json_schema",
                "type": "object",
                ui: "textarea",
                description: "JSON schema for structured output (llama.cpp compatible, leave empty for free-form text)"
            }
        ],
        outputs: [
            #{ name: "text", "type": "string", description: "generated response text" },
            #{ name: "thinking", "type": "string", description: "thinking content (if supported)" },
            #{ name: "usage", "type": "object", description: "token usage statistics" }
        ]
    };
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

    let body = #{
        model: inputs.model,
        messages: messages
    };

    if inputs.temperature != () {
        body.temperature = inputs.temperature;
    }

    if inputs.seed != () {
        body.seed = inputs.seed;
    }

    if inputs.max_tokens != () {
        body.max_tokens = inputs.max_tokens;
    }

    if inputs.thinking_budget_tokens != () {
        body.thinking_budget_tokens = inputs.thinking_budget_tokens;
    }

    // Add response_format for structured output if json_schema is provided
    if inputs.json_schema != () && inputs.json_schema != "" {
        // Handle both string (from textarea/Echo) and object (from other nodes) inputs
        let schema = inputs.json_schema;
        if type_of(schema) == "string" {
            schema = parse_json(schema);
        }
        body.response_format = #{
            "type": "json_object",
            schema: schema
        };
    }

    // use streaming to emit partial output as tokens arrive
    body.stream = true;
    body.stream_options = #{ include_usage: true };
    let url = api_base + "/chat/completions";
    let response = http_request_streaming("POST", url, body, headers, #{ output_name: "text" });

    // if failed, try upstream fallback
    if response.status != 200 {
        let server_base = api_base;
        if api_base.len >= 3 && api_base.sub_string(api_base.len - 3) == "/v1" {
            server_base = api_base.sub_string(0, api_base.len - 3);
        }
        let upstream_url = server_base + "/upstream/" + inputs.model + "/v1/chat/completions";
        log("request failed (" + response.status + "): " + response.body + ", trying upstream fallback: " + upstream_url);
        response = http_request_streaming("POST", upstream_url, body, headers, #{ output_name: "text" });
    }

    if response.status != 200 {
        throw "API Error " + response.status + ": " + response.body;
    }

    let choice = response.body.choices[0];
    let content = choice.message.content;
    let thinking = "";

    // Handle "thinking" field if present (e.g. some reasoning models)
    if choice.message.contains("reasoning_content") {
        thinking = choice.message.reasoning_content;
    }

    return #{
        text: content,
        thinking: thinking,
        usage: response.body.usage
    };
}

fn get_options(input_name, inputs) {
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
