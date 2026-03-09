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
use serde::Deserialize;
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
            if let Some(registry_url) = config.skills.registry_url.as_deref() {
                match fetch_registry_skills(registry_url).await {
                    Ok(skills) if !skills.is_empty() => {
                        println!("\nRegistry skills:");
                        for skill in skills {
                            println!("  - {}", skill);
                        }
                    }
                    Ok(_) => println!("\nRegistry skills: none found"),
                    Err(err) => println!("\nRegistry lookup failed: {}", err),
                }
            }
        }
        SkillsAction::Install { name } => {
            match install_skill(&config.skills.skills_path, &name, config.skills.registry_url.as_deref()).await {
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
    println!(
        "Vault: {}",
        config
            .security
            .vault
            .provider
            .as_deref()
            .unwrap_or("disabled")
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
    match config.security.vault.provider.as_deref() {
        Some("bitwarden") if cli_in_path("bw") => println!("✓ Bitwarden CLI available"),
        Some("bitwarden") => println!("✗ Bitwarden CLI missing (bw)"),
        Some("1password") if cli_in_path("op") => println!("✓ 1Password CLI available"),
        Some("1password") => println!("✗ 1Password CLI missing (op)"),
        Some(other) => println!("✗ Unsupported vault provider '{}'", other),
        None => println!("• Vault integration disabled"),
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

fn cli_in_path(binary: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths)
                .any(|dir| dir.join(binary).exists())
        })
        .unwrap_or(false)
}

async fn install_skill(
    skills_path: &std::path::Path,
    source: &str,
    registry_url: Option<&str>,
) -> Result<std::path::PathBuf, String> {
    let source_path = std::path::PathBuf::from(source);
    if source_path.exists() {
        return install_local_skill(skills_path, &source_path);
    }

    if let Some(url) = skill_source_url(source, registry_url) {
        let content = fetch_skill_manifest(&url).await?;
        return install_skill_manifest(skills_path, source, &content);
    }

    Err(format!(
        "Unsupported skill source '{}'. Use a local directory, owner/repo, or direct SKILL.md URL.",
        source
    ))
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

    copy_dir_recursive(&source_path, &destination)?;
    Ok(destination)
}

fn install_skill_manifest(
    skills_path: &std::path::Path,
    source: &str,
    content: &str,
) -> Result<std::path::PathBuf, String> {
    borgclaw_core::skills::SkillManifest::parse(content).map_err(|e| e.to_string())?;

    std::fs::create_dir_all(skills_path).map_err(|e| e.to_string())?;
    let skill_id = skill_install_id(source)?;
    let destination = skills_path.join(&skill_id);
    if destination.exists() {
        return Err(format!("Skill '{}' is already installed", skill_id));
    }

    std::fs::create_dir_all(&destination).map_err(|e| e.to_string())?;
    std::fs::write(destination.join("SKILL.md"), content).map_err(|e| e.to_string())?;
    Ok(destination)
}

async fn fetch_skill_manifest(url: &str) -> Result<String, String> {
    let response = reqwest::get(url).await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("failed to download skill manifest: http {}", response.status()));
    }

    let content = response.text().await.map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Err("downloaded skill manifest is empty".to_string());
    }

    Ok(content)
}

fn skill_source_url(source: &str, registry_url: Option<&str>) -> Option<String> {
    if source.starts_with("http://") || source.starts_with("https://") {
        return Some(source.to_string());
    }

    let normalized = source.trim_matches('/');
    let parts = normalized.split('/').collect::<Vec<_>>();
    if parts.len() != 2 || parts.iter().any(|part| part.is_empty()) {
        return None;
    }

    if let Some(base) = registry_url.and_then(github_registry_base) {
        return Some(format!("{}/{}/SKILL.md", base, normalized));
    }

    Some(format!(
        "https://raw.githubusercontent.com/{}/{}/main/SKILL.md",
        parts[0], parts[1]
    ))
}

fn github_registry_base(registry_url: &str) -> Option<String> {
    let trimmed = registry_url.strip_suffix('/').unwrap_or(registry_url);
    let repo = trimmed.strip_prefix("https://github.com/")?;
    let mut parts = repo.split('/');
    let owner = parts.next()?;
    let name = parts.next()?;
    Some(format!("https://raw.githubusercontent.com/{}/{}/main", owner, name))
}

async fn fetch_registry_skills(registry_url: &str) -> Result<Vec<String>, String> {
    let (owner, repo) = github_registry_repo(registry_url)
        .ok_or_else(|| "registry listing currently supports GitHub repositories only".to_string())?;

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

        let destination = install_local_skill(&skills_path, &source).unwrap();

        assert!(destination.join("SKILL.md").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn builds_github_repo_skill_url() {
        let url = skill_source_url("openclaw/weather", None).unwrap();
        assert_eq!(url, "https://raw.githubusercontent.com/openclaw/weather/main/SKILL.md");
    }

    #[test]
    fn builds_registry_backed_skill_url() {
        let url = skill_source_url("openclaw/weather", Some("https://github.com/openclaw/clawhub")).unwrap();
        assert_eq!(url, "https://raw.githubusercontent.com/openclaw/clawhub/main/openclaw/weather/SKILL.md");
    }

    #[test]
    fn extracts_github_registry_repo() {
        let repo = github_registry_repo("https://github.com/openclaw/clawhub").unwrap();
        assert_eq!(repo, ("openclaw".to_string(), "clawhub".to_string()));
    }

    #[test]
    fn derives_skill_id_from_sources() {
        assert_eq!(skill_install_id("openclaw/weather").unwrap(), "weather");
        assert_eq!(skill_install_id("https://example.com/skills/weather/SKILL.md").unwrap(), "weather");
    }

    #[test]
    fn installs_downloaded_skill_manifest() {
        let root = std::env::temp_dir().join(format!("borgclaw_cli_remote_skill_test_{}", uuid::Uuid::new_v4()));
        let skills_path = root.join("installed");
        let destination = install_skill_manifest(
            &skills_path,
            "openclaw/weather",
            "name: Weather\ndescription: Weather skill\n## Instructions\nUse weather APIs.\n",
        )
        .unwrap();

        assert!(destination.join("SKILL.md").exists());
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
            vec!["openclaw/calendar".to_string(), "openclaw/weather".to_string()]
        );
    }
}

fn print_help() {
    println!("Commands:");
    println!("  exit, quit   - Exit the REPL");
    println!("  help         - Show this help message");
    println!("  status       - Show agent status");
    println!("  clear        - Clear screen");
}
