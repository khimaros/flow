fn spec() {
    return #{
        name: "OpenAI_STT",
        title: "OpenAI Speech to Text",
        category: "Generative",
        description: "Convert speech to text using an OpenAI-compatible Whisper endpoint.",
        inputs: [
            #{
                name: "api_base",
                "type": "string",
                "default": "https://api.openai.com/v1",
                description: "API Base URL (overridden by OPENAI_API_BASE env var)",
                env_var: "OPENAI_API_BASE"
            },
            #{
                name: "api_key",
                "type": "string",
                ui: "password",
                required: false,
                description: "API Key (optional for local servers, overridden by OPENAI_API_KEY env var)",
                env_var: "OPENAI_API_KEY"
            },
            #{
                name: "model",
                "type": "string",
                ui: "dynamic_select",
                "default": "whisper-large-v3-turbo",
                description: "Whisper Model ID",
                depends_on: ["api_base", "api_key"]
            },
            #{
                name: "audio",
                "type": "file",
                required: true,
                description: "Audio file to transcribe (from AudioInput or other node)"
            },
            #{
                name: "language",
                "type": "string",
                "default": "",
                description: "Language code (e.g., 'en', 'es', 'fr') - leave empty for auto-detect"
            },
            #{
                name: "prompt",
                "type": "string",
                ui: "textarea",
                "default": "",
                description: "Optional prompt to guide transcription style"
            },
            #{
                name: "seed",
                "type": "integer",
                description: "Random seed (optional)"
            }
        ],
        outputs: [
            #{ name: "text", "type": "string", description: "Transcribed text" },
            #{ name: "language", "type": "string", description: "Detected language" },
            #{ name: "duration", "type": "float", description: "Audio duration in seconds" },
            #{ name: "json", "type": "object", description: "Full API response with segments and timing" }
        ]
    };
}

fn execute(inputs) {
    let api_base = inputs.api_base;
    if api_base.len > 0 && api_base.sub_string(api_base.len - 1) == "/" {
        api_base = api_base.sub_string(0, api_base.len - 1);
    }

    let headers = #{};
    if inputs.api_key != () && inputs.api_key != "" {
        headers["Authorization"] = "Bearer " + inputs.api_key;
    }

    // Get audio file path from input
    let audio_input = inputs.audio;
    let audio_path = "";
    let audio_filename = "audio.wav";
    let audio_mime = "audio/wav";

    // Handle different input formats
    if type_of(audio_input) == "map" {
        audio_path = audio_input.path;
        if audio_input.mime_type != () {
            audio_mime = audio_input.mime_type;
        }
        // Extract filename from path
        let parts = audio_path.split("/");
        if parts.len > 0 {
            audio_filename = parts[parts.len - 1];
        }
    } else if type_of(audio_input) == "string" {
        // Try to parse as JSON first
        if audio_input.starts_with("{") {
            let parsed = parse_json(audio_input);
            audio_path = parsed.path;
            if parsed.mime_type != () {
                audio_mime = parsed.mime_type;
            }
        } else {
            audio_path = audio_input;
        }
        let parts = audio_path.split("/");
        if parts.len > 0 {
            audio_filename = parts[parts.len - 1];
        }
    }

    if audio_path == "" {
        throw "no audio file provided";
    }

    // Read audio file as base64
    let audio_b64 = read_file_base64(audio_path);

    // Build multipart form data (always use verbose_json for full response)
    let fields = #{
        file: #{
            file_base64: audio_b64,
            filename: audio_filename,
            mime_type: audio_mime
        },
        model: inputs.model,
        response_format: "verbose_json"
    };

    if inputs.language != () && inputs.language != "" {
        fields.language = inputs.language;
    }

    if inputs.prompt != () && inputs.prompt != "" {
        fields.prompt = inputs.prompt;
    }

    if inputs.seed != () {
        fields.seed = inputs.seed;
    }

    let options = #{
        timeout: 300  // 5 minutes for long audio files
    };

    // Try api_base first
    let url = api_base + "/audio/transcriptions";
    let response = http_request_multipart(url, fields, headers, options);

    // If failed, try upstream fallback: /upstream/{model}/v1/audio/transcriptions
    if response.status != 200 {
        let server_base = api_base;
        if api_base.len >= 3 && api_base.sub_string(api_base.len - 3) == "/v1" {
            server_base = api_base.sub_string(0, api_base.len - 3);
        }
        let upstream_url = server_base + "/upstream/" + inputs.model + "/v1/audio/transcriptions";
        log("request failed (" + response.status + "): " + to_string(response.body) + ", trying upstream fallback: " + upstream_url);
        response = http_request_multipart(upstream_url, fields, headers, options);
    }

    if response.status != 200 {
        throw "STT API Error " + response.status + ": " + to_string(response.body);
    }

    // Parse verbose_json response
    let text = "";
    let detected_language = "";
    let duration = 0.0;
    let full_json = #{};

    let body = response.body;

    if type_of(body) == "map" {
        full_json = body;
        if body.text != () {
            text = body.text;
            text.replace("\r\n", " ");
            text.replace("\n", " ");
            text.replace("\r", " ");
            while text.contains("  ") {
                text.replace("  ", " ");
            }
            text.trim();
        }
        if body.language != () {
            detected_language = body.language;
        }
        if body.duration != () {
            duration = body.duration;
        }
    } else {
        text = to_string(body);
        full_json = #{ text: text };
    }

    return #{
        text: text,
        language: detected_language,
        duration: duration,
        json: full_json
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
                    if !modsi_list.contains("audio") { include = false; }
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
