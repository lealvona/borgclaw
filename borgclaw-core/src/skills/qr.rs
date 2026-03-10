//! QR Code generation

use qrcode::QrCode;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QrFormat {
    Png { width: u32, height: u32 },
    Svg,
    Terminal,
}

impl Default for QrFormat {
    fn default() -> Self {
        Self::Png {
            width: 300,
            height: 300,
        }
    }
}

pub struct QrSkill;

pub type QrCodeSkill = QrSkill;

impl QrSkill {
    pub fn encode(data: &str, format: QrFormat) -> Result<Vec<u8>, QrError> {
        let code =
            QrCode::new(data.as_bytes()).map_err(|e| QrError::EncodeFailed(e.to_string()))?;

        match format {
            QrFormat::Png { width, height } => {
                let image = code
                    .render::<image::Luma<u8>>()
                    .min_dimensions(width, height)
                    .build();

                let mut buffer = Vec::new();
                let dynamic_image = image::DynamicImage::ImageLuma8(image);
                dynamic_image
                    .write_to(
                        &mut std::io::Cursor::new(&mut buffer),
                        image::ImageFormat::Png,
                    )
                    .map_err(|e| QrError::ImageError(e.to_string()))?;

                Ok(buffer)
            }
            QrFormat::Svg => {
                let svg = code.render::<qrcode::render::svg::Color>().build();
                Ok(svg.into_bytes())
            }
            QrFormat::Terminal => {
                let render = code.render::<char>().build();
                Ok(render.into_bytes())
            }
        }
    }

    pub fn encode_url(url: &str, format: QrFormat) -> Result<Vec<u8>, QrError> {
        Self::encode(url, format)
    }

    pub fn decode(_image_bytes: &[u8]) -> Result<String, QrError> {
        Err(QrError::DecodeFailed(
            "QR decoding requires rqrr crate. Use external tool.".to_string(),
        ))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum QrError {
    #[error("Encode failed: {0}")]
    EncodeFailed(String),

    #[error("Decode failed: {0}")]
    DecodeFailed(String),

    #[error("Image error: {0}")]
    ImageError(String),
}
