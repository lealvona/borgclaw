mod colors;
mod providers;

use crate::onboarding::colors::{
    banner, paint, HEADER, INFO, MANDATORY, OPTIONAL, PROMPT, SUCCESS, WARN,
};
use crate::onboarding::providers::{ProviderDef, ProviderRegistry};
use borgclaw_core::config::{AppConfig, ChannelConfig, DmPolicy};
use clap::Args;
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Password, Select};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Args)]
pub struct InitArgs {
    #[arg(long, help = "Run minimal onboarding")]
    pub quick: bool,
    #[arg(long, help = "Force update flow when config exists")]
    pub update: bool,
    #[arg(long, help = "Reset to defaults")]
    pub reset: bool,
    #[arg(long = "list-providers", help = "List providers from registry")]
    pub list_providers: bool,
    #[arg(
        long = "refresh-models",
        help = "Fetch latest model lists from providers"
    )]
    pub refresh_models: bool,
    #[arg(
        long = "generate-env",
        help = "Generate .env from current configuration"
    )]
    pub generate_env: bool,
    #[arg(long, help = "Registrar title/component type")]
    pub component: Option<String>,
    #[arg(long, help = "Registrar chapter/component name")]
    pub chapter: Option<String>,
    #[arg(long, default_value = "add", value_parser = ["add", "update", "delete"], help = "Registrar action")]
    pub action: String,
    #[arg(long, default_value = "repl", value_parser = ["repl", "none"], help = "Auto-start target after onboarding")]
    pub start: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartTarget {
    Repl,
    None,
}

pub struct InitOutcome {
    pub config: AppConfig,
    pub start: StartTarget,
}

pub async fn run_init(
    config_path: &PathBuf,
    mut config: AppConfig,
    args: &InitArgs,
) -> Result<InitOutcome, String> {
    let theme = ColorfulTheme::default();
    let providers_path = config_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("providers.toml");
    let registry = ProviderRegistry::load_or_create(&providers_path)?;

    if args.list_providers {
        banner("NEON PROVIDER DIRECTORY");
        for provider in ordered_providers(&registry) {
            println!(
                "{}{}{} {}default={}{}{}",
                paint(INFO, "- "),
                paint(HEADER, &provider.display),
                paint(INFO, " ("),
                provider.id,
                paint(SUCCESS, " "),
                provider.default_model,
                paint(INFO, ")")
            );
        }
        println!(
            "{}",
            paint(INFO, format!("Registry path: {}", providers_path.display()))
        );
        return Ok(InitOutcome {
            config,
            start: StartTarget::None,
        });
    }

    if args.refresh_models {
        banner("NEON MODEL REFRESH");
        for provider in ordered_providers(&registry) {
            let models = fetch_models(provider, None).await;
            match models {
                Ok(list) => println!("{} {} {}", paint(SUCCESS, "OK"), provider.id, list.len()),
                Err(e) => println!("{} {} {}", paint(WARN, "WARN"), provider.id, e),
            }
        }
        return Ok(InitOutcome {
            config,
            start: StartTarget::None,
        });
    }

    if args.generate_env {
        generate_env_file(&config, &HashMap::new(), &PathBuf::from(".env"))?;
        println!("{}", paint(SUCCESS, "Generated .env"));
        return Ok(InitOutcome {
            config,
            start: StartTarget::None,
        });
    }

    if let Some(title) = args.component.as_ref() {
        let chapter = args
            .chapter
            .clone()
            .ok_or_else(|| "--chapter is required with --component".to_string())?;
        apply_component_action(&mut config, title, &chapter, &args.action, &theme)?;
        return Ok(InitOutcome {
            config,
            start: StartTarget::None,
        });
    }

    if args.reset {
        let do_reset = Confirm::with_theme(&theme)
            .with_prompt("Reset config to defaults? (mandatory confirmation)")
            .default(false)
            .interact()
            .map_err(|e| e.to_string())?;
        if do_reset {
            config = AppConfig::default();
            println!("{}", paint(SUCCESS, "Config reset to defaults."));
        }
    }

    banner("BORGCLAW // NEON ONBOARDING");
    println!(
        "{}",
        paint(INFO, "Mandatory fields are marked in neon red.")
    );
    println!(
        "{}",
        paint(OPTIONAL, "Optional fields are marked in neon yellow.")
    );

    let mut env_updates = read_env_file(&PathBuf::from(".env"));
    let has_existing = config_path.exists();
    if has_existing && !args.quick && !args.update {
        let choices = vec![
            "Update existing configuration",
            "Add component via wizard",
            "Delete component via wizard",
            "Keep current and only regenerate .env",
        ];
        let pick = Select::with_theme(&theme)
            .with_prompt("Existing config detected. Choose action")
            .items(&choices)
            .default(0)
            .interact()
            .map_err(|e| e.to_string())?;
        if pick == 1 {
            component_wizard(&mut config, "add", &theme)?;
        } else if pick == 2 {
            component_wizard(&mut config, "delete", &theme)?;
        } else if pick == 3 {
            generate_env_file(&config, &env_updates, &PathBuf::from(".env"))?;
            return Ok(InitOutcome {
                config,
                start: start_target_from(&args.start),
            });
        }
    }

    configure_provider_and_model(&mut config, &registry, &theme, &mut env_updates, args.quick)
        .await?;
    configure_channels(&mut config, &theme, args.quick)?;
    configure_security(&mut config, &theme, args.quick)?;
    configure_memory(&mut config, &theme, args.quick)?;
    configure_skills_registry(&mut config, &theme, args.quick)?;

    let show_summary = Confirm::with_theme(&theme)
        .with_prompt("Show summary before save?")
        .default(true)
        .interact()
        .map_err(|e| e.to_string())?;
    if show_summary {
        print_summary(&config);
    }

    generate_env_file(&config, &env_updates, &PathBuf::from(".env"))?;
    println!(
        "{}",
        paint(
            SUCCESS,
            "Generated .env with working defaults and credentials."
        )
    );

    Ok(InitOutcome {
        config,
        start: start_target_from(&args.start),
    })
}

fn start_target_from(s: &str) -> StartTarget {
    if s == "none" {
        StartTarget::None
    } else {
        StartTarget::Repl
    }
}

fn ordered_providers(registry: &ProviderRegistry) -> Vec<&ProviderDef> {
    let order = ["openai", "anthropic", "google", "ollama", "custom"];
    let mut out = Vec::new();
    for id in order {
        if let Some(p) = registry.providers.get(id) {
            out.push(p);
        }
    }
    for (k, p) in &registry.providers {
        if !order.contains(&k.as_str()) {
            out.push(p);
        }
    }
    out
}

async fn configure_provider_and_model(
    config: &mut AppConfig,
    registry: &ProviderRegistry,
    theme: &ColorfulTheme,
    env_updates: &mut HashMap<String, String>,
    quick: bool,
) -> Result<(), String> {
    println!(
        "{}",
        paint(MANDATORY, "[MANDATORY] Provider and model selection")
    );
    println!(
        "{}",
        paint(
            WARN,
            "Ramifications: cloud providers send prompt data externally; Ollama keeps data local."
        )
    );

    let providers = ordered_providers(registry);
    let labels: Vec<String> = providers
        .iter()
        .map(|p| format!("{} ({})", p.display, p.id))
        .collect();
    let current_idx = labels
        .iter()
        .position(|l| l.contains(&config.agent.provider))
        .unwrap_or(0);
    let selection = Select::with_theme(theme)
        .with_prompt(paint(PROMPT, "Choose provider"))
        .items(&labels)
        .default(current_idx)
        .interact()
        .map_err(|e| e.to_string())?;
    let provider = providers[selection];
    config.agent.provider = provider.id.clone();

    let models = fetch_models(provider, None)
        .await
        .unwrap_or_else(|_| provider.static_models.clone());
    let mut model_options = models.clone();
    if !model_options.contains(&provider.default_model) {
        model_options.insert(0, provider.default_model.clone());
    }
    let default_idx = model_options
        .iter()
        .position(|m| m == &config.agent.model)
        .or_else(|| {
            model_options
                .iter()
                .position(|m| m == &provider.default_model)
        })
        .unwrap_or(0);
    let model_idx = Select::with_theme(theme)
        .with_prompt(paint(PROMPT, "Choose model"))
        .items(&model_options)
        .default(default_idx)
        .interact()
        .map_err(|e| e.to_string())?;
    config.agent.model = model_options[model_idx].clone();

    if provider.requires_auth {
        let env_key = provider
            .api_key_env
            .clone()
            .unwrap_or_else(|| "BORGCLAW_API_KEY".to_string());
        let update_key = if quick {
            true
        } else {
            Confirm::with_theme(theme)
                .with_prompt(format!("Set {} now?", env_key))
                .default(true)
                .interact()
                .map_err(|e| e.to_string())?
        };
        if update_key {
            let key = Password::with_theme(theme)
                .with_prompt(format!("Enter {}", env_key))
                .allow_empty_password(false)
                .interact()
                .map_err(|e| e.to_string())?;
            env_updates.insert(env_key, key);
        }
    }
    Ok(())
}

fn configure_channels(
    config: &mut AppConfig,
    theme: &ColorfulTheme,
    quick: bool,
) -> Result<(), String> {
    println!("{}", paint(OPTIONAL, "[OPTIONAL] Channel configuration"));
    println!(
        "{}",
        paint(
            WARN,
            "Ramifications: Telegram/Signal route message metadata through third-party services."
        )
    );
    if quick {
        let ws = config
            .channels
            .entry("websocket".to_string())
            .or_insert_with(ChannelConfig::default);
        ws.enabled = true;
        ws.extra
            .insert("port".to_string(), toml::Value::Integer(18789));
        return Ok(());
    }

    let options = vec!["CLI", "WebSocket", "Telegram", "Signal"];
    for channel in options {
        let enable = Confirm::with_theme(theme)
            .with_prompt(format!("Enable {} channel?", channel))
            .default(channel == "CLI" || channel == "WebSocket")
            .interact()
            .map_err(|e| e.to_string())?;
        let id = channel.to_lowercase();
        let entry = config
            .channels
            .entry(id.clone())
            .or_insert_with(ChannelConfig::default);
        entry.enabled = enable;
        if !enable {
            continue;
        }
        if channel == "WebSocket" {
            let port: i64 = Input::with_theme(theme)
                .with_prompt("WebSocket port")
                .default(
                    entry
                        .extra
                        .get("port")
                        .and_then(|v| v.as_integer())
                        .unwrap_or(18789),
                )
                .interact_text()
                .map_err(|e| e.to_string())?;
            entry
                .extra
                .insert("port".to_string(), toml::Value::Integer(port));
        }
        if channel == "Telegram" {
            // Telegram: offer existing token OR create new via @BotFather
            let token_choice = Select::with_theme(theme)
                .with_prompt("Telegram bot setup")
                .items(&[
                    "I already have a bot token",
                    "Create new bot via @BotFather",
                ])
                .default(0)
                .interact()
                .map_err(|e| e.to_string())?;

            let token = match token_choice {
                0 => {
                    // Existing token - prompt for it
                    Password::with_theme(theme)
                        .with_prompt("Enter your Telegram bot token")
                        .allow_empty_password(false)
                        .interact()
                        .map_err(|e| e.to_string())?
                }
                1 => {
                    // Show instructions for creating new bot
                    println!();
                    println!("{}", paint(INFO, "To create a new Telegram bot:"));
                    println!(
                        "{}",
                        paint(INFO, "1. Open Telegram and search for @BotFather")
                    );
                    println!("{}", paint(INFO, "2. Send /newbot command"));
                    println!("{}", paint(INFO, "3. Follow prompts to name your bot"));
                    println!("{}", paint(INFO, "4. Copy the token provided"));
                    println!();
                    let _ = Confirm::with_theme(theme)
                        .with_prompt("Press Enter when you have your token...")
                        .default(true)
                        .interact();

                    Password::with_theme(theme)
                        .with_prompt("Enter your Telegram bot token")
                        .allow_empty_password(false)
                        .interact()
                        .map_err(|e| e.to_string())?
                }
                _ => String::new(),
            };

            if !token.is_empty() {
                entry.credentials = Some(token);
            }

            let bot_name: String = Input::with_theme(theme)
                .with_prompt("Telegram bot username (optional, e.g., mybot)")
                .allow_empty(true)
                .interact_text()
                .map_err(|e| e.to_string())?;
            if !bot_name.is_empty() {
                entry
                    .extra
                    .insert("bot_username".to_string(), toml::Value::String(bot_name));
            }
        }
        if channel == "Signal" {
            let phone: String = Input::with_theme(theme)
                .with_prompt("Signal phone number (E.164, optional)")
                .allow_empty(true)
                .interact_text()
                .map_err(|e| e.to_string())?;
            if !phone.is_empty() {
                entry
                    .extra
                    .insert("phone".to_string(), toml::Value::String(phone));
            }
        }
        entry.dm_policy = DmPolicy::Pairing;
    }
    Ok(())
}

fn configure_security(
    config: &mut AppConfig,
    theme: &ColorfulTheme,
    quick: bool,
) -> Result<(), String> {
    println!("{}", paint(OPTIONAL, "[OPTIONAL] Sandbox and security"));
    println!(
        "{}",
        paint(
            WARN,
            "Ramifications: disabling sandbox increases capability and operational risk."
        )
    );
    if quick {
        config.security.wasm_sandbox = true;
        config.security.docker_sandbox = false;
        return Ok(());
    }
    config.security.wasm_sandbox = Confirm::with_theme(theme)
        .with_prompt("Enable WASM sandbox?")
        .default(config.security.wasm_sandbox)
        .interact()
        .map_err(|e| e.to_string())?;
    config.security.docker_sandbox = Confirm::with_theme(theme)
        .with_prompt("Enable Docker sandbox (image borgclaw/sandbox:latest)?")
        .default(config.security.docker_sandbox)
        .interact()
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn configure_memory(
    config: &mut AppConfig,
    theme: &ColorfulTheme,
    quick: bool,
) -> Result<(), String> {
    println!("{}", paint(OPTIONAL, "[OPTIONAL] Memory backend"));
    if quick {
        return Ok(());
    }
    let choices = vec![
        "SQLite + FTS5 (default)",
        "PostgreSQL + pgvector",
        "In-memory only",
    ];
    let idx = Select::with_theme(theme)
        .with_prompt("Select memory backend")
        .items(&choices)
        .default(0)
        .interact()
        .map_err(|e| e.to_string())?;
    match idx {
        1 => {
            config.memory.vector_provider = "postgres".to_string();
            let conn = build_postgres_connection(theme)?;
            config
                .memory
                .database_path
                .clone_from(&PathBuf::from(".borgclaw/memory-postgres"));
            config
                .skills
                .registry_url
                .get_or_insert_with(|| "https://github.com/openclaw/clawhub".to_string());
            config
                .registrar
                .chapters
                .entry("memory".to_string())
                .or_default()
                .push("postgres".to_string());
            config
                .channels
                .entry("memory".to_string())
                .or_insert_with(ChannelConfig::default)
                .extra
                .insert("database_url".to_string(), toml::Value::String(conn));
        }
        2 => {
            config.memory.hybrid_search = false;
            config.memory.vector_provider = "memory".to_string();
        }
        _ => {
            config.memory.vector_provider = "sqlite".to_string();
        }
    }
    Ok(())
}

fn configure_skills_registry(
    config: &mut AppConfig,
    theme: &ColorfulTheme,
    quick: bool,
) -> Result<(), String> {
    println!("{}", paint(OPTIONAL, "[OPTIONAL] Skills registries"));
    println!(
        "{}",
        paint(
            WARN,
            "Ramifications: community registries can contain unsafe skills; review before install."
        )
    );
    if quick {
        config.skills.registry_url = Some("https://github.com/openclaw/clawhub".to_string());
        return Ok(());
    }
    let use_registry = Confirm::with_theme(theme)
        .with_prompt("Enable remote skill registry?")
        .default(config.skills.registry_url.is_some())
        .interact()
        .map_err(|e| e.to_string())?;
    if use_registry {
        let url: String = Input::with_theme(theme)
            .with_prompt("Registry URL")
            .default(
                config
                    .skills
                    .registry_url
                    .clone()
                    .unwrap_or_else(|| "https://github.com/openclaw/clawhub".to_string()),
            )
            .interact_text()
            .map_err(|e| e.to_string())?;
        config.skills.registry_url = Some(url);
    } else {
        config.skills.registry_url = None;
    }
    Ok(())
}

fn print_summary(config: &AppConfig) {
    banner("ONBOARDING SUMMARY");
    println!("{} {}", paint(INFO, "Provider:"), config.agent.provider);
    println!("{} {}", paint(INFO, "Model:"), config.agent.model);
    println!("{} {:?}", paint(INFO, "Workspace:"), config.agent.workspace);
    println!(
        "{} {}",
        paint(INFO, "WASM sandbox:"),
        config.security.wasm_sandbox
    );
    println!(
        "{} {}",
        paint(INFO, "Docker sandbox:"),
        config.security.docker_sandbox
    );
    println!(
        "{} {:?}",
        paint(INFO, "Registry:"),
        config.skills.registry_url
    );
    println!("{}", paint(SUCCESS, "Channels:"));
    for (name, channel) in &config.channels {
        println!(
            "  - {}: {}",
            name,
            if channel.enabled {
                "enabled"
            } else {
                "disabled"
            }
        );
    }
}

fn apply_component_action(
    config: &mut AppConfig,
    title: &str,
    chapter: &str,
    action: &str,
    theme: &ColorfulTheme,
) -> Result<(), String> {
    match action {
        "delete" => {
            if let Some(v) = config.registrar.chapters.get_mut(title) {
                v.retain(|c| c != chapter);
            }
            if title == "channel" {
                config.channels.remove(chapter);
            }
            println!(
                "{}",
                paint(SUCCESS, format!("Deleted {}:{}", title, chapter))
            );
        }
        _ => {
            config
                .registrar
                .chapters
                .entry(title.to_string())
                .or_default()
                .push(chapter.to_string());
            if title == "channel" {
                let entry = config
                    .channels
                    .entry(chapter.to_string())
                    .or_insert_with(ChannelConfig::default);
                entry.enabled = true;
            }
            if title == "sandbox" && chapter == "docker" {
                config.security.docker_sandbox = true;
                let image: String = Input::with_theme(theme)
                    .with_prompt("Docker sandbox image")
                    .default("borgclaw/sandbox:latest".to_string())
                    .interact_text()
                    .map_err(|e| e.to_string())?;
                config
                    .registrar
                    .chapters
                    .entry("sandbox_meta".to_string())
                    .or_default()
                    .push(format!("docker_image={}", image));
            }
            println!(
                "{}",
                paint(SUCCESS, format!("Registered {}:{}", title, chapter))
            );
        }
    }
    Ok(())
}

fn component_wizard(
    config: &mut AppConfig,
    action: &str,
    theme: &ColorfulTheme,
) -> Result<(), String> {
    let title: String = Input::with_theme(theme)
        .with_prompt("Title (component type, e.g., channel/sandbox/memory/skill)")
        .interact_text()
        .map_err(|e| e.to_string())?;
    let chapter: String = Input::with_theme(theme)
        .with_prompt("Chapter (component name, e.g., telegram/docker/postgres)")
        .interact_text()
        .map_err(|e| e.to_string())?;
    apply_component_action(config, &title, &chapter, action, theme)
}

fn build_postgres_connection(theme: &ColorfulTheme) -> Result<String, String> {
    println!("{}", paint(INFO, "PostgreSQL connection builder"));
    let host: String = Input::with_theme(theme)
        .with_prompt("Host")
        .default("localhost".to_string())
        .interact_text()
        .map_err(|e| e.to_string())?;
    let port: u16 = Input::with_theme(theme)
        .with_prompt("Port")
        .default(5432)
        .interact_text()
        .map_err(|e| e.to_string())?;
    let db: String = Input::with_theme(theme)
        .with_prompt("Database")
        .default("borgclaw".to_string())
        .interact_text()
        .map_err(|e| e.to_string())?;
    let user: String = Input::with_theme(theme)
        .with_prompt("Username")
        .default("postgres".to_string())
        .interact_text()
        .map_err(|e| e.to_string())?;
    let pass: String = Password::with_theme(theme)
        .with_prompt("Password")
        .allow_empty_password(true)
        .interact()
        .map_err(|e| e.to_string())?;
    Ok(format!(
        "postgres://{}:{}@{}:{}/{}",
        user, pass, host, port, db
    ))
}

fn read_env_file(path: &PathBuf) -> HashMap<String, String> {
    let mut out = HashMap::new();
    if let Ok(content) = std::fs::read_to_string(path) {
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                out.insert(k.trim().to_string(), v.trim().to_string());
            }
        }
    }
    out
}

fn generate_env_file(
    config: &AppConfig,
    env_updates: &HashMap<String, String>,
    out_path: &PathBuf,
) -> Result<(), String> {
    let mut env = read_env_file(out_path);
    for (k, v) in env_updates {
        env.insert(k.clone(), v.clone());
    }
    env.insert(
        "BORGCLAW_PROVIDER".to_string(),
        config.agent.provider.clone(),
    );
    env.insert("BORGCLAW_MODEL".to_string(), config.agent.model.clone());

    let mut lines = Vec::new();
    lines.push("# BorgClaw Environment (generated)".to_string());
    lines.push("# Mandatory".to_string());
    if !env.contains_key("OPENAI_API_KEY") && config.agent.provider == "openai" {
        lines.push("OPENAI_API_KEY=".to_string());
    }
    if !env.contains_key("ANTHROPIC_API_KEY") && config.agent.provider == "anthropic" {
        lines.push("ANTHROPIC_API_KEY=".to_string());
    }
    if !env.contains_key("GOOGLE_API_KEY") && config.agent.provider == "google" {
        lines.push("GOOGLE_API_KEY=".to_string());
    }
    for (k, v) in &env {
        lines.push(format!("{}={}", k, v));
    }
    std::fs::write(out_path, lines.join("\n")).map_err(|e| e.to_string())
}

async fn fetch_models(
    provider: &ProviderDef,
    api_key: Option<&str>,
) -> Result<Vec<String>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|e| e.to_string())?;

    match provider.id.as_str() {
        "openai" | "custom" => {
            let mut req = client.get(&provider.models_endpoint);
            if let Some(k) = api_key {
                req = req.bearer_auth(k);
            }
            let resp: Value = req
                .send()
                .await
                .map_err(|e| e.to_string())?
                .json()
                .await
                .map_err(|e| e.to_string())?;
            let mut out = Vec::new();
            if let Some(arr) = resp.get("data").and_then(|v| v.as_array()) {
                for item in arr {
                    if let Some(id) = item.get("id").and_then(|v| v.as_str()) {
                        out.push(id.to_string());
                    }
                }
            }
            if out.is_empty() {
                return Err("No models returned".to_string());
            }
            Ok(out)
        }
        "ollama" => {
            let resp: Value = client
                .get(&provider.models_endpoint)
                .send()
                .await
                .map_err(|e| e.to_string())?
                .json()
                .await
                .map_err(|e| e.to_string())?;
            let mut out = Vec::new();
            if let Some(arr) = resp.get("models").and_then(|v| v.as_array()) {
                for item in arr {
                    if let Some(name) = item.get("name").and_then(|v| v.as_str()) {
                        out.push(name.to_string());
                    }
                }
            }
            if out.is_empty() {
                return Err("No local models returned by Ollama".to_string());
            }
            Ok(out)
        }
        "anthropic" => {
            let mut req = client
                .get(&provider.models_endpoint)
                .header("anthropic-version", "2023-06-01");
            if let Some(k) = api_key {
                req = req.header("x-api-key", k);
            }
            let resp: Value = req
                .send()
                .await
                .map_err(|e| e.to_string())?
                .json()
                .await
                .map_err(|e| e.to_string())?;
            let mut out = Vec::new();
            if let Some(arr) = resp.get("data").and_then(|v| v.as_array()) {
                for item in arr {
                    if let Some(id) = item.get("id").and_then(|v| v.as_str()) {
                        out.push(id.to_string());
                    }
                }
            }
            if out.is_empty() {
                Err("Live fetch unavailable for Anthropic in current environment".to_string())
            } else {
                Ok(out)
            }
        }
        "google" => {
            let key = api_key.unwrap_or_default();
            let url = if key.is_empty() {
                provider.models_endpoint.clone()
            } else {
                format!("{}?key={}", provider.models_endpoint, key)
            };
            let resp: Value = client
                .get(url)
                .send()
                .await
                .map_err(|e| e.to_string())?
                .json()
                .await
                .map_err(|e| e.to_string())?;
            let mut out = Vec::new();
            if let Some(arr) = resp.get("models").and_then(|v| v.as_array()) {
                for item in arr {
                    if let Some(name) = item.get("name").and_then(|v| v.as_str()) {
                        out.push(name.to_string());
                    }
                }
            }
            if out.is_empty() {
                Err("Live fetch unavailable for Google in current environment".to_string())
            } else {
                Ok(out)
            }
        }
        _ => Err("Unknown provider".to_string()),
    }
}
