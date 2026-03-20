//! Image Generation - DALL-E 3 and Stable Diffusion

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageBackend {
    DallE3,
    StableDiffusion(SdConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdConfig {
    pub base_url: String,
    pub api_type: SdApiType,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SdApiType {
    Automatic1111,
    ComfyUI,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageParams {
    pub width: u32,
    pub height: u32,
    pub quality: String,
    pub steps: u32,
    pub cfg_scale: f32,
    pub seed: Option<i64>,
    pub negative_prompt: Option<String>,
}

impl Default for ImageParams {
    fn default() -> Self {
        Self {
            width: 1024,
            height: 1024,
            quality: "standard".to_string(),
            steps: 20,
            cfg_scale: 7.0,
            seed: None,
            negative_prompt: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageResult {
    pub url: Option<String>,
    pub bytes: Option<Vec<u8>>,
    pub format: ImageFormat,
    pub revised_prompt: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Webp,
}

pub struct ImageClient {
    backend: ImageBackend,
    openai_api_key: Option<String>,
    http: reqwest::Client,
}

impl ImageClient {
    pub fn new(backend: ImageBackend) -> Self {
        Self {
            backend,
            openai_api_key: None,
            http: reqwest::Client::new(),
        }
    }

    pub fn with_openai_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.openai_api_key = Some(api_key.into());
        self
    }

    pub async fn generate(
        &self,
        prompt: &str,
        params: ImageParams,
    ) -> Result<ImageResult, ImageError> {
        match &self.backend {
            ImageBackend::DallE3 => self.generate_dalle(prompt, &params).await,
            ImageBackend::StableDiffusion(cfg) => self.generate_sd(prompt, &params, cfg).await,
        }
    }

    async fn generate_dalle(
        &self,
        prompt: &str,
        params: &ImageParams,
    ) -> Result<ImageResult, ImageError> {
        let api_key = if let Some(api_key) = &self.openai_api_key {
            api_key.clone()
        } else {
            std::env::var("OPENAI_API_KEY")
                .map_err(|_| ImageError::ConfigError("OPENAI_API_KEY not set".to_string()))?
        };

        let body = serde_json::json!({
            "prompt": prompt,
            "model": "dall-e-3",
            "size": format!("{}x{}", params.width, params.height),
            "quality": params.quality,
            "n": 1
        });

        let response = self
            .http
            .post("https://api.openai.com/v1/images/generations")
            .bearer_auth(&api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ImageError::RequestFailed(e.to_string()))?;

        #[derive(Deserialize)]
        struct DallEResponse {
            data: Vec<DallEImage>,
        }

        #[derive(Deserialize)]
        struct DallEImage {
            url: Option<String>,
            #[serde(rename = "b64_json")]
            b64_json: Option<String>,
            #[serde(rename = "revised_prompt")]
            revised_prompt: Option<String>,
        }

        let result: DallEResponse = response
            .json()
            .await
            .map_err(|e| ImageError::ParseFailed(e.to_string()))?;

        if let Some(image) = result.data.first() {
            let (url, bytes) = if let Some(b64) = &image.b64_json {
                use base64::Engine;
                let decoded = base64::engine::general_purpose::STANDARD
                    .decode(b64)
                    .map_err(|e| ImageError::ParseFailed(e.to_string()))?;
                (None, Some(decoded))
            } else if let Some(url) = &image.url {
                (Some(url.clone()), None)
            } else {
                return Err(ImageError::ParseFailed("No image data".to_string()));
            };

            Ok(ImageResult {
                url,
                bytes,
                format: ImageFormat::Png,
                revised_prompt: image.revised_prompt.clone(),
            })
        } else {
            Err(ImageError::ParseFailed("No images in response".to_string()))
        }
    }

    async fn generate_sd(
        &self,
        prompt: &str,
        params: &ImageParams,
        config: &SdConfig,
    ) -> Result<ImageResult, ImageError> {
        let body = serde_json::json!({
            "prompt": prompt,
            "negative_prompt": params.negative_prompt.as_deref().unwrap_or(""),
            "steps": params.steps,
            "width": params.width,
            "height": params.height,
            "cfg_scale": params.cfg_scale,
            "seed": params.seed.unwrap_or(-1),
        });

        let url = format!("{}/sdapi/v1/txt2img", config.base_url);

        let response = self
            .http
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ImageError::RequestFailed(e.to_string()))?;

        #[derive(Deserialize)]
        struct SdResponse {
            images: Vec<String>,
        }

        let result: SdResponse = response
            .json()
            .await
            .map_err(|e| ImageError::ParseFailed(e.to_string()))?;

        if let Some(b64) = result.images.first() {
            use base64::Engine;
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(b64)
                .map_err(|e| ImageError::ParseFailed(e.to_string()))?;

            Ok(ImageResult {
                url: None,
                bytes: Some(decoded),
                format: ImageFormat::Png,
                revised_prompt: None,
            })
        } else {
            Err(ImageError::ParseFailed("No images in response".to_string()))
        }
    }

    pub async fn analyze(
        &self,
        image_url: &str,
        prompt: &str,
    ) -> Result<String, ImageError> {
        let api_key = if let Some(api_key) = &self.openai_api_key {
            api_key.clone()
        } else {
            std::env::var("OPENAI_API_KEY")
                .map_err(|_| ImageError::ConfigError("OPENAI_API_KEY not set".to_string()))?
        };

        let body = serde_json::json!({
            "model": "gpt-4o-mini",
            "messages": [
                {
                    "role": "user",
                    "content": [
                        {
                            "type": "text",
                            "text": prompt
                        },
                        {
                            "type": "image_url",
                            "image_url": {
                                "url": image_url
                            }
                        }
                    ]
                }
            ],
            "max_tokens": 1000
        });

        let response = self
            .http
            .post("https://api.openai.com/v1/chat/completions")
            .bearer_auth(&api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ImageError::RequestFailed(e.to_string()))?;

        #[derive(Deserialize)]
        struct ChatResponse {
            choices: Vec<Choice>,
        }

        #[derive(Deserialize)]
        struct Choice {
            message: Message,
        }

        #[derive(Deserialize)]
        struct Message {
            content: String,
        }

        let result: ChatResponse = response
            .json()
            .await
            .map_err(|e| ImageError::ParseFailed(e.to_string()))?;

        result
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| ImageError::ParseFailed("No response from vision model".to_string()))
    }

    pub async fn analyze_file(
        &self,
        image_bytes: &[u8],
        prompt: &str,
    ) -> Result<String, ImageError> {
        use base64::Engine;
        let base64_image = base64::engine::general_purpose::STANDARD.encode(image_bytes);
        let data_url = format!("data:image/png;base64,{}", base64_image);
        self.analyze(&data_url, prompt).await
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ImageError {
    #[error("Request failed: {0}")]
    RequestFailed(String),

    #[error("Parse failed: {0}")]
    ParseFailed(String),

    #[error("Config error: {0}")]
    ConfigError(String),
}
