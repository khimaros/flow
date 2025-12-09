fn spec() {
    return #{
        name: "OpenAI_Voice_Design",
        title: "OpenAI Voice Design",
        category: "Generative",
        description: "Create a custom voice on an OpenAI-compatible TTS server (e.g. qwen3-tts) from a reference audio sample. Returns a voice ID usable with OpenAI_TTS.",
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
                "default": "qwen3-tts",
                description: "TTS Model ID",
                depends_on: ["api_base", "api_key"],
                env_var: "FLOW_OPENAI_TTS_MODEL"
            },
            #{
                name: "name",
                "type": "string",
                "default": "custom",
                description: "Display name for the new voice"
            },
            #{
                name: "audio_sample",
                "type": "file",
                required: true,
                description: "Reference audio file (WAV, 24kHz preferred) to clone"
            },
            #{
                name: "ref_text",
                "type": "string",
                ui: "textarea",
                required: false,
                description: "Transcript of the reference audio (enables ICL voice cloning mode)"
            },
            #{
                name: "seed",
                "type": "integer",
                description: "Random seed (optional)"
            }
        ],
        outputs: [
            #{ name: "voice_id", "type": "string", description: "ID of the created voice (use as 'voice' input to OpenAI_TTS)" },
            #{ name: "name", "type": "string", description: "Display name of the created voice" },
            #{ name: "mode", "type": "string", description: "Voice cloning mode: 'icl' if ref_text was given, otherwise 'embedding'" },
            #{ name: "ref_frames", "type": "integer", description: "Number of encoded reference frames (ICL mode only)" }
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

    // Resolve audio file path from input (file map or string)
    let audio_input = inputs.audio_sample;
    let audio_path = "";
    let audio_filename = "reference.wav";
    let audio_mime = "audio/wav";

    if type_of(audio_input) == "map" {
        audio_path = audio_input.path;
        if audio_input.mime_type != () {
            audio_mime = audio_input.mime_type;
        }
        let parts = audio_path.split("/");
        if parts.len > 0 {
            audio_filename = parts[parts.len - 1];
        }
    } else if type_of(audio_input) == "string" {
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
        throw "no audio_sample file provided";
    }

    let audio_b64 = read_file_base64(audio_path);

    let fields = #{
        audio_sample: #{
            file_base64: audio_b64,
            filename: audio_filename,
            mime_type: audio_mime
        },
        name: inputs.name,
        model: inputs.model
    };

    if inputs.ref_text != () && inputs.ref_text != "" {
        fields.ref_text = inputs.ref_text;
    }

    if inputs.seed != () {
        fields.seed = inputs.seed;
    }

    let options = #{
        timeout: 300
    };

    // Try api_base first
    let url = api_base + "/audio/voices";
    let response = http_request_multipart(url, fields, headers, options);

    // Upstream fallback: /upstream/{model}/v1/audio/voices
    if response.status != 200 {
        let server_base = api_base;
        if api_base.len >= 3 && api_base.sub_string(api_base.len - 3) == "/v1" {
            server_base = api_base.sub_string(0, api_base.len - 3);
        }
        let upstream_url = server_base + "/upstream/" + inputs.model + "/v1/audio/voices";
        log("request failed (" + response.status + "): " + to_string(response.body) + ", trying upstream fallback: " + upstream_url);
        response = http_request_multipart(upstream_url, fields, headers, options);
    }

    if response.status != 200 {
        throw "Create Voice API Error " + response.status + ": " + to_string(response.body);
    }

    let body = response.body;
    let voice_id = "";
    let voice_name = inputs.name;
    let mode = "embedding";
    let ref_frames = 0;

    if type_of(body) == "map" {
        if body.id != () { voice_id = body.id; }
        if body.name != () { voice_name = body.name; }
        if body.mode != () { mode = body.mode; }
        if body.ref_frames != () { ref_frames = body.ref_frames; }
    }

    if voice_id == "" {
        throw "Create Voice API returned no voice id: " + to_string(body);
    }

    return #{
        voice_id: voice_id,
        name: voice_name,
        mode: mode,
        ref_frames: ref_frames
    };
}

fn get_options(input_name, inputs) {
    if input_name == "model" {
        let api_base = inputs.api_base;
        if api_base == "" {
            return [];
        }

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
                    if !modso_list.contains("audio") { include = false; }
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
