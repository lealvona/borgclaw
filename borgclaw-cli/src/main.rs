//! BorgClaw CLI - Command-line interface with REPL

use borgclaw_core::{
    agent::{Agent, AgentContext, AgentResponse, SimpleAgent, builtin_tools},
    channel::{create_cli_message, CliChannel},
    config::{load_config, save_config, AppConfig},
    security::SecurityLayer,
    AppState,
};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
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
    
    /// Verbose output
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Start interactive REPL
    Repl,
    /// Send a message
    Send { message: String },
    /// Initialize configuration
    Init,
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
    /// Set a config value
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
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();
    
    let cli = Cli::parse();
    
    // Get config path
    let config_path = cli.config.unwrap_or_else(|| {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("borgclaw")
            .join("config.toml")
    });
    
    // Ensure config directory exists
    if let Some(parent) = config_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    
    // Load or create config
    let config = if config_path.exists() {
        load_config(&config_path).unwrap_or_else(|e| {
            error!("Failed to load config: {}", e);
            AppConfig::default()
        })
    } else {
        AppConfig::default()
    };
    
    // Handle commands
    match cli.command {
        Commands::Repl => repl(config, config_path).await,
        Commands::Send { message } => send_message(config, message).await,
        Commands::Init => init_config(&config_path, config).await,
        Commands::Config { action } => config_action(&config_path, config, action).await,
        Commands::Skills { action } => skills_action(config, action).await,
        Commands::Status => status(config).await,
        Commands::Doctor => doctor(config).await,
    }
}

async fn repl(mut config: AppConfig, config_path: PathBuf) {
    info!("Starting BorgClaw REPL...");
    
    // Initialize agent
    let mut agent = SimpleAgent::new(config.agent.clone());
    
    // Register built-in tools
    for tool in borgclaw_core::agent::builtin_tools() {
        agent.register_tool(tool);
    }
    
    // Create app state
    let state = AppState::new(config.clone());
    
    // Replace agent in state
    *state.agent.write().await = Some(Box::new(agent));
    
    println!("🦞 BorgClaw REPL (type 'exit' to quit, 'help' for commands)\n");
    
    // Simple REPL loop
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
        
        // Create message context
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
        
        // Process message
        if let Some(ref mut agent) = *state.agent.write().await {
            let response = agent.process(&ctx).await;
            println!("{}", response.text);
        }
    }
}

async fn send_message(config: AppConfig, message: String) {
    let mut agent = SimpleAgent::new(config.agent.clone());
    
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

async fn init_config(path: &PathBuf, config: AppConfig) {
    if path.exists() {
        println!("Config already exists at {:?}", path);
        return;
    }
    
    if let Err(e) = save_config(&config, &path.to_owned()) {
        error!("Failed to save config: {}", e);
        return;
    }
    
    println!("Initialized config at {:?}", path);
    println!("\nEdit the config file to customize your setup, then run 'borgclaw repl' to start.");
}

async fn config_action(path: &PathBuf, mut config: AppConfig, action: ConfigAction) {
    match action {
        ConfigAction::Show => {
            let toml = toml::to_string_pretty(&config).unwrap();
            println!("{}", toml);
        }
        ConfigAction::Set { key, value } => {
            println!("Setting {} = {} (not implemented)", key, value);
        }
        ConfigAction::Reset => {
            config = AppConfig::default();
            if let Err(e) = save_config(&config, &path) {
                error!("Failed to save config: {}", e);
                return;
            }
            println!("Config reset to defaults");
        }
    }
}

async fn skills_action(config: AppConfig, action: SkillsAction) {
    match action {
        SkillsAction::List => {
            println!("Built-in skills:");
            println!("  - memory_store: Store information in memory");
            println!("  - memory_recall: Recall information from memory");
            println!("  - execute_command: Execute a shell command");
            println!("  - read_file: Read a file");
            println!("  - list_directory: List directory contents");
            println!("  - web_search: Search the web");
            println!("  - fetch_url: Fetch URL content");
            println!("  - message: Send a message");
            println!("  - schedule_task: Schedule a task");
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
    println!("");
    println!("Channels:");
    for (name, channel) in &config.channels {
        println!("  - {}: {}", name, if channel.enabled { "enabled" } else { "disabled" });
    }
}

async fn doctor(config: AppConfig) {
    println!("Running diagnostics...");
    
    // Check workspace
    if config.agent.workspace.exists() {
        println!("✓ Workspace exists");
    } else {
        println!("✗ Workspace does not exist: {:?}", config.agent.workspace);
    }
    
    // Check config
    println!("✓ Config loaded");
    
    // Check security
    let security = SecurityLayer::new();
    let check = security.check_command("rm -rf /");
    match check {
        borgclaw_core::security::CommandCheck::Blocked(_) => {
            println!("✓ Command blocklist working");
        }
        _ => {
            println!("✗ Command blocklist not working");
        }
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
