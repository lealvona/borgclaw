use super::{
    audio_format, get_required_string, get_u64, number_property, resolve_env_reference,
    resolve_tts_config, resolve_url_shortener_provider, resolve_workspace_path, string_property,
    Tool, ToolResult, ToolRuntime, ToolSchema,
};
use std::collections::HashMap;

pub fn register(tools: &mut Vec<Tool>) {
    tools.extend([
        Tool::new("stt_transcribe", "Transcribe audio to text")
            .with_schema(ToolSchema::object(
                [
                    ("path".to_string(), string_property("Audio file path")),
                    ("format".to_string(), string_property("Audio format")),
                ]
                .into(),
                vec!["path".to_string()],
            ))
            .with_tags(vec!["stt".to_string(), "integration".to_string()]),
        Tool::new("stt_transcribe_url", "Transcribe audio from URL")
            .with_schema(ToolSchema::object(
                [
                    ("url".to_string(), string_property("Audio file URL")),
                    ("format".to_string(), string_property("Audio format")),
                ]
                .into(),
                vec!["url".to_string()],
            ))
            .with_tags(vec!["stt".to_string(), "integration".to_string()]),
        Tool::new("tts_list_voices", "List available TTS voices")
            .with_schema(ToolSchema::object(HashMap::new(), Vec::new()))
            .with_tags(vec!["tts".to_string(), "integration".to_string()]),
        Tool::new("tts_speak", "Synthesize speech from text")
            .with_schema(ToolSchema::object(
                [("text".to_string(), string_property("Text to synthesize"))].into(),
                vec!["text".to_string()],
            ))
            .with_tags(vec!["tts".to_string(), "integration".to_string()]),
        Tool::new("tts_speak_stream", "Stream speech synthesis to file")
            .with_schema(ToolSchema::object(
                [
                    ("text".to_string(), string_property("Text to synthesize")),
                    (
                        "output_path".to_string(),
                        string_property("Output file path within workspace"),
                    ),
                ]
                .into(),
                vec!["text".to_string(), "output_path".to_string()],
            ))
            .with_tags(vec!["tts".to_string(), "integration".to_string()]),
        Tool::new("image_generate", "Generate an image from a prompt")
            .with_schema(ToolSchema::object(
                [
                    ("prompt".to_string(), string_property("Image prompt")),
                    (
                        "width".to_string(),
                        number_property("Image width", serde_json::json!(1024)),
                    ),
                    (
                        "height".to_string(),
                        number_property("Image height", serde_json::json!(1024)),
                    ),
                ]
                .into(),
                vec!["prompt".to_string()],
            ))
            .with_tags(vec!["image".to_string(), "integration".to_string()]),
        Tool::new("image_analyze", "Analyze an image from URL using vision AI")
            .with_schema(ToolSchema::object(
                [
                    (
                        "image_url".to_string(),
                        string_property("URL of the image to analyze"),
                    ),
                    (
                        "prompt".to_string(),
                        string_property("Question or prompt about the image"),
                    ),
                ]
                .into(),
                vec!["image_url".to_string(), "prompt".to_string()],
            ))
            .with_tags(vec!["image".to_string(), "integration".to_string()]),
        Tool::new(
            "image_analyze_file",
            "Analyze a local image file using vision AI",
        )
        .with_schema(ToolSchema::object(
            [
                (
                    "path".to_string(),
                    string_property("Path to image file within workspace"),
                ),
                (
                    "prompt".to_string(),
                    string_property("Question or prompt about the image"),
                ),
            ]
            .into(),
            vec!["path".to_string(), "prompt".to_string()],
        ))
        .with_tags(vec!["image".to_string(), "integration".to_string()]),
        Tool::new("qr_encode", "Generate a QR code")
            .with_schema(ToolSchema::object(
                [
                    ("data".to_string(), string_property("Data to encode")),
                    (
                        "format".to_string(),
                        string_property("png, svg, or terminal"),
                    ),
                ]
                .into(),
                vec!["data".to_string()],
            ))
            .with_tags(vec!["qr".to_string(), "integration".to_string()]),
        Tool::new("qr_encode_url", "Generate a QR code from a URL")
            .with_schema(ToolSchema::object(
                [
                    ("url".to_string(), string_property("URL to encode")),
                    (
                        "format".to_string(),
                        string_property("png, svg, or terminal"),
                    ),
                ]
                .into(),
                vec!["url".to_string()],
            ))
            .with_tags(vec!["qr".to_string(), "integration".to_string()]),
        Tool::new("url_shorten", "Shorten a URL")
            .with_schema(ToolSchema::object(
                [("url".to_string(), string_property("URL to shorten"))].into(),
                vec!["url".to_string()],
            ))
            .with_tags(vec!["url".to_string(), "integration".to_string()]),
        Tool::new("url_expand", "Expand a shortened URL")
            .with_schema(ToolSchema::object(
                [("url".to_string(), string_property("Shortened URL"))].into(),
                vec!["url".to_string()],
            ))
            .with_tags(vec!["url".to_string(), "integration".to_string()]),
    ]);
}

pub async fn stt_transcribe(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let path = match get_required_string(arguments, "path") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let format = match audio_format(arguments.get("format").and_then(|value| value.as_str())) {
        Ok(format) => format,
        Err(err) => return ToolResult::err(err),
    };
    let resolved =
        match resolve_workspace_path(&runtime.workspace_root, &runtime.workspace_policy, &path) {
            Ok(path) => path,
            Err(err) => return ToolResult::err(err),
        };
    let audio = match std::fs::read(&resolved) {
        Ok(audio) => audio,
        Err(err) => return ToolResult::err(err.to_string()),
    };
    let client = crate::skills::SttClient::new(runtime.skills.stt.backend_config());

    match client.transcribe(&audio, format).await {
        Ok(text) => ToolResult::ok(text),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn stt_transcribe_url(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let url = match get_required_string(arguments, "url") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let format = match audio_format(arguments.get("format").and_then(|value| value.as_str())) {
        Ok(format) => format,
        Err(err) => return ToolResult::err(err),
    };
    let client = crate::skills::SttClient::new(runtime.skills.stt.backend_config());

    match client.transcribe_url(&url, format).await {
        Ok(text) => ToolResult::ok(text),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn tts_speak(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let text = match get_required_string(arguments, "text") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let client = crate::skills::TtsClient::new(resolve_tts_config(&runtime.skills.tts));

    match client.speak(&text).await {
        Ok(audio) => ToolResult::ok(format!("generated {} bytes", audio.len()))
            .with_metadata("bytes", audio.len().to_string()),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn tts_speak_stream(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let text = match get_required_string(arguments, "text") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let output_path = match get_required_string(arguments, "output_path") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let client = crate::skills::TtsClient::new(resolve_tts_config(&runtime.skills.tts));

    let resolved = match resolve_workspace_path(
        &runtime.workspace_root,
        &runtime.workspace_policy,
        &output_path,
    ) {
        Ok(path) => path,
        Err(err) => return ToolResult::err(err),
    };

    use futures_util::StreamExt;
    let mut stream = match client.speak_stream(&text).await {
        Ok(s) => s,
        Err(err) => return ToolResult::err(err.to_string()),
    };

    let mut file = match std::fs::File::create(&resolved) {
        Ok(f) => f,
        Err(err) => return ToolResult::err(err.to_string()),
    };

    let mut total_bytes = 0usize;
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(bytes) => {
                total_bytes += bytes.len();
                if let Err(err) = std::io::Write::write_all(&mut file, &bytes) {
                    return ToolResult::err(err.to_string());
                }
            }
            Err(err) => return ToolResult::err(err.to_string()),
        }
    }

    ToolResult::ok(format!("streamed {} bytes to {}", total_bytes, output_path))
        .with_metadata("bytes", total_bytes.to_string())
        .with_metadata("path", output_path)
}

pub async fn tts_list_voices(runtime: &ToolRuntime) -> ToolResult {
    let client = crate::skills::TtsClient::new(resolve_tts_config(&runtime.skills.tts));

    match client.list_voices().await {
        Ok(voices) if voices.is_empty() => ToolResult::ok("no voices"),
        Ok(voices) => ToolResult::ok(
            voices
                .into_iter()
                .map(|voice| format!("{} | {}", voice.voice_id, voice.name))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn image_generate(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let prompt = match get_required_string(arguments, "prompt") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let mut params = crate::skills::ImageParams::default();
    if let Some(width) = get_u64(arguments, "width") {
        params.width = width as u32;
    }
    if let Some(height) = get_u64(arguments, "height") {
        params.height = height as u32;
    }
    let mut client = crate::skills::ImageClient::new(runtime.skills.image.backend());
    let openai_api_key = resolve_env_reference(&runtime.skills.image.dalle.api_key);
    if !openai_api_key.is_empty() {
        client = client.with_openai_api_key(openai_api_key);
    }

    match client.generate(&prompt, params).await {
        Ok(image) => {
            let byte_len = image.bytes.as_ref().map(|bytes| bytes.len()).unwrap_or(0);
            let mut result = ToolResult::ok(format!(
                "generated {:?} image ({}) bytes",
                image.format, byte_len
            ))
            .with_metadata("format", format!("{:?}", image.format).to_lowercase())
            .with_metadata("bytes", byte_len.to_string());
            if let Some(url) = image.url {
                result = result.with_metadata("url", url);
            }
            if let Some(revised_prompt) = image.revised_prompt {
                result = result.with_metadata("revised_prompt", revised_prompt);
            }
            result
        }
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn image_analyze(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let image_url = match get_required_string(arguments, "image_url") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let prompt = match get_required_string(arguments, "prompt") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    let mut client = crate::skills::ImageClient::new(runtime.skills.image.backend());
    let openai_api_key = resolve_env_reference(&runtime.skills.image.dalle.api_key);
    if !openai_api_key.is_empty() {
        client = client.with_openai_api_key(openai_api_key);
    }

    match client.analyze(&image_url, &prompt).await {
        Ok(analysis) => ToolResult::ok(analysis),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn image_analyze_file(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let path = match get_required_string(arguments, "path") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let prompt = match get_required_string(arguments, "prompt") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    let resolved =
        match resolve_workspace_path(&runtime.workspace_root, &runtime.workspace_policy, &path) {
            Ok(path) => path,
            Err(err) => return ToolResult::err(err),
        };

    let image_bytes = match std::fs::read(&resolved) {
        Ok(bytes) => bytes,
        Err(err) => return ToolResult::err(err.to_string()),
    };

    let mut client = crate::skills::ImageClient::new(runtime.skills.image.backend());
    let openai_api_key = resolve_env_reference(&runtime.skills.image.dalle.api_key);
    if !openai_api_key.is_empty() {
        client = client.with_openai_api_key(openai_api_key);
    }

    match client.analyze_file(&image_bytes, &prompt).await {
        Ok(analysis) => ToolResult::ok(analysis),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn qr_encode(arguments: &HashMap<String, serde_json::Value>) -> ToolResult {
    let data = match get_required_string(arguments, "data") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let format = match arguments.get("format").and_then(|value| value.as_str()) {
        Some("svg") => crate::skills::QrFormat::Svg,
        Some("terminal") => crate::skills::QrFormat::Terminal,
        _ => crate::skills::QrFormat::default(),
    };

    match crate::skills::QrSkill::encode(&data, format) {
        Ok(bytes) => ToolResult::ok(format!("generated {} bytes", bytes.len()))
            .with_metadata("bytes", bytes.len().to_string()),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn qr_encode_url(arguments: &HashMap<String, serde_json::Value>) -> ToolResult {
    let url = match get_required_string(arguments, "url") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let format = match arguments.get("format").and_then(|value| value.as_str()) {
        Some("svg") => crate::skills::QrFormat::Svg,
        Some("terminal") => crate::skills::QrFormat::Terminal,
        _ => crate::skills::QrFormat::default(),
    };

    match crate::skills::QrSkill::encode_url(&url, format) {
        Ok(bytes) => ToolResult::ok(format!("generated {} bytes", bytes.len()))
            .with_metadata("bytes", bytes.len().to_string()),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn url_shorten(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let url = match get_required_string(arguments, "url") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    if let Err(e) = runtime.security.validate_url(&url) {
        return ToolResult::err(format!("URL blocked by SSRF protection: {}", e));
    }

    let shortener =
        crate::skills::UrlShortener::new(resolve_url_shortener_provider(&runtime.skills));
    match shortener.shorten(&url).await {
        Ok(short) => ToolResult::ok(short),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn url_expand(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let url = match get_required_string(arguments, "url") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    if let Err(e) = runtime.security.validate_url(&url) {
        return ToolResult::err(format!("URL blocked by SSRF protection: {}", e));
    }

    let shortener =
        crate::skills::UrlShortener::new(resolve_url_shortener_provider(&runtime.skills));
    match shortener.expand(&url).await {
        Ok(expanded) => ToolResult::ok(expanded),
        Err(err) => ToolResult::err(err.to_string()),
    }
}
