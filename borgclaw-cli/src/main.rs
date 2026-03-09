//! BorgClaw CLI - Command-line interface with REPL

mod onboarding;

use crate::onboarding::{run_init, InitArgs, StartTarget};
use borgclaw_core::{
    channel::{ChannelType, InboundMessage, MessagePayload, MessageRouter, Sender},
    config::{load_config, save_config, AppConfig},
    security::SecurityLayer,
    skills::SkillsRegistry,
};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::{error, info};

/// BorgClaw CLI
#[derive(Parser)]
#[command(name = "borgclaw")]
#[command(about = "BorgClaw - Personal AI Agent", long_about = None)]
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
enum SkillsAction {
    /// List skills
    List,
    /// Install a skill
    Install { name: String },
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

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

    let config = if config_path.exists() {
        load_config(&config_path).unwrap_or_else(|e| {
            error!("Failed to load config: {}", e);
            AppConfig::default()
        })
    } else {
        AppConfig::default()
    };

    match cli.command {
        Commands::Repl => repl(config, config_path).await,
        Commands::Send { message } => send_message(config, message).await,
        Commands::Init(args) => {
            match run_init(&config_path, config, &args).await {
                Ok(outcome) => {
                    if let Err(e) = save_config(&outcome.config, &config_path) {
                        error!("Failed to save config: {}", e);
                        return;
                    }
                    println!("Saved config to {:?}", config_path);
                    if outcome.start == StartTarget::Repl {
                        repl(outcome.config, config_path).await;
                    }
                }
                Err(e) => error!("Init failed: {}", e),
            }
        }
        Commands::Config { action } => config_action(&config_path, config, action).await,
        Commands::Skills { action } => skills_action(&config, action).await,
        Commands::Status => status(config).await,
        Commands::Doctor => doctor(config).await,
    }
}

async fn repl(config: AppConfig, _config_path: PathBuf) {
    info!("Starting BorgClaw REPL...");
    let router = MessageRouter::from_config(&config);

    println!("🦞 BorgClaw REPL (type 'exit' to quit, 'help' for commands)\n");

    loop {
        print!("> ");
        std::io::Write::flush(&mut std::io::stdout()).unwrap();

        let mut input = String::new();
        if std::io::stdin().read_line(&mut input).unwrap() == 0 {
            break;
        }

        let input = input.trim();
        if input.is_empty() {
            continue;
        }

        if input == "exit" || input == "quit" {
            println!("Goodbye!");
            break;
        }

        if input == "help" {
            print_help();
            continue;
        }

        let message = InboundMessage {
            channel: ChannelType::cli(),
            sender: Sender::new("cli").with_name("User"),
            content: MessagePayload::text(input),
            group_id: Some("cli".to_string()),
            timestamp: chrono::Utc::now(),
            raw: serde_json::Value::Null,
        };

        match router.route(message).await {
            Ok(outcome) => println!("{}", outcome.response.text),
            Err(err) => println!("Error: {}", err),
        }
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
        "security.docker_sandbox" => {
            if let Ok(v) = value.parse::<bool>() {
                config.security.docker_sandbox = v;
            } else {
                return false;
            }
        }
        _ => return false,
    }
    true
}

async fn skills_action(config: &AppConfig, action: SkillsAction) {
    match action {
        SkillsAction::List => {
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
            if registry.load_all().await.is_ok() {
                let installed = registry.list().await;
                if !installed.is_empty() {
                    println!("\nInstalled skills:");
                    for (id, name) in installed {
                        println!("  - {} ({})", name, id);
                    }
                }
            }
        }
        SkillsAction::Install { name } => {
            match install_local_skill(&config.skills.skills_path, &name) {
                Ok(path) => println!("Installed skill to {:?}", path),
                Err(err) => println!("{}", err),
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
        provider_env_var(&config.agent.provider)
            .map(|key| if std::env::var(key).is_ok() { "configured" } else { "missing env" })
            .unwrap_or("unknown")
    );
    println!(
        "Security: wasm={}, docker={}, approval={:?}",
        config.security.wasm_sandbox,
        config.security.docker_sandbox,
        config.security.approval_mode
    );
    println!("Memory path: {:?}", config.memory.memory_path);
    println!("Skills path: {:?}", config.skills.skills_path);
    println!("\nChannels:");
    for (name, channel) in &config.channels {
        println!("  - {}: {}", name, if channel.enabled { "enabled" } else { "disabled" });
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
        borgclaw_core::security::CommandCheck::Blocked(_) => println!("✓ Command blocklist working"),
        _ => println!("✗ Command blocklist not working"),
    }
    match provider_env_var(&config.agent.provider) {
        Some(env_key) if std::env::var(env_key).is_ok() => println!("✓ Provider credential env present ({})", env_key),
        Some(env_key) => println!("✗ Provider credential env missing ({})", env_key),
        None => println!("✗ Unknown provider '{}'", config.agent.provider),
    }
    if std::fs::create_dir_all(&config.memory.memory_path).is_ok() && config.memory.memory_path.exists() {
        println!("✓ Memory path available");
    } else {
        println!("✗ Memory path unavailable: {:?}", config.memory.memory_path);
    }
    if std::fs::create_dir_all(&config.skills.skills_path).is_ok() && config.skills.skills_path.exists() {
        println!("✓ Skills path available");
    } else {
        println!("✗ Skills path unavailable: {:?}", config.skills.skills_path);
    }
    println!("\nDiagnostics complete.");
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

fn install_local_skill(skills_path: &std::path::Path, source: &str) -> Result<std::path::PathBuf, String> {
    let source_path = std::path::PathBuf::from(source);
    if !source_path.exists() {
        return Err(format!(
            "Skill install currently supports local skill directories only. '{}' was not found.",
            source
        ));
    }
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

    copy_dir_recursive(&source_path, &destination)?;
    Ok(destination)
}

fn copy_dir_recursive(source: &std::path::Path, destination: &std::path::Path) -> Result<(), String> {
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

    #[test]
    fn installs_local_skill_directory() {
        let root = std::env::temp_dir().join(format!("borgclaw_cli_skill_test_{}", uuid::Uuid::new_v4()));
        let source = root.join("source-skill");
        let skills_path = root.join("installed");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("SKILL.md"), "# Sample Skill").unwrap();

        let destination = install_local_skill(&skills_path, source.to_str().unwrap()).unwrap();

        assert!(destination.join("SKILL.md").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}

fn print_help() {
    println!("Commands:");
    println!("  exit, quit   - Exit the REPL");
    println!("  help         - Show this help message");
    println!("  status       - Show agent status");
    println!("  clear        - Clear screen");
}
