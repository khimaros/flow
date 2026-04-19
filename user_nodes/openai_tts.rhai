fn spec() {
    return #{
        name: "OpenAI_TTS",
        title: "OpenAI Text to Speech",
        category: "Generative",
        description: "Convert text to speech using an OpenAI-compatible TTS endpoint",
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
                "default": "kokoro-v1",
                description: "TTS model ID",
                depends_on: ["api_base", "api_key"]
            },
            #{
                name: "voice",
                "type": "string",
                ui: "dynamic_select",
                "default": "am_adam",
                description: "voice ID",
                depends_on: ["api_base", "api_key", "model"]
            },
            #{
                name: "input",
                "type": "string",
                ui: "textarea",
                required: true,
                description: "text to convert to speech"
            },
            #{
                name: "language",
                "type": "string",
                "default": "",
                description: "language code (e.g., 'en', 'es', 'fr') - leave empty for auto-detect"
            },
            #{
                name: "response_format",
                "type": "string",
                ui: "select",
                "default": "wav",
                description: "audio output format",
                options: [
                    #{ value: "wav", label: "WAV" },
                    #{ value: "mp3", label: "MP3" },
                    #{ value: "opus", label: "Opus" },
                    #{ value: "flac", label: "FLAC" }
                ]
            },
            #{
                name: "temperature",
                "type": "float",
                description: "sampling temperature"
            },
            #{
                name: "speed",
                "type": "float",
                "default": 1.0,
                description: "speech speed (0.25 to 4.0)"
            },
            #{
                name: "instructions",
                "type": "string",
                ui: "textarea",
                required: false,
                description: "instructions to guide speech style and tone"
            },
            #{
                name: "seed",
                "type": "integer",
                description: "random seed"
            }
        ],
        outputs: [
            #{ name: "audio", "type": "file", description: "generated audio file" }
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

    let body = #{
        model: inputs.model,
        voice: inputs.voice,
        input: inputs.input,
        response_format: inputs.response_format,
        speed: inputs.speed
    };

    if inputs.temperature != () {
        body.temperature = inputs.temperature;
    }

    if inputs.instructions != () && inputs.instructions != "" {
        body.instructions = inputs.instructions;
    }

    if inputs.language != () && inputs.language != "" {
        body.language = inputs.language;
    }

    if inputs.seed != () {
        body.seed = inputs.seed;
    }

    let options = #{
        timeout: 1200,
        retries: 2
    };

    // Try api_base first
    let url = api_base + "/audio/speech";
    let response = http_request_binary("POST", url, body, headers, options);

    // If failed, try upstream fallback: /upstream/{model}/v1/audio/speech
    if response.status != 200 {
        let server_base = api_base;
        if api_base.len >= 3 && api_base.sub_string(api_base.len - 3) == "/v1" {
            server_base = api_base.sub_string(0, api_base.len - 3);
        }
        let upstream_url = server_base + "/upstream/" + inputs.model + "/v1/audio/speech";
        let error_body = decode_base64_text(response.body_base64);
        log("request failed (" + response.status + "): " + error_body + ", trying upstream fallback: " + upstream_url);
        response = http_request_binary("POST", upstream_url, body, headers, options);
    }

    if response.status != 200 {
        let error_body = decode_base64_text(response.body_base64);
        throw "TTS API Error " + response.status + ": " + error_body;
    }

    // Determine file extension from format
    let ext = inputs.response_format;
    let mime_type = "audio/" + ext;
    if ext == "mp3" {
        mime_type = "audio/mpeg";
    }

    // Save the audio file
    let filename = "tts_" + uuid_v4() + "." + ext;
    let file_path = save_asset_base64(filename, response.body_base64);

    return #{
        audio: #{
            path: file_path,
            url: "/api/assets/" + filename,
            mime_type: mime_type
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
                    if !modso_list.contains("audio") { include = false; }
                }
            }
            if include {
                options.push(#{ value: model.id, label: model.id });
            }
        }
        return options;
    }

    if input_name == "voice" {
        let api_base = inputs.api_base;
        let model = inputs.model;

        if api_base == "" || model == "" {
            return [];
        }

        // Normalize URL
        if api_base.len > 0 && api_base.sub_string(api_base.len - 1) == "/" {
            api_base = api_base.sub_string(0, api_base.len - 1);
        }

        // Get server base by stripping /v1 suffix
        let server_base = api_base;
        if api_base.len >= 3 && api_base.sub_string(api_base.len - 3) == "/v1" {
            server_base = api_base.sub_string(0, api_base.len - 3);
        }

        let headers = #{};
        if inputs.api_key != () && inputs.api_key != "" {
            headers["Authorization"] = "Bearer " + inputs.api_key;
        }

        // Try api_base first
        let url = api_base + "/audio/voices";
        log("fetching voices from: " + url);
        let response = http_request("GET", url, #{}, headers, #{});

        if response.status != 200 {
            // Try upstream fallback: /upstream/{model}/v1/audio/voices
            let url2 = server_base + "/upstream/" + model + "/v1/audio/voices";
            log("request failed (" + response.status + "), trying upstream fallback: " + url2);
            response = http_request("GET", url2, #{}, headers, #{});

            if response.status != 200 {
                log("voice fetch failed: " + response.status);
                return [];
            }
        }

        let options = [];

        // Handle different response formats
        let body = response.body;

        // Format 1: { voices: [...] } or { items: [...] } (Mistral-style paginated)
        let voice_array = ();
        if body.voices != () && type_of(body.voices) == "array" {
            voice_array = body.voices;
        } else if body.items != () && type_of(body.items) == "array" {
            voice_array = body.items;
        }

        if voice_array != () {
            for voice in voice_array {
                if type_of(voice) == "string" {
                    options.push(#{ value: voice, label: voice });
                } else if type_of(voice) == "map" {
                    let id = voice.id;
                    if id == () { id = voice.voice_id; }
                    if id == () { id = voice.name; }
                    let label = voice.name;
                    if label == () { label = id; }
                    options.push(#{ value: id, label: label });
                }
            }
        }
        // Format 2: { "ModelName": ["voice1", "voice2", ...], ... }
        else if type_of(body) == "map" {
            for key in body.keys() {
                let voices_array = body[key];
                if type_of(voices_array) == "array" {
                    for voice in voices_array {
                        if type_of(voice) == "string" {
                            options.push(#{ value: voice, label: voice });
                        }
                    }
                }
            }
        }
        // Format 3: ["voice1", "voice2", ...]
        else if type_of(body) == "array" {
            for voice in body {
                if type_of(voice) == "string" {
                    options.push(#{ value: voice, label: voice });
                }
            }
        }

        return options;
    }

    return [];
}
