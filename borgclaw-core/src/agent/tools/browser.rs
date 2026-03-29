use super::{
    get_required_string, get_u64, number_property, string_property, truncate_output, with_browser,
    PropertySchema, Tool, ToolResult, ToolRuntime, ToolSchema,
};
use std::collections::HashMap;

pub fn register(tools: &mut Vec<Tool>) {
    tools.extend([
        Tool::new("browser_navigate", "Navigate the browser to a URL")
            .with_schema(ToolSchema::object(
                [("url".to_string(), string_property("Target URL"))].into(),
                vec!["url".to_string()],
            ))
            .with_tags(vec!["browser".to_string(), "integration".to_string()]),
        Tool::new("browser_click", "Click an element in the current page")
            .with_schema(ToolSchema::object(
                [("selector".to_string(), string_property("CSS selector"))].into(),
                vec!["selector".to_string()],
            ))
            .with_tags(vec!["browser".to_string(), "integration".to_string()]),
        Tool::new("browser_fill", "Fill an input element in the current page")
            .with_schema(ToolSchema::object(
                [
                    ("selector".to_string(), string_property("CSS selector")),
                    ("value".to_string(), string_property("Input value")),
                ]
                .into(),
                vec!["selector".to_string(), "value".to_string()],
            ))
            .with_tags(vec!["browser".to_string(), "integration".to_string()]),
        Tool::new(
            "browser_wait_for",
            "Wait for a selector or text on the current page",
        )
        .with_schema(ToolSchema::object(
            [
                (
                    "selector".to_string(),
                    string_property("Optional CSS selector"),
                ),
                ("text".to_string(), string_property("Optional page text")),
                (
                    "timeout_ms".to_string(),
                    number_property("Timeout in milliseconds", serde_json::json!(5000)),
                ),
            ]
            .into(),
            Vec::new(),
        ))
        .with_tags(vec!["browser".to_string(), "integration".to_string()]),
        Tool::new("browser_get_text", "Extract text from the current page")
            .with_schema(ToolSchema::object(
                [("selector".to_string(), string_property("CSS selector"))].into(),
                Vec::new(),
            ))
            .with_tags(vec!["browser".to_string(), "integration".to_string()]),
        Tool::new("browser_get_html", "Extract HTML from the current page")
            .with_schema(ToolSchema::object(
                [("selector".to_string(), string_property("CSS selector"))].into(),
                Vec::new(),
            ))
            .with_tags(vec!["browser".to_string(), "integration".to_string()]),
        Tool::new("browser_get_url", "Get the current browser URL")
            .with_schema(ToolSchema::object(HashMap::new(), Vec::new()))
            .with_tags(vec!["browser".to_string(), "integration".to_string()]),
        Tool::new("browser_eval_js", "Evaluate JavaScript in the current page")
            .with_schema(ToolSchema::object(
                [("script".to_string(), string_property("JavaScript source"))].into(),
                vec!["script".to_string()],
            ))
            .with_tags(vec!["browser".to_string(), "integration".to_string()]),
        Tool::new(
            "browser_screenshot",
            "Capture a screenshot from the current page",
        )
        .with_schema(ToolSchema::object(
            [(
                "full_page".to_string(),
                PropertySchema {
                    prop_type: "boolean".to_string(),
                    description: Some("Capture the full page".to_string()),
                    default: Some(serde_json::json!(true)),
                    enum_values: None,
                },
            )]
            .into(),
            Vec::new(),
        ))
        .with_tags(vec!["browser".to_string(), "integration".to_string()]),
        Tool::new("browser_go_back", "Navigate back in browser history")
            .with_schema(ToolSchema::object(HashMap::new(), Vec::new()))
            .with_tags(vec!["browser".to_string(), "integration".to_string()]),
        Tool::new("browser_go_forward", "Navigate forward in browser history")
            .with_schema(ToolSchema::object(HashMap::new(), Vec::new()))
            .with_tags(vec!["browser".to_string(), "integration".to_string()]),
        Tool::new("browser_reload", "Reload the current page")
            .with_schema(ToolSchema::object(HashMap::new(), Vec::new()))
            .with_tags(vec!["browser".to_string(), "integration".to_string()]),
    ]);
}

pub async fn browser_navigate(
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

    if url.starts_with("file://") || url.starts_with("data://") || url.starts_with("javascript:") {
        return ToolResult::err(
            "Browser cannot navigate to file://, data://, or javascript: URLs".to_string(),
        );
    }

    with_browser(runtime, |browser| async move {
        browser.navigate(&url).await?;
        Ok(ToolResult::ok(url))
    })
    .await
}

pub async fn browser_click(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let selector = match get_required_string(arguments, "selector") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    with_browser(runtime, |browser| async move {
        browser.click(&selector).await?;
        Ok(ToolResult::ok(format!("clicked {}", selector)))
    })
    .await
}

pub async fn browser_fill(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let selector = match get_required_string(arguments, "selector") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let value = match get_required_string(arguments, "value") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    with_browser(runtime, |browser| async move {
        browser.fill(&selector, &value).await?;
        Ok(ToolResult::ok(format!("filled {}", selector)))
    })
    .await
}

pub async fn browser_wait_for(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let timeout_ms = get_u64(arguments, "timeout_ms").unwrap_or(5000);
    if let Some(selector) = arguments.get("selector").and_then(|value| value.as_str()) {
        let selector = selector.to_string();
        return with_browser(runtime, |browser| async move {
            browser.wait_for(&selector, timeout_ms).await?;
            Ok(ToolResult::ok(format!("found {}", selector)))
        })
        .await;
    }
    if let Some(text) = arguments.get("text").and_then(|value| value.as_str()) {
        let text = text.to_string();
        return with_browser(runtime, |browser| async move {
            browser.wait_for_text(&text, timeout_ms).await?;
            Ok(ToolResult::ok(format!("found text {}", text)))
        })
        .await;
    }
    ToolResult::err("browser_wait_for requires 'selector' or 'text'")
}

pub async fn browser_get_text(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let selector = arguments
        .get("selector")
        .and_then(|value| value.as_str())
        .unwrap_or("body")
        .to_string();

    with_browser(runtime, |browser| async move {
        let text = browser.extract_text(&selector).await?;
        Ok(ToolResult::ok(text))
    })
    .await
}

pub async fn browser_get_html(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let selector = arguments
        .get("selector")
        .and_then(|value| value.as_str())
        .unwrap_or("body")
        .to_string();

    with_browser(runtime, |browser| async move {
        let html = browser.extract_html(&selector).await?;
        Ok(ToolResult::ok(truncate_output(&html)))
    })
    .await
}

pub async fn browser_get_url(runtime: &ToolRuntime) -> ToolResult {
    with_browser(runtime, |browser| async move {
        let url = browser.get_url().await?;
        Ok(ToolResult::ok(url))
    })
    .await
}

pub async fn browser_eval_js(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let script = match get_required_string(arguments, "script") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    with_browser(runtime, |browser| async move {
        let value = browser.eval_js(&script).await?;
        Ok(ToolResult::ok(value.to_string()))
    })
    .await
}

pub async fn browser_go_back(runtime: &ToolRuntime) -> ToolResult {
    with_browser(runtime, |browser| async move {
        browser.eval_js("history.back()").await?;
        Ok(ToolResult::ok("navigated back"))
    })
    .await
}

pub async fn browser_go_forward(runtime: &ToolRuntime) -> ToolResult {
    with_browser(runtime, |browser| async move {
        browser.eval_js("history.forward()").await?;
        Ok(ToolResult::ok("navigated forward"))
    })
    .await
}

pub async fn browser_reload(runtime: &ToolRuntime) -> ToolResult {
    with_browser(runtime, |browser| async move {
        browser.eval_js("location.reload()").await?;
        Ok(ToolResult::ok("reloaded page"))
    })
    .await
}

pub async fn browser_screenshot(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let _full_page = arguments
        .get("full_page")
        .and_then(|value| value.as_bool())
        .unwrap_or(true);

    with_browser(runtime, move |browser| async move {
        let bytes = browser.screenshot().await?;
        Ok(ToolResult::ok(format!("captured {} bytes", bytes.len()))
            .with_metadata("bytes", bytes.len().to_string()))
    })
    .await
}
