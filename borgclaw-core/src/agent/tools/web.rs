use super::{
    get_required_string, get_u64, truncate_output, PropertySchema, Tool, ToolResult, ToolRuntime,
    ToolSchema,
};
use regex::Regex;
use std::collections::HashMap;

pub fn register(tools: &mut Vec<Tool>) {
    tools.extend([
        Tool::new("web_search", "Search the web")
            .with_schema(ToolSchema::object(
                [
                    (
                        "query".to_string(),
                        PropertySchema {
                            prop_type: "string".to_string(),
                            description: Some("Search query".to_string()),
                            default: None,
                            enum_values: None,
                        },
                    ),
                    (
                        "num_results".to_string(),
                        PropertySchema {
                            prop_type: "number".to_string(),
                            description: Some("Number of results".to_string()),
                            default: Some(serde_json::json!(5)),
                            enum_values: None,
                        },
                    ),
                ]
                .into(),
                vec!["query".to_string()],
            ))
            .with_tags(vec!["web".to_string()]),
        Tool::new("fetch_url", "Fetch content from a URL")
            .with_schema(ToolSchema::object(
                [(
                    "url".to_string(),
                    PropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("URL to fetch".to_string()),
                        default: None,
                        enum_values: None,
                    },
                )]
                .into(),
                vec!["url".to_string()],
            ))
            .with_tags(vec!["web".to_string()]),
    ]);
}

pub async fn fetch_url(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let url = match get_required_string(arguments, "url") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    if let Err(err) = runtime.security.validate_url(&url) {
        return ToolResult::err(format!("URL blocked by SSRF protection: {}", err));
    }

    match reqwest::get(&url).await {
        Ok(response) if response.status().is_success() => match response.text().await {
            Ok(body) => ToolResult::ok(truncate_output(&body)),
            Err(err) => ToolResult::err(err.to_string()),
        },
        Ok(response) => ToolResult::err(format!("http {}", response.status())),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn web_search(arguments: &HashMap<String, serde_json::Value>) -> ToolResult {
    let query = match get_required_string(arguments, "query") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let num_results = get_u64(arguments, "num_results").unwrap_or(5).clamp(1, 10) as usize;

    let client = reqwest::Client::builder()
        .user_agent("BorgClaw/0.1")
        .build();
    let client = match client {
        Ok(client) => client,
        Err(err) => return ToolResult::err(err.to_string()),
    };

    let response = client
        .get("https://duckduckgo.com/html/")
        .query(&[("q", query.as_str())])
        .send()
        .await;
    let response = match response {
        Ok(response) if response.status().is_success() => response,
        Ok(response) => return ToolResult::err(format!("http {}", response.status())),
        Err(err) => return ToolResult::err(err.to_string()),
    };

    let body = match response.text().await {
        Ok(body) => body,
        Err(err) => return ToolResult::err(err.to_string()),
    };

    let results = parse_duckduckgo_results(&body, num_results);
    if results.is_empty() {
        return ToolResult::ok("no results");
    }

    let output = results
        .iter()
        .enumerate()
        .map(|(index, result)| {
            let mut line = format!("{}. {} - {}", index + 1, result.title, result.url);
            if let Some(snippet) = &result.snippet {
                line.push_str(&format!("\n   {}", snippet));
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n");

    ToolResult::ok(output)
        .with_metadata("source", "duckduckgo")
        .with_metadata("result_count", results.len().to_string())
}

#[derive(Debug, PartialEq, Eq)]
struct SearchResult {
    title: String,
    url: String,
    snippet: Option<String>,
}

fn parse_duckduckgo_results(body: &str, limit: usize) -> Vec<SearchResult> {
    let result_re = Regex::new(
        r#"(?s)<a[^>]*class="result__a"[^>]*href="(?P<url>[^"]+)"[^>]*>(?P<title>.*?)</a>.*?(?:<a[^>]*class="result__snippet"[^>]*>|<div[^>]*class="result__snippet"[^>]*>)(?P<snippet>.*?)(?:</a>|</div>)"#,
    )
    .unwrap();
    let tag_re = Regex::new(r"<[^>]+>").unwrap();

    result_re
        .captures_iter(body)
        .take(limit)
        .filter_map(|capture| {
            let title = decode_html_entities(tag_re.replace_all(&capture["title"], "").trim());
            let url = decode_html_entities(capture["url"].trim());
            let snippet = decode_html_entities(tag_re.replace_all(&capture["snippet"], "").trim());
            if title.is_empty() || url.is_empty() {
                return None;
            }

            Some(SearchResult {
                title,
                url,
                snippet: if snippet.is_empty() {
                    None
                } else {
                    Some(snippet)
                },
            })
        })
        .collect()
}

fn decode_html_entities(input: &str) -> String {
    input
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
}

#[cfg(test)]
mod tests {
    use super::parse_duckduckgo_results;

    #[test]
    fn parses_duckduckgo_results() {
        let html = r#"
            <div class="result">
              <a class="result__a" href="https://example.com/one">Example &amp; One</a>
              <div class="result__snippet">First <b>snippet</b></div>
            </div>
            <div class="result">
              <a class="result__a" href="https://example.com/two">Example Two</a>
              <a class="result__snippet">Second snippet</a>
            </div>
        "#;

        let results = parse_duckduckgo_results(html, 5);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].title, "Example & One");
        assert_eq!(results[0].url, "https://example.com/one");
        assert_eq!(results[0].snippet.as_deref(), Some("First snippet"));
    }
}
