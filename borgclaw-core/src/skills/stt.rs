//! Speech-to-Text (Whisper) - OpenAI, Open WebUI, whisper.cpp

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AudioFormat {
    Wav,
    Mp3,
    Webm,
    M4a,
    Ogg,
}

impl AudioFormat {
    pub fn mime_type(&self) -> &str {
        match self {
            Self::Wav => "audio/wav",
            Self::Mp3 => "audio/mpeg",
            Self::Webm => "audio/webm",
            Self::M4a => "audio/mp4",
            Self::Ogg => "audio/ogg",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SttBackend {
    OpenAI(OpenAiConfig),
    OpenWebUi(OpenWebUiConfig),
    WhisperCpp(WhisperCppConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OpenAiConfig {
    pub api_key: String,
    pub model: String,
}

impl Default for OpenAiConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: "whisper-1".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OpenWebUiConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

impl Default for OpenWebUiConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:3000".to_string(),
            api_key: String::new(),
            model: "whisper-1".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WhisperCppConfig {
    pub binary_path: PathBuf,
    pub model_path: PathBuf,
    pub language: Option<String>,
    pub extra_args: Vec<String>,
}

impl Default for WhisperCppConfig {
    fn default() -> Self {
        Self {
            binary_path: PathBuf::from(".local/tools/whisper.cpp/build/bin/whisper-cli"),
            model_path: PathBuf::from(".local/tools/whisper.cpp/models/ggml-base.en.bin"),
            language: None,
            extra_args: Vec::new(),
        }
    }
}

pub struct SttClient {
    backend: SttBackend,
    http: reqwest::Client,
}

impl SttClient {
    pub fn new(backend: SttBackend) -> Self {
        Self {
            backend,
            http: reqwest::Client::new(),
        }
    }

    pub async fn transcribe(&self, audio: &[u8], format: AudioFormat) -> Result<String, SttError> {
        match &self.backend {
            SttBackend::OpenAI(cfg) => self.transcribe_openai(audio, format, cfg).await,
            SttBackend::OpenWebUi(cfg) => self.transcribe_openwebui(audio, format, cfg).await,
            SttBackend::WhisperCpp(cfg) => self.transcribe_whisper_cpp(audio, &format, cfg).await,
        }
    }

    pub async fn transcribe_url(&self, url: &str, format: AudioFormat) -> Result<String, SttError> {
        let audio = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|e| SttError::RequestFailed(e.to_string()))?
            .bytes()
            .await
            .map_err(|e| SttError::RequestFailed(e.to_string()))?;
        self.transcribe(&audio, format).await
    }

    async fn transcribe_openai(
        &self,
        audio: &[u8],
        format: AudioFormat,
        config: &OpenAiConfig,
    ) -> Result<String, SttError> {
        let api_key = if config.api_key.is_empty() {
            std::env::var("OPENAI_API_KEY")
                .map_err(|_| SttError::ConfigError("OPENAI_API_KEY not set".to_string()))?
        } else {
            config.api_key.clone()
        };

        let form = reqwest::multipart::Form::new()
            .part(
                "file",
                reqwest::multipart::Part::bytes(audio.to_vec())
                    .mime_str(format.mime_type())
                    .map_err(|e| SttError::RequestFailed(e.to_string()))?
                    .file_name("audio".to_string()),
            )
            .text("model", config.model.clone());

        let response = self
            .http
            .post("https://api.openai.com/v1/audio/transcriptions")
            .bearer_auth(&api_key)
            .multipart(form)
            .send()
            .await
            .map_err(|e| SttError::RequestFailed(e.to_string()))?;

        #[derive(Deserialize)]
        struct TranscribeResponse {
            text: String,
        }

        let result: TranscribeResponse = response
            .json()
            .await
            .map_err(|e| SttError::ParseFailed(e.to_string()))?;

        Ok(result.text)
    }

    async fn transcribe_openwebui(
        &self,
        audio: &[u8],
        format: AudioFormat,
        config: &OpenWebUiConfig,
    ) -> Result<String, SttError> {
        let form = reqwest::multipart::Form::new()
            .part(
                "file",
                reqwest::multipart::Part::bytes(audio.to_vec())
                    .mime_str(format.mime_type())
                    .map_err(|e| SttError::RequestFailed(e.to_string()))?
                    .file_name("audio".to_string()),
            )
            .text("model", config.model.clone());

        let response = self
            .http
            .post(&format!("{}/api/v1/audio/transcriptions", config.base_url))
            .bearer_auth(&config.api_key)
            .multipart(form)
            .send()
            .await
            .map_err(|e| SttError::RequestFailed(e.to_string()))?;

        #[derive(Deserialize)]
        struct TranscribeResponse {
            text: String,
        }

        let result: TranscribeResponse = response
            .json()
            .await
            .map_err(|e| SttError::ParseFailed(e.to_string()))?;

        Ok(result.text)
    }

    async fn transcribe_whisper_cpp(
        &self,
        audio: &[u8],
        format: &AudioFormat,
        config: &WhisperCppConfig,
    ) -> Result<String, SttError> {
        let temp_dir = std::env::temp_dir();
        let _temp_file = temp_dir.join(format!("stt_{}.", temp_dir.to_string_lossy().len()));

        let ext = match format {
            AudioFormat::Wav => "wav",
            AudioFormat::Mp3 => "mp3",
            AudioFormat::Webm => "webm",
            AudioFormat::M4a => "m4a",
            AudioFormat::Ogg => "ogg",
        };

        let temp_path = temp_dir.join(format!("stt_{}.{}", uuid::Uuid::new_v4(), ext));
        std::fs::write(&temp_path, audio).map_err(|e| SttError::IoError(e.to_string()))?;

        let mut args = vec![
            "-m".to_string(),
            config.model_path.to_string_lossy().to_string(),
            "-f".to_string(),
            temp_path.to_string_lossy().to_string(),
            "-otxt".to_string(),
        ];

        if let Some(ref lang) = config.language {
            args.push("-l".to_string());
            args.push(lang.clone());
        }

        args.extend(config.extra_args.clone());

        let output = tokio::process::Command::new(&config.binary_path)
            .args(&args)
            .output()
            .await
            .map_err(|e| SttError::ExecutionFailed(e.to_string()))?;

        let _ = std::fs::remove_file(&temp_path);

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SttError::ExecutionFailed(stderr.to_string()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let text = stdout
            .lines()
            .skip_while(|line| line.contains("-->"))
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string();

        Ok(text)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SttError {
    #[error("Request failed: {0}")]
    RequestFailed(String),

    #[error("Parse failed: {0}")]
    ParseFailed(String),

    #[error("IO error: {0}")]
    IoError(String),

    #[error("Execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Config error: {0}")]
    ConfigError(String),

    #[error("Not available")]
    NotAvailable,
}
