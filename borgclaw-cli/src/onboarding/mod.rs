mod colors;
mod providers;

use crate::onboarding::colors::{
    banner, paint, HEADER, INFO, MANDATORY, OPTIONAL, PROMPT, SUCCESS, WARN,
};
use crate::onboarding::providers::{ProviderDef, ProviderRegistry};
use borgclaw_core::config::{AppConfig, ChannelConfig, DmPolicy};
use borgclaw_core::security::SecurityLayer;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExistingConfigAction {
    Reconfigure,
    ReconfigureSection,
    Status,
    Quit,
    AddComponent,
    DeleteComponent,
    RegenerateEnv,
}

pub struct InitOutcome {
    pub config: AppConfig,
    pub start: StartTarget,
}

fn print_provider_info(provider_id: &str) {
    println!();
    match provider_id {
        "openai" => {
            println!("{} OpenAI provides GPT models including GPT-4 and GPT-3.5.", paint(INFO, "ℹ"));
            println!("{} Get your API key at: https://platform.openai.com/api-keys", paint(INFO, "→"));
            println!("{} Pricing: https://openai.com/pricing", paint(INFO, "→"));
        }
        "anthropic" => {
            println!("{} Anthropic provides Claude models with strong reasoning capabilities.", paint(INFO, "ℹ"));
            println!("{} Get your API key at: https://console.anthropic.com/settings/keys", paint(INFO, "→"));
            println!("{} Pricing: https://www.anthropic.com/pricing", paint(INFO, "→"));
        }
        "google" => {
            println!("{} Google provides Gemini models with multimodal capabilities.", paint(INFO, "ℹ"));
            println!("{} Get your API key at: https://makersuite.google.com/app/apikey", paint(INFO, "→"));
            println!("{} Pricing: https://ai.google.dev/pricing", paint(INFO, "→"));
        }
        "kimi" => {
            println!("{} Kimi (Moonshot) provides kimi-k2.5 with 256K context and agent swarm capabilities.", paint(INFO, "ℹ"));
            println!("{} Get your API key at: https://platform.moonshot.ai/", paint(INFO, "→"));
            println!("{} Docs: https://platform.moonshot.ai/docs", paint(INFO, "→"));
        }
        "minimax" => {
            println!("{} MiniMax provides M2.7 series models optimized for agentic workflows.", paint(INFO, "ℹ"));
            println!("{} Get your API key at: https://platform.minimax.io/", paint(INFO, "→"));
            println!("{} Docs: https://platform.minimax.io/docs", paint(INFO, "→"));
        }
        "z" => {
            println!("{} Z.ai provides GLM-4.7 series models with strong coding capabilities.", paint(INFO, "ℹ"));
            println!("{} Get your API key at: https://z.ai/model-api", paint(INFO, "→"));
            println!("{} Docs: https://docs.z.ai/", paint(INFO, "→"));
        }
        "ollama" => {
            println!("{} Ollama runs models locally - no API key needed!", paint(INFO, "ℹ"));
            println!("{} Install from: https://ollama.com/download", paint(INFO, "→"));
            println!("{} Pull models: ollama pull llama3", paint(INFO, "→"));
        }
        _ => {
            println!("{} Custom OpenAI-compatible provider.", paint(INFO, "ℹ"));
            println!("{} Ensure your provider supports the OpenAI API format.", paint(INFO, "→"));
        }
    }
    println!();
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
    let mut registry = ProviderRegistry::load_or_create(&providers_path)?;

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
        let providers = ordered_providers(&registry)
            .into_iter()
            .map(|provider| provider.id.clone())
            .collect::<Vec<_>>();
        let mut changed = false;
        for provider_id in providers {
            let Some(provider) = registry.providers.get(&provider_id).cloned() else {
                continue;
            };
            let api_key = resolve_provider_api_key(&config, &provider).await;
            let models = fetch_models(&provider, api_key.as_deref()).await;
            match models {
                Ok(list) => {
                    if let Some(entry) = registry.providers.get_mut(&provider_id) {
                        entry.static_models = list.clone();
                    }
                    changed = true;
                    println!("{} {} {}", paint(SUCCESS, "OK"), provider.id, list.len());
                }
                Err(e) => println!("{} {} {}", paint(WARN, "WARN"), provider.id, e),
            }
        }
        if changed {
            registry.save(&providers_path)?;
            println!(
                "{}",
                paint(SUCCESS, format!("Updated {}", providers_path.display()))
            );
        }
        return Ok(InitOutcome {
            config,
            start: StartTarget::None,
        });
    }

    if args.generate_env {
        generate_env_file(&config, &HashMap::new(), &PathBuf::from(".env")).await?;
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
        apply_component_action(
            &mut config,
            title,
            &chapter,
            &args.action,
            &theme,
            &registry,
        )?;
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
        let choices = existing_config_choices();
        let pick = Select::with_theme(&theme)
            .with_prompt("Existing config detected. Choose action")
            .items(&choices)
            .default(0)
            .interact()
            .map_err(|e| e.to_string())?;
        match existing_config_action(pick) {
            ExistingConfigAction::Reconfigure => {}
            ExistingConfigAction::ReconfigureSection => {
                reconfigure_section(&mut config, &registry, &theme, &mut env_updates).await?;
                generate_env_file(&config, &env_updates, &PathBuf::from(".env")).await?;
                return Ok(InitOutcome {
                    config,
                    start: StartTarget::None,
                });
            }
            ExistingConfigAction::Status => {
                print_summary(&config);
                return Ok(InitOutcome {
                    config,
                    start: StartTarget::None,
                });
            }
            ExistingConfigAction::Quit => {
                println!(
                    "{}",
                    paint(INFO, "Leaving current configuration unchanged.")
                );
                return Ok(InitOutcome {
                    config,
                    start: StartTarget::None,
                });
            }
            ExistingConfigAction::AddComponent => {
                component_wizard(&mut config, "add", &theme, &registry)?;
            }
            ExistingConfigAction::DeleteComponent => {
                component_wizard(&mut config, "delete", &theme, &registry)?;
            }
            ExistingConfigAction::RegenerateEnv => {
                generate_env_file(&config, &env_updates, &PathBuf::from(".env")).await?;
                return Ok(InitOutcome {
                    config,
                    start: StartTarget::None,
                });
            }
        }
    }

    configure_provider_and_model(&mut config, &registry, &theme, &mut env_updates, args.quick)
        .await?;
    configure_channels(&mut config, &theme, args.quick, &mut env_updates).await?;
    configure_security(&mut config, &theme, args.quick)?;
    configure_memory(&mut config, &theme, args.quick)?;
    configure_skills_registry(&mut config, &theme, args.quick)?;
    configure_skill_integrations(&mut config, &theme, args.quick, &mut env_updates).await?;

    let show_summary = Confirm::with_theme(&theme)
        .with_prompt("Show summary before save?")
        .default(true)
        .interact()
        .map_err(|e| e.to_string())?;
    if show_summary {
        print_summary(&config);
    }

    generate_env_file(&config, &env_updates, &PathBuf::from(".env")).await?;
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

fn existing_config_choices() -> Vec<&'static str> {
    vec![
        "Reconfigure all (full wizard)",
        "Reconfigure specific section...",
        "Status",
        "Quit",
        "Add component via wizard",
        "Delete component via wizard",
        "Keep current and only regenerate .env",
    ]
}

fn existing_config_action(pick: usize) -> ExistingConfigAction {
    match pick {
        0 => ExistingConfigAction::Reconfigure,
        1 => ExistingConfigAction::ReconfigureSection,
        2 => ExistingConfigAction::Status,
        3 => ExistingConfigAction::Quit,
        4 => ExistingConfigAction::AddComponent,
        5 => ExistingConfigAction::DeleteComponent,
        6 => ExistingConfigAction::RegenerateEnv,
        _ => ExistingConfigAction::Reconfigure,
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
        "{} This chooses the AI brain for BorgClaw. Select from commercial APIs (OpenAI, Anthropic, etc.) or local models.",
        paint(INFO, "ℹ")
    );
    println!(
        "{} Cloud providers send prompts externally; Ollama keeps everything local on your machine.",
        paint(WARN, "⚠")
    );
    println!();

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
    config.agent.rate_limit_rpm = Some(provider.rate_limit_rpm_with_default());

    let resolved_api_key = resolve_provider_api_key(config, provider).await;
    let models = fetch_models(provider, resolved_api_key.as_deref())
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
        
        // Show provider-specific info with links
        print_provider_info(&provider.id);
        
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
            store_provider_secret(&config.security, &env_key, &key).await?;
            env_updates.remove(&env_key);
        }
    }
    Ok(())
}

async fn configure_channels(
    config: &mut AppConfig,
    theme: &ColorfulTheme,
    quick: bool,
    env_updates: &mut HashMap<String, String>,
) -> Result<(), String> {
    println!("{}", paint(OPTIONAL, "[OPTIONAL] Channel configuration"));
    println!(
        "{} Channels are how BorgClaw communicates - choose where you want to interact with your agent.",
        paint(INFO, "ℹ")
    );
    println!(
        "{} Telegram/Signal route metadata externally; CLI and local WebSocket stay private.",
        paint(WARN, "⚠")
    );
    println!(
        "{} Telegram Bot Father: https://t.me/BotFather | Signal: https://signal.org/download/",
        paint(INFO, "→")
    );
    println!();
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

    let options = vec!["CLI", "WebSocket", "Webhook", "Telegram", "Signal"];
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
        if channel == "Webhook" {
            let port: i64 = Input::with_theme(theme)
                .with_prompt("Webhook port")
                .default(
                    entry
                        .extra
                        .get("port")
                        .and_then(|v| v.as_integer())
                        .unwrap_or(8080),
                )
                .interact_text()
                .map_err(|e| e.to_string())?;
            entry
                .extra
                .insert("port".to_string(), toml::Value::Integer(port));

            let rate_limit: i64 = Input::with_theme(theme)
                .with_prompt("Webhook rate limit per minute")
                .default(
                    entry
                        .extra
                        .get("rate_limit_per_minute")
                        .and_then(|v| v.as_integer())
                        .unwrap_or(60),
                )
                .interact_text()
                .map_err(|e| e.to_string())?;
            entry.extra.insert(
                "rate_limit_per_minute".to_string(),
                toml::Value::Integer(rate_limit),
            );

            if Confirm::with_theme(theme)
                .with_prompt("Configure WEBHOOK_SECRET now?")
                .default(true)
                .interact()
                .map_err(|e| e.to_string())?
            {
                let secret = Password::with_theme(theme)
                    .with_prompt("Enter WEBHOOK_SECRET")
                    .allow_empty_password(false)
                    .interact()
                    .map_err(|e| e.to_string())?;
                store_provider_secret(&config.security, "WEBHOOK_SECRET", &secret).await?;
                env_updates.remove("WEBHOOK_SECRET");
                entry.extra.insert(
                    "secret".to_string(),
                    toml::Value::String("${WEBHOOK_SECRET}".to_string()),
                );
            }
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
                store_provider_secret(&config.security, "TELEGRAM_BOT_TOKEN", &token).await?;
                env_updates.remove("TELEGRAM_BOT_TOKEN");
                entry.credentials = Some("${TELEGRAM_BOT_TOKEN}".to_string());
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
                    .insert("phone_number".to_string(), toml::Value::String(phone));
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
        return Ok(());
    }
    config.security.wasm_sandbox = Confirm::with_theme(theme)
        .with_prompt("Enable WASM sandbox? (Recommended: provides secure plugin execution)")
        .default(config.security.wasm_sandbox)
        .interact()
        .map_err(|e| e.to_string())?;

    if config.security.secrets_encryption {
        let key_path = borgclaw_core::security::secrets_key_path(&config.security.secrets_path);
        println!(
            "{}",
            paint(
                WARN,
                &format!(
                    "Encryption key: {}\n  Back up this file — without it, stored secrets cannot be recovered.",
                    key_path.display()
                ),
            )
        );
    }
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
            let _conn = build_postgres_connection(theme)?;
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

async fn configure_skill_integrations(
    config: &mut AppConfig,
    theme: &ColorfulTheme,
    quick: bool,
    env_updates: &mut HashMap<String, String>,
) -> Result<(), String> {
    println!("{}", paint(OPTIONAL, "[OPTIONAL] Skill integrations"));
    println!(
        "{}",
        paint(
            WARN,
            "Ramifications: integration credentials unlock external side effects and API usage."
        )
    );

    configure_github_skill(config, theme, quick, env_updates).await?;
    configure_google_skill(config, theme, quick, env_updates).await?;
    configure_browser_skill(config, theme, quick)?;
    configure_stt_skill(config, theme, quick, env_updates).await?;
    configure_tts_skill(config, theme, quick, env_updates).await?;
    configure_image_skill(config, theme, quick, env_updates).await?;
    configure_url_shortener_skill(config, theme, quick, env_updates).await?;

    Ok(())
}

async fn configure_github_skill(
    config: &mut AppConfig,
    theme: &ColorfulTheme,
    quick: bool,
    env_updates: &mut HashMap<String, String>,
) -> Result<(), String> {
    let enable = if quick {
        false
    } else {
        Confirm::with_theme(theme)
            .with_prompt("Configure GitHub integration now?")
            .default(!config.skills.github.token.is_empty())
            .interact()
            .map_err(|e| e.to_string())?
    };

    if !enable {
        return Ok(());
    }

    let env_key = "GITHUB_TOKEN";
    let token = Password::with_theme(theme)
        .with_prompt("Enter GITHUB_TOKEN")
        .allow_empty_password(false)
        .interact()
        .map_err(|e| e.to_string())?;
    store_provider_secret(&config.security, env_key, &token).await?;
    env_updates.remove(env_key);
    config.skills.github.token = format!("${{{}}}", env_key);

    let user_agent: String = Input::with_theme(theme)
        .with_prompt("GitHub user agent")
        .default(config.skills.github.user_agent.clone())
        .interact_text()
        .map_err(|e| e.to_string())?;
    config.skills.github.user_agent = user_agent;

    let repo_access_items = vec!["owned_only", "allowlist", "all"];
    let default_access = repo_access_items
        .iter()
        .position(|item| *item == config.skills.github.safety.repo_access)
        .unwrap_or(0);
    let access_idx = Select::with_theme(theme)
        .with_prompt("GitHub repo access policy")
        .items(&repo_access_items)
        .default(default_access)
        .interact()
        .map_err(|e| e.to_string())?;
    config.skills.github.safety.repo_access = repo_access_items[access_idx].to_string();

    if config.skills.github.safety.repo_access == "allowlist" {
        let allowlist: String = Input::with_theme(theme)
            .with_prompt("Allowlisted repos (comma-separated owner/repo)")
            .default(config.skills.github.safety.allowlist.join(","))
            .interact_text()
            .map_err(|e| e.to_string())?;
        config.skills.github.safety.allowlist = allowlist
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
            .collect();
    } else {
        config.skills.github.safety.allowlist.clear();
    }

    config.skills.github.safety.require_confirmation = Confirm::with_theme(theme)
        .with_prompt("Require confirmation for destructive GitHub operations?")
        .default(config.skills.github.safety.require_confirmation)
        .interact()
        .map_err(|e| e.to_string())?;

    Ok(())
}

async fn configure_google_skill(
    config: &mut AppConfig,
    theme: &ColorfulTheme,
    quick: bool,
    env_updates: &mut HashMap<String, String>,
) -> Result<(), String> {
    let enable = if quick {
        false
    } else {
        Confirm::with_theme(theme)
            .with_prompt("Configure Google Workspace integration now?")
            .default(!config.skills.google.client_id.is_empty())
            .interact()
            .map_err(|e| e.to_string())?
    };

    if !enable {
        return Ok(());
    }

    let client_id_key = "GOOGLE_CLIENT_ID";
    let client_secret_key = "GOOGLE_CLIENT_SECRET";

    let client_id: String = Input::with_theme(theme)
        .with_prompt("Enter GOOGLE_CLIENT_ID")
        .interact_text()
        .map_err(|e| e.to_string())?;
    let client_secret = Password::with_theme(theme)
        .with_prompt("Enter GOOGLE_CLIENT_SECRET")
        .allow_empty_password(false)
        .interact()
        .map_err(|e| e.to_string())?;

    store_provider_secret(&config.security, client_id_key, &client_id).await?;
    store_provider_secret(&config.security, client_secret_key, &client_secret).await?;
    env_updates.remove(client_id_key);
    env_updates.remove(client_secret_key);

    config.skills.google.client_id = format!("${{{}}}", client_id_key);
    config.skills.google.client_secret = format!("${{{}}}", client_secret_key);

    let redirect_uri: String = Input::with_theme(theme)
        .with_prompt("Google OAuth redirect URI")
        .default(config.skills.google.redirect_uri.clone())
        .interact_text()
        .map_err(|e| e.to_string())?;
    config.skills.google.redirect_uri = redirect_uri;

    let token_path: String = Input::with_theme(theme)
        .with_prompt("Google OAuth token path")
        .default(config.skills.google.token_path.display().to_string())
        .interact_text()
        .map_err(|e| e.to_string())?;
    config.skills.google.token_path = PathBuf::from(token_path);

    Ok(())
}

fn configure_browser_skill(
    config: &mut AppConfig,
    theme: &ColorfulTheme,
    quick: bool,
) -> Result<(), String> {
    if quick {
        return Ok(());
    }

    let enable = Confirm::with_theme(theme)
        .with_prompt("Configure browser automation now?")
        .default(true)
        .interact()
        .map_err(|e| e.to_string())?;
    if !enable {
        return Ok(());
    }

    let browser_items = vec!["chromium", "firefox", "webkit"];
    let browser_idx = Select::with_theme(theme)
        .with_prompt("Browser engine")
        .items(&browser_items)
        .default(0)
        .interact()
        .map_err(|e| e.to_string())?;
    config.skills.browser.browser = match browser_items[browser_idx] {
        "firefox" => borgclaw_core::skills::BrowserType::Firefox,
        "webkit" => borgclaw_core::skills::BrowserType::Webkit,
        _ => borgclaw_core::skills::BrowserType::Chromium,
    };

    config.skills.browser.headless = Confirm::with_theme(theme)
        .with_prompt("Run browser in headless mode?")
        .default(config.skills.browser.headless)
        .interact()
        .map_err(|e| e.to_string())?;

    let node_path: String = Input::with_theme(theme)
        .with_prompt("Node.js binary path")
        .default(config.skills.browser.node_path.display().to_string())
        .interact_text()
        .map_err(|e| e.to_string())?;
    config.skills.browser.node_path = PathBuf::from(node_path);

    let bridge_path: String = Input::with_theme(theme)
        .with_prompt("Playwright bridge path")
        .default(config.skills.browser.bridge_path.display().to_string())
        .interact_text()
        .map_err(|e| e.to_string())?;
    config.skills.browser.bridge_path = PathBuf::from(bridge_path);

    let use_cdp = Confirm::with_theme(theme)
        .with_prompt("Use CDP fallback URL?")
        .default(config.skills.browser.cdp_url.is_some())
        .interact()
        .map_err(|e| e.to_string())?;
    if use_cdp {
        let cdp_url: String = Input::with_theme(theme)
            .with_prompt("CDP URL")
            .default(
                config
                    .skills
                    .browser
                    .cdp_url
                    .clone()
                    .unwrap_or_else(|| "http://localhost:9222".to_string()),
            )
            .interact_text()
            .map_err(|e| e.to_string())?;
        config.skills.browser.cdp_url = Some(cdp_url);
    } else {
        config.skills.browser.cdp_url = None;
    }

    Ok(())
}

async fn configure_stt_skill(
    config: &mut AppConfig,
    theme: &ColorfulTheme,
    quick: bool,
    env_updates: &mut HashMap<String, String>,
) -> Result<(), String> {
    if quick {
        return Ok(());
    }

    let enable = Confirm::with_theme(theme)
        .with_prompt("Configure speech-to-text now?")
        .default(false)
        .interact()
        .map_err(|e| e.to_string())?;
    if !enable {
        return Ok(());
    }

    let backends = vec!["openai", "openwebui", "whispercpp"];
    let default_idx = backends
        .iter()
        .position(|backend| *backend == config.skills.stt.backend)
        .unwrap_or(0);
    let backend_idx = Select::with_theme(theme)
        .with_prompt("STT backend")
        .items(&backends)
        .default(default_idx)
        .interact()
        .map_err(|e| e.to_string())?;
    config.skills.stt.backend = backends[backend_idx].to_string();

    match config.skills.stt.backend.as_str() {
        "openwebui" => {
            let base_url: String = Input::with_theme(theme)
                .with_prompt("Open WebUI base URL")
                .default(config.skills.stt.openwebui.base_url.clone())
                .interact_text()
                .map_err(|e| e.to_string())?;
            config.skills.stt.openwebui.base_url = base_url;
            let api_key = Password::with_theme(theme)
                .with_prompt("Enter OPENWEBUI_API_KEY")
                .allow_empty_password(false)
                .interact()
                .map_err(|e| e.to_string())?;
            store_provider_secret(&config.security, "OPENWEBUI_API_KEY", &api_key).await?;
            env_updates.remove("OPENWEBUI_API_KEY");
            config.skills.stt.openwebui.api_key = "${OPENWEBUI_API_KEY}".to_string();
        }
        "whispercpp" => {
            let binary_path: String = Input::with_theme(theme)
                .with_prompt("whisper.cpp binary path")
                .default(
                    config
                        .skills
                        .stt
                        .whispercpp
                        .binary_path
                        .display()
                        .to_string(),
                )
                .interact_text()
                .map_err(|e| e.to_string())?;
            config.skills.stt.whispercpp.binary_path = PathBuf::from(binary_path);

            let model_path: String = Input::with_theme(theme)
                .with_prompt("whisper.cpp model path")
                .default(
                    config
                        .skills
                        .stt
                        .whispercpp
                        .model_path
                        .display()
                        .to_string(),
                )
                .interact_text()
                .map_err(|e| e.to_string())?;
            config.skills.stt.whispercpp.model_path = PathBuf::from(model_path);
        }
        _ => {
            let use_existing = std::env::var("OPENAI_API_KEY").is_ok()
                || SecurityLayer::with_config(config.security.clone())
                    .get_secret("OPENAI_API_KEY")
                    .await
                    .is_some();
            if !use_existing {
                let api_key = Password::with_theme(theme)
                    .with_prompt("Enter OPENAI_API_KEY for STT")
                    .allow_empty_password(false)
                    .interact()
                    .map_err(|e| e.to_string())?;
                store_provider_secret(&config.security, "OPENAI_API_KEY", &api_key).await?;
                env_updates.remove("OPENAI_API_KEY");
            }
            config.skills.stt.openai.api_key = "${OPENAI_API_KEY}".to_string();
        }
    }

    Ok(())
}

async fn configure_tts_skill(
    config: &mut AppConfig,
    theme: &ColorfulTheme,
    quick: bool,
    env_updates: &mut HashMap<String, String>,
) -> Result<(), String> {
    if quick {
        return Ok(());
    }

    let enable = Confirm::with_theme(theme)
        .with_prompt("Configure text-to-speech now?")
        .default(false)
        .interact()
        .map_err(|e| e.to_string())?;
    if !enable {
        return Ok(());
    }

    config.skills.tts.provider = "elevenlabs".to_string();
    let api_key = Password::with_theme(theme)
        .with_prompt("Enter ELEVENLABS_API_KEY")
        .allow_empty_password(false)
        .interact()
        .map_err(|e| e.to_string())?;
    store_provider_secret(&config.security, "ELEVENLABS_API_KEY", &api_key).await?;
    env_updates.remove("ELEVENLABS_API_KEY");
    config.skills.tts.elevenlabs.api_key = "${ELEVENLABS_API_KEY}".to_string();

    let voice_id: String = Input::with_theme(theme)
        .with_prompt("ElevenLabs voice ID")
        .default(config.skills.tts.elevenlabs.voice_id.clone())
        .interact_text()
        .map_err(|e| e.to_string())?;
    config.skills.tts.elevenlabs.voice_id = voice_id;

    let model_id: String = Input::with_theme(theme)
        .with_prompt("ElevenLabs model ID")
        .default(config.skills.tts.elevenlabs.model_id.clone())
        .interact_text()
        .map_err(|e| e.to_string())?;
    config.skills.tts.elevenlabs.model_id = model_id;

    Ok(())
}

async fn configure_image_skill(
    config: &mut AppConfig,
    theme: &ColorfulTheme,
    quick: bool,
    env_updates: &mut HashMap<String, String>,
) -> Result<(), String> {
    if quick {
        return Ok(());
    }

    let enable = Confirm::with_theme(theme)
        .with_prompt("Configure image generation now?")
        .default(false)
        .interact()
        .map_err(|e| e.to_string())?;
    if !enable {
        return Ok(());
    }

    let providers = vec!["dalle", "stable_diffusion"];
    let default_idx = providers
        .iter()
        .position(|provider| *provider == config.skills.image.provider)
        .unwrap_or(0);
    let provider_idx = Select::with_theme(theme)
        .with_prompt("Image provider")
        .items(&providers)
        .default(default_idx)
        .interact()
        .map_err(|e| e.to_string())?;
    config.skills.image.provider = providers[provider_idx].to_string();

    if config.skills.image.provider == "stable_diffusion" {
        let base_url: String = Input::with_theme(theme)
            .with_prompt("Stable Diffusion base URL")
            .default(config.skills.image.stable_diffusion.base_url.clone())
            .interact_text()
            .map_err(|e| e.to_string())?;
        config.skills.image.stable_diffusion.base_url = base_url;
    } else {
        let use_existing = std::env::var("OPENAI_API_KEY").is_ok()
            || SecurityLayer::with_config(config.security.clone())
                .get_secret("OPENAI_API_KEY")
                .await
                .is_some();
        if !use_existing {
            let api_key = Password::with_theme(theme)
                .with_prompt("Enter OPENAI_API_KEY for image generation")
                .allow_empty_password(false)
                .interact()
                .map_err(|e| e.to_string())?;
            store_provider_secret(&config.security, "OPENAI_API_KEY", &api_key).await?;
            env_updates.remove("OPENAI_API_KEY");
        }
        config.skills.image.dalle.api_key = "${OPENAI_API_KEY}".to_string();
    }

    Ok(())
}

async fn configure_url_shortener_skill(
    config: &mut AppConfig,
    theme: &ColorfulTheme,
    quick: bool,
    env_updates: &mut HashMap<String, String>,
) -> Result<(), String> {
    if quick {
        return Ok(());
    }

    let enable = Confirm::with_theme(theme)
        .with_prompt("Configure URL shortener now?")
        .default(false)
        .interact()
        .map_err(|e| e.to_string())?;
    if !enable {
        return Ok(());
    }

    let providers = vec!["isgd", "tinyurl", "yourls"];
    let default_idx = providers
        .iter()
        .position(|provider| *provider == config.skills.url_shortener.provider)
        .unwrap_or(0);
    let provider_idx = Select::with_theme(theme)
        .with_prompt("URL shortener provider")
        .items(&providers)
        .default(default_idx)
        .interact()
        .map_err(|e| e.to_string())?;
    config.skills.url_shortener.provider = providers[provider_idx].to_string();

    if config.skills.url_shortener.provider == "yourls" {
        let base_url: String = Input::with_theme(theme)
            .with_prompt("YOURLS base URL")
            .default(config.skills.url_shortener.yourls.base_url.clone())
            .interact_text()
            .map_err(|e| e.to_string())?;
        config.skills.url_shortener.yourls.base_url = base_url;

        let username: String = Input::with_theme(theme)
            .with_prompt("YOURLS username")
            .default(config.skills.url_shortener.yourls.username.clone())
            .interact_text()
            .map_err(|e| e.to_string())?;
        config.skills.url_shortener.yourls.username = username;

        let password = Password::with_theme(theme)
            .with_prompt("Enter YOURLS_PASSWORD")
            .allow_empty_password(false)
            .interact()
            .map_err(|e| e.to_string())?;
        store_provider_secret(&config.security, "YOURLS_PASSWORD", &password).await?;
        env_updates.remove("YOURLS_PASSWORD");
        config.skills.url_shortener.yourls.password = "${YOURLS_PASSWORD}".to_string();
    }

    Ok(())
}

fn print_summary(config: &AppConfig) {
    banner("ONBOARDING SUMMARY");
    println!("{} {}", paint(INFO, "Provider:"), config.agent.provider);
    println!("{} {}", paint(INFO, "Model:"), config.agent.model);
    println!("{} {:?}", paint(INFO, "Workspace:"), config.agent.workspace);
    println!(
        "{} {} (max_instances={})",
        paint(INFO, "WASM sandbox:"),
        config.security.wasm_sandbox,
        config.security.wasm_max_instances
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
    _theme: &ColorfulTheme,
    registry: &ProviderRegistry,
) -> Result<(), String> {
    let title = title.trim().to_ascii_lowercase();
    let chapter = chapter.trim().to_ascii_lowercase();
    match action {
        "delete" => {
            if let Some(v) = config.registrar.chapters.get_mut(&title) {
                v.retain(|c| c != &chapter);
            }
            match (title.as_str(), chapter.as_str()) {
                ("channel", _) => {
                    config.channels.remove(&chapter);
                }
                ("sandbox", "wasm") => config.security.wasm_sandbox = false,
                ("memory", "sqlite") => config.memory.hybrid_search = false,
                ("memory", "vector") => config.memory.vector_provider = "sqlite".to_string(),
                ("provider", provider) if config.agent.provider == provider => {
                    config.agent.provider = AppConfig::default().agent.provider;
                    config.agent.model = AppConfig::default().agent.model;
                    config.agent.rate_limit_rpm = AppConfig::default().agent.rate_limit_rpm;
                }
                _ => {}
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
                .entry(title.clone())
                .or_default()
                .retain(|c| c != &chapter);
            config
                .registrar
                .chapters
                .entry(title.clone())
                .or_default()
                .push(chapter.clone());

            match (title.as_str(), chapter.as_str()) {
                ("channel", _) => {
                    let entry = config
                        .channels
                        .entry(chapter.clone())
                        .or_insert_with(ChannelConfig::default);
                    entry.enabled = true;
                }
                ("sandbox", "wasm") => {
                    config.security.wasm_sandbox = true;
                }
                ("memory", "sqlite") => {
                    config.memory.hybrid_search = true;
                    config.memory.vector_provider = "sqlite".to_string();
                }
                ("memory", "vector") => {
                    config.memory.hybrid_search = true;
                    if config.memory.vector_provider == "sqlite" {
                        config.memory.vector_provider = "memory".to_string();
                    }
                }
                ("provider", provider) => {
                    config.agent.provider = provider.to_string();
                    if let Some(def) = registry.providers.get(provider) {
                        config.agent.model = def.default_model.clone();
                        config.agent.rate_limit_rpm = Some(def.rate_limit_rpm_with_default());
                    }
                }
                _ => {}
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
    registry: &ProviderRegistry,
) -> Result<(), String> {
    let title: String = Input::with_theme(theme)
        .with_prompt("Title (component type, e.g., channel/sandbox/memory/skill)")
        .interact_text()
        .map_err(|e| e.to_string())?;
    let chapter: String = Input::with_theme(theme)
        .with_prompt("Chapter (component name, e.g., telegram/docker/postgres)")
        .interact_text()
        .map_err(|e| e.to_string())?;
    apply_component_action(config, &title, &chapter, action, theme, registry)
}

async fn reconfigure_section(
    config: &mut AppConfig,
    registry: &ProviderRegistry,
    theme: &ColorfulTheme,
    env_updates: &mut HashMap<String, String>,
) -> Result<(), String> {
    println!();
    println!("{}", paint(HEADER, "Reconfigure Specific Section"));
    println!("{} Select which group of settings to update:", paint(INFO, "ℹ"));
    println!();

    let sections = vec![
        ("Provider & Model", "AI brain configuration (OpenAI, Anthropic, local models)"),
        ("Channels", "Communication interfaces (Telegram, Signal, WebSocket, Webhook)"),
        ("Security", "WASM sandbox, secrets, encryption settings"),
        ("Memory", "SQLite settings, session management, context windows"),
        ("Skills Registry", "GitHub, Google Workspace, browser automation"),
        ("Skill Integrations", "STT/TTS, image generation, URL shortener"),
    ];

    let labels: Vec<String> = sections
        .iter()
        .map(|(name, desc)| format!("{} - {}", paint(HEADER, name), desc))
        .collect();

    let selection = Select::with_theme(theme)
        .with_prompt("Choose section to reconfigure")
        .items(&labels)
        .default(0)
        .interact()
        .map_err(|e| e.to_string())?;

    match selection {
        0 => configure_provider_and_model(config, registry, theme, env_updates, false).await?,
        1 => configure_channels(config, theme, false, env_updates).await?,
        2 => configure_security(config, theme, false)?,
        3 => configure_memory(config, theme, false)?,
        4 => configure_skills_registry(config, theme, false)?,
        5 => configure_skill_integrations(config, theme, false, env_updates).await?,
        _ => {}
    }

    println!();
    println!("{}", paint(SUCCESS, "Section updated successfully!"));
    Ok(())
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

async fn generate_env_file(
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

    if let Some((env_key, env_value)) = provider_env_entry(config).await {
        env.insert(env_key, env_value);
    }
    for (env_key, env_value) in integration_env_entries(config).await {
        env.insert(env_key, env_value);
    }
    for (env_key, env_value) in channel_env_entries(config).await {
        env.insert(env_key, env_value);
    }

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

async fn provider_env_entry(config: &AppConfig) -> Option<(String, String)> {
    let env_key = match config.agent.provider.as_str() {
        "openai" => "OPENAI_API_KEY",
        "anthropic" => "ANTHROPIC_API_KEY",
        "google" => "GOOGLE_API_KEY",
        _ => return None,
    };

    if let Ok(value) = std::env::var(env_key) {
        if !value.trim().is_empty() {
            return Some((env_key.to_string(), value));
        }
    }

    SecurityLayer::with_config(config.security.clone())
        .get_secret(env_key)
        .await
        .map(|value| (env_key.to_string(), value))
}

async fn integration_env_entries(config: &AppConfig) -> Vec<(String, String)> {
    let mut entries = Vec::new();

    for key in [
        "GITHUB_TOKEN",
        "GOOGLE_CLIENT_ID",
        "GOOGLE_CLIENT_SECRET",
        "OPENWEBUI_API_KEY",
        "ELEVENLABS_API_KEY",
        "YOURLS_PASSWORD",
    ] {
        if let Some(value) = config_secret_entry(config, key).await {
            entries.push((key.to_string(), value));
        }
    }

    entries
}

async fn channel_env_entries(config: &AppConfig) -> Vec<(String, String)> {
    let mut entries = Vec::new();

    if let Some(value) = channel_credentials_entry(config, "telegram", "TELEGRAM_BOT_TOKEN").await {
        entries.push(("TELEGRAM_BOT_TOKEN".to_string(), value));
    }

    if let Some(value) = channel_secret_entry(config, "webhook", "secret", "WEBHOOK_SECRET").await {
        entries.push(("WEBHOOK_SECRET".to_string(), value));
    }

    entries
}

async fn channel_secret_entry(
    config: &AppConfig,
    channel_name: &str,
    key: &str,
    env_key: &str,
) -> Option<String> {
    let channel = config.channels.get(channel_name)?;
    let configured = channel.extra.get(key)?.as_str()?;
    if configured != format!("${{{}}}", env_key) {
        return None;
    }

    config_secret_entry(config, env_key).await
}

async fn channel_credentials_entry(
    config: &AppConfig,
    channel_name: &str,
    env_key: &str,
) -> Option<String> {
    let channel = config.channels.get(channel_name)?;
    let configured = channel.credentials.as_deref()?;
    if configured != format!("${{{}}}", env_key) {
        return None;
    }

    config_secret_entry(config, env_key).await
}

async fn config_secret_entry(config: &AppConfig, env_key: &str) -> Option<String> {
    if let Ok(value) = std::env::var(env_key) {
        if !value.trim().is_empty() {
            return Some(value);
        }
    }

    SecurityLayer::with_config(config.security.clone())
        .get_secret(env_key)
        .await
}

async fn resolve_provider_api_key(config: &AppConfig, provider: &ProviderDef) -> Option<String> {
    let env_key = provider.api_key_env.as_deref()?;

    if let Ok(value) = std::env::var(env_key) {
        if !value.trim().is_empty() {
            return Some(value);
        }
    }

    SecurityLayer::with_config(config.security.clone())
        .get_secret(env_key)
        .await
}

async fn store_provider_secret(
    security_config: &borgclaw_core::config::SecurityConfig,
    env_key: &str,
    value: &str,
) -> Result<(), String> {
    SecurityLayer::with_config(security_config.clone())
        .store_secret(env_key, value)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn test_theme() -> ColorfulTheme {
        ColorfulTheme::default()
    }

    fn refresh_args() -> InitArgs {
        InitArgs {
            quick: false,
            update: false,
            reset: false,
            list_providers: false,
            refresh_models: true,
            generate_env: false,
            component: None,
            chapter: None,
            action: "add".to_string(),
            start: "none".to_string(),
        }
    }

    #[test]
    fn existing_config_choices_include_documented_status_flow() {
        let choices = existing_config_choices();
        assert_eq!(choices[0], "Reconfigure");
        assert_eq!(choices[1], "Status");
        assert_eq!(choices[2], "Quit");
        assert!(matches!(
            existing_config_action(1),
            ExistingConfigAction::Status
        ));
        assert!(matches!(
            existing_config_action(2),
            ExistingConfigAction::Quit
        ));
    }

    #[test]
    fn provider_secret_is_persisted_without_env_file_copy() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_onboarding_secret_test_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let mut security = borgclaw_core::config::SecurityConfig::default();
        security.secrets_path = root.join("secrets.enc");

        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime
            .block_on(store_provider_secret(
                &security,
                "ANTHROPIC_API_KEY",
                "secret-value",
            ))
            .unwrap();
        let restored = runtime
            .block_on(SecurityLayer::with_config(security.clone()).get_secret("ANTHROPIC_API_KEY"));
        assert_eq!(restored.as_deref(), Some("secret-value"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn generate_env_includes_provider_secret_from_secure_store() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_onboarding_env_test_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();

        let mut config = AppConfig::default();
        config.agent.provider = "anthropic".to_string();
        config.security.secrets_path = root.join("secrets.enc");

        let env_path = root.join(".env");
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime
            .block_on(store_provider_secret(
                &config.security,
                "ANTHROPIC_API_KEY",
                "secret-value",
            ))
            .unwrap();
        runtime
            .block_on(generate_env_file(&config, &HashMap::new(), &env_path))
            .unwrap();

        let env = std::fs::read_to_string(&env_path).unwrap();
        assert!(env.contains("BORGCLAW_PROVIDER=anthropic"));
        assert!(env.contains("ANTHROPIC_API_KEY=secret-value"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolve_provider_api_key_prefers_secure_store_when_env_missing() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_onboarding_provider_key_test_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();

        let mut config = AppConfig::default();
        config.security.secrets_path = root.join("secrets.enc");
        let provider = ProviderDef {
            id: "anthropic".to_string(),
            display: "Anthropic".to_string(),
            api_base: "https://api.anthropic.com/v1".to_string(),
            models_endpoint: "https://api.anthropic.com/v1/models".to_string(),
            api_key_env: Some("ANTHROPIC_API_KEY".to_string()),
            default_model: "claude-sonnet-4-20250514".to_string(),
            static_models: vec![],
            requires_auth: true,
            ..Default::default()
        };

        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime
            .block_on(store_provider_secret(
                &config.security,
                "ANTHROPIC_API_KEY",
                "secret-value",
            ))
            .unwrap();

        let resolved = runtime.block_on(resolve_provider_api_key(&config, &provider));
        assert_eq!(resolved.as_deref(), Some("secret-value"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn generate_env_includes_skill_integration_secrets_from_secure_store() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_onboarding_skill_env_test_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();

        let mut config = AppConfig::default();
        config.security.secrets_path = root.join("secrets.enc");
        config.security.secrets_encryption = false;
        config.skills.github.token = "${GITHUB_TOKEN}".to_string();
        config.skills.google.client_id = "${GOOGLE_CLIENT_ID}".to_string();
        config.skills.google.client_secret = "${GOOGLE_CLIENT_SECRET}".to_string();
        config.skills.tts.elevenlabs.api_key = "${ELEVENLABS_API_KEY}".to_string();
        config.skills.url_shortener.yourls.password = "${YOURLS_PASSWORD}".to_string();

        let env_path = root.join(".env");
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime
            .block_on(store_provider_secret(
                &config.security,
                "GITHUB_TOKEN",
                "ghp-secret",
            ))
            .unwrap();
        runtime
            .block_on(store_provider_secret(
                &config.security,
                "GOOGLE_CLIENT_ID",
                "google-client-id",
            ))
            .unwrap();
        runtime
            .block_on(store_provider_secret(
                &config.security,
                "GOOGLE_CLIENT_SECRET",
                "google-client-secret",
            ))
            .unwrap();
        runtime
            .block_on(store_provider_secret(
                &config.security,
                "ELEVENLABS_API_KEY",
                "eleven-secret",
            ))
            .unwrap();
        runtime
            .block_on(store_provider_secret(
                &config.security,
                "YOURLS_PASSWORD",
                "yourls-secret",
            ))
            .unwrap();

        std::env::remove_var("GITHUB_TOKEN");

        runtime
            .block_on(generate_env_file(&config, &HashMap::new(), &env_path))
            .unwrap();

        let env = std::fs::read_to_string(&env_path).unwrap();
        assert!(env.contains("GITHUB_TOKEN=ghp-secret"));
        assert!(env.contains("GOOGLE_CLIENT_ID=google-client-id"));
        assert!(env.contains("GOOGLE_CLIENT_SECRET=google-client-secret"));
        assert!(env.contains("ELEVENLABS_API_KEY=eleven-secret"));
        assert!(env.contains("YOURLS_PASSWORD=yourls-secret"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn provider_component_action_updates_agent_provider_and_model() {
        let mut config = AppConfig::default();
        let registry = ProviderRegistry::default_registry();

        apply_component_action(
            &mut config,
            "provider",
            "openai",
            "add",
            &test_theme(),
            &registry,
        )
        .unwrap();

        assert_eq!(config.agent.provider, "openai");
        assert_eq!(config.agent.model, "gpt-4o");
        assert_eq!(
            config.registrar.chapters.get("provider").unwrap(),
            &vec!["openai".to_string()]
        );
    }

    #[test]
    fn memory_component_action_updates_runtime_memory_mode() {
        let mut config = AppConfig::default();
        let registry = ProviderRegistry::default_registry();

        apply_component_action(
            &mut config,
            "memory",
            "vector",
            "add",
            &test_theme(),
            &registry,
        )
        .unwrap();

        assert_eq!(config.memory.vector_provider, "memory");
        assert!(config.memory.hybrid_search);
    }

    #[test]
    fn deleting_channel_component_removes_channel_config() {
        let mut config = AppConfig::default();
        let registry = ProviderRegistry::default_registry();
        config
            .channels
            .insert("telegram".to_string(), ChannelConfig::default());
        config
            .registrar
            .chapters
            .insert("channel".to_string(), vec!["telegram".to_string()]);

        apply_component_action(
            &mut config,
            "channel",
            "telegram",
            "delete",
            &test_theme(),
            &registry,
        )
        .unwrap();

        assert!(!config.channels.contains_key("telegram"));
        assert!(config.registrar.chapters["channel"]
            .iter()
            .all(|chapter| chapter != "telegram"));
    }

    #[test]
    fn generate_env_includes_webhook_secret_from_secure_store() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_onboarding_webhook_env_test_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();

        let mut config = AppConfig::default();
        config.security.secrets_path = root.join("secrets.enc");
        let webhook = config.channels.entry("webhook".to_string()).or_default();
        webhook.enabled = true;
        webhook.extra.insert(
            "secret".to_string(),
            toml::Value::String("${WEBHOOK_SECRET}".to_string()),
        );

        let env_path = root.join(".env");
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime
            .block_on(store_provider_secret(
                &config.security,
                "WEBHOOK_SECRET",
                "hook-secret",
            ))
            .unwrap();
        runtime
            .block_on(generate_env_file(&config, &HashMap::new(), &env_path))
            .unwrap();

        let env = std::fs::read_to_string(&env_path).unwrap();
        assert!(env.contains("WEBHOOK_SECRET=hook-secret"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn generate_env_includes_telegram_token_from_secure_store() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_onboarding_telegram_env_test_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();

        let mut config = AppConfig::default();
        config.security.secrets_path = root.join("secrets.enc");
        let telegram = config.channels.entry("telegram".to_string()).or_default();
        telegram.enabled = true;
        telegram.credentials = Some("${TELEGRAM_BOT_TOKEN}".to_string());

        let env_path = root.join(".env");
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime
            .block_on(store_provider_secret(
                &config.security,
                "TELEGRAM_BOT_TOKEN",
                "tg-secret",
            ))
            .unwrap();
        runtime
            .block_on(generate_env_file(&config, &HashMap::new(), &env_path))
            .unwrap();

        let env = std::fs::read_to_string(&env_path).unwrap();
        assert!(env.contains("TELEGRAM_BOT_TOKEN=tg-secret"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn refresh_models_updates_provider_registry_from_live_source() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_onboarding_refresh_test_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let config_path = root.join("config.toml");
        let providers_path = root.join("providers.toml");

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = [0_u8; 1024];
            let _ = socket.read(&mut buf).await.unwrap();
            let body = r#"{"data":[{"id":"model-a"},{"id":"model-b"}]}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        std::fs::write(
            &providers_path,
            format!(
                r#"[custom]
name = "Custom"
api_base = "http://{addr}"
models_endpoint = "http://{addr}/models"
env_key = ""
default_model = "custom-model"
static_models = ["stale-model"]
"#
            ),
        )
        .unwrap();

        let outcome = run_init(&config_path, AppConfig::default(), &refresh_args())
            .await
            .unwrap();
        let registry = ProviderRegistry::load_or_create(&providers_path).unwrap();

        server.await.unwrap();
        std::fs::remove_dir_all(root).unwrap();
        assert!(matches!(outcome.start, StartTarget::None));
        assert_eq!(
            registry.providers["custom"].static_models,
            vec!["model-a".to_string(), "model-b".to_string()]
        );
    }

    #[tokio::test]
    async fn refresh_models_keeps_existing_static_models_when_live_fetch_fails() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_onboarding_refresh_fallback_test_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let config_path = root.join("config.toml");
        let providers_path = root.join("providers.toml");

        std::fs::write(
            &providers_path,
            r#"[custom]
name = "Custom"
api_base = "http://127.0.0.1:9"
models_endpoint = "http://127.0.0.1:9/models"
env_key = ""
default_model = "custom-model"
static_models = ["cached-model"]
"#,
        )
        .unwrap();

        let outcome = run_init(&config_path, AppConfig::default(), &refresh_args())
            .await
            .unwrap();
        let registry = ProviderRegistry::load_or_create(&providers_path).unwrap();

        std::fs::remove_dir_all(root).unwrap();
        assert!(matches!(outcome.start, StartTarget::None));
        assert_eq!(
            registry.providers["custom"].static_models,
            vec!["cached-model".to_string()]
        );
    }
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
        "kimi" => {
            // OpenAI-compatible model listing
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
                return Err(format!("No models returned from {}", provider.display));
            }
            Ok(out)
        }
        "minimax" | "z" => {
            // These providers don't support model listing - use static models
            Ok(provider.static_models.clone())
        }
        _ => Err(format!("Unknown provider: {}", provider.id)),
    }
}
