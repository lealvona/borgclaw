//! BorgClaw CLI - Command-line interface with REPL

mod onboarding;

use crate::onboarding::{run_init, InitArgs, StartTarget};
use borgclaw_core::{
    channel::{ChannelType, InboundMessage, MessagePayload, MessageRouter, Sender},
    config::{load_config, save_config, AppConfig, MemoryBackend},
    mcp::{
        client::{McpClient, McpClientConfig},
        transport::{
            McpTransportConfig, SseTransportConfig, StdioTransportConfig, WebSocketTransportConfig,
        },
    },
    scheduler::{Job, JobStatus, JobTrigger},
    security::{
        cancel_process_record, load_process_records, process_state_path, CommandProcessStatus,
        ProviderProfile, SecurityLayer,
    },
    skills::SkillsRegistry,
};
use clap::{Parser, Subcommand};
use colored::Colorize;
use dialoguer::Password;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{error, info};

/// Load .env file if it exists
fn load_env_file() {
    let env_path = PathBuf::from(".env");
    if env_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&env_path) {
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some((key, value)) = line.split_once('=') {
                    let key = key.trim();
                    let value = value.trim();
                    // Only set if not already set in environment
                    if std::env::var(key).is_err() {
                        std::env::set_var(key, value);
                        info!("Loaded env var from .env: {}", key);
                    }
                }
            }
        }
    }
}

/// BorgClaw CLI
#[derive(Parser)]
#[command(name = "borgclaw")]
#[command(about = "BorgClaw - Personal AI Agent", long_about = None)]
#[command(version = env!("CARGO_PKG_VERSION"))]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Config file path
    #[arg(short, long, global = true)]
    config: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start interactive REPL
    Repl,
    /// Send a message
    Send { message: String },
    /// Interactive onboarding and initialization
    Init(InitArgs),
    /// Configure settings
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Manage skills
    Skills {
        #[command(subcommand)]
        action: SkillsAction,
    },
    /// Show status
    Status,
    /// Run health check
    Doctor,
    /// Run self-test with exit status
    SelfTest,
    /// Inspect persisted scheduled tasks
    Schedules {
        #[command(subcommand)]
        action: ScheduleAction,
    },
    /// Export persisted local runtime state for backup/recovery
    Backup {
        #[command(subcommand)]
        action: BackupAction,
    },
    /// Inspect persisted heartbeat tasks
    Heartbeat {
        #[command(subcommand)]
        action: HeartbeatAction,
    },
    /// Inspect persisted sub-agent tasks
    Subagent {
        #[command(subcommand)]
        action: SubagentAction,
    },
    /// Inspect persisted background command processes
    Processes {
        #[command(subcommand)]
        action: ProcessAction,
    },
    /// Manage encrypted secrets
    Secrets {
        #[command(subcommand)]
        action: SecretAction,
    },
    /// Show comprehensive runtime status
    Runtime,
    /// Manage provider profiles
    Providers {
        #[command(subcommand)]
        action: ProviderAction,
    },
}

#[derive(Subcommand)]
enum SecretAction {
    /// List secret keys (values are not shown)
    List,
    /// Store or update a secret (prompts for value)
    Set { key: String },
    /// Delete a secret
    Delete { key: String },
    /// Check whether a secret exists
    Check { key: String },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Show current config
    Show,
    /// Set a config value (dot notation, e.g. agent.model)
    Set { key: String, value: String },
    /// Reset to defaults
    Reset,
}

#[derive(Subcommand)]
enum ProviderAction {
    /// List configured provider profiles
    List,
    /// Show one provider profile
    Show { id: String },
    /// Add or update a provider profile
    Add {
        id: String,
        provider: String,
        #[arg(long)]
        env_key: Option<String>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        api_key: Option<String>,
    },
    /// Select a provider profile for the agent
    Select { id: String },
    /// Delete a provider profile
    Delete { id: String },
}

#[derive(Subcommand)]
enum SkillsAction {
    /// List skills
    List {
        /// Optional substring filter for installed and registry skills
        filter: Option<String>,
    },
    /// Install a skill
    Install { name: String },
    /// Package a skill for distribution
    Package {
        /// Path to skill directory
        path: PathBuf,
        /// Output path for the package (optional, defaults to skill-name.tar.gz)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Publish a skill to the registry
    Publish {
        /// Path to packaged skill file (.tar.gz)
        path: PathBuf,
        /// Registry URL (optional, uses config default)
        #[arg(short, long)]
        registry: Option<String>,
        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
    },
    /// Inspect a packaged skill archive
    Inspect {
        /// Path to packaged skill file (.tar.gz)
        path: PathBuf,
    },
}

#[derive(Subcommand)]
enum ScheduleAction {
    /// List persisted scheduled tasks
    List,
    /// Show persisted details for one scheduled task
    Show { id: String },
    /// Create a new scheduled task
    Create {
        /// Task name
        name: String,
        /// Task action (command to execute)
        action: String,
        /// Trigger type: cron, interval, or oneshot
        #[arg(short, long)]
        trigger: String,
        /// Trigger value (cron expr, interval seconds, or ISO datetime)
        #[arg(short, long)]
        value: String,
        /// Optional description
        #[arg(short, long)]
        description: Option<String>,
        /// Maximum retry attempts (default: 0)
        #[arg(long, default_value = "0")]
        retries: u32,
        /// Retry delay in seconds (default: 60)
        #[arg(long, default_value = "60")]
        retry_delay: u64,
    },
    /// Delete a scheduled task
    Delete { id: String },
    /// Pause (disable) a scheduled task
    Pause { id: String },
    /// Resume (enable) a paused scheduled task
    Resume { id: String },
    /// List dead-lettered jobs
    DeadLetters,
    /// Retry (reset) a dead-lettered job
    Retry { id: String },
}

#[derive(Subcommand)]
enum BackupAction {
    /// Export persisted local runtime state to a JSON snapshot
    Export { output: PathBuf },
    /// Import persisted local runtime state from a JSON snapshot
    Import {
        /// Path to the backup snapshot JSON file
        input: PathBuf,
        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
    },
    /// Verify a backup snapshot without importing
    Verify {
        /// Path to the backup snapshot JSON file
        input: PathBuf,
    },
}

#[derive(Subcommand)]
enum HeartbeatAction {
    /// List persisted heartbeat tasks
    List,
    /// Show persisted details for one heartbeat task
    Show { id: String },
    /// Create a new heartbeat task
    Create {
        /// Task name
        name: String,
        /// Cron schedule expression
        schedule: String,
        /// Optional description
        #[arg(short, long)]
        description: Option<String>,
        /// Maximum retry attempts
        #[arg(long, default_value = "0")]
        retries: u32,
        /// Retry delay in seconds
        #[arg(long, default_value = "60")]
        retry_delay: u64,
    },
    /// Delete a heartbeat task
    Delete { id: String },
    /// Enable a heartbeat task
    Enable { id: String },
    /// Disable a heartbeat task
    Disable { id: String },
    /// Trigger a heartbeat task manually
    Trigger { id: String },
}

#[derive(Subcommand)]
enum SubagentAction {
    /// List persisted sub-agent tasks
    List,
    /// Show persisted details for one sub-agent task
    Show { id: String },
    /// Cancel a running sub-agent task
    Cancel { id: String },
}

#[derive(Subcommand)]
enum ProcessAction {
    /// List persisted background command processes
    List,
    /// Show persisted details for one background process
    Show { id: String },
    /// Cancel a running background process
    Cancel { id: String },
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    // Load .env file early so env vars can affect config
    load_env_file();

    let cli = Cli::parse();

    let config_path = cli.config.unwrap_or_else(|| {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("borgclaw")
            .join("config.toml")
    });

    if let Some(parent) = config_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let mut config = if config_path.exists() {
        match load_config(&config_path) {
            Ok(cfg) => {
                info!("Loaded config from {}", config_path.display());
                cfg
            }
            Err(e) => {
                eprintln!(
                    "ERROR: Failed to load config from {}: {}",
                    config_path.display(),
                    e
                );
                eprintln!("Using default config. Run 'borgclaw init' to reconfigure.");
                AppConfig::default()
            }
        }
    } else {
        info!(
            "No config found at {}, using defaults",
            config_path.display()
        );
        AppConfig::default()
    };

    // Override config with environment variables (BORGCLAW_* > config file)
    if let Ok(provider) = std::env::var("BORGCLAW_PROVIDER") {
        if !provider.is_empty() {
            info!(
                "Overriding provider from env: {} -> {}",
                config.agent.provider, provider
            );
            config.agent.provider = provider;
        }
    }
    if let Ok(model) = std::env::var("BORGCLAW_MODEL") {
        if !model.is_empty() {
            info!(
                "Overriding model from env: {} -> {}",
                config.agent.model, model
            );
            config.agent.model = model;
        }
    }

    info!(
        "Using config: provider={}, model={}",
        config.agent.provider, config.agent.model
    );

    match cli.command {
        Commands::Repl => repl(config, config_path).await,
        Commands::Send { message } => send_message(config, message).await,
        Commands::Init(args) => match run_init(&config_path, config, &args).await {
            Ok(outcome) => {
                // Debug: Log config being saved
                info!(
                    "Saving config to {}: provider={}, model={}",
                    config_path.display(),
                    outcome.config.agent.provider,
                    outcome.config.agent.model
                );
                if let Err(e) = save_config(&outcome.config, &config_path) {
                    error!("Failed to save config: {}", e);
                    return;
                }
                println!("Saved config to {:?}", config_path);
                println!("  Provider: {}", outcome.config.agent.provider);
                println!("  Model: {}", outcome.config.agent.model);
                if outcome.start == StartTarget::Repl {
                    repl(outcome.config, config_path).await;
                }
            }
            Err(e) => error!("Init failed: {}", e),
        },
        Commands::Config { action } => config_action(&config_path, config, action).await,
        Commands::Skills { action } => skills_action(&config, action).await,
        Commands::Status => status(config).await,
        Commands::Doctor => doctor(config).await,
        Commands::SelfTest => {
            let ok = self_test(config).await;
            std::process::exit(if ok { 0 } else { 1 });
        }
        Commands::Schedules { action } => schedules(config, action),
        Commands::Backup { action } => backup(config, action),
        Commands::Heartbeat { action } => heartbeat(config, action).await,
        Commands::Subagent { action } => subagent(config, action),
        Commands::Processes { action } => processes(config, action),
        Commands::Secrets { action } => secrets(config, action).await,
        Commands::Runtime => runtime(config).await,
        Commands::Providers { action } => providers_action(&config_path, &mut config, action).await,
    }
}

async fn repl(config: AppConfig, _config_path: PathBuf) {
    info!("Starting BorgClaw REPL...");
    let router = MessageRouter::from_config(&config);

    // Print welcome banner
    print_repl_banner(&config);

    // Load command history if available
    let history_path = get_repl_history_path();
    let mut history = load_repl_history(&history_path);

    loop {
        print!("{} ", "🧊🦾".to_string().cyan());
        std::io::Write::flush(&mut std::io::stdout()).unwrap();

        let mut input = String::new();
        if std::io::stdin().read_line(&mut input).unwrap() == 0 {
            break;
        }

        let input = input.trim();
        if input.is_empty() {
            continue;
        }

        // Add to history
        if !history.contains(&input.to_string()) {
            history.push(input.to_string());
            if history.len() > 100 {
                history.remove(0);
            }
        }

        match parse_repl_command(input) {
            ReplCommand::Exit => {
                println!("{} Assimilation complete. Goodbye!", "✓".green());
                break;
            }
            ReplCommand::Help => {
                print_help();
                continue;
            }
            ReplCommand::Status => {
                status(config.clone()).await;
                continue;
            }
            ReplCommand::Clear => {
                clear_screen();
                continue;
            }
            ReplCommand::History => {
                print_history(&history);
                continue;
            }
            ReplCommand::Message => {}
        }

        // Create message
        let message = InboundMessage {
            channel: ChannelType::cli(),
            sender: Sender::new("cli").with_name("User"),
            content: MessagePayload::text(input),
            group_id: Some("cli".to_string()),
            timestamp: chrono::Utc::now(),
            raw: serde_json::Value::Null,
        };

        match router.route(message).await {
            Ok(outcome) => {
                let text = process_response_text(&outcome.response.text);
                println!("{}", text);
            }
            Err(err) => {
                eprintln!("{} {}", "✗ Error:".red(), err);
            }
        }
    }

    // Save history
    let _ = save_repl_history(&history_path, &history);
}

/// Print the REPL welcome banner
fn print_repl_banner(config: &AppConfig) {
    println!(
        "{}",
        "╭──────────────────────────────────────────────╮".cyan()
    );
    println!(
        "{}",
        "│                                              │".cyan()
    );
    println!(
        "{}",
        format!(
            "│  🧊🦾  BorgClaw Neural Link v{:<17}│",
            env!("CARGO_PKG_VERSION")
        )
        .cyan()
    );
    println!(
        "{}",
        "│     The Hypercube Agent Collective           │".cyan()
    );
    println!(
        "{}",
        "│                                              │".cyan()
    );
    println!(
        "{}",
        format!("│  Provider: {:<34}│", config.agent.provider).cyan()
    );
    println!(
        "{}",
        format!("│  Model:    {:<34}│", config.agent.model).cyan()
    );
    println!(
        "{}",
        "│                                              │".cyan()
    );
    println!(
        "{}",
        "│  Commands: help, status, clear, history    │".dimmed()
    );
    println!(
        "{}",
        "│           exit/quit                         │".dimmed()
    );
    println!(
        "{}",
        "╰──────────────────────────────────────────────╯".cyan()
    );
    println!();
}

/// Process response text to strip thinking blocks and format nicely
fn process_response_text(text: &str) -> String {
    // Strip <think> blocks (model reasoning)
    let text = strip_think_blocks(text);

    // Trim whitespace
    let text = text.trim();

    text.to_string()
}

/// Strip <think>...</think> blocks from text
fn strip_think_blocks(text: &str) -> String {
    let mut result = String::new();
    let mut in_think_block = false;
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '<' {
            if in_think_block {
                // Check for </think>
                let ahead: String = chars.clone().take(7).collect();
                if ahead == "/think>" {
                    in_think_block = false;
                    // Skip past "/think>"
                    for _ in 0..8 {
                        chars.next();
                    }
                }
                // Don't add '<' or content inside think block
            } else {
                // Check if this is the start of a think block
                let ahead: String = chars.clone().take(5).collect();
                if ahead == "think" {
                    in_think_block = true;
                    // Skip past "think>"
                    for _ in 0..6 {
                        chars.next();
                    }
                } else {
                    // Not a think tag, add the '<'
                    result.push(ch);
                }
            }
        } else if !in_think_block {
            result.push(ch);
        }
        // If in_think_block, don't add anything
    }

    result
}

/// Get path to REPL history file
fn get_repl_history_path() -> std::path::PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("borgclaw")
        .join("repl_history")
}

/// Load REPL history from file
fn load_repl_history(path: &std::path::Path) -> Vec<String> {
    if let Ok(content) = std::fs::read_to_string(path) {
        content.lines().map(|s| s.to_string()).collect()
    } else {
        Vec::new()
    }
}

/// Save REPL history to file
fn save_repl_history(path: &std::path::Path, history: &[String]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, history.join("\n"))
}

/// Print command history
fn print_history(history: &[String]) {
    if history.is_empty() {
        println!("{} No history yet", "ℹ".blue());
        return;
    }

    println!("{}", "Command History:".cyan().bold());
    for (i, cmd) in history.iter().enumerate().rev().take(20) {
        println!("  {:>3}. {}", i + 1, cmd);
    }
}

async fn send_message(config: AppConfig, message: String) {
    let router = MessageRouter::from_config(&config);
    let inbound = InboundMessage {
        channel: ChannelType::cli(),
        sender: Sender::new("cli").with_name("User"),
        content: MessagePayload::text(message),
        group_id: Some("cli".to_string()),
        timestamp: chrono::Utc::now(),
        raw: serde_json::Value::Null,
    };

    match router.route(inbound).await {
        Ok(outcome) => println!("{}", outcome.response.text),
        Err(err) => println!("Error: {}", err),
    }
}

async fn config_action(path: &PathBuf, mut config: AppConfig, action: ConfigAction) {
    match action {
        ConfigAction::Show => {
            let toml = toml::to_string_pretty(&config).unwrap();
            println!("{}", toml);
        }
        ConfigAction::Set { key, value } => {
            if set_config_key(&mut config, &key, &value) {
                if let Err(e) = save_config(&config, path) {
                    error!("Failed to save config: {}", e);
                    return;
                }
                println!("Updated {}", key);
            } else {
                println!("Unsupported key: {}", key);
            }
        }
        ConfigAction::Reset => {
            config = AppConfig::default();
            if let Err(e) = save_config(&config, path) {
                error!("Failed to save config: {}", e);
                return;
            }
            println!("Config reset to defaults");
        }
    }
}

fn set_config_key(config: &mut AppConfig, key: &str, value: &str) -> bool {
    match key {
        "agent.provider" => config.agent.provider = value.to_string(),
        "agent.provider_profile" => config.agent.provider_profile = Some(value.to_string()),
        "agent.model" => config.agent.model = value.to_string(),
        "agent.max_tokens" => {
            if let Ok(v) = value.parse::<u32>() {
                config.agent.max_tokens = v;
            } else {
                return false;
            }
        }
        "agent.temperature" => {
            if let Ok(v) = value.parse::<f32>() {
                config.agent.temperature = v;
            } else {
                return false;
            }
        }
        "security.wasm_sandbox" => {
            if let Ok(v) = value.parse::<bool>() {
                config.security.wasm_sandbox = v;
            } else {
                return false;
            }
        }
        _ => return false,
    }
    true
}

async fn providers_action(path: &PathBuf, config: &mut AppConfig, action: ProviderAction) {
    let security = SecurityLayer::with_config(config.security.clone());
    match action {
        ProviderAction::List => match security.list_provider_profiles().await {
            Ok(profiles) if profiles.is_empty() => println!("No provider profiles configured"),
            Ok(profiles) => {
                for profile in profiles {
                    let marker = if config.agent.provider_profile.as_deref() == Some(&profile.id) {
                        "*"
                    } else {
                        " "
                    };
                    println!(
                        "{} {} [{}] model={} key={}",
                        marker,
                        profile.id,
                        profile.provider,
                        profile.model.as_deref().unwrap_or("-"),
                        if profile
                            .api_key
                            .as_deref()
                            .is_some_and(|value| !value.trim().is_empty())
                        {
                            "configured"
                        } else {
                            "missing"
                        }
                    );
                }
            }
            Err(err) => println!("Failed to list provider profiles: {}", err),
        },
        ProviderAction::Show { id } => match security.get_provider_profile(&id).await {
            Ok(Some(profile)) => {
                println!("id: {}", profile.id);
                println!("provider: {}", profile.provider);
                println!("env_key: {}", profile.env_key.as_deref().unwrap_or("-"));
                println!("model: {}", profile.model.as_deref().unwrap_or("-"));
                println!(
                    "api_key: {}",
                    if profile
                        .api_key
                        .as_deref()
                        .is_some_and(|value| !value.trim().is_empty())
                    {
                        "configured"
                    } else {
                        "missing"
                    }
                );
            }
            Ok(None) => println!("Unknown provider profile: {}", id),
            Err(err) => println!("Failed to read provider profile: {}", err),
        },
        ProviderAction::Add {
            id,
            provider,
            env_key,
            model,
            api_key,
        } => {
            let api_key = match api_key {
                Some(value) => value,
                None => match Password::new()
                    .with_prompt("Enter provider API key")
                    .allow_empty_password(true)
                    .interact()
                {
                    Ok(value) => value,
                    Err(err) => {
                        println!("Failed to read API key: {}", err);
                        return;
                    }
                },
            };
            let profile = ProviderProfile {
                id: id.clone(),
                provider,
                env_key,
                api_key: if api_key.trim().is_empty() {
                    None
                } else {
                    Some(api_key)
                },
                model,
            };
            match security.upsert_provider_profile(profile).await {
                Ok(()) => println!("Stored provider profile {}", id),
                Err(err) => println!("Failed to store provider profile: {}", err),
            }
        }
        ProviderAction::Select { id } => match security.get_provider_profile(&id).await {
            Ok(Some(profile)) => {
                config.agent.provider_profile = Some(profile.id.clone());
                config.agent.provider = profile.provider.clone();
                if let Some(model) = profile.model {
                    config.agent.model = model;
                }
                if let Err(err) = save_config(config, path) {
                    println!("Failed to save config: {}", err);
                    return;
                }
                println!("Selected provider profile {}", id);
            }
            Ok(None) => println!("Unknown provider profile: {}", id),
            Err(err) => println!("Failed to select provider profile: {}", err),
        },
        ProviderAction::Delete { id } => match security.delete_provider_profile(&id).await {
            Ok(true) => {
                if config.agent.provider_profile.as_deref() == Some(id.as_str()) {
                    config.agent.provider_profile = None;
                    if let Err(err) = save_config(config, path) {
                        println!("Deleted profile but failed to save config: {}", err);
                        return;
                    }
                }
                println!("Deleted provider profile {}", id);
            }
            Ok(false) => println!("Unknown provider profile: {}", id),
            Err(err) => println!("Failed to delete provider profile: {}", err),
        },
    }
}

async fn skills_action(config: &AppConfig, action: SkillsAction) {
    match action {
        SkillsAction::List { filter } => {
            let filter = filter.map(|value| value.to_lowercase());
            println!("Built-in skills:");
            println!("  - memory_store");
            println!("  - memory_recall");
            println!("  - execute_command");
            println!("  - read_file");
            println!("  - list_directory");
            println!("  - web_search");
            println!("  - fetch_url");
            println!("  - message");
            println!("  - schedule_task");
            let registry = SkillsRegistry::new(config.skills.skills_path.clone());
            let mut installed_ids = std::collections::HashSet::new();
            if registry.load_all().await.is_ok() {
                let mut installed = registry.list().await;
                if let Some(filter) = &filter {
                    installed.retain(|(id, name)| {
                        id.to_lowercase().contains(filter) || name.to_lowercase().contains(filter)
                    });
                }
                if !installed.is_empty() {
                    println!("\nInstalled skills:");
                    for (id, name) in installed {
                        installed_ids.insert(id.clone());
                        println!("  - {} ({})", name, id);
                    }
                }
            }
            if let Some(registry_url) = config.skills.registry_url.as_deref() {
                match fetch_registry_skills(registry_url).await {
                    Ok(skills) if !skills.is_empty() => {
                        let skills = filter_registry_skills(skills, filter.as_deref());
                        println!("\nRegistry skills:");
                        for skill in skills {
                            let status =
                                if installed_ids.contains(&registry_skill_install_id(&skill)) {
                                    "installed"
                                } else {
                                    "available"
                                };
                            println!("  - {} [{}]", skill, status);
                        }
                    }
                    Ok(_) => println!("\nRegistry skills: none found"),
                    Err(err) => println!("\nRegistry lookup failed: {}", err),
                }
            }
        }
        SkillsAction::Install { name } => {
            match install_skill(
                &config.skills.skills_path,
                &name,
                config.skills.registry_url.as_deref(),
            )
            .await
            {
                Ok(path) => println!("Installed skill to {:?}", path),
                Err(err) => println!("{}", err),
            }
        }
        SkillsAction::Package { path, output } => {
            println!("Packaging skill");
            println!("===============");
            match package_skill(&path, output.as_deref()).await {
                Ok(package_path) => {
                    println!("✓ Packaged skill to {}", package_path.display());
                    println!(
                        "  You can now install it with: borgclaw skills install {}",
                        package_path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                    );
                }
                Err(err) => println!("✗ Failed to package skill: {}", err),
            }
        }
        SkillsAction::Publish {
            path,
            registry,
            force,
        } => {
            println!("Publishing skill");
            println!("================");
            let registry_url = registry
                .as_deref()
                .or(config.skills.registry_url.as_deref())
                .unwrap_or("https://borgclaw.io/registry");
            match publish_skill(&path, registry_url, force).await {
                Ok(result) => {
                    println!("✓ Published skill successfully");
                    println!("  Registry: {}", registry_url);
                    println!("  Package: {}", result.package_id);
                    if let Some(url) = result.public_url {
                        println!("  URL: {}", url);
                    }
                }
                Err(err) => println!("✗ Failed to publish skill: {}", err),
            }
        }
        SkillsAction::Inspect { path } => {
            println!("Skill package contents");
            println!("======================");
            match inspect_skill_package(&path) {
                Ok(()) => {}
                Err(err) => println!("✗ Failed to inspect package: {}", err),
            }
        }
    }
}

async fn status(config: AppConfig) {
    println!("BorgClaw Status");
    println!("===============");
    println!("Model: {}", config.agent.model);
    println!("Provider: {}", config.agent.provider);
    println!("Workspace: {:?}", config.agent.workspace);
    println!("Heartbeat: {} minutes", config.agent.heartbeat_interval);
    println!(
        "Provider auth: {}",
        match provider_credential_status(&config).await {
            ProviderCredentialStatus::Env(_) => "configured via env",
            ProviderCredentialStatus::SecureStore(_) => "configured via secure store",
            ProviderCredentialStatus::NotRequired => "not required",
            ProviderCredentialStatus::Missing(_) => "missing",
            ProviderCredentialStatus::UnknownProvider => "unknown",
        }
    );
    println!(
        "Security: wasm_sandbox={}, docker_sandbox={}, approval={:?}",
        config.security.wasm_sandbox, config.security.docker.enabled, config.security.approval_mode
    );
    println!("Security policy: {}", security_policy_status(&config));
    println!(
        "Vault: {}",
        config
            .security
            .vault
            .provider
            .as_deref()
            .unwrap_or("disabled")
    );
    println!(
        "Memory backend: {}",
        memory_backend_label(config.memory.effective_backend())
    );
    match config.memory.effective_backend() {
        MemoryBackend::Sqlite => println!("Memory database: {:?}", config.memory.database_path),
        MemoryBackend::Postgres => {
            println!(
                "Memory connection: {}",
                if config
                    .memory
                    .connection_string
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty())
                {
                    "configured"
                } else {
                    "missing"
                }
            );
            println!(
                "Embedding endpoint: {}",
                config
                    .memory
                    .embedding_endpoint
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or("not configured")
            );
        }
        MemoryBackend::Memory => println!("Memory persistence: disabled (in-memory only)"),
    }
    println!(
        "Session compaction: max_entries={}, keep_recent={}, keep_important={}",
        config.memory.session_max_entries,
        config.memory.session_keep_recent,
        config.memory.session_keep_important
    );
    println!(
        "Heartbeat config: enabled={}, poll={}s",
        config.heartbeat.enabled, config.heartbeat.check_interval_seconds
    );
    println!(
        "Scheduler config: enabled={}, max_jobs={}, timeout={}s",
        config.scheduler.enabled,
        config.scheduler.max_concurrent_jobs,
        config.scheduler.job_timeout
    );
    println!(
        "Background state: {}",
        background_state_status(&config.agent.workspace)
    );
    println!("Skills path: {:?}", config.skills.skills_path);
    println!(
        "Skill providers: github={}, google={}, browser={:?}, stt={}, tts={}, image={}, url={}",
        if config.skills.github.token.is_empty() {
            "disabled"
        } else {
            "configured"
        },
        if config.skills.google.client_id.is_empty() {
            "disabled"
        } else {
            "configured"
        },
        config.skills.browser.browser,
        config.skills.stt.backend,
        config.skills.tts.provider,
        config.skills.image.provider,
        config.skills.url_shortener.provider,
    );
    println!("Integrations:");
    for line in integration_status_lines(&config).await {
        println!("  - {}", line);
    }
    println!("\nChannels:");
    for line in channel_status_lines(&config).await {
        println!("  - {}", line);
    }
}

async fn doctor(config: AppConfig) {
    println!("Running diagnostics...");
    if config.agent.workspace.exists() {
        println!("✓ Workspace exists");
    } else {
        println!("✗ Workspace does not exist: {:?}", config.agent.workspace);
    }
    println!("✓ Config loaded");

    let security = SecurityLayer::new();
    let check = security.check_command("rm -rf /");
    match check {
        borgclaw_core::security::CommandCheck::Blocked(_) => {
            println!("✓ Command blocklist working")
        }
        _ => println!("✗ Command blocklist not working"),
    }
    match provider_credential_status(&config).await {
        ProviderCredentialStatus::Env(env_key) => {
            println!("✓ Provider credential env present ({})", env_key)
        }
        ProviderCredentialStatus::SecureStore(env_key) => {
            println!("✓ Provider credential stored securely ({})", env_key)
        }
        ProviderCredentialStatus::NotRequired => {
            println!(
                "✓ Provider credential not required ({})",
                config.agent.provider
            )
        }
        ProviderCredentialStatus::Missing(env_key) => {
            println!("✗ Provider credential missing ({})", env_key)
        }
        ProviderCredentialStatus::UnknownProvider => {
            println!("✗ Unknown provider '{}'", config.agent.provider)
        }
    }
    report_memory_status(&config);
    if std::fs::create_dir_all(&config.skills.skills_path).is_ok()
        && config.skills.skills_path.exists()
    {
        println!("✓ Skills path available");
    } else {
        println!("✗ Skills path unavailable: {:?}", config.skills.skills_path);
    }
    for line in background_state_doctor_lines(&config.agent.workspace) {
        println!("{}", line);
    }
    match config.security.vault.provider.as_deref() {
        Some("bitwarden") if cli_path_available(&config.security.vault.bitwarden.cli_path) => {
            println!(
                "✓ Bitwarden CLI available ({})",
                config.security.vault.bitwarden.cli_path.display()
            )
        }
        Some("bitwarden") => println!(
            "✗ Bitwarden CLI missing ({})",
            config.security.vault.bitwarden.cli_path.display()
        ),
        Some("1password") if cli_path_available(&config.security.vault.one_password.cli_path) => {
            println!(
                "✓ 1Password CLI available ({})",
                config.security.vault.one_password.cli_path.display()
            )
        }
        Some("1password") => println!(
            "✗ 1Password CLI missing ({})",
            config.security.vault.one_password.cli_path.display()
        ),
        Some(other) => println!("✗ Unsupported vault provider '{}'", other),
        None => println!("• Vault integration disabled"),
    }
    for line in security_doctor_lines(&config) {
        println!("{}", line);
    }
    for line in integration_doctor_lines(&config).await {
        println!("{}", line);
    }
    for line in channel_doctor_lines(&config).await {
        println!("{}", line);
    }
    println!("\nDiagnostics complete.");
}

async fn self_test(config: AppConfig) -> bool {
    let failures = self_test_failures(&config).await;

    println!("BorgClaw self-test");
    println!("==================");

    if failures.is_empty() {
        println!("✓ PASS");
        return true;
    }

    println!("✗ FAIL ({})", pluralize(failures.len(), "issue"));
    for failure in failures {
        println!("  - {}", failure);
    }
    false
}

fn schedules(config: AppConfig, action: ScheduleAction) {
    let path = config.agent.workspace.join("scheduler.json");
    match action {
        ScheduleAction::List => {
            println!("Scheduled tasks");
            println!("===============");
            match schedule_list_lines(&path) {
                Ok(lines) if lines.is_empty() => {
                    println!("No scheduled tasks found in {}", path.display())
                }
                Ok(lines) => {
                    for line in lines {
                        println!("  - {}", line);
                    }
                }
                Err(err) => println!("Could not read {}: {}", path.display(), err),
            }
        }
        ScheduleAction::Show { id } => {
            println!("Scheduled task details");
            println!("======================");
            match schedule_detail_lines(&path, &id) {
                Ok(lines) if lines.is_empty() => {
                    println!(
                        "No scheduled task named '{}' found in {}",
                        id,
                        path.display()
                    )
                }
                Ok(lines) => {
                    for line in lines {
                        println!("{}", line);
                    }
                }
                Err(err) => println!("Could not read {}: {}", path.display(), err),
            }
        }
        ScheduleAction::Create {
            name,
            action: job_action,
            trigger,
            value,
            description,
            retries,
            retry_delay,
        } => {
            println!("Creating scheduled task");
            println!("=======================");
            match create_scheduled_job(
                &path,
                CreateScheduledJobRequest {
                    name: &name,
                    action: &job_action,
                    trigger_type: &trigger,
                    trigger_value: &value,
                    description: description.as_deref(),
                    max_retries: retries,
                    retry_delay_seconds: retry_delay,
                },
            ) {
                Ok(id) => println!("Created scheduled task '{}' (ID: {})", name, id),
                Err(err) => println!("Failed to create scheduled task: {}", err),
            }
        }
        ScheduleAction::Delete { id } => {
            println!("Deleting scheduled task");
            println!("=======================");
            match delete_scheduled_job(&path, &id) {
                Ok(true) => println!("Deleted scheduled task '{}'", id),
                Ok(false) => println!("No scheduled task '{}' found", id),
                Err(err) => println!("Failed to delete scheduled task: {}", err),
            }
        }
        ScheduleAction::Pause { id } => {
            println!("Pausing scheduled task");
            println!("======================");
            match update_job_status(&path, &id, JobStatus::Disabled) {
                Ok(true) => println!("Paused scheduled task '{}'", id),
                Ok(false) => println!("No scheduled task '{}' found", id),
                Err(err) => println!("Failed to pause scheduled task: {}", err),
            }
        }
        ScheduleAction::Resume { id } => {
            println!("Resuming scheduled task");
            println!("=======================");
            match resume_scheduled_job(&path, &id) {
                Ok(true) => println!("Resumed scheduled task '{}'", id),
                Ok(false) => println!("No scheduled task '{}' found or not paused", id),
                Err(err) => println!("Failed to resume scheduled task: {}", err),
            }
        }
        ScheduleAction::DeadLetters => {
            println!("Dead-lettered scheduled tasks");
            println!("=============================");
            match dead_lettered_jobs(&path) {
                Ok(lines) if lines.is_empty() => {
                    println!("No dead-lettered tasks found")
                }
                Ok(lines) => {
                    for line in lines {
                        println!("  - {}", line);
                    }
                }
                Err(err) => println!("Could not read {}: {}", path.display(), err),
            }
        }
        ScheduleAction::Retry { id } => {
            println!("Retrying dead-lettered task");
            println!("===========================");
            match retry_dead_lettered_job(&path, &id) {
                Ok(true) => println!("Reset dead-lettered task '{}' to pending", id),
                Ok(false) => println!("No dead-lettered task '{}' found", id),
                Err(err) => println!("Failed to retry task: {}", err),
            }
        }
    }
}

fn memory_backend_label(backend: MemoryBackend) -> &'static str {
    match backend {
        MemoryBackend::Sqlite => "sqlite",
        MemoryBackend::Postgres => "postgres",
        MemoryBackend::Memory => "memory",
    }
}

fn docker_network_label(network: borgclaw_core::config::DockerNetworkPolicy) -> &'static str {
    match network {
        borgclaw_core::config::DockerNetworkPolicy::None => "none",
        borgclaw_core::config::DockerNetworkPolicy::Bridge => "bridge",
    }
}

fn docker_workspace_mount_label(
    mount: borgclaw_core::config::DockerWorkspaceMount,
) -> &'static str {
    match mount {
        borgclaw_core::config::DockerWorkspaceMount::ReadOnly => "ro",
        borgclaw_core::config::DockerWorkspaceMount::ReadWrite => "rw",
        borgclaw_core::config::DockerWorkspaceMount::Off => "off",
    }
}

fn report_memory_status(config: &AppConfig) {
    match config.memory.effective_backend() {
        MemoryBackend::Sqlite => {
            if let Some(parent) = config.memory.database_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if config.memory.database_path.parent().is_some()
                || config.memory.database_path.is_absolute()
            {
                println!(
                    "✓ Memory backend '{}' path configured",
                    memory_backend_label(MemoryBackend::Sqlite)
                );
            } else {
                println!(
                    "✗ Memory database path unavailable: {:?}",
                    config.memory.database_path
                );
            }
        }
        MemoryBackend::Postgres => {
            if config
                .memory
                .connection_string
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
            {
                println!("✓ Memory backend 'postgres' connection configured");
            } else {
                println!("✗ Memory backend 'postgres' missing connection_string");
            }
            if config.memory.hybrid_search {
                if config
                    .memory
                    .embedding_endpoint
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty())
                {
                    println!("✓ PostgreSQL hybrid search embedding endpoint configured");
                } else {
                    println!("✗ PostgreSQL hybrid search missing embedding_endpoint");
                }
            }
        }
        MemoryBackend::Memory => {
            println!("✓ Memory backend 'memory' configured (non-persistent)");
        }
    }
}

fn backup(config: AppConfig, action: BackupAction) {
    match action {
        BackupAction::Export { output } => {
            match export_backup_snapshot(&config.agent.workspace, &output) {
                Ok(snapshot) => {
                    println!(
                        "Exported backup snapshot to {} (scheduler={}, heartbeat={}, subagents={})",
                        output.display(),
                        snapshot
                            .scheduler
                            .as_ref()
                            .map(|value| value.len())
                            .unwrap_or(0),
                        snapshot
                            .heartbeat
                            .as_ref()
                            .map(|value| value.len())
                            .unwrap_or(0),
                        snapshot
                            .subagents
                            .as_ref()
                            .map(|value| value.len())
                            .unwrap_or(0)
                    );
                }
                Err(err) => println!("Backup export failed: {}", err),
            }
        }
        BackupAction::Import { input, force } => {
            println!("Importing backup snapshot");
            println!("=======================");
            match import_backup_snapshot(&config.agent.workspace, &input, force) {
                Ok(stats) => {
                    println!("Imported backup from {}", input.display());
                    println!("  - Scheduler jobs: {}", stats.scheduler_count);
                    println!("  - Heartbeat tasks: {}", stats.heartbeat_count);
                    println!("  - Sub-agent tasks: {}", stats.subagent_count);
                }
                Err(err) => println!("Backup import failed: {}", err),
            }
        }
        BackupAction::Verify { input } => {
            println!("Verifying backup snapshot");
            println!("=========================");
            match verify_backup_snapshot(&input) {
                Ok(stats) => {
                    println!("✓ Backup is valid (version: {})", stats.version);
                    println!("  - Exported at: {}", stats.exported_at);
                    println!("  - Original workspace: {}", stats.workspace);
                    println!("  - Scheduler jobs: {}", stats.scheduler_count);
                    println!("  - Heartbeat tasks: {}", stats.heartbeat_count);
                    println!("  - Sub-agent tasks: {}", stats.subagent_count);
                }
                Err(err) => println!("✗ Backup verification failed: {}", err),
            }
        }
    }
}

async fn heartbeat(config: AppConfig, action: HeartbeatAction) {
    let path = config.agent.workspace.join("heartbeat.json");
    match action {
        HeartbeatAction::List => {
            println!("Heartbeat tasks");
            println!("===============");
            match heartbeat_list_lines(&path) {
                Ok(lines) if lines.is_empty() => {
                    println!("No heartbeat tasks found in {}", path.display())
                }
                Ok(lines) => {
                    for line in lines {
                        println!("  - {}", line);
                    }
                }
                Err(err) => println!("Could not read {}: {}", path.display(), err),
            }
        }
        HeartbeatAction::Show { id } => {
            println!("Heartbeat task details");
            println!("======================");
            match heartbeat_detail_lines(&path, &id) {
                Ok(lines) if lines.is_empty() => {
                    println!("No heartbeat task '{}' found in {}", id, path.display())
                }
                Ok(lines) => {
                    for line in lines {
                        println!("{}", line);
                    }
                }
                Err(err) => println!("Could not read {}: {}", path.display(), err),
            }
        }
        HeartbeatAction::Create {
            name,
            schedule,
            description,
            retries,
            retry_delay,
        } => {
            println!("Creating heartbeat task");
            println!("=======================");
            // Validate cron schedule
            if schedule.parse::<cron::Schedule>().is_err() {
                println!("Invalid cron schedule: '{}'", schedule);
                return;
            }
            match create_heartbeat_task(
                &path,
                &name,
                &schedule,
                description.as_deref().unwrap_or(""),
                retries,
                retry_delay,
            ) {
                Ok(id) => println!("Created heartbeat task '{}' ({})", name, id),
                Err(err) => println!("Failed to create heartbeat task: {}", err),
            }
        }
        HeartbeatAction::Delete { id } => {
            println!("Deleting heartbeat task");
            println!("=======================");
            match delete_heartbeat_task(&path, &id) {
                Ok(true) => println!("Deleted heartbeat task '{}'", id),
                Ok(false) => println!("No heartbeat task '{}' found", id),
                Err(err) => println!("Failed to delete heartbeat task: {}", err),
            }
        }
        HeartbeatAction::Enable { id } => {
            println!("Enabling heartbeat task");
            println!("=======================");
            match update_heartbeat_task_status(&path, &id, true) {
                Ok(true) => println!("Enabled heartbeat task '{}'", id),
                Ok(false) => println!("No heartbeat task '{}' found", id),
                Err(err) => println!("Failed to enable heartbeat task: {}", err),
            }
        }
        HeartbeatAction::Disable { id } => {
            println!("Disabling heartbeat task");
            println!("========================");
            match update_heartbeat_task_status(&path, &id, false) {
                Ok(true) => println!("Disabled heartbeat task '{}'", id),
                Ok(false) => println!("No heartbeat task '{}' found", id),
                Err(err) => println!("Failed to disable heartbeat task: {}", err),
            }
        }
        HeartbeatAction::Trigger { id } => {
            println!("Triggering heartbeat task");
            println!("=========================");
            match trigger_heartbeat_task(&path, &id).await {
                Ok(HeartbeatTriggerOutcome::Triggered(result)) => {
                    println!(
                        "Triggered heartbeat task '{}' [{}] {}",
                        id,
                        if result.success { "ok" } else { "failed" },
                        result.message
                    );
                }
                Ok(HeartbeatTriggerOutcome::Disabled) => {
                    println!("Heartbeat task '{}' is disabled", id);
                }
                Ok(HeartbeatTriggerOutcome::Missing) => {
                    println!("No heartbeat task '{}' found", id);
                }
                Err(err) => println!("Failed to trigger heartbeat task: {}", err),
            }
        }
    }
}

enum HeartbeatTriggerOutcome {
    Triggered(borgclaw_core::memory::HeartbeatResult),
    Disabled,
    Missing,
}

async fn trigger_heartbeat_task(
    path: &std::path::Path,
    id: &str,
) -> Result<HeartbeatTriggerOutcome, String> {
    let engine = borgclaw_core::memory::HeartbeatEngine::new().with_state_path(path.to_path_buf());

    let Some(task) = engine.get(id).await else {
        return Ok(HeartbeatTriggerOutcome::Missing);
    };

    if !task.enabled {
        return Ok(HeartbeatTriggerOutcome::Disabled);
    }

    let result = engine
        .run_task_now(id)
        .await
        .ok_or_else(|| format!("heartbeat task '{}' could not be triggered", id))?;
    Ok(HeartbeatTriggerOutcome::Triggered(result))
}

fn subagent(config: AppConfig, action: SubagentAction) {
    let path = config.agent.workspace.join("subagents.json");
    match action {
        SubagentAction::List => {
            println!("Sub-agent tasks");
            println!("===============");
            match subagent_list_lines(&path) {
                Ok(lines) if lines.is_empty() => {
                    println!("No sub-agent tasks found in {}", path.display())
                }
                Ok(lines) => {
                    for line in lines {
                        println!("  - {}", line);
                    }
                }
                Err(err) => println!("Could not read {}: {}", path.display(), err),
            }
        }
        SubagentAction::Show { id } => {
            println!("Sub-agent task details");
            println!("======================");
            match subagent_detail_lines(&path, &id) {
                Ok(lines) if lines.is_empty() => {
                    println!("No sub-agent task '{}' found in {}", id, path.display())
                }
                Ok(lines) => {
                    for line in lines {
                        println!("{}", line);
                    }
                }
                Err(err) => println!("Could not read {}: {}", path.display(), err),
            }
        }
        SubagentAction::Cancel { id } => {
            println!("Cancelling sub-agent task");
            println!("=========================");
            match cancel_subagent_task(&path, &id) {
                Ok(true) => println!("Cancelled sub-agent task '{}'", id),
                Ok(false) => println!("No sub-agent task '{}' found", id),
                Err(err) => println!("Failed to cancel sub-agent task: {}", err),
            }
        }
    }
}

fn processes(config: AppConfig, action: ProcessAction) {
    let path = process_state_path(&config.agent.workspace);
    match action {
        ProcessAction::List => {
            println!("Background command processes");
            println!("============================");
            match process_list_lines(&path) {
                Ok(lines) if lines.is_empty() => {
                    println!(
                        "No background command processes found in {}",
                        path.display()
                    )
                }
                Ok(lines) => {
                    for line in lines {
                        println!("  - {}", line);
                    }
                }
                Err(err) => println!("Could not read {}: {}", path.display(), err),
            }
        }
        ProcessAction::Show { id } => {
            println!("Background process details");
            println!("==========================");
            match process_detail_lines(&path, &id) {
                Ok(lines) if lines.is_empty() => {
                    println!("No background process '{}' found in {}", id, path.display())
                }
                Ok(lines) => {
                    for line in lines {
                        println!("{}", line);
                    }
                }
                Err(err) => println!("Could not read {}: {}", path.display(), err),
            }
        }
        ProcessAction::Cancel { id } => {
            println!("Cancelling background process");
            println!("============================");
            match cancel_process_record(&path, &id) {
                Ok(true) => println!("Cancelled background process '{}'", id),
                Ok(false) => println!("No running background process '{}' found", id),
                Err(err) => println!("Failed to cancel background process: {}", err),
            }
        }
    }
}

async fn secrets(config: AppConfig, action: SecretAction) {
    let store = borgclaw_core::security::SecretStore::with_config(
        borgclaw_core::security::SecretStoreConfig {
            encryption_enabled: config.security.secrets_encryption,
            secrets_path: Some(config.security.secrets_path.clone()),
        },
    );

    match action {
        SecretAction::List => {
            let keys = store.keys().await;
            if keys.is_empty() {
                println!("No secrets stored.");
            } else {
                println!("Stored secrets ({}):", keys.len());
                for key in keys {
                    println!("  - {}", key);
                }
            }
        }
        SecretAction::Set { key } => {
            let value = dialoguer::Password::new()
                .with_prompt(format!("Enter value for '{}'", key))
                .interact()
                .unwrap_or_default();
            match store.store(&key, &value).await {
                Ok(()) => println!("✓ Secret '{}' stored", key),
                Err(e) => println!("✗ Failed to store secret: {}", e),
            }
        }
        SecretAction::Delete { key } => match store.delete(&key).await {
            Some(_) => println!("✓ Secret '{}' deleted", key),
            None => println!("Secret '{}' not found", key),
        },
        SecretAction::Check { key } => {
            if store.exists(&key).await {
                println!("✓ Secret '{}' exists", key);
            } else {
                println!("✗ Secret '{}' not found", key);
            }
        }
    }
}

async fn runtime(config: AppConfig) {
    println!("BorgClaw Runtime Status");
    println!("=======================\n");

    let scheduler_path = config.agent.workspace.join("scheduler.json");
    println!("Scheduler: {}", scheduler_path.display());
    match schedule_list_lines(&scheduler_path) {
        Ok(lines) if lines.is_empty() => println!("  No scheduled tasks"),
        Ok(lines) => {
            for line in lines {
                println!("  - {}", line);
            }
        }
        Err(_) => println!("  (not available)"),
    }

    let heartbeat_path = config.agent.workspace.join("heartbeat.json");
    println!("\nHeartbeat: {}", heartbeat_path.display());
    match heartbeat_list_lines(&heartbeat_path) {
        Ok(lines) if lines.is_empty() => println!("  No heartbeat tasks"),
        Ok(lines) => {
            for line in lines {
                println!("  - {}", line);
            }
        }
        Err(_) => println!("  (not available)"),
    }

    let subagent_path = config.agent.workspace.join("subagents.json");
    println!("\nSub-agents: {}", subagent_path.display());
    match subagent_list_lines(&subagent_path) {
        Ok(lines) if lines.is_empty() => println!("  No sub-agent tasks"),
        Ok(lines) => {
            for line in lines {
                println!("  - {}", line);
            }
        }
        Err(_) => println!("  (not available)"),
    }

    let process_path = process_state_path(&config.agent.workspace);
    println!("\nProcesses: {}", process_path.display());
    match process_list_lines(&process_path) {
        Ok(lines) if lines.is_empty() => println!("  No background command processes"),
        Ok(lines) => {
            for line in lines {
                println!("  - {}", line);
            }
        }
        Err(_) => println!("  (not available)"),
    }

    println!(
        "\nProvider: {} ({})",
        config.agent.provider, config.agent.model
    );
    let cred_status = match provider_credential_status(&config).await {
        ProviderCredentialStatus::Env(key) => format!("env: {}", key),
        ProviderCredentialStatus::SecureStore(key) => format!("secure store: {}", key),
        ProviderCredentialStatus::NotRequired => "not required".to_string(),
        ProviderCredentialStatus::Missing(key) => format!("missing: {}", key),
        ProviderCredentialStatus::UnknownProvider => "unknown".to_string(),
    };
    println!("  Credentials: {}", cred_status);
}
async fn self_test_failures(config: &AppConfig) -> Vec<String> {
    let mut failures = Vec::new();

    if !config.agent.workspace.exists() {
        failures.push(format!(
            "workspace missing: {}",
            config.agent.workspace.display()
        ));
    }

    match provider_credential_status(config).await {
        ProviderCredentialStatus::Env(_)
        | ProviderCredentialStatus::SecureStore(_)
        | ProviderCredentialStatus::NotRequired => {}
        ProviderCredentialStatus::Missing(env_key) => {
            failures.push(format!("provider credential missing ({})", env_key));
        }
        ProviderCredentialStatus::UnknownProvider => {
            failures.push(format!("unknown provider '{}'", config.agent.provider));
        }
    }

    match config.memory.effective_backend() {
        MemoryBackend::Sqlite => {
            if !(config.memory.database_path.parent().is_some()
                || config.memory.database_path.is_absolute())
            {
                failures.push(format!(
                    "memory database path unavailable: {}",
                    config.memory.database_path.display()
                ));
            }
        }
        MemoryBackend::Postgres => {
            if config
                .memory
                .connection_string
                .as_deref()
                .map_or(true, |value| value.trim().is_empty())
            {
                failures.push("memory.connection_string missing for postgres backend".to_string());
            }
            if config.memory.hybrid_search
                && config
                    .memory
                    .embedding_endpoint
                    .as_deref()
                    .map_or(true, |value| value.trim().is_empty())
            {
                failures.push(
                    "memory.embedding_endpoint missing for postgres hybrid search".to_string(),
                );
            }
        }
        MemoryBackend::Memory => {}
    }

    if !(std::fs::create_dir_all(&config.skills.skills_path).is_ok()
        && config.skills.skills_path.exists())
    {
        failures.push(format!(
            "skills path unavailable: {}",
            config.skills.skills_path.display()
        ));
    }

    let security = SecurityLayer::new();
    if !matches!(
        security.check_command("rm -rf /"),
        borgclaw_core::security::CommandCheck::Blocked(_)
    ) {
        failures.push("command blocklist not working".to_string());
    }

    match config.security.vault.provider.as_deref() {
        Some("bitwarden") if !cli_path_available(&config.security.vault.bitwarden.cli_path) => {
            failures.push(format!(
                "Bitwarden CLI missing ({})",
                config.security.vault.bitwarden.cli_path.display()
            ));
        }
        Some("1password") if !cli_path_available(&config.security.vault.one_password.cli_path) => {
            failures.push(format!(
                "1Password CLI missing ({})",
                config.security.vault.one_password.cli_path.display()
            ));
        }
        Some(other) if other != "bitwarden" && other != "1password" => {
            failures.push(format!("unsupported vault provider '{}'", other));
        }
        _ => {}
    }

    failures.extend(
        security_doctor_lines(config)
            .into_iter()
            .filter(|line| line.starts_with('✗'))
            .map(|line| line.trim_start_matches("✗ ").to_string()),
    );
    failures.extend(
        integration_doctor_lines(config)
            .await
            .into_iter()
            .filter(|line| line.starts_with('✗'))
            .map(|line| line.trim_start_matches("✗ ").to_string()),
    );
    failures.extend(
        channel_doctor_lines(config)
            .await
            .into_iter()
            .filter(|line| line.starts_with('✗'))
            .map(|line| line.trim_start_matches("✗ ").to_string()),
    );
    failures.extend(background_state_failure_lines(&config.agent.workspace));

    failures
}

fn provider_env_var(provider: &str) -> Option<&'static str> {
    match provider {
        "openai" => Some("OPENAI_API_KEY"),
        "anthropic" => Some("ANTHROPIC_API_KEY"),
        "google" => Some("GOOGLE_API_KEY"),
        "ollama" => None,
        _ => None,
    }
}

enum ProviderCredentialStatus {
    Env(&'static str),
    SecureStore(&'static str),
    NotRequired,
    Missing(&'static str),
    UnknownProvider,
}

async fn provider_credential_status(config: &AppConfig) -> ProviderCredentialStatus {
    if config.agent.provider == "ollama" {
        return ProviderCredentialStatus::NotRequired;
    }

    let Some(env_key) = provider_env_var(&config.agent.provider) else {
        return ProviderCredentialStatus::UnknownProvider;
    };

    if std::env::var(env_key).is_ok() {
        return ProviderCredentialStatus::Env(env_key);
    }

    let security = SecurityLayer::with_config(config.security.clone());
    if security.get_secret(env_key).await.is_some() {
        ProviderCredentialStatus::SecureStore(env_key)
    } else {
        ProviderCredentialStatus::Missing(env_key)
    }
}

fn cli_path_available(binary: &std::path::Path) -> bool {
    if binary.components().count() > 1 {
        return binary.exists();
    }

    let Some(binary_name) = binary.to_str() else {
        return false;
    };

    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| {
                let candidate = dir.join(binary_name);
                candidate.exists()
                    || cfg!(windows) && dir.join(format!("{}.exe", binary_name)).exists()
            })
        })
        .unwrap_or(false)
}

fn security_policy_status(config: &AppConfig) -> String {
    format!(
        "pairing={}({} digits/{}s), prompt_injection={}({:?}), leak_detection={}({:?}), blocklist={}+{}, allowlist={}, wasm_instances={}, docker={}({}/{}, image={}, timeout={}s)",
        enabled_disabled(config.security.pairing.enabled),
        config.security.pairing.code_length,
        config.security.pairing.expiry_seconds,
        enabled_disabled(config.security.prompt_injection_defense),
        config.security.injection_action,
        enabled_disabled(config.security.secret_leak_detection),
        config.security.leak_action,
        enabled_disabled(config.security.command_blocklist),
        config.security.extra_blocked.len(),
        config.security.allowed_commands.len(),
        config.security.wasm_max_instances,
        enabled_disabled(config.security.docker.enabled),
        docker_network_label(config.security.docker.network),
        docker_workspace_mount_label(config.security.docker.workspace_mount),
        config.security.docker.image,
        config.security.docker.timeout_seconds
    )
}

fn security_doctor_lines(config: &AppConfig) -> Vec<String> {
    let mut lines = vec![
        format!(
            "{} Approval mode {:?}",
            marker(true),
            config.security.approval_mode
        ),
        format!(
            "{} Pairing {} ({} digits, {}s expiry)",
            marker(config.security.pairing.enabled),
            enabled_disabled(config.security.pairing.enabled),
            config.security.pairing.code_length,
            config.security.pairing.expiry_seconds
        ),
        format!(
            "{} Prompt injection defense {} ({:?})",
            marker(config.security.prompt_injection_defense),
            enabled_disabled(config.security.prompt_injection_defense),
            config.security.injection_action
        ),
        format!(
            "{} Secret leak detection {} ({:?})",
            marker(config.security.secret_leak_detection),
            enabled_disabled(config.security.secret_leak_detection),
            config.security.leak_action
        ),
        format!(
            "{} WASM sandbox {} (max_instances={})",
            marker(config.security.wasm_sandbox),
            enabled_disabled(config.security.wasm_sandbox),
            config.security.wasm_max_instances
        ),
        format!(
            "{} Docker sandbox {} (image={}, network={}, mount={}, timeout={}s)",
            marker(!config.security.docker.enabled || cli_path_available(&PathBuf::from("docker"))),
            enabled_disabled(config.security.docker.enabled),
            config.security.docker.image,
            docker_network_label(config.security.docker.network),
            docker_workspace_mount_label(config.security.docker.workspace_mount),
            config.security.docker.timeout_seconds
        ),
        format!(
            "{} Command blocklist {} (extra_patterns={})",
            marker(config.security.command_blocklist),
            enabled_disabled(config.security.command_blocklist),
            config.security.extra_blocked.len()
        ),
        format!(
            "{} Command allowlist {} (patterns={})",
            marker(true),
            if config.security.allowed_commands.is_empty() {
                "disabled"
            } else {
                "enabled"
            },
            config.security.allowed_commands.len()
        ),
    ];

    let workspace_status = if config.security.workspace.workspace_only {
        "workspace-only".to_string()
    } else {
        format!(
            " unrestricted ({} allowed roots, {} forbidden)",
            config.security.workspace.allowed_roots.len(),
            config.security.workspace.forbidden_paths.len()
        )
    };
    lines.push(format!(
        "{} Workspace policy {}",
        marker(config.security.workspace.workspace_only),
        workspace_status
    ));

    if config.security.docker.enabled {
        lines.push(format!(
            "{} Docker binary {}",
            marker(cli_path_available(&PathBuf::from("docker"))),
            if cli_path_available(&PathBuf::from("docker")) {
                "available"
            } else {
                "missing"
            }
        ));
        lines.push(format!(
            "{} Docker allowed tools {}",
            marker(
                config
                    .security
                    .docker
                    .allowed_tools
                    .iter()
                    .any(|name| name == "execute_command")
            ),
            config.security.docker.allowed_tools.join(", ")
        ));
    }

    lines
}

async fn integration_status_lines(config: &AppConfig) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(format!(
        "GitHub: {}",
        if skill_secret_available(config, &config.skills.github.token).await {
            "configured"
        } else {
            "not configured"
        }
    ));
    lines.push(format!(
        "Google: {}",
        if skill_secret_available(config, &config.skills.google.client_id).await
            && skill_secret_available(config, &config.skills.google.client_secret).await
        {
            "configured"
        } else {
            "not configured"
        }
    ));
    lines.push(format!(
        "Browser: node={}, bridge={}",
        binary_or_placeholder_status(&config.skills.browser.node_path),
        path_or_placeholder_status(&config.skills.browser.bridge_path)
    ));
    lines.push(format!(
        "STT: backend={} ({})",
        config.skills.stt.backend,
        stt_backend_status(config).await
    ));
    lines.push(format!(
        "TTS: {}",
        if skill_secret_available(config, &config.skills.tts.elevenlabs.api_key).await {
            "configured"
        } else {
            "not configured"
        }
    ));
    lines.push(format!(
        "Image: provider={} ({})",
        config.skills.image.provider,
        image_provider_status(config).await
    ));
    lines.push(format!(
        "URL shortener: provider={}",
        config.skills.url_shortener.provider
    ));
    let mut server_names = config.mcp.servers.keys().cloned().collect::<Vec<_>>();
    server_names.sort();
    if server_names.is_empty() {
        lines.push("MCP: none configured".to_string());
    } else {
        let servers = server_names
            .into_iter()
            .map(|name| {
                let transport = config
                    .mcp
                    .servers
                    .get(&name)
                    .map(mcp_transport_label)
                    .unwrap_or("unknown");
                format!("{name}:{transport}")
            })
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!(
            "MCP: {} configured ({})",
            config.mcp.servers.len(),
            servers
        ));
    }
    lines
}

async fn integration_doctor_lines(config: &AppConfig) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(format!(
        "{} GitHub integration {}",
        marker(skill_secret_available(config, &config.skills.github.token).await),
        if skill_secret_available(config, &config.skills.github.token).await {
            "configured"
        } else {
            "missing token"
        }
    ));
    let google_ready = skill_secret_available(config, &config.skills.google.client_id).await
        && skill_secret_available(config, &config.skills.google.client_secret).await;
    lines.push(format!(
        "{} Google OAuth {}",
        marker(google_ready),
        if google_ready {
            "configured"
        } else {
            "missing client_id or client_secret"
        }
    ));
    lines.push(format!(
        "{} Browser node path {}",
        marker(cli_path_available(&config.skills.browser.node_path)),
        config.skills.browser.node_path.display()
    ));
    lines.push(format!(
        "{} Browser bridge path {}",
        marker(path_exists_or_placeholder(
            &config.skills.browser.bridge_path
        )),
        config.skills.browser.bridge_path.display()
    ));
    lines.push(format!(
        "{} STT backend {}",
        marker(stt_backend_ready(config).await),
        config.skills.stt.backend
    ));
    lines.push(format!(
        "{} TTS credentials {}",
        marker(skill_secret_available(config, &config.skills.tts.elevenlabs.api_key).await),
        if skill_secret_available(config, &config.skills.tts.elevenlabs.api_key).await {
            "present"
        } else {
            "missing"
        }
    ));
    lines.push(format!(
        "{} Image provider {}",
        marker(image_provider_ready(config).await),
        config.skills.image.provider
    ));
    lines.push(format!(
        "{} URL shortener provider {}",
        marker(url_shortener_ready(config).await),
        config.skills.url_shortener.provider
    ));
    lines.extend(mcp_doctor_lines(config).await);
    lines.push(format!(
        "{} MCP servers configured ({})",
        marker(!config.mcp.servers.is_empty()),
        config.mcp.servers.len()
    ));
    lines
}

async fn channel_status_lines(config: &AppConfig) -> Vec<String> {
    let mut names = config.channels.keys().cloned().collect::<Vec<_>>();
    names.sort();

    let mut lines = Vec::new();
    for name in names {
        let Some(channel) = config.channels.get(&name) else {
            continue;
        };
        if !channel.enabled {
            lines.push(format!("{name}: disabled"));
            continue;
        }

        let status = match name.as_str() {
            "telegram" => {
                if channel_credentials_available(config, channel).await {
                    "ready"
                } else {
                    "missing token"
                }
            }
            "signal" => {
                if signal_channel_ready(channel) {
                    "ready"
                } else if !signal_cli_ready(channel) {
                    "missing signal-cli"
                } else {
                    "missing phone_number"
                }
            }
            "webhook" => {
                if webhook_channel_ready(config, channel).await {
                    "ready"
                } else {
                    "missing secret"
                }
            }
            "websocket" => "ready",
            _ => "enabled",
        };
        lines.push(format!("{name}: enabled ({status})"));
    }

    lines
}

async fn channel_doctor_lines(config: &AppConfig) -> Vec<String> {
    let mut names = config.channels.keys().cloned().collect::<Vec<_>>();
    names.sort();

    let mut lines = Vec::new();
    for name in names {
        let Some(channel) = config.channels.get(&name) else {
            continue;
        };
        if !channel.enabled {
            lines.push(format!("• Channel {name} disabled"));
            continue;
        }

        let line = match name.as_str() {
            "telegram" => format!(
                "{} Telegram token {}",
                marker(channel_credentials_available(config, channel).await),
                if channel_credentials_available(config, channel).await {
                    "present"
                } else {
                    "missing"
                }
            ),
            "signal" => format!(
                "{} Signal phone={} cli={}",
                marker(signal_channel_ready(channel)),
                channel
                    .extra
                    .get("phone_number")
                    .and_then(|value| value.as_str())
                    .unwrap_or("missing"),
                signal_cli_display(channel)
            ),
            "webhook" => format!(
                "{} Webhook secret {} on port {}",
                marker(webhook_channel_ready(config, channel).await),
                if webhook_channel_ready(config, channel).await {
                    "configured"
                } else {
                    "missing"
                },
                channel
                    .extra
                    .get("port")
                    .and_then(|value| value.as_integer())
                    .unwrap_or(8080)
            ),
            "websocket" => format!(
                "{} WebSocket port {} pairing={}",
                marker(true),
                channel
                    .extra
                    .get("port")
                    .and_then(|value| value.as_integer())
                    .unwrap_or(18789),
                channel
                    .extra
                    .get("require_pairing")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(matches!(
                        channel.dm_policy,
                        borgclaw_core::config::DmPolicy::Pairing
                    ))
            ),
            _ => format!("• Channel {name} enabled"),
        };
        lines.push(line);
    }

    lines
}

async fn stt_backend_status(config: &AppConfig) -> &'static str {
    if stt_backend_ready(config).await {
        "ready"
    } else {
        "missing config"
    }
}

async fn image_provider_status(config: &AppConfig) -> &'static str {
    if image_provider_ready(config).await {
        "ready"
    } else {
        "missing config"
    }
}

fn background_state_status(workspace: &std::path::Path) -> String {
    let scheduler = background_state_summary(&workspace.join("scheduler.json"));
    let heartbeat = background_state_summary(&workspace.join("heartbeat.json"));
    let subagents = background_state_summary(&workspace.join("subagents.json"));
    let processes = background_state_summary(&process_state_path(workspace));
    format!(
        "scheduler={}, heartbeat={}, subagents={}, processes={}",
        scheduler, heartbeat, subagents, processes
    )
}

fn background_state_doctor_lines(workspace: &std::path::Path) -> Vec<String> {
    vec![
        background_state_doctor_line("Scheduler", &workspace.join("scheduler.json")),
        background_state_doctor_line("Heartbeat", &workspace.join("heartbeat.json")),
        background_state_doctor_line("Sub-agent", &workspace.join("subagents.json")),
        background_state_doctor_line("Process", &process_state_path(workspace)),
    ]
}

fn background_state_doctor_line(label: &str, path: &std::path::Path) -> String {
    match background_state_counts(path) {
        Some((tasks, dead)) => format!(
            "{} {} state present ({}, dead-lettered={}) at {}",
            marker(true),
            label,
            pluralize(tasks, "task"),
            dead,
            path.display()
        ),
        None => format!("• {} state not created yet ({})", label, path.display()),
    }
}

fn background_state_summary(path: &std::path::Path) -> String {
    match background_state_counts(path) {
        Some((tasks, dead)) => format!("{} (dead-lettered={})", pluralize(tasks, "task"), dead),
        None => "not created".to_string(),
    }
}

fn background_state_failure_lines(workspace: &std::path::Path) -> Vec<String> {
    let mut failures = Vec::new();
    for (label, path) in [
        ("scheduler", workspace.join("scheduler.json")),
        ("heartbeat", workspace.join("heartbeat.json")),
        ("sub-agent", workspace.join("subagents.json")),
        ("process", process_state_path(workspace)),
    ] {
        if let Some((tasks, dead)) = background_state_counts(&path) {
            if dead > 0 {
                failures.push(format!(
                    "{} state has {} dead-lettered of {} persisted {}",
                    label,
                    dead,
                    tasks,
                    if tasks == 1 { "task" } else { "tasks" }
                ));
            }
        }
    }
    failures
}

fn background_state_counts(path: &std::path::Path) -> Option<(usize, usize)> {
    let contents = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&contents).ok()?;
    let map = value.as_object()?;
    let dead_lettered = map
        .values()
        .filter(|entry| {
            entry
                .get("dead_lettered_at")
                .map(|value| !value.is_null())
                .unwrap_or(false)
        })
        .count();
    Some((map.len(), dead_lettered))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BackupSnapshot {
    version: String,
    exported_at: String,
    workspace: String,
    scheduler: Option<serde_json::Map<String, serde_json::Value>>,
    heartbeat: Option<serde_json::Map<String, serde_json::Value>>,
    subagents: Option<serde_json::Map<String, serde_json::Value>>,
}

fn export_backup_snapshot(
    workspace: &std::path::Path,
    output: &std::path::Path,
) -> Result<BackupSnapshot, String> {
    let snapshot = BackupSnapshot {
        version: env!("CARGO_PKG_VERSION").to_string(),
        exported_at: chrono::Utc::now().to_rfc3339(),
        workspace: workspace.display().to_string(),
        scheduler: read_state_map(&workspace.join("scheduler.json"))?,
        heartbeat: read_state_map(&workspace.join("heartbeat.json"))?,
        subagents: read_state_map(&workspace.join("subagents.json"))?,
    };

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let contents = serde_json::to_string_pretty(&snapshot).map_err(|e| e.to_string())?;
    std::fs::write(output, contents).map_err(|e| e.to_string())?;

    Ok(snapshot)
}

fn read_state_map(
    path: &std::path::Path,
) -> Result<Option<serde_json::Map<String, serde_json::Value>>, String> {
    if !path.exists() {
        return Ok(None);
    }

    let contents = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let value: serde_json::Value = serde_json::from_str(&contents).map_err(|e| e.to_string())?;
    let object = value
        .as_object()
        .cloned()
        .ok_or_else(|| format!("{} does not contain a JSON object", path.display()))?;
    Ok(Some(object))
}

#[derive(Debug)]
struct ImportStats {
    scheduler_count: usize,
    heartbeat_count: usize,
    subagent_count: usize,
}

fn import_backup_snapshot(
    workspace: &std::path::Path,
    input: &std::path::Path,
    force: bool,
) -> Result<ImportStats, String> {
    if !input.exists() {
        return Err(format!("backup file not found: {}", input.display()));
    }

    let contents = std::fs::read_to_string(input).map_err(|e| e.to_string())?;
    let snapshot: BackupSnapshot =
        serde_json::from_str(&contents).map_err(|e| format!("invalid backup format: {}", e))?;

    if !force {
        println!("This will restore:");
        println!(
            "  - {} scheduler jobs",
            snapshot.scheduler.as_ref().map(|m| m.len()).unwrap_or(0)
        );
        println!(
            "  - {} heartbeat tasks",
            snapshot.heartbeat.as_ref().map(|m| m.len()).unwrap_or(0)
        );
        println!(
            "  - {} sub-agent tasks",
            snapshot.subagents.as_ref().map(|m| m.len()).unwrap_or(0)
        );
        println!("\nContinue? [y/N]");

        let mut response = String::new();
        std::io::stdin()
            .read_line(&mut response)
            .map_err(|e| e.to_string())?;
        if !response.trim().eq_ignore_ascii_case("y") {
            return Err("import cancelled by user".to_string());
        }
    }

    std::fs::create_dir_all(workspace).map_err(|e| e.to_string())?;

    let scheduler_count = if let Some(scheduler) = snapshot.scheduler {
        let path = workspace.join("scheduler.json");
        let contents = serde_json::to_string_pretty(&scheduler).map_err(|e| e.to_string())?;
        std::fs::write(&path, contents).map_err(|e| e.to_string())?;
        scheduler.len()
    } else {
        0
    };

    let heartbeat_count = if let Some(heartbeat) = snapshot.heartbeat {
        let path = workspace.join("heartbeat.json");
        let contents = serde_json::to_string_pretty(&heartbeat).map_err(|e| e.to_string())?;
        std::fs::write(&path, contents).map_err(|e| e.to_string())?;
        heartbeat.len()
    } else {
        0
    };

    let subagent_count = if let Some(subagents) = snapshot.subagents {
        let path = workspace.join("subagents.json");
        let contents = serde_json::to_string_pretty(&subagents).map_err(|e| e.to_string())?;
        std::fs::write(&path, contents).map_err(|e| e.to_string())?;
        subagents.len()
    } else {
        0
    };

    Ok(ImportStats {
        scheduler_count,
        heartbeat_count,
        subagent_count,
    })
}

#[derive(Debug)]
struct VerifyStats {
    version: String,
    exported_at: String,
    workspace: String,
    scheduler_count: usize,
    heartbeat_count: usize,
    subagent_count: usize,
}

fn verify_backup_snapshot(input: &std::path::Path) -> Result<VerifyStats, String> {
    if !input.exists() {
        return Err(format!("backup file not found: {}", input.display()));
    }

    let contents = std::fs::read_to_string(input).map_err(|e| e.to_string())?;
    let snapshot: BackupSnapshot =
        serde_json::from_str(&contents).map_err(|e| format!("invalid backup format: {}", e))?;

    Ok(VerifyStats {
        version: snapshot.version,
        exported_at: snapshot.exported_at,
        workspace: snapshot.workspace,
        scheduler_count: snapshot.scheduler.as_ref().map(|m| m.len()).unwrap_or(0),
        heartbeat_count: snapshot.heartbeat.as_ref().map(|m| m.len()).unwrap_or(0),
        subagent_count: snapshot.subagents.as_ref().map(|m| m.len()).unwrap_or(0),
    })
}

fn schedule_list_lines(path: &std::path::Path) -> Result<Vec<String>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let contents = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let jobs: std::collections::HashMap<String, Job> =
        serde_json::from_str(&contents).map_err(|e| e.to_string())?;
    let mut jobs = jobs.into_values().collect::<Vec<_>>();
    jobs.sort_by(|left, right| {
        left.next_run
            .cmp(&right.next_run)
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.id.cmp(&right.id))
    });

    Ok(jobs.into_iter().map(format_schedule_job_line).collect())
}

fn schedule_detail_lines(path: &std::path::Path, id: &str) -> Result<Vec<String>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let contents = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let jobs: std::collections::HashMap<String, Job> =
        serde_json::from_str(&contents).map_err(|e| e.to_string())?;
    let Some(job) = jobs.get(id).cloned() else {
        return Ok(Vec::new());
    };

    let mut lines = vec![
        format!("id: {}", job.id),
        format!("name: {}", job.name),
        format!("status: {}", job.status),
        format!("trigger: {}", schedule_trigger_label(&job.trigger)),
        format!(
            "description: {}",
            job.description.unwrap_or_else(|| "none".to_string())
        ),
        format!("action: {}", job.action),
        format!("created_at: {}", job.created_at.to_rfc3339()),
        format!(
            "last_run: {}",
            job.last_run
                .map(|value| value.to_rfc3339())
                .unwrap_or_else(|| "never".to_string())
        ),
        format!(
            "next_run: {}",
            job.next_run
                .map(|value| value.to_rfc3339())
                .unwrap_or_else(|| "none".to_string())
        ),
        format!("run_count: {}", job.run_count),
        format!(
            "retries: {}/{} (delay={}s)",
            job.retry_count, job.max_retries, job.retry_delay_seconds
        ),
        format!(
            "dead_lettered_at: {}",
            job.dead_lettered_at
                .map(|value| value.to_rfc3339())
                .unwrap_or_else(|| "none".to_string())
        ),
    ];

    if job.metadata.is_empty() {
        lines.push("metadata: none".to_string());
    } else {
        let mut metadata = job.metadata.into_iter().collect::<Vec<_>>();
        metadata.sort_by(|left, right| left.0.cmp(&right.0));
        lines.push(format!(
            "metadata: {}",
            metadata
                .into_iter()
                .map(|(key, value)| format!("{key}={value}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    if job.run_history.is_empty() {
        lines.push("history: none".to_string());
    } else {
        lines.push("history:".to_string());
        for run in job.run_history {
            let retry = run
                .retry_scheduled
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string());
            let error = run.error.unwrap_or_else(|| "none".to_string());
            lines.push(format!(
                "  - {} -> {} [{}] retry={} error={}",
                run.started_at.to_rfc3339(),
                run.finished_at.to_rfc3339(),
                run.status,
                retry,
                error
            ));
        }
    }

    Ok(lines)
}

fn format_schedule_job_line(job: Job) -> String {
    let next_run = job
        .next_run
        .map(|value| value.to_rfc3339())
        .unwrap_or_else(|| "none".to_string());
    let last_run = job
        .last_run
        .map(|value| value.to_rfc3339())
        .unwrap_or_else(|| "never".to_string());
    let dead_letter = if job.dead_lettered_at.is_some() {
        "dead-lettered"
    } else {
        "active"
    };
    format!(
        "{} [{}] trigger={} next={} last={} runs={} retries={}/{} {}",
        job.name,
        job.status,
        schedule_trigger_label(&job.trigger),
        next_run,
        last_run,
        job.run_count,
        job.retry_count,
        job.max_retries,
        dead_letter
    )
}

fn schedule_trigger_label(trigger: &JobTrigger) -> String {
    match trigger {
        JobTrigger::Cron(expr) => format!("cron({})", expr),
        JobTrigger::Interval(seconds) => format!("interval({}s)", seconds),
        JobTrigger::OneShot(at) => format!("oneshot({})", at.to_rfc3339()),
    }
}

struct CreateScheduledJobRequest<'a> {
    name: &'a str,
    action: &'a str,
    trigger_type: &'a str,
    trigger_value: &'a str,
    description: Option<&'a str>,
    max_retries: u32,
    retry_delay_seconds: u64,
}

fn create_scheduled_job(
    path: &std::path::Path,
    request: CreateScheduledJobRequest<'_>,
) -> Result<String, String> {
    use borgclaw_core::scheduler::{with_retry_policy, Job, JobTrigger};
    use chrono::Utc;
    use std::collections::HashMap;
    use uuid::Uuid;

    let trigger = match request.trigger_type.to_lowercase().as_str() {
        "cron" => JobTrigger::Cron(request.trigger_value.to_string()),
        "interval" => {
            let seconds = request
                .trigger_value
                .parse::<u64>()
                .map_err(|_| "interval value must be a number (seconds)".to_string())?;
            JobTrigger::Interval(seconds)
        }
        "oneshot" => {
            let datetime = request
                .trigger_value
                .parse::<chrono::DateTime<Utc>>()
                .map_err(|_| "oneshot value must be an ISO 8601 datetime".to_string())?;
            JobTrigger::OneShot(datetime)
        }
        _ => {
            return Err(format!(
                "unknown trigger type: {}. Use cron, interval, or oneshot",
                request.trigger_type
            ))
        }
    };

    let next_run = trigger.next_run();
    let job = Job {
        id: Uuid::new_v4().to_string(),
        name: request.name.to_string(),
        description: request.description.map(str::to_string),
        trigger,
        action: request.action.to_string(),
        status: borgclaw_core::scheduler::JobStatus::Pending,
        created_at: Utc::now(),
        last_run: None,
        next_run,
        run_count: 0,
        max_retries: request.max_retries,
        retry_count: 0,
        retry_delay_seconds: request.retry_delay_seconds.max(1),
        dead_lettered_at: None,
        run_history: Vec::new(),
        metadata: HashMap::new(),
        catch_up_policy: borgclaw_core::scheduler::CatchUpPolicy::default(),
        missed_runs: 0,
    };

    let job = if request.max_retries > 0 {
        with_retry_policy(job, request.max_retries, request.retry_delay_seconds)
    } else {
        job
    };

    let id = job.id.clone();

    let mut jobs: HashMap<String, Job> = if path.exists() {
        let contents = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str(&contents).unwrap_or_default()
    } else {
        HashMap::new()
    };

    jobs.insert(id.clone(), job);

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let serialized = serde_json::to_string_pretty(&jobs).map_err(|e| e.to_string())?;
    std::fs::write(path, serialized).map_err(|e| e.to_string())?;

    Ok(id)
}

fn delete_scheduled_job(path: &std::path::Path, id: &str) -> Result<bool, String> {
    use borgclaw_core::scheduler::Job;

    if !path.exists() {
        return Ok(false);
    }

    let contents = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut jobs: std::collections::HashMap<String, Job> =
        serde_json::from_str(&contents).map_err(|e| e.to_string())?;

    if jobs.remove(id).is_none() {
        return Ok(false);
    }

    let serialized = serde_json::to_string_pretty(&jobs).map_err(|e| e.to_string())?;
    std::fs::write(path, serialized).map_err(|e| e.to_string())?;

    Ok(true)
}

fn update_job_status(
    path: &std::path::Path,
    id: &str,
    status: borgclaw_core::scheduler::JobStatus,
) -> Result<bool, String> {
    use borgclaw_core::scheduler::Job;

    if !path.exists() {
        return Ok(false);
    }

    let contents = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut jobs: std::collections::HashMap<String, Job> =
        serde_json::from_str(&contents).map_err(|e| e.to_string())?;

    let job = match jobs.get_mut(id) {
        Some(job) => job,
        None => return Ok(false),
    };

    job.status = status;

    let serialized = serde_json::to_string_pretty(&jobs).map_err(|e| e.to_string())?;
    std::fs::write(path, serialized).map_err(|e| e.to_string())?;

    Ok(true)
}

fn resume_scheduled_job(path: &std::path::Path, id: &str) -> Result<bool, String> {
    use borgclaw_core::scheduler::{Job, JobStatus};

    if !path.exists() {
        return Ok(false);
    }

    let contents = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut jobs: std::collections::HashMap<String, Job> =
        serde_json::from_str(&contents).map_err(|e| e.to_string())?;

    let job = match jobs.get_mut(id) {
        Some(job) => job,
        None => return Ok(false),
    };

    if job.status != JobStatus::Disabled {
        return Ok(false);
    }

    job.status = JobStatus::Pending;
    if job.next_run.is_none() && job.trigger.next_run().is_some() {
        job.next_run = job.trigger.next_run();
    }

    let serialized = serde_json::to_string_pretty(&jobs).map_err(|e| e.to_string())?;
    std::fs::write(path, serialized).map_err(|e| e.to_string())?;

    Ok(true)
}

fn dead_lettered_jobs(path: &std::path::Path) -> Result<Vec<String>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let contents = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let jobs: std::collections::HashMap<String, Job> =
        serde_json::from_str(&contents).map_err(|e| e.to_string())?;

    let mut lines = Vec::new();
    for (id, job) in &jobs {
        if let Some(dead_at) = job.dead_lettered_at {
            lines.push(format!(
                "{} [{}] action={} dead_lettered_at={} retries={}/{}",
                job.name, id, job.action, dead_at, job.retry_count, job.max_retries
            ));
        }
    }

    lines.sort();
    Ok(lines)
}

fn retry_dead_lettered_job(path: &std::path::Path, id: &str) -> Result<bool, String> {
    if !path.exists() {
        return Ok(false);
    }

    let contents = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut jobs: std::collections::HashMap<String, Job> =
        serde_json::from_str(&contents).map_err(|e| e.to_string())?;

    let job = match jobs.get_mut(id) {
        Some(job) => job,
        None => return Ok(false),
    };

    if job.dead_lettered_at.is_none() {
        return Ok(false);
    }

    job.dead_lettered_at = None;
    job.retry_count = 0;
    job.status = JobStatus::Pending;
    job.missed_runs = 0;
    if job.next_run.is_none()
        || job
            .next_run
            .map(|dt| dt < chrono::Utc::now())
            .unwrap_or(false)
    {
        job.next_run = job.trigger.next_run().or(Some(chrono::Utc::now()));
    }

    let serialized = serde_json::to_string_pretty(&jobs).map_err(|e| e.to_string())?;
    std::fs::write(path, serialized).map_err(|e| e.to_string())?;

    Ok(true)
}

fn pluralize(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("1 {}", noun)
    } else {
        format!("{} {}s", count, noun)
    }
}

fn heartbeat_list_lines(path: &std::path::Path) -> Result<Vec<String>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let contents = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let tasks: std::collections::HashMap<String, serde_json::Value> =
        serde_json::from_str(&contents).map_err(|e| e.to_string())?;

    let mut lines = Vec::new();
    for (id, task) in tasks {
        let name = task
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("unnamed");
        let enabled = task
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let schedule = task
            .get("schedule")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let run_count = task.get("run_count").and_then(|v| v.as_u64()).unwrap_or(0);
        let next_run = task
            .get("next_run")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let dead_lettered = task.get("dead_lettered_at").and_then(|v| v.as_str());
        let status = if dead_lettered.is_some() {
            "dead-lettered"
        } else if enabled {
            "enabled"
        } else {
            "disabled"
        };
        lines.push(format!(
            "{} [{}] schedule={} runs={} next={} {}",
            name, id, schedule, run_count, next_run, status
        ));
    }

    lines.sort();
    Ok(lines)
}

fn heartbeat_detail_lines(path: &std::path::Path, id: &str) -> Result<Vec<String>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let contents = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let tasks: std::collections::HashMap<String, serde_json::Value> =
        serde_json::from_str(&contents).map_err(|e| e.to_string())?;

    let Some(task) = tasks.get(id) else {
        return Ok(Vec::new());
    };

    let mut lines = vec![format!("id: {}", id)];

    if let Some(name) = task.get("name").and_then(|v| v.as_str()) {
        lines.push(format!("name: {}", name));
    }

    let enabled = task
        .get("enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    lines.push(format!("enabled: {}", enabled));

    if let Some(schedule) = task.get("schedule").and_then(|v| v.as_str()) {
        lines.push(format!("schedule: {}", schedule));
    }

    if let Some(last_run) = task.get("last_run").and_then(|v| v.as_str()) {
        lines.push(format!("last_run: {}", last_run));
    } else {
        lines.push("last_run: never".to_string());
    }

    if let Some(next_run) = task.get("next_run").and_then(|v| v.as_str()) {
        lines.push(format!("next_run: {}", next_run));
    } else {
        lines.push("next_run: unknown".to_string());
    }

    if let Some(run_count) = task.get("run_count").and_then(|v| v.as_u64()) {
        lines.push(format!("run_count: {}", run_count));
    }

    if let Some(max_retries) = task.get("max_retries").and_then(|v| v.as_u64()) {
        let retry_count = task
            .get("retry_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let retry_delay = task
            .get("retry_delay_seconds")
            .and_then(|v| v.as_u64())
            .unwrap_or(60);
        lines.push(format!(
            "retries: {}/{} (delay: {}s)",
            retry_count, max_retries, retry_delay
        ));
    }

    if let Some(dead_lettered) = task.get("dead_lettered_at").and_then(|v| v.as_str()) {
        lines.push(format!("dead_lettered_at: {}", dead_lettered));
    }

    if let Some(description) = task.get("description").and_then(|v| v.as_str()) {
        if !description.is_empty() {
            lines.push(format!("description: {}", description));
        }
    }

    if let Some(last_result) = task.get("last_result") {
        if !last_result.is_null() {
            let success = last_result
                .get("success")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let message = last_result
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            lines.push(format!(
                "last_result: {} - {}",
                if success { "ok" } else { "failed" },
                message
            ));
        }
    }

    if let Some(metadata) = task.get("metadata").and_then(|v| v.as_object()) {
        if !metadata.is_empty() {
            lines.push("metadata:".to_string());
            for (k, v) in metadata {
                lines.push(format!("  {}: {}", k, v));
            }
        }
    }

    Ok(lines)
}

fn create_heartbeat_task(
    path: &std::path::Path,
    name: &str,
    schedule: &str,
    description: &str,
    max_retries: u32,
    retry_delay: u64,
) -> Result<String, String> {
    let mut tasks: std::collections::HashMap<String, serde_json::Value> = if path.exists() {
        let contents = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str(&contents).map_err(|e| e.to_string())?
    } else {
        std::collections::HashMap::new()
    };

    let id = uuid::Uuid::new_v4().to_string();

    // Compute next_run from cron schedule
    let next_run = schedule
        .parse::<cron::Schedule>()
        .ok()
        .and_then(|s| s.upcoming(chrono::Utc).next())
        .map(|dt| dt.to_rfc3339());

    let task = serde_json::json!({
        "id": id,
        "name": name,
        "description": description,
        "schedule": schedule,
        "enabled": true,
        "last_run": null,
        "next_run": next_run,
        "run_count": 0,
        "max_retries": max_retries,
        "retry_count": 0,
        "retry_delay_seconds": retry_delay,
        "dead_lettered_at": null,
        "last_result": null,
        "metadata": {}
    });

    tasks.insert(id.clone(), task);

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let serialized = serde_json::to_string_pretty(&tasks).map_err(|e| e.to_string())?;
    std::fs::write(path, serialized).map_err(|e| e.to_string())?;

    Ok(id)
}

fn delete_heartbeat_task(path: &std::path::Path, id: &str) -> Result<bool, String> {
    if !path.exists() {
        return Ok(false);
    }

    let contents = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut tasks: std::collections::HashMap<String, serde_json::Value> =
        serde_json::from_str(&contents).map_err(|e| e.to_string())?;

    if tasks.remove(id).is_none() {
        return Ok(false);
    }

    let serialized = serde_json::to_string_pretty(&tasks).map_err(|e| e.to_string())?;
    std::fs::write(path, serialized).map_err(|e| e.to_string())?;

    Ok(true)
}

fn update_heartbeat_task_status(
    path: &std::path::Path,
    id: &str,
    enabled: bool,
) -> Result<bool, String> {
    if !path.exists() {
        return Ok(false);
    }

    let contents = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut tasks: std::collections::HashMap<String, serde_json::Value> =
        serde_json::from_str(&contents).map_err(|e| e.to_string())?;

    let task = match tasks.get_mut(id) {
        Some(task) => task,
        None => return Ok(false),
    };

    if let Some(obj) = task.as_object_mut() {
        obj.insert("enabled".to_string(), serde_json::json!(enabled));
    }

    let serialized = serde_json::to_string_pretty(&tasks).map_err(|e| e.to_string())?;
    std::fs::write(path, serialized).map_err(|e| e.to_string())?;

    Ok(true)
}

fn subagent_list_lines(path: &std::path::Path) -> Result<Vec<String>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let contents = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let tasks: std::collections::HashMap<String, serde_json::Value> =
        serde_json::from_str(&contents).map_err(|e| e.to_string())?;

    let mut lines = Vec::new();
    for (id, task) in tasks {
        let status = task
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let goal = task
            .get("goal")
            .and_then(|v| v.as_str())
            .unwrap_or("unnamed");
        lines.push(format!("{} [{}] {}", id, status, goal));
    }

    lines.sort();
    Ok(lines)
}

fn subagent_detail_lines(path: &std::path::Path, id: &str) -> Result<Vec<String>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let contents = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let tasks: std::collections::HashMap<String, serde_json::Value> =
        serde_json::from_str(&contents).map_err(|e| e.to_string())?;

    let Some(task) = tasks.get(id) else {
        return Ok(Vec::new());
    };

    let mut lines = vec![format!("id: {}", id)];

    if let Some(status) = task.get("status").and_then(|v| v.as_str()) {
        lines.push(format!("status: {}", status));
    }

    if let Some(goal) = task.get("goal").and_then(|v| v.as_str()) {
        lines.push(format!("goal: {}", goal));
    }

    if let Some(created) = task.get("created_at").and_then(|v| v.as_str()) {
        lines.push(format!("created_at: {}", created));
    }

    Ok(lines)
}

fn cancel_subagent_task(path: &std::path::Path, id: &str) -> Result<bool, String> {
    if !path.exists() {
        return Ok(false);
    }

    let contents = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut tasks: std::collections::HashMap<String, serde_json::Value> =
        serde_json::from_str(&contents).map_err(|e| e.to_string())?;

    let task = match tasks.get_mut(id) {
        Some(task) => task,
        None => return Ok(false),
    };

    if let Some(obj) = task.as_object_mut() {
        obj.insert("status".to_string(), serde_json::json!("cancelled"));
    }

    let serialized = serde_json::to_string_pretty(&tasks).map_err(|e| e.to_string())?;
    std::fs::write(path, serialized).map_err(|e| e.to_string())?;

    Ok(true)
}

fn process_list_lines(path: &std::path::Path) -> Result<Vec<String>, String> {
    let mut records = load_process_records(path).map_err(|err| err.to_string())?;
    let mut processes = records
        .drain()
        .map(|(_, record)| record)
        .collect::<Vec<_>>();
    processes.sort_by(|a, b| b.started_at.cmp(&a.started_at));

    let mut lines = Vec::new();
    for record in processes {
        let finished_at = record
            .finished_at
            .map(|value| value.to_rfc3339())
            .unwrap_or_else(|| "running".to_string());
        lines.push(format!(
            "{} [{}] pid={} pty={} finished={}",
            record.id,
            process_status_label(&record.status),
            record
                .pid
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            record.pty,
            finished_at
        ));
    }

    Ok(lines)
}

fn process_detail_lines(path: &std::path::Path, id: &str) -> Result<Vec<String>, String> {
    let Some(record) =
        borgclaw_core::security::get_process_record(path, id).map_err(|err| err.to_string())?
    else {
        return Ok(Vec::new());
    };

    let mut lines = vec![format!("id: {}", record.id)];
    lines.push(format!("status: {}", process_status_label(&record.status)));
    lines.push(format!("command: {}", record.command));
    lines.push(format!(
        "pid: {}",
        record
            .pid
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".to_string())
    ));
    lines.push(format!("started_at: {}", record.started_at.to_rfc3339()));
    lines.push(format!(
        "finished_at: {}",
        record
            .finished_at
            .map(|value| value.to_rfc3339())
            .unwrap_or_else(|| "running".to_string())
    ));
    lines.push(format!(
        "execution_mode: {}",
        match record.execution_mode {
            borgclaw_core::security::CommandExecutionMode::Host => "host",
            borgclaw_core::security::CommandExecutionMode::Docker => "docker",
        }
    ));
    lines.push(format!("pty: {}", record.pty));
    lines.push(format!("timeout_secs: {}", record.timeout_secs));
    if let Some(yield_ms) = record.yield_ms {
        lines.push(format!("yield_ms: {}", yield_ms));
    }
    if let Some(exit_code) = record.exit_code {
        lines.push(format!("exit_code: {}", exit_code));
    }
    if let Some(image) = record.image {
        lines.push(format!("docker_image: {}", image));
    }
    if !record.output.is_empty() {
        lines.push("output:".to_string());
        for line in record.output.lines() {
            lines.push(format!("  {}", line));
        }
    }

    Ok(lines)
}

fn process_status_label(status: &CommandProcessStatus) -> &'static str {
    match status {
        CommandProcessStatus::Running => "running",
        CommandProcessStatus::Succeeded => "succeeded",
        CommandProcessStatus::Failed => "failed",
        CommandProcessStatus::Cancelled => "cancelled",
        CommandProcessStatus::TimedOut => "timed_out",
    }
}

fn mcp_transport_label(server: &borgclaw_core::config::McpServerConfig) -> &'static str {
    match server.transport.as_str() {
        "stdio" => "stdio",
        "sse" => "sse",
        "websocket" => "websocket",
        _ => "unknown",
    }
}

async fn mcp_doctor_lines(config: &AppConfig) -> Vec<String> {
    let mut server_names = config.mcp.servers.keys().cloned().collect::<Vec<_>>();
    server_names.sort();

    if server_names.is_empty() {
        return vec!["• MCP servers not configured".to_string()];
    }

    let security = SecurityLayer::with_config(config.security.clone());
    let mut lines = Vec::new();
    let mut failures = Vec::new();
    let total = server_names.len();

    for name in server_names {
        let Some(server) = config.mcp.servers.get(&name) else {
            continue;
        };
        let (ok, detail) = probe_mcp_server(&security, &name, server).await;
        if !ok {
            failures.push(name.clone());
        }
        lines.push(format!(
            "{} MCP {} ({}) {}",
            marker(ok),
            name,
            mcp_transport_label(server),
            detail
        ));
    }

    let failure_count = failures.len();
    let success_count = total.saturating_sub(failure_count);
    let summary = if failure_count == 0 {
        format!("✓ MCP summary: all {} configured servers reachable", total)
    } else if success_count == 0 {
        format!(
            "✗ MCP summary: all {} configured servers failing ({})",
            total,
            failures.join(", ")
        )
    } else {
        format!(
            "✗ MCP summary: {} of {} configured servers failing ({})",
            failure_count,
            total,
            failures.join(", ")
        )
    };
    lines.insert(0, summary);

    lines
}

async fn probe_mcp_server(
    security: &SecurityLayer,
    name: &str,
    server: &borgclaw_core::config::McpServerConfig,
) -> (bool, String) {
    let transport_config = match mcp_transport_config(security, server) {
        Ok(config) => config,
        Err(err) => return (false, err),
    };

    let mut client = McpClient::new(McpClientConfig {
        name: "borgclaw-doctor".to_string(),
        transport_config,
        protocol_version: "2024-11-05".to_string(),
    });

    let result = tokio::time::timeout(std::time::Duration::from_secs(2), client.initialize()).await;
    let _ = client.disconnect().await;

    match result {
        Ok(Ok(())) => (true, "reachable".to_string()),
        Ok(Err(err)) => (false, err.to_string()),
        Err(_) => (false, format!("timeout connecting to {}", name)),
    }
}

fn mcp_transport_config(
    security: &SecurityLayer,
    server: &borgclaw_core::config::McpServerConfig,
) -> Result<McpTransportConfig, String> {
    match server.transport.as_str() {
        "stdio" => {
            let command = server
                .command
                .clone()
                .ok_or_else(|| "missing command".to_string())?;
            match security.check_command(&command) {
                borgclaw_core::security::CommandCheck::Blocked(pattern) => {
                    return Err(format!("blocked by policy: {}", pattern));
                }
                borgclaw_core::security::CommandCheck::Allowed => {}
            }
            if !cli_path_available(std::path::Path::new(&command)) {
                return Err(format!("missing binary: {}", command));
            }
            Ok(McpTransportConfig::Stdio(StdioTransportConfig {
                command,
                args: server.args.clone(),
                env: server.env.clone(),
            }))
        }
        "sse" => Ok(McpTransportConfig::Sse(SseTransportConfig {
            url: server
                .url
                .clone()
                .ok_or_else(|| "missing url".to_string())?,
            post_url: server
                .url
                .clone()
                .ok_or_else(|| "missing url".to_string())?,
            headers: server.headers.clone(),
        })),
        "websocket" => Ok(McpTransportConfig::WebSocket(WebSocketTransportConfig {
            url: server
                .url
                .clone()
                .ok_or_else(|| "missing url".to_string())?,
        })),
        other => Err(format!("unsupported transport: {}", other)),
    }
}

async fn stt_backend_ready(config: &AppConfig) -> bool {
    match config.skills.stt.backend.as_str() {
        "openwebui" => {
            !config.skills.stt.openwebui.base_url.is_empty()
                && skill_secret_available(config, &config.skills.stt.openwebui.api_key).await
        }
        "whispercpp" => config.skills.stt.whispercpp.binary_path.exists(),
        _ => skill_secret_available(config, &config.skills.stt.openai.api_key).await,
    }
}

async fn image_provider_ready(config: &AppConfig) -> bool {
    match config.skills.image.provider.as_str() {
        "stable_diffusion" => !config.skills.image.stable_diffusion.base_url.is_empty(),
        _ => skill_secret_available(config, &config.skills.image.dalle.api_key).await,
    }
}

async fn url_shortener_ready(config: &AppConfig) -> bool {
    match config.skills.url_shortener.provider.as_str() {
        "yourls" => {
            !config.skills.url_shortener.yourls.base_url.is_empty()
                && (!config.skills.url_shortener.yourls.signature.is_empty()
                    || skill_secret_available(
                        config,
                        &config.skills.url_shortener.yourls.signature,
                    )
                    .await
                    || (!config.skills.url_shortener.yourls.username.is_empty()
                        && skill_secret_available(
                            config,
                            &config.skills.url_shortener.yourls.password,
                        )
                        .await))
        }
        _ => true,
    }
}

async fn channel_credentials_available(
    config: &AppConfig,
    channel: &borgclaw_core::config::ChannelConfig,
) -> bool {
    match channel.credentials.as_deref() {
        Some(value) => skill_secret_available(config, value).await,
        None => false,
    }
}

fn signal_channel_ready(channel: &borgclaw_core::config::ChannelConfig) -> bool {
    signal_cli_ready(channel)
        && channel
            .extra
            .get("phone_number")
            .and_then(|value| value.as_str())
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false)
}

fn signal_cli_ready(channel: &borgclaw_core::config::ChannelConfig) -> bool {
    cli_path_available(std::path::Path::new(signal_cli_display(channel)))
}

fn signal_cli_display(channel: &borgclaw_core::config::ChannelConfig) -> &str {
    channel
        .extra
        .get("signal_cli_path")
        .and_then(|value| value.as_str())
        .unwrap_or("signal-cli")
}

async fn webhook_channel_ready(
    config: &AppConfig,
    channel: &borgclaw_core::config::ChannelConfig,
) -> bool {
    match channel.extra.get("secret").and_then(|value| value.as_str()) {
        Some(secret) => skill_secret_available(config, secret).await,
        None => false,
    }
}

async fn skill_secret_available(config: &AppConfig, value: &str) -> bool {
    if let Some(env) = value.strip_prefix("${").and_then(|v| v.strip_suffix('}')) {
        if std::env::var(env)
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false)
        {
            return true;
        }

        return SecurityLayer::with_config(config.security.clone())
            .get_secret(env)
            .await
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false);
    }

    !value.trim().is_empty()
}

fn path_or_placeholder_status(path: &std::path::Path) -> &'static str {
    if path_exists_or_placeholder(path) {
        "ready"
    } else {
        "missing"
    }
}

fn path_exists_or_placeholder(path: &std::path::Path) -> bool {
    path.exists() || path.to_string_lossy().starts_with("${")
}

fn binary_or_placeholder_status(path: &std::path::Path) -> &'static str {
    if cli_path_available(path) || path.to_string_lossy().starts_with("${") {
        "ready"
    } else {
        "missing"
    }
}

fn marker(ok: bool) -> &'static str {
    if ok {
        "✓"
    } else {
        "✗"
    }
}

fn enabled_disabled(value: bool) -> &'static str {
    if value {
        "enabled"
    } else {
        "disabled"
    }
}

async fn install_skill(
    skills_path: &std::path::Path,
    source: &str,
    registry_url: Option<&str>,
) -> Result<std::path::PathBuf, String> {
    let source_path = std::path::PathBuf::from(source);
    if source_path.exists() {
        if source_path.is_dir() {
            return install_local_skill(skills_path, &source_path);
        }
        if is_tar_gz_path(&source_path) {
            return install_skill_archive(skills_path, &source_path, None);
        }
        return Err(format!(
            "Unsupported local skill source '{}'. Use a skill directory containing SKILL.md or a .tar.gz package.",
            source
        ));
    }

    if let Some(spec) = resolve_skill_source(source, registry_url) {
        return match spec {
            SkillSource::RemoteManifest { url } => {
                let content = fetch_skill_manifest(&url).await?;
                install_skill_manifest(skills_path, &url, &content).await
            }
            SkillSource::RemoteArchive {
                url,
                install_id,
                archive_layout,
            } => {
                let bytes = fetch_skill_archive(&url).await?;
                install_skill_archive_bytes(
                    skills_path,
                    install_id.as_deref(),
                    &bytes,
                    &archive_layout,
                )
            }
        };
    }

    Err(format!(
        "Unsupported skill source '{}'. Use a local skill directory, local .tar.gz archive, owner/repo, direct GitHub SKILL.md URL, or remote .tar.gz archive URL.",
        source
    ))
}

#[derive(Debug, Clone)]
enum SkillSource {
    RemoteManifest {
        url: String,
    },
    RemoteArchive {
        url: String,
        install_id: Option<String>,
        archive_layout: ArchiveLayout,
    },
}

#[derive(Debug, Clone)]
struct ArchiveLayout {
    strip_first_component: bool,
    subdir: Option<std::path::PathBuf>,
}

impl ArchiveLayout {
    fn packaged_skill() -> Self {
        Self {
            strip_first_component: false,
            subdir: None,
        }
    }

    fn github_repo_root() -> Self {
        Self {
            strip_first_component: true,
            subdir: None,
        }
    }

    fn github_subdir(subdir: impl Into<std::path::PathBuf>) -> Self {
        Self {
            strip_first_component: true,
            subdir: Some(subdir.into()),
        }
    }
}

fn install_local_skill(
    skills_path: &std::path::Path,
    source_path: &std::path::Path,
) -> Result<std::path::PathBuf, String> {
    if !source_path.is_dir() || !source_path.join("SKILL.md").exists() {
        return Err("Expected a local skill directory containing SKILL.md".to_string());
    }

    std::fs::create_dir_all(skills_path).map_err(|e| e.to_string())?;
    let skill_id = source_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Could not determine skill directory name".to_string())?;
    let destination = skills_path.join(skill_id);
    if destination.exists() {
        return Err(format!("Skill '{}' is already installed", skill_id));
    }

    copy_dir_recursive(source_path, &destination)?;
    Ok(destination)
}

fn install_skill_archive(
    skills_path: &std::path::Path,
    archive_path: &std::path::Path,
    install_id: Option<&str>,
) -> Result<std::path::PathBuf, String> {
    let bytes = std::fs::read(archive_path)
        .map_err(|e| format!("Failed to read archive {}: {}", archive_path.display(), e))?;
    install_skill_archive_bytes(
        skills_path,
        install_id,
        &bytes,
        &ArchiveLayout::packaged_skill(),
    )
}

async fn install_skill_manifest(
    skills_path: &std::path::Path,
    source: &str,
    content: &str,
) -> Result<std::path::PathBuf, String> {
    let manifest =
        borgclaw_core::skills::SkillManifest::parse(content).map_err(|e| e.to_string())?;
    let companion_files = fetch_manifest_companion_paths(source, &manifest).await?;

    std::fs::create_dir_all(skills_path).map_err(|e| e.to_string())?;
    let skill_id = skill_install_id(source)?;
    let destination = skills_path.join(&skill_id);
    if destination.exists() {
        return Err(format!("Skill '{}' is already installed", skill_id));
    }

    let temp_root =
        std::env::temp_dir().join(format!("borgclaw_skill_manifest_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_root).map_err(|e| e.to_string())?;

    let result = async {
        std::fs::write(temp_root.join("SKILL.md"), content).map_err(|e| e.to_string())?;

        for relative_file in &companion_files {
            let relative_path = std::path::PathBuf::from(relative_file);
            ensure_safe_archive_path(&relative_path)?;
            let file_url = resolve_companion_url(source, &relative_path)?;
            let bytes = fetch_remote_resource(&file_url).await?;
            let output_path = temp_root.join(&relative_path);
            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(output_path, bytes).map_err(|e| e.to_string())?;
        }

        std::fs::rename(&temp_root, &destination)
            .or_else(|_| copy_dir_recursive(&temp_root, &destination).map(|_| ()))
            .map_err(|e| e.to_string())?;
        Ok(destination)
    }
    .await;

    if temp_root.exists() {
        let _ = std::fs::remove_dir_all(&temp_root);
    }

    result
}

fn install_skill_archive_bytes(
    skills_path: &std::path::Path,
    install_id: Option<&str>,
    bytes: &[u8],
    archive_layout: &ArchiveLayout,
) -> Result<std::path::PathBuf, String> {
    std::fs::create_dir_all(skills_path).map_err(|e| e.to_string())?;

    let temp_root =
        std::env::temp_dir().join(format!("borgclaw_skill_extract_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_root).map_err(|e| e.to_string())?;

    let result = (|| {
        extract_skill_archive(bytes, &temp_root, archive_layout)?;
        let manifest = read_skill_manifest(&temp_root)?;
        let skill_id = install_id
            .map(ToString::to_string)
            .unwrap_or_else(|| manifest.name.clone());
        let destination = skills_path.join(&skill_id);
        if destination.exists() {
            return Err(format!("Skill '{}' is already installed", skill_id));
        }

        std::fs::rename(&temp_root, &destination)
            .or_else(|_| copy_dir_recursive(&temp_root, &destination).map(|_| ()))
            .map_err(|e| e.to_string())?;
        Ok(destination)
    })();

    if temp_root.exists() {
        let _ = std::fs::remove_dir_all(&temp_root);
    }

    result
}

async fn fetch_skill_manifest(url: &str) -> Result<String, String> {
    let content = String::from_utf8(fetch_remote_resource(url).await?)
        .map_err(|e| format!("downloaded skill manifest is not valid UTF-8: {}", e))?;
    if content.trim().is_empty() {
        return Err("downloaded skill manifest is empty".to_string());
    }

    Ok(content)
}

async fn fetch_skill_archive(url: &str) -> Result<Vec<u8>, String> {
    fetch_remote_resource(url).await
}

async fn fetch_remote_resource(url: &str) -> Result<Vec<u8>, String> {
    match try_fetch_remote_resource(url).await? {
        Some(bytes) => Ok(bytes),
        None => Err(format!(
            "failed to download remote resource '{}': not found",
            url
        )),
    }
}

async fn try_fetch_remote_resource(url: &str) -> Result<Option<Vec<u8>>, String> {
    if let Some(path) = file_url_to_path(url)? {
        return match std::fs::read(&path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(format!(
                "failed to read local resource {}: {}",
                path.display(),
                err
            )),
        };
    }

    let response = reqwest::get(url).await.map_err(|e| e.to_string())?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(format!(
            "failed to download remote resource: http {}",
            response.status()
        ));
    }

    response
        .bytes()
        .await
        .map(|bytes| Some(bytes.to_vec()))
        .map_err(|e| e.to_string())
}

async fn fetch_manifest_companion_paths(
    manifest_url: &str,
    manifest: &borgclaw_core::skills::SkillManifest,
) -> Result<Vec<String>, String> {
    if !manifest.files.is_empty() {
        return Ok(manifest.files.clone());
    }

    if reqwest::Url::parse(manifest_url).is_err() {
        return Ok(Vec::new());
    }

    let sidecar_url =
        resolve_companion_url(manifest_url, std::path::Path::new("SKILL.files.json"))?;
    if let Some(bytes) = try_fetch_remote_resource(&sidecar_url).await? {
        return parse_manifest_file_index(&bytes);
    }

    if let Some(path) = file_url_to_path(manifest_url)? {
        let base_dir = path.parent().ok_or_else(|| {
            format!(
                "Manifest file URL '{}' does not have a parent directory",
                manifest_url
            )
        })?;
        return discover_local_manifest_files(base_dir);
    }

    discover_remote_manifest_files(manifest_url).await
}

fn parse_manifest_file_index(bytes: &[u8]) -> Result<Vec<String>, String> {
    #[derive(serde::Deserialize)]
    struct FileIndex {
        files: Vec<String>,
    }

    if let Ok(files) = serde_json::from_slice::<Vec<String>>(bytes) {
        return Ok(files);
    }

    serde_json::from_slice::<FileIndex>(bytes)
        .map(|index| index.files)
        .map_err(|e| format!("invalid SKILL.files.json: {}", e))
}

fn discover_local_manifest_files(base_dir: &std::path::Path) -> Result<Vec<String>, String> {
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(base_dir) {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.file_type().is_file() {
            continue;
        }
        let relative_path = entry
            .path()
            .strip_prefix(base_dir)
            .map_err(|e| e.to_string())?;
        if should_skip_manifest_support_file(relative_path) {
            continue;
        }
        ensure_safe_archive_path(relative_path)?;
        files.push(relative_path.to_string_lossy().to_string());
    }
    files.sort();
    Ok(files)
}

async fn discover_remote_manifest_files(manifest_url: &str) -> Result<Vec<String>, String> {
    let base_url = manifest_directory_url(manifest_url)?;
    let base_path = base_url.path().to_string();
    let mut pending = std::collections::VecDeque::from([base_url.clone()]);
    let mut visited = std::collections::HashSet::from([base_url.to_string()]);
    let mut files = std::collections::BTreeSet::new();

    while let Some(dir_url) = pending.pop_front() {
        let Some(body) = try_fetch_remote_text(dir_url.as_str()).await? else {
            continue;
        };

        for href in extract_directory_listing_hrefs(&body) {
            if href.starts_with('#')
                || href.starts_with('?')
                || href.starts_with("../")
                || href == "../"
            {
                continue;
            }

            let Some(target_url) = dir_url.join(&href).ok() else {
                continue;
            };
            if target_url.scheme() != base_url.scheme()
                || target_url.domain() != base_url.domain()
                || !target_url.path().starts_with(&base_path)
            {
                continue;
            }

            let relative_path = target_url
                .path()
                .strip_prefix(&base_path)
                .unwrap_or(target_url.path())
                .trim_start_matches('/');

            if href.ends_with('/') || target_url.path().ends_with('/') {
                if !relative_path.is_empty() && visited.insert(target_url.to_string()) {
                    pending.push_back(target_url);
                }
                continue;
            }

            if relative_path.is_empty() {
                continue;
            }

            let relative_path = std::path::PathBuf::from(relative_path);
            if should_skip_manifest_support_file(&relative_path) {
                continue;
            }
            ensure_safe_archive_path(&relative_path)?;
            files.insert(relative_path.to_string_lossy().to_string());
        }
    }

    Ok(files.into_iter().collect())
}

async fn try_fetch_remote_text(url: &str) -> Result<Option<String>, String> {
    match try_fetch_remote_resource(url).await? {
        Some(bytes) => String::from_utf8(bytes)
            .map(Some)
            .map_err(|e| format!("downloaded directory listing is not valid UTF-8: {}", e)),
        None => Ok(None),
    }
}

fn manifest_directory_url(manifest_url: &str) -> Result<reqwest::Url, String> {
    let mut url = reqwest::Url::parse(manifest_url).map_err(|e| e.to_string())?;
    {
        let mut segments = url.path_segments_mut().map_err(|_| {
            format!(
                "Manifest URL '{}' cannot be used as a base path",
                manifest_url
            )
        })?;
        segments.pop_if_empty();
        segments.pop();
    }
    if !url.path().ends_with('/') {
        let new_path = format!("{}/", url.path());
        url.set_path(&new_path);
    }
    Ok(url)
}

fn extract_directory_listing_hrefs(body: &str) -> Vec<String> {
    let mut hrefs = Vec::new();
    let mut remaining = body;
    while let Some(index) = remaining.find("href=") {
        remaining = &remaining[index + 5..];
        let Some(quote) = remaining.chars().next() else {
            break;
        };
        if quote != '"' && quote != '\'' {
            continue;
        }
        remaining = &remaining[quote.len_utf8()..];
        let Some(end) = remaining.find(quote) else {
            break;
        };
        let href = &remaining[..end];
        if !href.is_empty() {
            hrefs.push(html_unescape_minimal(href));
        }
        remaining = &remaining[end + quote.len_utf8()..];
    }
    hrefs
}

fn html_unescape_minimal(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

fn should_skip_manifest_support_file(path: &std::path::Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some("SKILL.md") | Some("SKILL.files.json")
    )
}

fn resolve_skill_source(source: &str, registry_url: Option<&str>) -> Option<SkillSource> {
    if source.starts_with("http://")
        || source.starts_with("https://")
        || source.starts_with("file://")
    {
        if is_tar_gz_str(source) {
            return Some(SkillSource::RemoteArchive {
                url: source.to_string(),
                install_id: None,
                archive_layout: ArchiveLayout::packaged_skill(),
            });
        }
        if let Some(spec) = github_archive_source_from_url(source) {
            return Some(spec);
        }
        return Some(SkillSource::RemoteManifest {
            url: source.to_string(),
        });
    }

    let normalized = source.trim_matches('/');
    let parts = normalized.split('/').collect::<Vec<_>>();
    if parts.len() != 2 || parts.iter().any(|part| part.is_empty()) {
        return None;
    }

    if let Some((owner, repo)) = registry_url.and_then(github_registry_repo) {
        return Some(SkillSource::RemoteArchive {
            url: github_archive_url(&owner, &repo, "main"),
            install_id: Some(registry_skill_install_id(source)),
            archive_layout: ArchiveLayout::github_subdir(normalized),
        });
    }

    Some(SkillSource::RemoteArchive {
        url: github_archive_url(parts[0], parts[1], "main"),
        install_id: Some(registry_skill_install_id(source)),
        archive_layout: ArchiveLayout::github_repo_root(),
    })
}

fn github_archive_url(owner: &str, repo: &str, reference: &str) -> String {
    format!("https://codeload.github.com/{owner}/{repo}/tar.gz/refs/heads/{reference}")
}

fn github_archive_source_from_url(source: &str) -> Option<SkillSource> {
    let path = source
        .trim_end_matches('/')
        .strip_prefix("https://raw.githubusercontent.com/")?
        .split('/')
        .collect::<Vec<_>>();
    if path.len() < 4 {
        return None;
    }

    let owner = path[0];
    let repo = path[1];
    let reference = path[2];
    let tail = &path[3..];

    if tail.last()?.eq_ignore_ascii_case("SKILL.md") {
        let install_id = if tail.len() >= 2 {
            Some(tail[tail.len() - 2].to_string())
        } else {
            Some(repo.to_string())
        };
        let subdir = if tail.len() > 1 {
            Some(std::path::PathBuf::from(tail[..tail.len() - 1].join("/")))
        } else {
            None
        };
        return Some(SkillSource::RemoteArchive {
            url: github_archive_url(owner, repo, reference),
            install_id,
            archive_layout: ArchiveLayout {
                strip_first_component: true,
                subdir,
            },
        });
    }

    None
}

fn resolve_companion_url(
    manifest_url: &str,
    relative_path: &std::path::Path,
) -> Result<String, String> {
    if let Some(path) = file_url_to_path(manifest_url)? {
        let base_dir = path.parent().ok_or_else(|| {
            format!(
                "Manifest file URL '{}' does not have a parent directory",
                manifest_url
            )
        })?;
        return reqwest::Url::from_file_path(base_dir.join(relative_path))
            .map(|url| url.to_string())
            .map_err(|_| format!("Could not build companion file URL from '{}'", manifest_url));
    }

    let mut base = reqwest::Url::parse(manifest_url).map_err(|e| e.to_string())?;
    if !base.path().ends_with('/') {
        let mut segments = base.path_segments_mut().map_err(|_| {
            format!(
                "Manifest URL '{}' cannot be used as a base path",
                manifest_url
            )
        })?;
        segments.pop();
    }
    base.join(&relative_path.to_string_lossy())
        .map(|url| url.to_string())
        .map_err(|e| e.to_string())
}

fn file_url_to_path(url: &str) -> Result<Option<std::path::PathBuf>, String> {
    let parsed = reqwest::Url::parse(url).map_err(|e| e.to_string())?;
    if parsed.scheme() != "file" {
        return Ok(None);
    }
    parsed
        .to_file_path()
        .map(Some)
        .map_err(|_| format!("Invalid file URL '{}'", url))
}

fn is_tar_gz_path(path: &std::path::Path) -> bool {
    is_tar_gz_str(&path.to_string_lossy())
}

fn is_tar_gz_str(value: &str) -> bool {
    value.ends_with(".tar.gz")
}

fn read_skill_manifest(
    path: &std::path::Path,
) -> Result<borgclaw_core::skills::SkillManifest, String> {
    let content = std::fs::read_to_string(path.join("SKILL.md"))
        .map_err(|e| format!("Failed to read extracted SKILL.md: {}", e))?;
    borgclaw_core::skills::SkillManifest::parse(&content)
        .map_err(|e| format!("Failed to parse extracted SKILL.md: {}", e))
}

fn extract_skill_archive(
    bytes: &[u8],
    destination: &std::path::Path,
    archive_layout: &ArchiveLayout,
) -> Result<(), String> {
    let cursor = std::io::Cursor::new(bytes);
    let decoder = flate2::read::GzDecoder::new(cursor);
    let mut archive = tar::Archive::new(decoder);
    let mut extracted_any = false;
    let mut skill_md_found = false;

    for entry in archive
        .entries()
        .map_err(|e| format!("Failed to read archive: {}", e))?
    {
        let mut entry = entry.map_err(|e| format!("Failed to read archive entry: {}", e))?;
        let original_path = entry.path().map_err(|e| e.to_string())?.into_owned();
        let Some(relative_path) = archive_relative_path(&original_path, archive_layout) else {
            continue;
        };
        if relative_path.as_os_str().is_empty() {
            continue;
        }

        ensure_safe_archive_path(&relative_path)?;
        let output_path = destination.join(&relative_path);
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        entry.unpack(&output_path).map_err(|e| e.to_string())?;
        if relative_path == std::path::Path::new("SKILL.md") {
            skill_md_found = true;
        }
        extracted_any = true;
    }

    if !extracted_any {
        return Err("Archive did not contain any installable skill files".to_string());
    }
    if !skill_md_found {
        return Err("Archive did not contain SKILL.md in the selected skill root".to_string());
    }

    Ok(())
}

fn archive_relative_path(
    original_path: &std::path::Path,
    archive_layout: &ArchiveLayout,
) -> Option<std::path::PathBuf> {
    let mut path = original_path;
    if archive_layout.strip_first_component {
        path = strip_first_component(path)?;
    }
    if let Some(subdir) = &archive_layout.subdir {
        if !path.starts_with(subdir) {
            return None;
        }
        path = path.strip_prefix(subdir).ok()?;
    }
    Some(path.to_path_buf())
}

fn strip_first_component(path: &std::path::Path) -> Option<&std::path::Path> {
    let mut components = path.components();
    components.next()?;
    Some(std::path::Path::new(components.as_path()))
}

fn ensure_safe_archive_path(path: &std::path::Path) -> Result<(), String> {
    use std::path::Component;

    if path.is_absolute() {
        return Err(format!(
            "Archive contains absolute path '{}'",
            path.display()
        ));
    }
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(format!(
            "Archive contains unsafe parent-relative path '{}'",
            path.display()
        ));
    }
    Ok(())
}

/// Package a skill directory into a distributable .tar.gz archive
async fn package_skill(
    skill_path: &std::path::Path,
    output_path: Option<&std::path::Path>,
) -> Result<std::path::PathBuf, String> {
    // Validate skill directory
    if !skill_path.is_dir() {
        return Err(format!("{} is not a directory", skill_path.display()));
    }

    let skill_md_path = skill_path.join("SKILL.md");
    if !skill_md_path.exists() {
        return Err(format!(
            "Skill directory must contain a SKILL.md file: {}",
            skill_path.display()
        ));
    }

    // Parse manifest to get skill name
    let manifest_content = std::fs::read_to_string(&skill_md_path)
        .map_err(|e| format!("Failed to read SKILL.md: {}", e))?;
    let manifest = borgclaw_core::skills::SkillManifest::parse(&manifest_content)
        .map_err(|e| format!("Failed to parse SKILL.md: {}", e))?;

    if manifest.description.is_empty() {
        println!("⚠ Warning: SKILL.md has empty description");
    }

    println!(
        "Packaging skill: {} v{} ({} commands)",
        manifest.name,
        manifest.version,
        manifest.commands.len()
    );
    if let Some(min) = &manifest.min_version {
        println!("  min_version: {}", min);
    }

    let skill_name = manifest.name;
    let version = manifest.version;

    // Determine output path
    let output = output_path.map(|p| p.to_path_buf()).unwrap_or_else(|| {
        std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .join(format!("{}-{}.tar.gz", skill_name, version))
    });

    // Create archive
    create_skill_archive(skill_path, &output, &skill_name, &version)?;

    Ok(output)
}

/// Create a tar.gz archive from a skill directory
fn create_skill_archive(
    source: &std::path::Path,
    output: &std::path::Path,
    skill_name: &str,
    version: &str,
) -> Result<(), String> {
    let tar_gz = std::fs::File::create(output)
        .map_err(|e| format!("Failed to create archive file: {}", e))?;
    let enc = flate2::write::GzEncoder::new(tar_gz, flate2::Compression::default());
    let mut tar = tar::Builder::new(enc);

    // Add metadata file
    let metadata = serde_json::json!({
        "name": skill_name,
        "version": version,
        "packaged_at": chrono::Utc::now().to_rfc3339(),
        "packaged_by": "borgclaw-cli",
    });
    let metadata_bytes = metadata.to_string().into_bytes();
    let mut header = tar::Header::new_gnu();
    header.set_size(metadata_bytes.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    tar.append_data(&mut header, "borgclaw-package.json", &metadata_bytes[..])
        .map_err(|e| format!("Failed to add metadata to archive: {}", e))?;

    // Walk directory and add files
    for entry in walkdir::WalkDir::new(source) {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        let path = entry.path();
        let relative_path = path.strip_prefix(source).map_err(|e| e.to_string())?;

        if path.is_file() {
            tar.append_path_with_name(path, relative_path)
                .map_err(|e| format!("Failed to add file to archive: {}", e))?;
        }
    }

    // Finish archive
    let enc = tar
        .into_inner()
        .map_err(|e| format!("Failed to finalize archive: {}", e))?;
    enc.finish()
        .map_err(|e| format!("Failed to finish compression: {}", e))?;

    Ok(())
}

/// Result of publishing a skill
struct PublishResult {
    package_id: String,
    public_url: Option<String>,
}

/// Publish a packaged skill to the registry
async fn publish_skill(
    package_path: &std::path::Path,
    registry_url: &str,
    force: bool,
) -> Result<PublishResult, String> {
    // Validate package file
    if !package_path.exists() {
        return Err(format!(
            "Package file not found: {}",
            package_path.display()
        ));
    }

    if !package_path.extension().is_some_and(|ext| ext == "gz")
        && !package_path.to_string_lossy().ends_with(".tar.gz")
    {
        return Err("Package file must be a .tar.gz archive".to_string());
    }

    // Validate SKILL.md exists and is parseable in the archive
    validate_archive_skill_md(package_path)?;

    // Extract package metadata
    let package_metadata = extract_package_metadata(package_path)?;
    let skill_name = package_metadata
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or("Package metadata missing skill name")?;
    let version = package_metadata
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    // Confirm with user if not forced
    if !force {
        println!("You are about to publish:");
        println!("  Skill: {}", skill_name);
        println!("  Version: {}", version);
        println!("  Package: {}", package_path.display());
        println!("  Registry: {}", registry_url);
        println!("\nContinue? [y/N]");

        let mut response = String::new();
        std::io::stdin()
            .read_line(&mut response)
            .map_err(|e| format!("Failed to read confirmation: {}", e))?;
        if !response.trim().eq_ignore_ascii_case("y") {
            return Err("Publish cancelled by user".to_string());
        }
    }

    // Upload to registry
    let package_id = upload_to_registry(package_path, registry_url, skill_name).await?;

    Ok(PublishResult {
        package_id,
        public_url: Some(format!("{}/skills/{}", registry_url, skill_name)),
    })
}

/// Inspect the contents of a packaged skill archive
fn inspect_skill_package(package_path: &std::path::Path) -> Result<(), String> {
    use std::io::Read;

    let tar_gz =
        std::fs::File::open(package_path).map_err(|e| format!("Failed to open package: {}", e))?;
    let dec = flate2::read::GzDecoder::new(tar_gz);
    let mut archive = tar::Archive::new(dec);

    let mut files: Vec<(String, u64)> = Vec::new();
    let mut skill_md_content = None;
    let mut metadata_content = None;

    for entry in archive
        .entries()
        .map_err(|e| format!("Failed to read archive: {}", e))?
    {
        let mut entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let path = entry
            .path()
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .to_string();
        let size = entry.size();
        files.push((path.clone(), size));

        if path == "SKILL.md" || path.ends_with("/SKILL.md") {
            let mut content = String::new();
            entry.read_to_string(&mut content).ok();
            skill_md_content = Some(content);
        } else if path == "borgclaw-package.json" {
            let mut content = String::new();
            entry.read_to_string(&mut content).ok();
            metadata_content = Some(content);
        }
    }

    println!("Files ({}):", files.len());
    for (path, size) in &files {
        println!("  {} ({} bytes)", path, size);
    }

    if let Some(meta) = metadata_content {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&meta) {
            println!("\nMetadata:");
            if let Some(name) = parsed.get("name").and_then(|v| v.as_str()) {
                println!("  name: {}", name);
            }
            if let Some(version) = parsed.get("version").and_then(|v| v.as_str()) {
                println!("  version: {}", version);
            }
            if let Some(at) = parsed.get("packaged_at").and_then(|v| v.as_str()) {
                println!("  packaged_at: {}", at);
            }
        }
    }

    if let Some(content) = skill_md_content {
        match borgclaw_core::skills::SkillManifest::parse(&content) {
            Ok(manifest) => {
                println!("\nManifest:");
                println!("  name: {}", manifest.name);
                println!("  version: {}", manifest.version);
                if !manifest.description.is_empty() {
                    println!("  description: {}", manifest.description);
                }
                println!("  commands: {}", manifest.commands.len());
                if let Some(min) = &manifest.min_version {
                    println!("  min_version: {}", min);
                }
            }
            Err(e) => println!("\n⚠ SKILL.md parse error: {}", e),
        }
    } else {
        println!("\n⚠ No SKILL.md found in archive");
    }

    Ok(())
}

/// Validate that a skill archive contains a valid SKILL.md
fn validate_archive_skill_md(package_path: &std::path::Path) -> Result<(), String> {
    use std::io::Read;

    let tar_gz =
        std::fs::File::open(package_path).map_err(|e| format!("Failed to open package: {}", e))?;
    let dec = flate2::read::GzDecoder::new(tar_gz);
    let mut archive = tar::Archive::new(dec);

    for entry in archive
        .entries()
        .map_err(|e| format!("Failed to read archive: {}", e))?
    {
        let mut entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let path = entry
            .path()
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .to_string();

        if path == "SKILL.md" || path.ends_with("/SKILL.md") {
            let mut content = String::new();
            entry
                .read_to_string(&mut content)
                .map_err(|e| format!("Failed to read SKILL.md: {}", e))?;
            let manifest = borgclaw_core::skills::SkillManifest::parse(&content)
                .map_err(|e| format!("Invalid SKILL.md: {}", e))?;
            if manifest.name.is_empty() {
                return Err("SKILL.md has empty name".to_string());
            }
            return Ok(());
        }
    }

    Err("Archive does not contain a SKILL.md".to_string())
}

/// Extract metadata from a package archive
fn extract_package_metadata(package_path: &std::path::Path) -> Result<serde_json::Value, String> {
    use std::io::Read;

    let tar_gz =
        std::fs::File::open(package_path).map_err(|e| format!("Failed to open package: {}", e))?;
    let dec = flate2::read::GzDecoder::new(tar_gz);
    let mut archive = tar::Archive::new(dec);

    for entry in archive
        .entries()
        .map_err(|e| format!("Failed to read archive: {}", e))?
    {
        let mut entry = entry.map_err(|e| format!("Failed to read archive entry: {}", e))?;
        let path = entry.path().map_err(|e| e.to_string())?;

        if path == std::path::Path::new("borgclaw-package.json") {
            let mut content = String::new();
            entry
                .read_to_string(&mut content)
                .map_err(|e| format!("Failed to read metadata: {}", e))?;
            return serde_json::from_str(&content)
                .map_err(|e| format!("Failed to parse metadata: {}", e));
        }
    }

    Err("Package metadata not found (borgclaw-package.json)".to_string())
}

/// Upload package to registry
async fn upload_to_registry(
    package_path: &std::path::Path,
    registry_url: &str,
    skill_name: &str,
) -> Result<String, String> {
    // Read package file
    let package_data = tokio::fs::read(package_path)
        .await
        .map_err(|e| format!("Failed to read package: {}", e))?;

    // Create multipart form
    let form = reqwest::multipart::Form::new()
        .text("name", skill_name.to_string())
        .part(
            "package",
            reqwest::multipart::Part::bytes(package_data)
                .file_name(format!("{}.tar.gz", skill_name))
                .mime_str("application/gzip")
                .map_err(|e| e.to_string())?,
        );

    // Upload
    let client = reqwest::Client::new();
    let upload_url = format!("{}/api/v1/skills/upload", registry_url);

    let response = client
        .post(&upload_url)
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("Failed to upload to registry: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Registry upload failed ({}): {}", status, body));
    }

    // Parse response
    let result: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse registry response: {}", e))?;

    let package_id = result
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or("Registry response missing package ID")?;

    Ok(package_id)
}

async fn fetch_registry_skills(registry_url: &str) -> Result<Vec<String>, String> {
    let (owner, repo) = github_registry_repo(registry_url).ok_or_else(|| {
        "registry listing currently supports GitHub repositories only".to_string()
    })?;

    let client = reqwest::Client::builder()
        .user_agent("BorgClaw/0.1")
        .build()
        .map_err(|e| e.to_string())?;
    let url = format!("https://api.github.com/repos/{owner}/{repo}/git/trees/main?recursive=1");
    let response = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("http {}", response.status()));
    }

    let tree: GitHubTreeResponse = response.json().await.map_err(|e| e.to_string())?;
    Ok(extract_registry_skills(&tree))
}

fn github_registry_repo(registry_url: &str) -> Option<(String, String)> {
    let trimmed = registry_url.strip_suffix('/').unwrap_or(registry_url);
    let repo = trimmed.strip_prefix("https://github.com/")?;
    let mut parts = repo.split('/');
    Some((parts.next()?.to_string(), parts.next()?.to_string()))
}

fn extract_registry_skills(tree: &GitHubTreeResponse) -> Vec<String> {
    let mut skills = tree
        .tree
        .iter()
        .filter(|entry| entry.kind == "blob" && entry.path.ends_with("/SKILL.md"))
        .filter_map(|entry| entry.path.strip_suffix("/SKILL.md"))
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    skills.sort();
    skills.dedup();
    skills
}

fn filter_registry_skills(skills: Vec<String>, filter: Option<&str>) -> Vec<String> {
    match filter {
        Some(filter) => skills
            .into_iter()
            .filter(|skill| skill.to_lowercase().contains(filter))
            .collect(),
        None => skills,
    }
}

fn registry_skill_install_id(skill: &str) -> String {
    skill.rsplit('/').next().unwrap_or(skill).to_string()
}

#[derive(Debug, Deserialize)]
struct GitHubTreeResponse {
    tree: Vec<GitHubTreeEntry>,
}

#[derive(Debug, Deserialize)]
struct GitHubTreeEntry {
    path: String,
    #[serde(rename = "type")]
    kind: String,
}

fn skill_install_id(source: &str) -> Result<String, String> {
    let trimmed = source.trim_end_matches('/');
    let tail = trimmed
        .rsplit('/')
        .next()
        .filter(|segment| !segment.is_empty())
        .ok_or_else(|| format!("Could not determine skill directory name from '{}'", source))?;
    let tail = tail.strip_suffix(".git").unwrap_or(tail);
    if tail.eq_ignore_ascii_case("skill.md") {
        let parent = trimmed
            .rsplit('/')
            .nth(1)
            .filter(|segment| !segment.is_empty())
            .ok_or_else(|| format!("Could not determine skill directory name from '{}'", source))?;
        return Ok(parent.to_string());
    }
    Ok(tail.to_string())
}

fn copy_dir_recursive(
    source: &std::path::Path,
    destination: &std::path::Path,
) -> Result<(), String> {
    std::fs::create_dir_all(destination).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(source).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let entry_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if entry_path.is_dir() {
            copy_dir_recursive(&entry_path, &destination_path)?;
        } else {
            std::fs::copy(&entry_path, &destination_path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use borgclaw_core::config::AppConfig;
    use borgclaw_core::security::SecurityLayer;
    use std::collections::HashMap;

    fn temp_config() -> AppConfig {
        let mut config = AppConfig::default();
        let root =
            std::env::temp_dir().join(format!("borgclaw_cli_status_test_{}", uuid::Uuid::new_v4()));
        config.security.secrets_path = root.join("secrets.enc");
        config
    }

    #[test]
    fn installs_local_skill_directory() {
        let root =
            std::env::temp_dir().join(format!("borgclaw_cli_skill_test_{}", uuid::Uuid::new_v4()));
        let source = root.join("source-skill");
        let skills_path = root.join("installed");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("SKILL.md"), "# Sample Skill").unwrap();

        let destination = install_local_skill(&skills_path, &source).unwrap();

        assert!(destination.join("SKILL.md").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolves_github_repo_to_archive_install_source() {
        let spec = resolve_skill_source("openclaw/weather", None).unwrap();
        match spec {
            SkillSource::RemoteArchive {
                url,
                install_id,
                archive_layout,
            } => {
                assert_eq!(
                    url,
                    "https://codeload.github.com/openclaw/weather/tar.gz/refs/heads/main"
                );
                assert_eq!(install_id.as_deref(), Some("weather"));
                assert!(archive_layout.strip_first_component);
                assert!(archive_layout.subdir.is_none());
            }
            _ => panic!("expected remote archive source"),
        }
    }

    #[test]
    fn resolves_registry_backed_skill_to_archive_install_source() {
        let spec = resolve_skill_source(
            "openclaw/weather",
            Some("https://github.com/openclaw/clawhub"),
        )
        .unwrap();
        match spec {
            SkillSource::RemoteArchive {
                url,
                install_id,
                archive_layout,
            } => {
                assert_eq!(
                    url,
                    "https://codeload.github.com/openclaw/clawhub/tar.gz/refs/heads/main"
                );
                assert_eq!(install_id.as_deref(), Some("weather"));
                assert_eq!(
                    archive_layout.subdir,
                    Some(std::path::PathBuf::from("openclaw/weather"))
                );
                assert!(archive_layout.strip_first_component);
            }
            _ => panic!("expected remote archive source"),
        }
    }

    #[test]
    fn extracts_github_registry_repo() {
        let repo = github_registry_repo("https://github.com/openclaw/clawhub").unwrap();
        assert_eq!(repo, ("openclaw".to_string(), "clawhub".to_string()));
    }

    #[test]
    fn derives_skill_id_from_sources() {
        assert_eq!(skill_install_id("openclaw/weather").unwrap(), "weather");
        assert_eq!(
            skill_install_id("https://example.com/skills/weather/SKILL.md").unwrap(),
            "weather"
        );
    }

    #[tokio::test]
    async fn installs_downloaded_skill_manifest() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_cli_remote_skill_test_{}",
            uuid::Uuid::new_v4()
        ));
        let skills_path = root.join("installed");
        let destination = install_skill_manifest(
            &skills_path,
            "openclaw/weather",
            "name: Weather\ndescription: Weather skill\n## Instructions\nUse weather APIs.\n",
        )
        .await
        .unwrap();

        assert!(destination.join("SKILL.md").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn installs_manifest_declared_companion_files_from_file_url() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_cli_manifest_files_test_{}",
            uuid::Uuid::new_v4()
        ));
        let remote = root.join("remote");
        let skills_path = root.join("installed");
        std::fs::create_dir_all(remote.join("assets")).unwrap();
        std::fs::create_dir_all(remote.join("prompts")).unwrap();
        std::fs::write(
            remote.join("SKILL.md"),
            "name: weather\nversion: 1.0.0\ndescription: Weather skill\nfiles:\n- assets/icon.txt\n- prompts/system.txt\n## Instructions\nUse weather APIs.\n",
        )
        .unwrap();
        std::fs::write(remote.join("assets/icon.txt"), "icon").unwrap();
        std::fs::write(remote.join("prompts/system.txt"), "prompt").unwrap();

        let manifest_url = reqwest::Url::from_file_path(remote.join("SKILL.md"))
            .unwrap()
            .to_string();
        let content = fetch_skill_manifest(&manifest_url).await.unwrap();
        let destination = install_skill_manifest(&skills_path, &manifest_url, &content)
            .await
            .unwrap();

        assert!(destination.join("SKILL.md").exists());
        assert_eq!(
            std::fs::read_to_string(destination.join("assets/icon.txt")).unwrap(),
            "icon"
        );
        assert_eq!(
            std::fs::read_to_string(destination.join("prompts/system.txt")).unwrap(),
            "prompt"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn installs_manifest_companion_files_from_sidecar_index() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_cli_manifest_sidecar_test_{}",
            uuid::Uuid::new_v4()
        ));
        let remote = root.join("remote");
        let skills_path = root.join("installed");
        std::fs::create_dir_all(remote.join("assets")).unwrap();
        std::fs::create_dir_all(remote.join("prompts")).unwrap();
        std::fs::write(
            remote.join("SKILL.md"),
            "name: weather\nversion: 1.0.0\ndescription: Weather skill\n## Instructions\nUse weather APIs.\n",
        )
        .unwrap();
        std::fs::write(
            remote.join("SKILL.files.json"),
            "[\"assets/icon.txt\", \"prompts/system.txt\"]",
        )
        .unwrap();
        std::fs::write(remote.join("assets/icon.txt"), "icon").unwrap();
        std::fs::write(remote.join("prompts/system.txt"), "prompt").unwrap();

        let manifest_url = reqwest::Url::from_file_path(remote.join("SKILL.md"))
            .unwrap()
            .to_string();
        let content = fetch_skill_manifest(&manifest_url).await.unwrap();
        let destination = install_skill_manifest(&skills_path, &manifest_url, &content)
            .await
            .unwrap();

        assert_eq!(
            std::fs::read_to_string(destination.join("assets/icon.txt")).unwrap(),
            "icon"
        );
        assert_eq!(
            std::fs::read_to_string(destination.join("prompts/system.txt")).unwrap(),
            "prompt"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn installs_manifest_companion_files_from_local_directory_discovery() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_cli_manifest_discovery_test_{}",
            uuid::Uuid::new_v4()
        ));
        let remote = root.join("remote");
        let skills_path = root.join("installed");
        std::fs::create_dir_all(remote.join("assets")).unwrap();
        std::fs::create_dir_all(remote.join("prompts")).unwrap();
        std::fs::write(
            remote.join("SKILL.md"),
            "name: weather\nversion: 1.0.0\ndescription: Weather skill\n## Instructions\nUse weather APIs.\n",
        )
        .unwrap();
        std::fs::write(remote.join("assets/icon.txt"), "icon").unwrap();
        std::fs::write(remote.join("prompts/system.txt"), "prompt").unwrap();

        let manifest_url = reqwest::Url::from_file_path(remote.join("SKILL.md"))
            .unwrap()
            .to_string();
        let content = fetch_skill_manifest(&manifest_url).await.unwrap();
        let destination = install_skill_manifest(&skills_path, &manifest_url, &content)
            .await
            .unwrap();

        assert_eq!(
            std::fs::read_to_string(destination.join("assets/icon.txt")).unwrap(),
            "icon"
        );
        assert_eq!(
            std::fs::read_to_string(destination.join("prompts/system.txt")).unwrap(),
            "prompt"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn extracts_hrefs_from_directory_listing_html() {
        let body = r#"
            <html><body>
            <a href="assets/">assets/</a>
            <a href="prompts/system.txt">system</a>
            <a href="../">parent</a>
            </body></html>
        "#;
        assert_eq!(
            extract_directory_listing_hrefs(body),
            vec![
                "assets/".to_string(),
                "prompts/system.txt".to_string(),
                "../".to_string()
            ]
        );
    }

    #[tokio::test]
    async fn installs_local_packaged_skill_archive() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_cli_local_archive_test_{}",
            uuid::Uuid::new_v4()
        ));
        let source = root.join("archive-skill");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(
            source.join("SKILL.md"),
            "name: archive-skill\nversion: 1.0.0\ndescription: Archive test\n## Instructions\nUse archive install.\n",
        )
        .unwrap();
        std::fs::write(source.join("README.md"), "archive docs").unwrap();
        let archive = root.join("archive-skill.tar.gz");
        let skills_path = root.join("installed");

        package_skill(&source, Some(&archive)).await.unwrap();
        let destination = install_skill(&skills_path, archive.to_str().unwrap(), None)
            .await
            .unwrap();

        assert!(destination.join("SKILL.md").exists());
        assert!(destination.join("README.md").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolves_github_raw_skill_urls_to_archive_installs() {
        let spec = resolve_skill_source(
            "https://raw.githubusercontent.com/openclaw/clawhub/main/openclaw/weather/SKILL.md",
            None,
        )
        .unwrap();

        match spec {
            SkillSource::RemoteArchive {
                url,
                install_id,
                archive_layout,
            } => {
                assert_eq!(
                    url,
                    "https://codeload.github.com/openclaw/clawhub/tar.gz/refs/heads/main"
                );
                assert_eq!(install_id.as_deref(), Some("weather"));
                assert_eq!(
                    archive_layout.subdir,
                    Some(std::path::PathBuf::from("openclaw/weather"))
                );
                assert!(archive_layout.strip_first_component);
            }
            _ => panic!("expected remote archive install source"),
        }
    }

    #[test]
    fn extracts_selected_skill_subdirectory_from_archive() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_cli_archive_extract_test_{}",
            uuid::Uuid::new_v4()
        ));
        let archive = root.join("registry.tar.gz");
        let extract = root.join("extract");
        std::fs::create_dir_all(&root).unwrap();
        let tar_gz = std::fs::File::create(&archive).unwrap();
        let enc = flate2::write::GzEncoder::new(tar_gz, flate2::Compression::default());
        let mut tar = tar::Builder::new(enc);

        let entries = [
            (
                "clawhub-main/openclaw/weather/SKILL.md",
                "name: weather\ndescription: Weather skill\n## Instructions\nUse weather APIs.\n",
            ),
            ("clawhub-main/openclaw/weather/assets/icon.txt", "icon"),
            (
                "clawhub-main/openclaw/calendar/SKILL.md",
                "name: calendar\ndescription: Calendar skill\n## Instructions\nUse calendar APIs.\n",
            ),
        ];
        for (path, content) in entries {
            let bytes = content.as_bytes();
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, path, bytes).unwrap();
        }
        let enc = tar.into_inner().unwrap();
        enc.finish().unwrap();

        std::fs::create_dir_all(&extract).unwrap();

        let bytes = std::fs::read(&archive).unwrap();
        extract_skill_archive(
            &bytes,
            &extract,
            &ArchiveLayout::github_subdir("openclaw/weather"),
        )
        .unwrap();

        assert!(extract.join("SKILL.md").exists());
        assert!(extract.join("assets/icon.txt").exists());
        assert!(!extract.join("calendar/SKILL.md").exists());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn extracts_registry_skills_from_github_tree() {
        let tree = GitHubTreeResponse {
            tree: vec![
                GitHubTreeEntry {
                    path: "openclaw/weather/SKILL.md".to_string(),
                    kind: "blob".to_string(),
                },
                GitHubTreeEntry {
                    path: "openclaw/weather/README.md".to_string(),
                    kind: "blob".to_string(),
                },
                GitHubTreeEntry {
                    path: "openclaw/calendar/SKILL.md".to_string(),
                    kind: "blob".to_string(),
                },
            ],
        };

        assert_eq!(
            extract_registry_skills(&tree),
            vec![
                "openclaw/calendar".to_string(),
                "openclaw/weather".to_string()
            ]
        );
    }

    #[test]
    fn filters_registry_skills_by_substring() {
        let skills = vec![
            "openclaw/weather".to_string(),
            "openclaw/calendar".to_string(),
        ];

        assert_eq!(
            filter_registry_skills(skills, Some("weath")),
            vec!["openclaw/weather".to_string()]
        );
    }

    #[test]
    fn derives_install_id_from_registry_skill() {
        assert_eq!(registry_skill_install_id("openclaw/weather"), "weather");
    }

    #[test]
    fn clear_screen_sequence_matches_documented_terminal_reset() {
        assert_eq!(CLEAR_SCREEN_SEQUENCE, "\x1B[2J\x1B[H");
    }

    #[test]
    fn repl_parser_recognizes_documented_local_commands() {
        assert_eq!(parse_repl_command("exit"), ReplCommand::Exit);
        assert_eq!(parse_repl_command("quit"), ReplCommand::Exit);
        assert_eq!(parse_repl_command("help"), ReplCommand::Help);
        assert_eq!(parse_repl_command("?"), ReplCommand::Help);
        assert_eq!(parse_repl_command("status"), ReplCommand::Status);
        assert_eq!(parse_repl_command("clear"), ReplCommand::Clear);
        assert_eq!(parse_repl_command("history"), ReplCommand::History);
        assert_eq!(parse_repl_command("hist"), ReplCommand::History);
        assert_eq!(parse_repl_command("hello"), ReplCommand::Message);
    }

    #[test]
    fn strip_think_blocks_removes_reasoning_content() {
        let input = "Hello <think>this is reasoning</think> world";
        assert_eq!(strip_think_blocks(input), "Hello world");

        let input_with_newlines = "Response\n<think>\nMulti-line\nreasoning\n</think>\nMore text";
        assert_eq!(
            strip_think_blocks(input_with_newlines),
            "Response\nMore text"
        );

        let no_think = "Just regular text";
        assert_eq!(strip_think_blocks(no_think), "Just regular text");

        let empty_think = "Before <think></think> After";
        assert_eq!(strip_think_blocks(empty_think), "Before After");

        let multiple_thinks = "Start <think>first</think> middle <think>second</think> end";
        assert_eq!(strip_think_blocks(multiple_thinks), "Start middle end");
    }

    #[test]
    fn secure_store_skill_placeholders_count_as_available() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let config = temp_config();
        runtime
            .block_on(
                SecurityLayer::with_config(config.security.clone())
                    .store_secret("ELEVENLABS_API_KEY", "eleven-secret"),
            )
            .unwrap();

        assert!(runtime.block_on(skill_secret_available(&config, "${ELEVENLABS_API_KEY}")));
    }

    #[test]
    fn openwebui_backend_requires_secret_and_base_url() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let mut config = temp_config();
        config.skills.stt.backend = "openwebui".to_string();
        config.skills.stt.openwebui.base_url = "http://localhost:3000".to_string();
        config.skills.stt.openwebui.api_key = "${OPENWEBUI_API_KEY}".to_string();

        assert!(!runtime.block_on(stt_backend_ready(&config)));

        runtime
            .block_on(
                SecurityLayer::with_config(config.security.clone())
                    .store_secret("OPENWEBUI_API_KEY", "openwebui-secret"),
            )
            .unwrap();

        assert!(runtime.block_on(stt_backend_ready(&config)));
    }

    #[test]
    fn yourls_provider_accepts_secure_store_password() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let mut config = temp_config();
        config.skills.url_shortener.provider = "yourls".to_string();
        config.skills.url_shortener.yourls.base_url = "https://sho.rt".to_string();
        config.skills.url_shortener.yourls.username = "borg".to_string();
        config.skills.url_shortener.yourls.password = "${YOURLS_PASSWORD}".to_string();

        assert!(!runtime.block_on(url_shortener_ready(&config)));

        runtime
            .block_on(
                SecurityLayer::with_config(config.security.clone())
                    .store_secret("YOURLS_PASSWORD", "yourls-secret"),
            )
            .unwrap();

        assert!(runtime.block_on(url_shortener_ready(&config)));
    }

    #[test]
    fn telegram_channel_status_uses_secure_store_placeholder() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let mut config = temp_config();
        let telegram = config.channels.entry("telegram".to_string()).or_default();
        telegram.enabled = true;
        telegram.credentials = Some("${TELEGRAM_BOT_TOKEN}".to_string());

        runtime
            .block_on(
                SecurityLayer::with_config(config.security.clone())
                    .store_secret("TELEGRAM_BOT_TOKEN", "tg-secret"),
            )
            .unwrap();

        let lines = runtime.block_on(channel_status_lines(&config));
        assert!(lines.iter().any(|line| line == "telegram: enabled (ready)"));
    }

    #[test]
    fn webhook_channel_doctor_reports_missing_secret() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let mut config = temp_config();
        let webhook = config.channels.entry("webhook".to_string()).or_default();
        webhook.enabled = true;
        webhook
            .extra
            .insert("port".to_string(), toml::Value::Integer(8080));

        let lines = runtime.block_on(channel_doctor_lines(&config));
        assert!(lines
            .iter()
            .any(|line| line == "✗ Webhook secret missing on port 8080"));
    }

    #[test]
    fn signal_channel_status_reports_missing_cli() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let mut config = temp_config();
        let signal = config.channels.entry("signal".to_string()).or_default();
        signal.enabled = true;
        signal.extra.insert(
            "phone_number".to_string(),
            toml::Value::String("+1234567890".to_string()),
        );
        signal.extra.insert(
            "signal_cli_path".to_string(),
            toml::Value::String("/definitely/missing/signal-cli".to_string()),
        );

        let lines = runtime.block_on(channel_status_lines(&config));
        assert!(lines
            .iter()
            .any(|line| line == "signal: enabled (missing signal-cli)"));
    }

    #[test]
    fn integration_status_lists_configured_mcp_servers() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let mut config = temp_config();
        config.mcp.servers.insert(
            "filesystem".to_string(),
            borgclaw_core::config::McpServerConfig {
                transport: "stdio".to_string(),
                command: Some("mcp-filesystem".to_string()),
                args: Vec::new(),
                env: HashMap::new(),
                url: None,
                headers: HashMap::new(),
            },
        );
        config.mcp.servers.insert(
            "github".to_string(),
            borgclaw_core::config::McpServerConfig {
                transport: "sse".to_string(),
                command: None,
                args: Vec::new(),
                env: HashMap::new(),
                url: Some("https://example.com/sse".to_string()),
                headers: HashMap::new(),
            },
        );

        let lines = runtime.block_on(integration_status_lines(&config));
        assert!(lines
            .iter()
            .any(|line| line == "MCP: 2 configured (filesystem:stdio, github:sse)"));
    }

    #[test]
    fn mcp_doctor_reports_blocked_stdio_server() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let mut config = temp_config();
        config.mcp.servers.insert(
            "blocked".to_string(),
            borgclaw_core::config::McpServerConfig {
                transport: "stdio".to_string(),
                command: Some("rm -rf /".to_string()),
                args: Vec::new(),
                env: HashMap::new(),
                url: None,
                headers: HashMap::new(),
            },
        );

        let lines = runtime.block_on(mcp_doctor_lines(&config));
        assert!(lines
            .iter()
            .any(|line| line == "✗ MCP summary: all 1 configured servers failing (blocked)"));
        assert!(lines
            .iter()
            .any(|line| line.starts_with("✗ MCP blocked (stdio) blocked by policy:")));
    }

    #[test]
    fn mcp_doctor_reports_missing_binary_for_stdio_server() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let mut config = temp_config();
        config.mcp.servers.insert(
            "missing".to_string(),
            borgclaw_core::config::McpServerConfig {
                transport: "stdio".to_string(),
                command: Some("/definitely/not/a/real/mcp".to_string()),
                args: Vec::new(),
                env: HashMap::new(),
                url: None,
                headers: HashMap::new(),
            },
        );

        let lines = runtime.block_on(mcp_doctor_lines(&config));
        assert!(lines
            .iter()
            .any(|line| line == "✗ MCP summary: all 1 configured servers failing (missing)"));
        assert!(lines.iter().any(|line| {
            line == "✗ MCP missing (stdio) missing binary: /definitely/not/a/real/mcp"
        }));
    }

    #[test]
    fn mcp_doctor_summarizes_multiple_failures() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let mut config = temp_config();
        config.mcp.servers.insert(
            "github".to_string(),
            borgclaw_core::config::McpServerConfig {
                transport: "sse".to_string(),
                command: None,
                args: Vec::new(),
                env: HashMap::new(),
                url: Some("http://127.0.0.1:9/sse".to_string()),
                headers: HashMap::new(),
            },
        );
        config.mcp.servers.insert(
            "missing".to_string(),
            borgclaw_core::config::McpServerConfig {
                transport: "stdio".to_string(),
                command: Some("/definitely/not/a/real/mcp".to_string()),
                args: Vec::new(),
                env: HashMap::new(),
                url: None,
                headers: HashMap::new(),
            },
        );

        let lines = runtime.block_on(mcp_doctor_lines(&config));
        assert!(lines.iter().any(
            |line| line == "✗ MCP summary: all 2 configured servers failing (github, missing)"
        ));
        assert!(lines
            .iter()
            .any(|line| line.starts_with("✗ MCP github (sse) ")));
    }

    #[test]
    fn security_policy_status_reports_effective_settings() {
        let mut config = temp_config();
        config.security.pairing.enabled = true;
        config.security.pairing.code_length = 8;
        config.security.pairing.expiry_seconds = 600;
        config.security.prompt_injection_defense = true;
        config.security.secret_leak_detection = true;
        config.security.extra_blocked = vec!["^danger$".to_string(), "^rm ".to_string()];
        config.security.allowed_commands = vec!["^git status$".to_string()];
        config.security.wasm_max_instances = 16;

        let line = security_policy_status(&config);

        assert!(line.contains("pairing=enabled(8 digits/600s)"));
        assert!(line.contains("prompt_injection=enabled(Block)"));
        assert!(line.contains("leak_detection=enabled(Redact)"));
        assert!(line.contains("blocklist=enabled+2"));
        assert!(line.contains("allowlist=1"));
        assert!(line.contains("wasm_instances=16"));
    }

    #[test]
    fn security_doctor_lines_report_disabled_controls() {
        let mut config = temp_config();
        config.security.pairing.enabled = false;
        config.security.prompt_injection_defense = false;
        config.security.secret_leak_detection = false;
        config.security.wasm_sandbox = false;
        config.security.command_blocklist = false;

        let lines = security_doctor_lines(&config);

        assert!(lines
            .iter()
            .any(|line| line == "✗ Pairing disabled (6 digits, 300s expiry)"));
        assert!(lines
            .iter()
            .any(|line| line == "✗ Prompt injection defense disabled (Block)"));
        assert!(lines
            .iter()
            .any(|line| line == "✗ Secret leak detection disabled (Redact)"));
        assert!(lines
            .iter()
            .any(|line| line == "✗ WASM sandbox disabled (max_instances=10)"));
        assert!(lines
            .iter()
            .any(|line| line == "✗ Command blocklist disabled (extra_patterns=0)"));
        assert!(lines
            .iter()
            .any(|line| line == "✓ Command allowlist disabled (patterns=0)"));
    }

    #[test]
    fn background_state_status_reports_persisted_counts() {
        let workspace = std::env::temp_dir().join(format!(
            "borgclaw_cli_background_state_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::write(
            workspace.join("scheduler.json"),
            r#"{
              "job-1": {"dead_lettered_at": null},
              "job-2": {"dead_lettered_at": null},
              "job-3": {"dead_lettered_at": "2026-03-19T00:00:00Z"}
            }"#,
        )
        .unwrap();
        std::fs::write(
            workspace.join("heartbeat.json"),
            r#"{
              "hb-1": {"dead_lettered_at": null},
              "hb-2": {"dead_lettered_at": "2026-03-19T00:00:00Z"}
            }"#,
        )
        .unwrap();
        std::fs::write(
            workspace.join("subagents.json"),
            r#"{
              "sg-1": {"dead_lettered_at": null}
            }"#,
        )
        .unwrap();

        let line = background_state_status(&workspace);
        assert_eq!(
            line,
            "scheduler=3 tasks (dead-lettered=1), heartbeat=2 tasks (dead-lettered=1), subagents=1 task (dead-lettered=0), processes=not created"
        );
    }

    #[test]
    fn background_state_doctor_lines_report_missing_and_present_state_files() {
        let workspace = std::env::temp_dir().join(format!(
            "borgclaw_cli_background_doctor_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::write(
            workspace.join("heartbeat.json"),
            r#"{
              "hb-1": {"dead_lettered_at": null}
            }"#,
        )
        .unwrap();

        let lines = background_state_doctor_lines(&workspace);
        assert!(lines
            .iter()
            .any(|line| line.contains("• Scheduler state not created yet")));
        assert!(lines
            .iter()
            .any(|line| line.contains("✓ Heartbeat state present (1 task, dead-lettered=0)")));
        assert!(lines
            .iter()
            .any(|line| line.contains("• Sub-agent state not created yet")));
        assert!(lines
            .iter()
            .any(|line| line.contains("• Process state not created yet")));
    }

    #[test]
    fn background_state_failure_lines_report_dead_lettered_tasks() {
        let workspace = std::env::temp_dir().join(format!(
            "borgclaw_cli_background_failures_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::write(
            workspace.join("scheduler.json"),
            r#"{
              "job-1": {"dead_lettered_at": null},
              "job-2": {"dead_lettered_at": "2026-03-19T00:00:00Z"}
            }"#,
        )
        .unwrap();
        std::fs::write(
            workspace.join("heartbeat.json"),
            r#"{
              "hb-1": {"dead_lettered_at": "2026-03-19T00:00:00Z"}
            }"#,
        )
        .unwrap();

        let failures = background_state_failure_lines(&workspace);
        assert!(failures
            .iter()
            .any(|line| line == "scheduler state has 1 dead-lettered of 2 persisted tasks"));
        assert!(failures
            .iter()
            .any(|line| line == "heartbeat state has 1 dead-lettered of 1 persisted task"));
    }

    #[test]
    fn process_list_and_detail_lines_report_persisted_background_processes() {
        let workspace = std::env::temp_dir().join(format!(
            "borgclaw_cli_process_state_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&workspace).unwrap();
        let state_path = process_state_path(&workspace);
        std::fs::write(
            &state_path,
            r#"{
              "proc-1": {
                "id": "proc-1",
                "command": "printf hello",
                "pid": 123,
                "started_at": "2026-03-30T12:00:00Z",
                "finished_at": "2026-03-30T12:00:01Z",
                "status": "succeeded",
                "exit_code": 0,
                "output": "hello",
                "pty": false,
                "timeout_secs": 60,
                "yield_ms": 250,
                "execution_mode": "host",
                "image": null
              }
            }"#,
        )
        .unwrap();

        let list_lines = process_list_lines(&state_path).unwrap();
        assert!(list_lines
            .iter()
            .any(|line| line.contains("proc-1 [succeeded] pid=123")));

        let detail_lines = process_detail_lines(&state_path, "proc-1").unwrap();
        assert!(detail_lines.iter().any(|line| line == "status: succeeded"));
        assert!(detail_lines
            .iter()
            .any(|line| line == "command: printf hello"));
        assert!(detail_lines
            .iter()
            .any(|line| line == "execution_mode: host"));

        std::fs::remove_dir_all(&workspace).unwrap();
    }

    #[test]
    fn schedule_list_lines_returns_empty_when_scheduler_state_is_missing() {
        let workspace = std::env::temp_dir().join(format!(
            "borgclaw_cli_schedule_list_missing_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&workspace).unwrap();

        let lines = schedule_list_lines(&workspace.join("scheduler.json")).unwrap();
        assert!(lines.is_empty());
    }

    #[test]
    fn schedule_list_lines_formats_persisted_jobs() {
        let workspace = std::env::temp_dir().join(format!(
            "borgclaw_cli_schedule_list_present_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::write(
            workspace.join("scheduler.json"),
            r#"{
              "job-1": {
                "id": "job-1",
                "name": "nightly-backup",
                "description": null,
                "trigger": {"type": "Cron", "value": "0 0 * * * *"},
                "action": "message",
                "status": "pending",
                "created_at": "2026-03-19T00:00:00Z",
                "last_run": null,
                "next_run": "2026-03-20T00:00:00Z",
                "run_count": 2,
                "max_retries": 3,
                "retry_count": 1,
                "retry_delay_seconds": 60,
                "dead_lettered_at": null,
                "run_history": [],
                "metadata": {}
              },
              "job-2": {
                "id": "job-2",
                "name": "stale-job",
                "description": null,
                "trigger": {"type": "Interval", "value": 300},
                "action": "message",
                "status": "failed",
                "created_at": "2026-03-19T00:00:00Z",
                "last_run": "2026-03-19T00:05:00Z",
                "next_run": null,
                "run_count": 4,
                "max_retries": 2,
                "retry_count": 2,
                "retry_delay_seconds": 30,
                "dead_lettered_at": "2026-03-19T00:06:00Z",
                "run_history": [],
                "metadata": {}
              }
            }"#,
        )
        .unwrap();

        let lines = schedule_list_lines(&workspace.join("scheduler.json")).unwrap();

        assert_eq!(lines.len(), 2);
        assert_eq!(
            lines[0],
            "stale-job [failed] trigger=interval(300s) next=none last=2026-03-19T00:05:00+00:00 runs=4 retries=2/2 dead-lettered"
        );
        assert_eq!(
            lines[1],
            "nightly-backup [pending] trigger=cron(0 0 * * * *) next=2026-03-20T00:00:00+00:00 last=never runs=2 retries=1/3 active"
        );
    }

    #[test]
    fn schedule_detail_lines_formats_metadata_and_history() {
        let workspace = std::env::temp_dir().join(format!(
            "borgclaw_cli_schedule_detail_present_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::write(
            workspace.join("scheduler.json"),
            r#"{
              "job-1": {
                "id": "job-1",
                "name": "nightly-backup",
                "description": "Backup local workspace",
                "trigger": {"type": "Cron", "value": "0 0 * * * *"},
                "action": "message",
                "status": "failed",
                "created_at": "2026-03-19T00:00:00Z",
                "last_run": "2026-03-19T00:05:00Z",
                "next_run": "2026-03-20T00:00:00Z",
                "run_count": 2,
                "max_retries": 3,
                "retry_count": 1,
                "retry_delay_seconds": 60,
                "dead_lettered_at": null,
                "run_history": [
                  {
                    "started_at": "2026-03-19T00:05:00Z",
                    "finished_at": "2026-03-19T00:05:03Z",
                    "status": "failed",
                    "error": "disk full",
                    "retry_scheduled": 2
                  }
                ],
                "metadata": {
                  "origin": "self-test",
                  "group_id": "ops"
                }
              }
            }"#,
        )
        .unwrap();

        let lines = schedule_detail_lines(&workspace.join("scheduler.json"), "job-1").unwrap();

        assert!(lines.iter().any(|line| line == "id: job-1"));
        assert!(lines.iter().any(|line| line == "name: nightly-backup"));
        assert!(lines
            .iter()
            .any(|line| line == "trigger: cron(0 0 * * * *)"));
        assert!(lines
            .iter()
            .any(|line| line == "metadata: group_id=ops, origin=self-test"));
        assert!(lines.iter().any(|line| {
            line == "  - 2026-03-19T00:05:00+00:00 -> 2026-03-19T00:05:03+00:00 [failed] retry=2 error=disk full"
        }));
    }

    #[test]
    fn schedule_detail_lines_returns_empty_for_missing_job() {
        let workspace = std::env::temp_dir().join(format!(
            "borgclaw_cli_schedule_detail_missing_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::write(workspace.join("scheduler.json"), "{}").unwrap();

        let lines = schedule_detail_lines(&workspace.join("scheduler.json"), "missing").unwrap();
        assert!(lines.is_empty());
    }

    #[test]
    fn export_backup_snapshot_writes_available_runtime_state() {
        let workspace = std::env::temp_dir().join(format!(
            "borgclaw_cli_backup_export_{}",
            uuid::Uuid::new_v4()
        ));
        let output = workspace.join("exports").join("snapshot.json");
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::write(
            workspace.join("scheduler.json"),
            r#"{"job-1":{"status":"pending","dead_lettered_at":null}}"#,
        )
        .unwrap();
        std::fs::write(
            workspace.join("heartbeat.json"),
            r#"{"hb-1":{"enabled":true,"dead_lettered_at":null}}"#,
        )
        .unwrap();

        let snapshot = export_backup_snapshot(&workspace, &output).unwrap();
        let written: BackupSnapshot =
            serde_json::from_str(&std::fs::read_to_string(&output).unwrap()).unwrap();

        assert_eq!(snapshot.workspace, workspace.display().to_string());
        assert_eq!(written.workspace, workspace.display().to_string());
        assert_eq!(written.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(written.scheduler.as_ref().map(|value| value.len()), Some(1));
        assert_eq!(written.heartbeat.as_ref().map(|value| value.len()), Some(1));
        assert!(written.subagents.is_none());
    }

    #[test]
    fn read_state_map_rejects_non_object_json() {
        let workspace = std::env::temp_dir().join(format!(
            "borgclaw_cli_backup_invalid_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&workspace).unwrap();
        let path = workspace.join("scheduler.json");
        std::fs::write(&path, "[]").unwrap();

        let err = read_state_map(&path).unwrap_err();
        assert!(err.contains("does not contain a JSON object"));
    }

    #[test]
    fn ollama_provider_does_not_require_credentials() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let mut config = temp_config();
        config.agent.provider = "ollama".to_string();

        assert!(matches!(
            runtime.block_on(provider_credential_status(&config)),
            ProviderCredentialStatus::NotRequired
        ));
    }

    #[test]
    fn self_test_failures_surface_missing_provider_credentials() {
        // Clear any existing provider env vars to ensure test is deterministic
        std::env::remove_var("OPENAI_API_KEY");
        std::env::remove_var("ANTHROPIC_API_KEY");

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let mut config = temp_config();
        config.agent.workspace = std::env::temp_dir().join(format!(
            "borgclaw_self_test_workspace_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&config.agent.workspace).unwrap();
        config.skills.skills_path = config.agent.workspace.join("skills");
        config.memory.database_path = config.agent.workspace.join("memory.db");
        config.agent.provider = "openai".to_string();

        let failures = runtime.block_on(self_test_failures(&config));
        assert!(failures
            .iter()
            .any(|line| line == "provider credential missing (OPENAI_API_KEY)"));
    }

    #[test]
    fn self_test_failures_surface_missing_pgvector_embedding_endpoint() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let mut config = temp_config();
        config.agent.workspace = std::env::temp_dir().join(format!(
            "borgclaw_self_test_pgvector_workspace_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&config.agent.workspace).unwrap();
        config.skills.skills_path = config.agent.workspace.join("skills");
        std::fs::create_dir_all(&config.skills.skills_path).unwrap();
        config.agent.provider = "ollama".to_string();
        config.memory.backend = MemoryBackend::Postgres;
        config.memory.connection_string = Some("postgres://localhost/borgclaw".to_string());
        config.memory.hybrid_search = true;
        config.memory.embedding_endpoint = None;

        let failures = runtime.block_on(self_test_failures(&config));
        assert!(failures
            .iter()
            .any(|line| line == "memory.embedding_endpoint missing for postgres hybrid search"));
    }

    #[test]
    fn self_test_failures_surface_dead_lettered_background_state() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let mut config = temp_config();
        config.agent.workspace = std::env::temp_dir().join(format!(
            "borgclaw_self_test_dead_lettered_workspace_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&config.agent.workspace).unwrap();
        config.skills.skills_path = config.agent.workspace.join("skills");
        std::fs::create_dir_all(&config.skills.skills_path).unwrap();
        config.memory.database_path = config.agent.workspace.join("memory.db");
        config.agent.provider = "ollama".to_string();
        std::fs::write(
            config.agent.workspace.join("subagents.json"),
            r#"{
              "sg-1": {"dead_lettered_at": "2026-03-19T00:00:00Z"}
            }"#,
        )
        .unwrap();

        let failures = runtime.block_on(self_test_failures(&config));
        assert!(failures
            .iter()
            .any(|line| line == "sub-agent state has 1 dead-lettered of 1 persisted task"));
    }

    #[tokio::test]
    async fn trigger_heartbeat_task_executes_persisted_task_immediately() {
        let workspace = std::env::temp_dir().join(format!(
            "borgclaw_cli_heartbeat_trigger_{}",
            uuid::Uuid::new_v4()
        ));
        let path = workspace.join("heartbeat.json");
        std::fs::create_dir_all(&workspace).unwrap();

        let id = create_heartbeat_task(&path, "health_check", "0 0 0 * * *", "", 0, 60).unwrap();

        let outcome = trigger_heartbeat_task(&path, &id).await.unwrap();
        match outcome {
            HeartbeatTriggerOutcome::Triggered(result) => {
                assert!(result.success);
                assert_eq!(result.message, "Health check passed");
            }
            _ => panic!("expected triggered heartbeat result"),
        }

        let lines = heartbeat_detail_lines(&path, &id).unwrap();
        assert!(lines.iter().any(|line| line == "run_count: 1"));
        assert!(lines
            .iter()
            .any(|line| line == "last_result: ok - Health check passed"));
    }

    #[tokio::test]
    async fn trigger_heartbeat_task_reports_disabled_tasks_without_running_them() {
        let workspace = std::env::temp_dir().join(format!(
            "borgclaw_cli_heartbeat_disabled_{}",
            uuid::Uuid::new_v4()
        ));
        let path = workspace.join("heartbeat.json");
        std::fs::create_dir_all(&workspace).unwrap();

        let id = create_heartbeat_task(&path, "health_check", "0 0 0 * * *", "", 0, 60).unwrap();
        assert!(update_heartbeat_task_status(&path, &id, false).unwrap());

        let outcome = trigger_heartbeat_task(&path, &id).await.unwrap();
        assert!(matches!(outcome, HeartbeatTriggerOutcome::Disabled));
    }

    #[tokio::test]
    async fn trigger_heartbeat_task_reports_missing_tasks() {
        let workspace = std::env::temp_dir().join(format!(
            "borgclaw_cli_heartbeat_missing_{}",
            uuid::Uuid::new_v4()
        ));
        let path = workspace.join("heartbeat.json");
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::write(&path, "{}").unwrap();

        let outcome = trigger_heartbeat_task(&path, "missing").await.unwrap();
        assert!(matches!(outcome, HeartbeatTriggerOutcome::Missing));
    }
}

fn print_help() {
    println!("{}", "Available Commands:".cyan().bold());
    println!();
    println!("  {}  {:12} - Exit the REPL", "🚪".yellow(), "exit, quit",);
    println!(
        "  {}  {:12} - Show this help message",
        "❓".yellow(),
        "help",
    );
    println!(
        "  {}  {:12} - Show agent and system status",
        "📊".yellow(),
        "status",
    );
    println!("  {}  {:12} - Clear the screen", "🧹".yellow(), "clear",);
    println!(
        "  {}  {:12} - Show command history",
        "📜".yellow(),
        "history",
    );
    println!();
    println!("{}", "Tips:".dimmed().bold());
    println!("  • Type any message to chat with the agent");
    println!(
        "  • Use {} to enable verbose timing output",
        "BORGCLAW_REPL_VERBOSE=1".cyan()
    );
    println!("  • Command history is saved between sessions");
}

const CLEAR_SCREEN_SEQUENCE: &str = "\x1B[2J\x1B[H";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReplCommand {
    Exit,
    Help,
    Status,
    Clear,
    History,
    Message,
}

fn parse_repl_command(input: &str) -> ReplCommand {
    match input {
        "exit" | "quit" => ReplCommand::Exit,
        "help" | "?" => ReplCommand::Help,
        "status" => ReplCommand::Status,
        "clear" => ReplCommand::Clear,
        "history" | "hist" => ReplCommand::History,
        _ => ReplCommand::Message,
    }
}

fn clear_screen() {
    print!("{}", CLEAR_SCREEN_SEQUENCE);
    let _ = std::io::Write::flush(&mut std::io::stdout());
}

#[cfg(test)]
mod skill_packaging_tests {
    use super::*;
    use std::path::Path;

    fn create_test_skill_dir(root: &Path, name: &str) -> PathBuf {
        let skill_dir = root.join(name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            format!(
                "name: {}\nversion: 1.0.0\ndescription: Test skill\n## Instructions\nTest instructions.\n",
                name
            ),
        )
        .unwrap();
        skill_dir
    }

    #[test]
    fn package_skill_creates_valid_tar_gz() {
        let root =
            std::env::temp_dir().join(format!("borgclaw_package_test_{}", uuid::Uuid::new_v4()));
        let skill_dir = create_test_skill_dir(&root, "test-skill");
        let output = root.join("test-skill-1.0.0.tar.gz");

        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(package_skill(&skill_dir, Some(&output)));

        assert!(result.is_ok());
        assert!(output.exists());

        // Verify we can extract metadata from the created package
        let metadata = extract_package_metadata(&output).unwrap();
        assert_eq!(metadata["name"].as_str().unwrap(), "test-skill");
        assert_eq!(metadata["version"].as_str().unwrap(), "1.0.0");

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn package_skill_rejects_missing_skill_md() {
        let root =
            std::env::temp_dir().join(format!("borgclaw_package_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let output = root.join("test.tar.gz");

        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(package_skill(&root, Some(&output)));

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("SKILL.md"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn package_skill_rejects_non_directory() {
        let root =
            std::env::temp_dir().join(format!("borgclaw_package_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("not-a-directory");
        std::fs::write(&file, "test").unwrap();

        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(package_skill(&file, None));

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not a directory"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn package_skill_includes_metadata_file() {
        let root =
            std::env::temp_dir().join(format!("borgclaw_package_test_{}", uuid::Uuid::new_v4()));
        let skill_dir = create_test_skill_dir(&root, "metadata-test");
        let output = root.join("metadata-test.tar.gz");

        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(package_skill(&skill_dir, Some(&output)))
            .unwrap();

        // Verify borgclaw-package.json exists in archive
        let metadata = extract_package_metadata(&output).unwrap();
        assert!(metadata["packaged_at"].as_str().is_some());
        assert_eq!(metadata["packaged_by"].as_str().unwrap(), "borgclaw-cli");

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn extract_package_metadata_fails_for_missing_metadata() {
        let root =
            std::env::temp_dir().join(format!("borgclaw_package_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();

        // Create a valid tar.gz without metadata file
        let output = root.join("invalid.tar.gz");
        let tar_gz = std::fs::File::create(&output).unwrap();
        let enc = flate2::write::GzEncoder::new(tar_gz, flate2::Compression::default());
        let mut tar = tar::Builder::new(enc);

        let mut header = tar::Header::new_gnu();
        header.set_size(12);
        header.set_mode(0o644);
        header.set_cksum();
        tar.append_data(&mut header, "just-a-file.txt", "test content".as_bytes())
            .unwrap();

        let enc = tar.into_inner().unwrap();
        enc.finish().unwrap();

        let result = extract_package_metadata(&output);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Package metadata not found"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn create_skill_archive_preserves_directory_structure() {
        let root =
            std::env::temp_dir().join(format!("borgclaw_package_test_{}", uuid::Uuid::new_v4()));
        let skill_dir = root.join("complex-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "name: complex\nversion: 2.0.0\ndescription: Complex skill\n## Instructions\n",
        )
        .unwrap();

        // Create subdirectory with files
        let subdir = skill_dir.join("src");
        std::fs::create_dir_all(&subdir).unwrap();
        std::fs::write(subdir.join("main.rs"), "// main code").unwrap();
        std::fs::write(subdir.join("lib.rs"), "// lib code").unwrap();

        let output = root.join("complex.tar.gz");
        create_skill_archive(&skill_dir, &output, "complex", "2.0.0").unwrap();

        // Verify archive was created
        assert!(output.exists());

        // Extract and verify structure
        let tar_gz = std::fs::File::open(&output).unwrap();
        let dec = flate2::read::GzDecoder::new(tar_gz);
        let mut archive = tar::Archive::new(dec);

        let entries: Vec<_> = archive.entries().unwrap().collect();
        let paths: Vec<_> = entries
            .iter()
            .filter_map(|e| e.as_ref().ok())
            .map(|e| e.path().unwrap().to_string_lossy().to_string())
            .collect();

        assert!(paths.contains(&"SKILL.md".to_string()));
        assert!(paths.contains(&"src/main.rs".to_string()));
        assert!(paths.contains(&"src/lib.rs".to_string()));
        assert!(paths.contains(&"borgclaw-package.json".to_string()));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn inspect_skill_package_lists_contents() {
        let root =
            std::env::temp_dir().join(format!("borgclaw_inspect_test_{}", uuid::Uuid::new_v4()));
        let skill_dir = create_test_skill_dir(&root, "inspect-skill");
        let output = root.join("inspect-skill-1.0.0.tar.gz");

        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(package_skill(&skill_dir, Some(&output)))
            .unwrap();

        // inspect_skill_package should succeed and not return an error
        let result = inspect_skill_package(&output);
        assert!(result.is_ok());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn validate_archive_skill_md_passes_valid_package() {
        let root =
            std::env::temp_dir().join(format!("borgclaw_validate_test_{}", uuid::Uuid::new_v4()));
        let skill_dir = create_test_skill_dir(&root, "valid-skill");
        let output = root.join("valid-skill-1.0.0.tar.gz");

        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(package_skill(&skill_dir, Some(&output)))
            .unwrap();

        assert!(validate_archive_skill_md(&output).is_ok());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn validate_archive_skill_md_rejects_archive_without_skill_md() {
        let root =
            std::env::temp_dir().join(format!("borgclaw_validate_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();

        let output = root.join("no-skill.tar.gz");
        let tar_gz = std::fs::File::create(&output).unwrap();
        let enc = flate2::write::GzEncoder::new(tar_gz, flate2::Compression::default());
        let mut tar = tar::Builder::new(enc);

        let mut header = tar::Header::new_gnu();
        header.set_size(4);
        header.set_mode(0o644);
        header.set_cksum();
        tar.append_data(&mut header, "README.md", "test".as_bytes())
            .unwrap();

        let enc = tar.into_inner().unwrap();
        enc.finish().unwrap();

        let result = validate_archive_skill_md(&output);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not contain a SKILL.md"));

        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod cli_path_tests {
    use super::{cli_path_available, set_config_key};
    use borgclaw_core::config::AppConfig;
    use std::path::Path;

    #[test]
    fn cli_path_available_accepts_existing_explicit_paths() {
        assert!(cli_path_available(Path::new("/bin/sh")));
    }

    #[test]
    fn cli_path_available_rejects_missing_explicit_paths() {
        assert!(!cli_path_available(Path::new(
            "/definitely/not/a/real/binary"
        )));
    }

    #[test]
    fn set_config_key_updates_agent_provider_profile() {
        let mut config = AppConfig::default();
        assert!(set_config_key(
            &mut config,
            "agent.provider_profile",
            "openai-work"
        ));
        assert_eq!(
            config.agent.provider_profile.as_deref(),
            Some("openai-work")
        );
    }
}

#[cfg(test)]
mod secrets_tests {
    use borgclaw_core::security::{SecretStore, SecretStoreConfig};

    fn temp_store() -> (SecretStore, std::path::PathBuf) {
        let root =
            std::env::temp_dir().join(format!("borgclaw_secrets_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("secrets.json");
        let store = SecretStore::with_config(SecretStoreConfig {
            encryption_enabled: false,
            secrets_path: Some(path),
        });
        (store, root)
    }

    #[tokio::test]
    async fn secrets_list_returns_stored_keys() {
        let (store, root) = temp_store();
        store.store("KEY_A", "val_a").await.unwrap();
        store.store("KEY_B", "val_b").await.unwrap();
        let mut keys = store.keys().await;
        keys.sort();
        assert_eq!(keys, vec!["KEY_A", "KEY_B"]);
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn secrets_set_and_check() {
        let (store, root) = temp_store();
        assert!(!store.exists("MY_SECRET").await);
        store.store("MY_SECRET", "hidden").await.unwrap();
        assert!(store.exists("MY_SECRET").await);
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn secrets_delete_removes_key() {
        let (store, root) = temp_store();
        store.store("TO_DELETE", "val").await.unwrap();
        assert!(store.exists("TO_DELETE").await);
        store.delete("TO_DELETE").await;
        assert!(!store.exists("TO_DELETE").await);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn secrets_key_path_derives_correctly() {
        let path = std::path::Path::new("/tmp/borgclaw/secrets.enc");
        let key_path = borgclaw_core::security::secrets_key_path(path);
        assert_eq!(
            key_path,
            std::path::PathBuf::from("/tmp/borgclaw/secrets.enc.key")
        );
    }
}
