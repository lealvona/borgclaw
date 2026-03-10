//! Text-to-Speech (ElevenLabs)

use futures_core::Stream;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ElevenLabsConfig {
    pub api_key: String,
    pub voice_id: String,
    pub model_id: String,
    pub stability: f32,
    pub similarity_boost: f32,
}

impl Default for ElevenLabsConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            voice_id: "21m00Tcm4TlvDq8ikWAM".to_string(),
            model_id: "eleven_monolingual_v1".to_string(),
            stability: 0.5,
            similarity_boost: 0.75,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Voice {
    pub voice_id: String,
    pub name: String,
    pub category: String,
    pub description: Option<String>,
}

pub struct TtsClient {
    config: ElevenLabsConfig,
    http: reqwest::Client,
}

pub type ElevenLabsClient = TtsClient;

impl TtsClient {
    pub fn new(config: ElevenLabsConfig) -> Self {
        Self {
            config,
            http: reqwest::Client::new(),
        }
    }

    pub async fn speak(&self, text: &str) -> Result<Vec<u8>, TtsError> {
        let url = format!(
            "https://api.elevenlabs.io/v1/text-to-speech/{}",
            self.config.voice_id
        );

        let body = serde_json::json!({
            "text": text,
            "model_id": self.config.model_id,
            "voice_settings": {
                "stability": self.config.stability,
                "similarity_boost": self.config.similarity_boost
            }
        });

        let response = self
            .http
            .post(&url)
            .header("xi-api-key", &self.config.api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| TtsError::RequestFailed(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(TtsError::RequestFailed(format!("{}: {}", status, text)));
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| TtsError::RequestFailed(e.to_string()))?
            .to_vec();

        Ok(bytes)
    }

    pub async fn speak_stream(
        &self,
        text: &str,
    ) -> Result<impl Stream<Item = Result<bytes::Bytes, TtsError>>, TtsError> {
        let url = format!(
            "https://api.elevenlabs.io/v1/text-to-speech/{}/stream",
            self.config.voice_id
        );

        let body = serde_json::json!({
            "text": text,
            "model_id": self.config.model_id,
            "voice_settings": {
                "stability": self.config.stability,
                "similarity_boost": self.config.similarity_boost
            }
        });

        let request = self
            .http
            .post(&url)
            .header("xi-api-key", &self.config.api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .build()
            .map_err(|e| TtsError::RequestFailed(e.to_string()))?;

        let response = self
            .http
            .execute(request)
            .await
            .map_err(|e| TtsError::RequestFailed(e.to_string()))?;

        if !response.status().is_success() {
            return Err(TtsError::RequestFailed(format!("{}", response.status())));
        }

        Ok(response
            .bytes_stream()
            .map(|b| b.map_err(|e| TtsError::RequestFailed(e.to_string())))
            .boxed())
    }

    pub async fn list_voices(&self) -> Result<Vec<Voice>, TtsError> {
        let response = self
            .http
            .get("https://api.elevenlabs.io/v1/voices")
            .header("xi-api-key", &self.config.api_key)
            .send()
            .await
            .map_err(|e| TtsError::RequestFailed(e.to_string()))?;

        #[derive(Deserialize)]
        struct VoicesResponse {
            voices: Vec<Voice>,
        }

        let result: VoicesResponse = response
            .json()
            .await
            .map_err(|e| TtsError::ParseFailed(e.to_string()))?;

        Ok(result.voices)
    }

    pub fn set_voice(&mut self, voice_id: &str) {
        self.config.voice_id = voice_id.to_string();
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TtsError {
    #[error("Request failed: {0}")]
    RequestFailed(String),

    #[error("Parse failed: {0}")]
    ParseFailed(String),
}
