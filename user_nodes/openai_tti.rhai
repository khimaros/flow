fn spec() {
    return #{
        name: "OpenAI_TTI",
        title: "OpenAI Image Generation",
        category: "Generative",
        description: "Generate images using an OpenAI-compatible image generation endpoint",
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
                required: false,
                description: "API key (optional for local servers)",
                env_var: "OPENAI_API_KEY"
            },
            #{
                name: "model",
                "type": "string",
                ui: "dynamic_select",
                "default": "dall-e-3",
                description: "image model ID",
                depends_on: ["api_base", "api_key"]
            },
            #{
                name: "prompt",
                "type": "string",
                ui: "textarea",
                required: true,
                description: "image prompt"
            },
            #{
                name: "negative_prompt",
                "type": "string",
                ui: "textarea",
                "default": "",
                description: "negative prompt"
            },
            #{
                name: "steps",
                "type": "integer",
                "default": 10,
                description: "sampling steps"
            },
            #{
                name: "cfg_scale",
                "type": "float",
                "default": 1.0,
                description: "CFG scale"
            },
            #{
                name: "width",
                "type": "integer",
                "default": 512,
                description: "image width"
            },
            #{
                name: "height",
                "type": "integer",
                "default": 512,
                description: "image height"
            },
            #{
                name: "seed",
                "type": "integer",
                "default": -1,
                description: "random seed (-1 for random)"
            }
        ],
        outputs: [
            #{ name: "image", "type": "file", description: "generated image" }
        ]
    };
}

fn execute(inputs) {
    let api_base = inputs.api_base;
    if api_base.len > 0 && api_base.sub_string(api_base.len - 1) == "/" {
        api_base = api_base.sub_string(0, api_base.len - 1);
    }

    let headers = #{
        "Content-Type": "application/json"
    };

    if inputs.api_key != () && inputs.api_key != "" {
        headers["Authorization"] = "Bearer " + inputs.api_key;
    }

    // Format prompt with embedded parameters for stable-diffusion.cpp server
    // Ref: https://github.com/leejet/stable-diffusion.cpp/blob/master/examples/server/main.cpp
    // The server parses <sd_cpp_extra_args> JSON for advanced params
    let extra_args = #{
        cfg_scale: inputs.cfg_scale,
        steps: inputs.steps,
        width: inputs.width,
        height: inputs.height
    };

    if inputs.negative_prompt != "" {
        extra_args["negative_prompt"] = inputs.negative_prompt;
    }

    if inputs.seed != -1 {
        extra_args["seed"] = inputs.seed;
    }

    let extra_args_json = to_json(extra_args);

    // stable-diffusion.cpp server expects: "prompt <sd_cpp_extra_args>{json}</sd_cpp_extra_args>"
    let full_prompt = inputs.prompt + " <sd_cpp_extra_args>" + extra_args_json + "</sd_cpp_extra_args>";

    let body = #{
        model: inputs.model,
        prompt: full_prompt,
        n: 1,
        size: inputs.width.to_string() + "x" + inputs.height.to_string(),
        response_format: "b64_json"
    };

    let options = #{
        timeout: 600,
        retries: 3
    };

    // Try api_base first
    let url = api_base + "/images/generations";
    let response = http_request("POST", url, body, headers, options);

    // If failed, try upstream fallback: /upstream/{model}/v1/images/generations
    if response.status != 200 {
        let server_base = api_base;
        if api_base.len >= 3 && api_base.sub_string(api_base.len - 3) == "/v1" {
            server_base = api_base.sub_string(0, api_base.len - 3);
        }
        let upstream_url = server_base + "/upstream/" + inputs.model + "/v1/images/generations";
        log("request failed (" + response.status + "): " + to_string(response.body) + ", trying upstream fallback: " + upstream_url);
        response = http_request("POST", upstream_url, body, headers, options);
    }

    if response.status != 200 {
        throw "API Error " + response.status + ": " + to_string(response.body);
    }

    let b64_data = response.body.data[0].b64_json;

    let filename = "tti_" + uuid_v4() + ".png";
    let file_path = save_asset_base64(filename, b64_data);

    return #{
        image: #{
            path: file_path,
            url: "/api/assets/" + filename,
            mime_type: "image/png"
        }
    };
}

fn get_options(input_name, inputs) {
    if input_name == "model" {
        let api_base = inputs.api_base;
        if api_base == "" {
            return [];
        }

        // Normalize URL
        if api_base.len > 0 && api_base.sub_string(api_base.len - 1) == "/" {
            api_base = api_base.sub_string(0, api_base.len - 1);
        }

        let headers = #{};
        if inputs.api_key != () && inputs.api_key != "" {
            headers["Authorization"] = "Bearer " + inputs.api_key;
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
                    if !modso_list.contains("image") { include = false; }
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
