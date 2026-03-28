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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qr_format_default_is_png() {
        let format = QrFormat::default();
        match format {
            QrFormat::Png { width, height } => {
                assert_eq!(width, 300);
                assert_eq!(height, 300);
            }
            _ => panic!("Expected default to be Png format"),
        }
    }

    #[test]
    fn qr_encode_terminal_returns_bytes() {
        let data = "https://example.com";
        let result = QrSkill::encode(data, QrFormat::Terminal);
        assert!(result.is_ok());
        let bytes = result.unwrap();
        assert!(!bytes.is_empty());
        // Terminal output should be text (UTF-8)
        let text = String::from_utf8(bytes);
        assert!(text.is_ok());
    }

    #[test]
    fn qr_encode_svg_returns_bytes() {
        let data = "https://example.com";
        let result = QrSkill::encode(data, QrFormat::Svg);
        assert!(result.is_ok());
        let bytes = result.unwrap();
        assert!(!bytes.is_empty());
        // SVG output should be valid UTF-8 XML
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("<svg"));
        assert!(text.contains("</svg>"));
    }

    #[test]
    fn qr_encode_png_returns_bytes() {
        let data = "https://example.com";
        let format = QrFormat::Png {
            width: 200,
            height: 200,
        };
        let result = QrSkill::encode(data, format);
        assert!(result.is_ok());
        let bytes = result.unwrap();
        assert!(!bytes.is_empty());
        // PNG magic bytes
        assert_eq!(&bytes[0..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn qr_encode_url_delegates_to_encode() {
        let url = "https://example.com";
        let result = QrSkill::encode_url(url, QrFormat::Terminal);
        assert!(result.is_ok());
    }

    #[test]
    fn qr_encode_empty_string_succeeds() {
        // Empty string can be encoded (produces minimal QR)
        let result = QrSkill::encode("", QrFormat::Terminal);
        assert!(result.is_ok());
    }

    #[test]
    fn qr_decode_always_returns_error() {
        let dummy_bytes = b"dummy";
        let result = QrSkill::decode(dummy_bytes);
        assert!(result.is_err());
        match result {
            Err(QrError::DecodeFailed(_)) => (), // Expected
            _ => panic!("Expected DecodeFailed error"),
        }
    }

    #[test]
    fn qr_error_display_messages() {
        let encode_err = QrError::EncodeFailed("test error".to_string());
        assert!(encode_err.to_string().contains("Encode failed"));
        assert!(encode_err.to_string().contains("test error"));

        let decode_err = QrError::DecodeFailed("test error".to_string());
        assert!(decode_err.to_string().contains("Decode failed"));

        let image_err = QrError::ImageError("test error".to_string());
        assert!(image_err.to_string().contains("Image error"));
    }
}
