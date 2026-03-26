//! URL Shortener - is.gd, tinyurl, YOURLS

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UrlShortenerProvider {
    IsGd,
    TinyUrl,
    Yourls(YourlsConfig),
    Custom(CustomConfig),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct YourlsConfig {
    #[serde(alias = "base_url")]
    pub api_url: String,
    pub signature: String,
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomConfig {
    pub shorten_url: String,
    pub body_template: String,
    pub result_path: String,
    pub api_key: Option<String>,
}

pub struct UrlShortener {
    provider: UrlShortenerProvider,
    http: reqwest::Client,
}

impl UrlShortener {
    pub fn new(provider: UrlShortenerProvider) -> Self {
        Self {
            provider,
            http: reqwest::Client::new(),
        }
    }

    pub async fn shorten(&self, url: &str) -> Result<String, UrlError> {
        // Validate URL against SSRF attacks
        Self::validate_url(url)?;

        match &self.provider {
            UrlShortenerProvider::IsGd => self.shorten_isgd(url).await,
            UrlShortenerProvider::TinyUrl => self.shorten_tinyurl(url).await,
            UrlShortenerProvider::Yourls(cfg) => self.shorten_yourls(url, cfg).await,
            UrlShortenerProvider::Custom(cfg) => self.shorten_custom(url, cfg).await,
        }
    }

    pub async fn expand(&self, short_url: &str) -> Result<String, UrlError> {
        // Validate URL against SSRF attacks
        Self::validate_url(short_url)?;

        match &self.provider {
            UrlShortenerProvider::IsGd => self.expand_isgd(short_url).await,
            UrlShortenerProvider::TinyUrl => self.expand_tinyurl(short_url).await,
            UrlShortenerProvider::Yourls(cfg) => self.expand_yourls(short_url, cfg).await,
            UrlShortenerProvider::Custom(cfg) => self.expand_custom(short_url, cfg).await,
        }
    }

    /// Validate URL to prevent SSRF attacks
    fn validate_url(url: &str) -> Result<(), UrlError> {
        // Parse the URL to extract the host
        let parsed = url::Url::parse(url)
            .map_err(|e| UrlError::InvalidUrl(format!("Failed to parse URL: {}", e)))?;
        
        let host = parsed.host_str()
            .ok_or_else(|| UrlError::InvalidUrl("URL has no host".to_string()))?;

        // Block localhost variants
        let lower = host.to_lowercase();
        if lower == "localhost" 
            || lower == "127.0.0.1" 
            || lower == "::1" 
            || lower.starts_with("127.") {
            return Err(UrlError::InvalidUrl(
                "URLs pointing to localhost are not allowed".to_string()
            ));
        }

        // Block private IP ranges
        // 10.x.x.x
        if host.starts_with("10.") {
            return Err(UrlError::InvalidUrl(
                "URLs pointing to private IP ranges are not allowed".to_string()
            ));
        }

        // 172.16-31.x.x
        if let Some(rest) = host.strip_prefix("172.") {
            if let Some(dot_pos) = rest.find('.') {
                if let Ok(second_octet) = rest[..dot_pos].parse::<u8>() {
                    if (16..=31).contains(&second_octet) {
                        return Err(UrlError::InvalidUrl(
                            "URLs pointing to private IP ranges are not allowed".to_string()
                        ));
                    }
                }
            }
        }

        // 192.168.x.x
        if host.starts_with("192.168.") {
            return Err(UrlError::InvalidUrl(
                "URLs pointing to private IP ranges are not allowed".to_string()
            ));
        }

        // 169.254.x.x (link-local)
        if host.starts_with("169.254.") {
            return Err(UrlError::InvalidUrl(
                "URLs pointing to link-local addresses are not allowed".to_string()
            ));
        }

        Ok(())
    }

    async fn shorten_isgd(&self, url: &str) -> Result<String, UrlError> {
        let response = self
            .http
            .get("https://is.gd/create.php")
            .query(&[("format", "simple"), ("url", url)])
            .send()
            .await
            .map_err(|e| UrlError::RequestFailed(e.to_string()))?;

        let text = response
            .text()
            .await
            .map_err(|e| UrlError::RequestFailed(e.to_string()))?;

        if text.starts_with("Error:") {
            Err(UrlError::ShortenFailed(text))
        } else {
            Ok(text.trim().to_string())
        }
    }

    async fn shorten_tinyurl(&self, url: &str) -> Result<String, UrlError> {
        let response = self
            .http
            .get("https://tinyurl.com/api-create.php")
            .query(&[("url", url)])
            .send()
            .await
            .map_err(|e| UrlError::RequestFailed(e.to_string()))?;

        let text = response
            .text()
            .await
            .map_err(|e| UrlError::RequestFailed(e.to_string()))?;

        if text.contains("Error") {
            Err(UrlError::ShortenFailed(text))
        } else {
            Ok(text.trim().to_string())
        }
    }

    async fn shorten_yourls(&self, url: &str, config: &YourlsConfig) -> Result<String, UrlError> {
        let mut params = vec![
            ("action", "shorturl".to_string()),
            ("url", url.to_string()),
            ("format", "json".to_string()),
        ];

        if !config.signature.is_empty() {
            params.push(("signature", config.signature.clone()));
        } else if let (Some(username), Some(password)) =
            (config.username.as_ref(), config.password.as_ref())
        {
            params.push(("username", username.clone()));
            params.push(("password", password.clone()));
        } else {
            return Err(UrlError::ConfigError(
                "YOURLS requires either signature or username/password".to_string(),
            ));
        }

        let response = self
            .http
            .post(&config.api_url)
            .form(&params)
            .send()
            .await
            .map_err(|e| UrlError::RequestFailed(e.to_string()))?;

        #[derive(Deserialize)]
        struct YourlsResponse {
            shorturl: Option<String>,
            error: Option<String>,
        }

        let result: YourlsResponse = response
            .json()
            .await
            .map_err(|e| UrlError::ParseFailed(e.to_string()))?;

        if let Some(shorturl) = result.shorturl {
            Ok(shorturl)
        } else {
            Err(UrlError::ShortenFailed(
                result.error.unwrap_or_else(|| "Unknown error".to_string()),
            ))
        }
    }

    async fn shorten_custom(&self, url: &str, config: &CustomConfig) -> Result<String, UrlError> {
        let body = config.body_template.replace("{url}", url);

        let mut request = self
            .http
            .post(&config.shorten_url)
            .header("Content-Type", "application/json");

        if let Some(ref key) = config.api_key {
            request = request.header("Authorization", format!("Bearer {}", key));
        }

        let response = request
            .body(body)
            .send()
            .await
            .map_err(|e| UrlError::RequestFailed(e.to_string()))?;

        #[derive(Deserialize)]
        struct CustomResponse {
            url: Option<String>,
        }

        let result: CustomResponse = response
            .json()
            .await
            .map_err(|e| UrlError::ParseFailed(e.to_string()))?;

        result
            .url
            .ok_or(UrlError::ShortenFailed("No URL in response".to_string()))
    }

    async fn expand_isgd(&self, short_url: &str) -> Result<String, UrlError> {
        let response = self
            .http
            .get("https://is.gd/forward.php")
            .query(&[("shorturl", short_url)])
            .send()
            .await
            .map_err(|e| UrlError::RequestFailed(e.to_string()))?;

        Ok(response.url().to_string())
    }

    async fn expand_tinyurl(&self, _short_url: &str) -> Result<String, UrlError> {
        Err(UrlError::NotSupported(
            "tinyurl does not support expansion".to_string(),
        ))
    }

    async fn expand_yourls(
        &self,
        short_url: &str,
        config: &YourlsConfig,
    ) -> Result<String, UrlError> {
        let mut params = vec![
            ("action", "expand".to_string()),
            ("shorturl", short_url.to_string()),
            ("format", "json".to_string()),
        ];

        if !config.signature.is_empty() {
            params.push(("signature", config.signature.clone()));
        } else if let (Some(username), Some(password)) =
            (config.username.as_ref(), config.password.as_ref())
        {
            params.push(("username", username.clone()));
            params.push(("password", password.clone()));
        } else {
            return Err(UrlError::ConfigError(
                "YOURLS requires either signature or username/password".to_string(),
            ));
        }

        let response = self
            .http
            .post(&config.api_url)
            .form(&params)
            .send()
            .await
            .map_err(|e| UrlError::RequestFailed(e.to_string()))?;

        #[derive(Deserialize)]
        struct YourlsExpandResponse {
            #[serde(rename = "longurl")]
            longurl: Option<String>,
        }

        let result: YourlsExpandResponse = response
            .json()
            .await
            .map_err(|e| UrlError::ParseFailed(e.to_string()))?;

        result.longurl.ok_or(UrlError::ExpandFailed(
            "No long URL in response".to_string(),
        ))
    }

    async fn expand_custom(
        &self,
        _short_url: &str,
        _config: &CustomConfig,
    ) -> Result<String, UrlError> {
        Err(UrlError::NotSupported(
            "Custom provider does not support expansion".to_string(),
        ))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum UrlError {
    #[error("Request failed: {0}")]
    RequestFailed(String),

    #[error("Parse failed: {0}")]
    ParseFailed(String),

    #[error("Shorten failed: {0}")]
    ShortenFailed(String),

    #[error("Expand failed: {0}")]
    ExpandFailed(String),

    #[error("Not supported: {0}")]
    NotSupported(String),

    #[error("Config error: {0}")]
    ConfigError(String),

    #[error("Invalid URL: {0}")]
    InvalidUrl(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_url_rejects_localhost() {
        assert!(UrlShortener::validate_url("http://localhost/admin").is_err());
        assert!(UrlShortener::validate_url("http://127.0.0.1/api").is_err());
        assert!(UrlShortener::validate_url("http://127.0.0.2:8080/api").is_err());
    }

    #[test]
    fn validate_url_rejects_private_10() {
        assert!(UrlShortener::validate_url("http://10.0.0.1/api").is_err());
        assert!(UrlShortener::validate_url("http://10.255.255.255/api").is_err());
    }

    #[test]
    fn validate_url_rejects_private_172() {
        assert!(UrlShortener::validate_url("http://172.16.0.1/api").is_err());
        assert!(UrlShortener::validate_url("http://172.31.255.255/api").is_err());
    }

    #[test]
    fn validate_url_rejects_private_192_168() {
        assert!(UrlShortener::validate_url("http://192.168.0.1/api").is_err());
        assert!(UrlShortener::validate_url("http://192.168.1.100/api").is_err());
    }

    #[test]
    fn validate_url_rejects_link_local() {
        assert!(UrlShortener::validate_url("http://169.254.169.254/meta-data").is_err());
    }

    #[test]
    fn validate_url_allows_external_urls() {
        assert!(UrlShortener::validate_url("https://example.com/page").is_ok());
        assert!(UrlShortener::validate_url("https://api.github.com/repos").is_ok());
        assert!(UrlShortener::validate_url("http://8.8.8.8/dns").is_ok());
    }

    #[test]
    fn validate_url_rejects_invalid_urls() {
        assert!(UrlShortener::validate_url("not-a-url").is_err());
        assert!(UrlShortener::validate_url("").is_err());
    }

    #[test]
    fn validate_url_allows_172_outside_private_range() {
        // 172.15.x.x is NOT private
        assert!(UrlShortener::validate_url("http://172.15.0.1/api").is_ok());
        // 172.32.x.x is NOT private
        assert!(UrlShortener::validate_url("http://172.32.0.1/api").is_ok());
    }
}
