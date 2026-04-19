use crate::node::{DataType, InputSpec, Node, NodeContext, OutputSpec, UIComponent};
use crate::value::{FileValue, Value};
use anyhow::Result;
use async_trait::async_trait;
use std::collections::BTreeMap;

#[derive(Default)]
pub struct AudioInputNode;

#[async_trait]
impl Node for AudioInputNode {
    fn name(&self) -> &str {
        "AudioInput"
    }

    fn title(&self) -> &str {
        "Audio Input"
    }

    fn category(&self) -> &str {
        "Input"
    }

    fn description(&self) -> &str {
        "Upload or record audio input"
    }

    fn inputs(&self) -> Vec<InputSpec> {
        vec![InputSpec {
            name: "audio_data".to_string(),
            r#type: DataType::String,
            ui_component: UIComponent::AudioRecorder {},
            default: None,
            required: false,
            description: Some("audio data (base64 or URL)".to_string()),
            ..Default::default()
        }]
    }

    fn outputs(&self) -> Vec<OutputSpec> {
        vec![OutputSpec {
            name: "audio".to_string(),
            r#type: DataType::File,
            description: Some("audio file".to_string()),
        }]
    }

    async fn execute(
        &self,
        inputs: BTreeMap<String, Value>,
        _ctx: NodeContext,
    ) -> Result<BTreeMap<String, Value>> {
        let audio_data = inputs
            .get("audio_data")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if audio_data.is_empty() {
            return Err(anyhow::anyhow!("no audio data provided"));
        }

        // check if it's a base64 data URL or a file path/URL
        let (file_path, url, mime_type) = if audio_data.starts_with("data:") {
            // parse data URL: data:audio/wav;base64,xxxxx
            let parts: Vec<&str> = audio_data.splitn(2, ',').collect();
            if parts.len() != 2 {
                return Err(anyhow::anyhow!("invalid data URL format"));
            }

            let header = parts[0];
            let base64_data = parts[1];

            // extract mime type from header
            let mime = header
                .strip_prefix("data:")
                .and_then(|s| s.strip_suffix(";base64"))
                .unwrap_or("audio/wav");

            // determine file extension from mime type
            let ext = match mime {
                "audio/wav" | "audio/wave" | "audio/x-wav" => "wav",
                "audio/mpeg" | "audio/mp3" => "mp3",
                "audio/ogg" => "ogg",
                "audio/webm" => "webm",
                "audio/flac" => "flac",
                _ => "wav",
            };

            // decode and save the file
            let filename = format!("audio_input_{}.{}", uuid::Uuid::new_v4(), ext);
            let file_path = std::path::Path::new("generated_assets").join(&filename);

            // ensure directory exists
            std::fs::create_dir_all("generated_assets")?;

            let bytes =
                base64::Engine::decode(&base64::engine::general_purpose::STANDARD, base64_data)?;
            std::fs::write(&file_path, bytes)?;

            (
                file_path.to_string_lossy().to_string(),
                format!("/api/assets/{}", filename),
                mime.to_string(),
            )
        } else if audio_data.starts_with("/api/assets/") || audio_data.starts_with("http") {
            // already a URL, extract filename
            let filename = audio_data.rsplit('/').next().unwrap_or("audio.wav");
            let file_path = std::path::Path::new("generated_assets").join(filename);
            (
                file_path.to_string_lossy().to_string(),
                audio_data.to_string(),
                "audio/wav".to_string(),
            )
        } else {
            // assume it's a local file path
            (
                audio_data.to_string(),
                format!(
                    "/api/assets/{}",
                    audio_data.rsplit('/').next().unwrap_or("audio.wav")
                ),
                "audio/wav".to_string(),
            )
        };

        let file_value = Value::File(FileValue {
            path: file_path,
            url,
            mime_type,
        });

        let mut outputs = BTreeMap::new();
        outputs.insert("audio".to_string(), file_value);
        Ok(outputs)
    }
}

#[derive(Default)]
pub struct DisplayImageNode;

#[async_trait]
impl Node for DisplayImageNode {
    fn name(&self) -> &str {
        "DisplayImage"
    }

    fn is_stream_passthrough(&self) -> bool {
        true
    }

    fn title(&self) -> &str {
        "Display Image"
    }

    fn category(&self) -> &str {
        "Output"
    }

    fn description(&self) -> &str {
        "Display an image"
    }

    fn inputs(&self) -> Vec<InputSpec> {
        vec![InputSpec {
            name: "image".to_string(),
            r#type: DataType::File,
            ui_component: UIComponent::Auto {},
            default: None,
            required: true,
            description: Some("image file to display".to_string()),
            ..Default::default()
        }]
    }

    fn outputs(&self) -> Vec<OutputSpec> {
        vec![]
    }

    async fn execute(
        &self,
        inputs: BTreeMap<String, Value>,
        _ctx: NodeContext,
    ) -> Result<BTreeMap<String, Value>> {
        // pass-through execution so the UI can access the data via 'outputs'
        Ok(inputs)
    }
}

#[derive(Default)]
pub struct DisplayMarkdownNode;

#[async_trait]
impl Node for DisplayMarkdownNode {
    fn name(&self) -> &str {
        "DisplayMarkdown"
    }

    fn is_stream_passthrough(&self) -> bool {
        true
    }

    fn title(&self) -> &str {
        "Display Markdown"
    }

    fn category(&self) -> &str {
        "Output"
    }

    fn description(&self) -> &str {
        "Display markdown content"
    }

    fn inputs(&self) -> Vec<InputSpec> {
        vec![InputSpec {
            name: "markdown".to_string(),
            r#type: DataType::String,
            ui_component: UIComponent::Auto {},
            default: None,
            required: true,
            description: Some("markdown to display".to_string()),
            ..Default::default()
        }]
    }

    fn outputs(&self) -> Vec<OutputSpec> {
        vec![]
    }

    async fn execute(
        &self,
        inputs: BTreeMap<String, Value>,
        _ctx: NodeContext,
    ) -> Result<BTreeMap<String, Value>> {
        // pass-through execution so the UI can access the data via 'outputs'
        Ok(inputs)
    }
}

#[derive(Default)]
pub struct DisplayAudioNode;

#[async_trait]
impl Node for DisplayAudioNode {
    fn name(&self) -> &str {
        "DisplayAudio"
    }

    fn is_stream_passthrough(&self) -> bool {
        true
    }

    fn title(&self) -> &str {
        "Display Audio"
    }

    fn category(&self) -> &str {
        "Output"
    }

    fn description(&self) -> &str {
        "Display an audio player"
    }

    fn inputs(&self) -> Vec<InputSpec> {
        vec![InputSpec {
            name: "audio".to_string(),
            r#type: DataType::File,
            ui_component: UIComponent::Auto {},
            default: None,
            required: true,
            description: Some("audio file to play".to_string()),
            ..Default::default()
        }]
    }

    fn outputs(&self) -> Vec<OutputSpec> {
        vec![]
    }

    async fn execute(
        &self,
        inputs: BTreeMap<String, Value>,
        _ctx: NodeContext,
    ) -> Result<BTreeMap<String, Value>> {
        // pass-through execution so the UI can access the data via 'outputs'
        Ok(inputs)
    }
}

#[derive(Default)]
pub struct DisplayJsonNode;

#[async_trait]
impl Node for DisplayJsonNode {
    fn name(&self) -> &str {
        "DisplayJson"
    }

    fn is_stream_passthrough(&self) -> bool {
        true
    }

    fn title(&self) -> &str {
        "Display JSON"
    }

    fn category(&self) -> &str {
        "Output"
    }

    fn description(&self) -> &str {
        "Display JSON content with collapsible/expandable objects"
    }

    fn inputs(&self) -> Vec<InputSpec> {
        vec![InputSpec {
            name: "json".to_string(),
            r#type: DataType::Object,
            ui_component: UIComponent::Text {},
            default: None,
            required: true,
            description: Some("JSON object to display".to_string()),
            ..Default::default()
        }]
    }

    fn outputs(&self) -> Vec<OutputSpec> {
        vec![]
    }

    async fn execute(
        &self,
        inputs: BTreeMap<String, Value>,
        _ctx: NodeContext,
    ) -> Result<BTreeMap<String, Value>> {
        // pass-through execution so the UI can access the data via 'outputs'
        Ok(inputs)
    }
}

#[derive(Default)]
pub struct DisplayListNode;

#[async_trait]
impl Node for DisplayListNode {
    fn name(&self) -> &str {
        "List"
    }

    fn title(&self) -> &str {
        "List"
    }

    fn category(&self) -> &str {
        "Data"
    }

    fn description(&self) -> &str {
        "Edit or view a list of items"
    }

    fn inputs(&self) -> Vec<InputSpec> {
        vec![InputSpec {
            name: "list".to_string(),
            r#type: DataType::List,
            ui_component: UIComponent::Auto {},
            default: None,
            required: true,
            description: Some("list to display".to_string()),
            ..Default::default()
        }]
    }

    fn outputs(&self) -> Vec<OutputSpec> {
        vec![OutputSpec {
            name: "list".to_string(),
            r#type: DataType::List,
            description: Some("list (pass-through)".to_string()),
        }]
    }

    async fn execute(
        &self,
        inputs: BTreeMap<String, Value>,
        _ctx: NodeContext,
    ) -> Result<BTreeMap<String, Value>> {
        let list = inputs.get("list").cloned().unwrap_or(Value::Array(vec![]));
        let mut outputs = BTreeMap::new();
        outputs.insert("list".to_string(), list);
        Ok(outputs)
    }
}
