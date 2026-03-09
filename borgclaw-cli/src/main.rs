//! BorgClaw CLI - Command-line interface with REPL

mod onboarding;

use crate::onboarding::{run_init, InitArgs, StartTarget};
use borgclaw_core::{
    agent::{Agent, AgentContext, SimpleAgent},
    config::{load_config, save_config, AppConfig},
    security::SecurityLayer,
    AppState,
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
        Commands::Skills { action } => skills_action(action).await,
        Commands::Status => status(config).await,
        Commands::Doctor => doctor(config).await,
    }
}

async fn repl(config: AppConfig, _config_path: PathBuf) {
    info!("Starting BorgClaw REPL...");

    let mut agent = SimpleAgent::new(
        config.agent.clone(),
        Some(config.memory.clone()),
        Some(config.security.clone()),
    );
    for tool in borgclaw_core::agent::builtin_tools() {
        agent.register_tool(tool);
    }

    let state = AppState::new(config.clone());
    *state.agent.write().await = Some(Box::new(agent));

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

        let ctx = AgentContext {
            session_id: borgclaw_core::agent::SessionId::new(),
            message: input.to_string(),
            sender: borgclaw_core::agent::SenderInfo {
                id: "cli".to_string(),
                name: Some("User".to_string()),
                channel: "cli".to_string(),
            },
            metadata: std::collections::HashMap::new(),
        };

        if let Some(ref mut agent) = *state.agent.write().await {
            let response = agent.process(&ctx).await;
            println!("{}", response.text);
        }
    }
}

async fn send_message(config: AppConfig, message: String) {
    let mut agent = SimpleAgent::new(
        config.agent.clone(),
        Some(config.memory.clone()),
        Some(config.security.clone()),
    );
    let ctx = AgentContext {
        session_id: borgclaw_core::agent::SessionId::new(),
        message,
        sender: borgclaw_core::agent::SenderInfo {
            id: "cli".to_string(),
            name: Some("User".to_string()),
            channel: "cli".to_string(),
        },
        metadata: std::collections::HashMap::new(),
    };
    let response = agent.process(&ctx).await;
    println!("{}", response.text);
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

async fn skills_action(action: SkillsAction) {
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
        }
        SkillsAction::Install { name } => {
            println!("Installing skill '{}' (not implemented)", name);
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
    println!("\nDiagnostics complete.");
}

fn print_help() {
    println!("Commands:");
    println!("  exit, quit   - Exit the REPL");
    println!("  help         - Show this help message");
    println!("  status       - Show agent status");
    println!("  clear        - Clear screen");
}
