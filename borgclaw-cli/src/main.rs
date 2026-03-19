//! BorgClaw CLI - Command-line interface with REPL

mod onboarding;

use crate::onboarding::{run_init, InitArgs, StartTarget};
use borgclaw_core::{
    channel::{ChannelType, InboundMessage, MessagePayload, MessageRouter, Sender},
    config::{load_config, save_config, AppConfig},
    mcp::{
        client::{McpClient, McpClientConfig},
        transport::{
            McpTransportConfig, SseTransportConfig, StdioTransportConfig, WebSocketTransportConfig,
        },
    },
    scheduler::{Job, JobTrigger},
    security::SecurityLayer,
    skills::SkillsRegistry,
};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
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
    List {
        /// Optional substring filter for installed and registry skills
        filter: Option<String>,
    },
    /// Install a skill
    Install { name: String },
}

#[derive(Subcommand)]
enum ScheduleAction {
    /// List persisted scheduled tasks
    List,
    /// Show persisted details for one scheduled task
    Show { id: String },
}

#[derive(Subcommand)]
enum BackupAction {
    /// Export persisted local runtime state to a JSON snapshot
    Export { output: PathBuf },
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
        Commands::Init(args) => match run_init(&config_path, config, &args).await {
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

        match parse_repl_command(input) {
            ReplCommand::Exit => {
                println!("Goodbye!");
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
            ReplCommand::Message => {}
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
        "Security: wasm={}, docker={}, approval={:?}",
        config.security.wasm_sandbox, config.security.docker_sandbox, config.security.approval_mode
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
    println!("Memory database: {:?}", config.memory.database_path);
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
    if let Some(parent) = config.memory.database_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if config.memory.database_path.parent().is_some() || config.memory.database_path.is_absolute() {
        println!("✓ Memory database path configured");
    } else {
        println!(
            "✗ Memory database path unavailable: {:?}",
            config.memory.database_path
        );
    }
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
    }
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

    if !(config.memory.database_path.parent().is_some()
        || config.memory.database_path.is_absolute())
    {
        failures.push(format!(
            "memory database path unavailable: {}",
            config.memory.database_path.display()
        ));
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
        "pairing={}({} digits/{}s), prompt_injection={}({:?}), leak_detection={}({:?}), blocklist={}+{}, allowlist={}, wasm_instances={}",
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
        config.security.wasm_max_instances
    )
}

fn security_doctor_lines(config: &AppConfig) -> Vec<String> {
    vec![
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
    ]
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
    format!(
        "scheduler={}, heartbeat={}, subagents={}",
        scheduler, heartbeat, subagents
    )
}

fn background_state_doctor_lines(workspace: &std::path::Path) -> Vec<String> {
    vec![
        background_state_doctor_line("Scheduler", &workspace.join("scheduler.json")),
        background_state_doctor_line("Heartbeat", &workspace.join("heartbeat.json")),
        background_state_doctor_line("Sub-agent", &workspace.join("subagents.json")),
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

fn pluralize(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("1 {}", noun)
    } else {
        format!("{} {}s", count, noun)
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
        return Err(format!(
            "failed to download skill manifest: http {}",
            response.status()
        ));
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
    Some(format!(
        "https://raw.githubusercontent.com/{}/{}/main",
        owner, name
    ))
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
    fn builds_github_repo_skill_url() {
        let url = skill_source_url("openclaw/weather", None).unwrap();
        assert_eq!(
            url,
            "https://raw.githubusercontent.com/openclaw/weather/main/SKILL.md"
        );
    }

    #[test]
    fn builds_registry_backed_skill_url() {
        let url = skill_source_url(
            "openclaw/weather",
            Some("https://github.com/openclaw/clawhub"),
        )
        .unwrap();
        assert_eq!(
            url,
            "https://raw.githubusercontent.com/openclaw/clawhub/main/openclaw/weather/SKILL.md"
        );
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

    #[test]
    fn installs_downloaded_skill_manifest() {
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
        assert_eq!(parse_repl_command("status"), ReplCommand::Status);
        assert_eq!(parse_repl_command("clear"), ReplCommand::Clear);
        assert_eq!(parse_repl_command("hello"), ReplCommand::Message);
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
            "scheduler=3 tasks (dead-lettered=1), heartbeat=2 tasks (dead-lettered=1), subagents=1 task (dead-lettered=0)"
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
}

fn print_help() {
    println!("Commands:");
    println!("  exit, quit   - Exit the REPL");
    println!("  help         - Show this help message");
    println!("  status       - Show agent status");
    println!("  clear        - Clear screen");
}

const CLEAR_SCREEN_SEQUENCE: &str = "\x1B[2J\x1B[H";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReplCommand {
    Exit,
    Help,
    Status,
    Clear,
    Message,
}

fn parse_repl_command(input: &str) -> ReplCommand {
    match input {
        "exit" | "quit" => ReplCommand::Exit,
        "help" => ReplCommand::Help,
        "status" => ReplCommand::Status,
        "clear" => ReplCommand::Clear,
        _ => ReplCommand::Message,
    }
}

fn clear_screen() {
    print!("{}", CLEAR_SCREEN_SEQUENCE);
    let _ = std::io::Write::flush(&mut std::io::stdout());
}

#[cfg(test)]
mod cli_path_tests {
    use super::cli_path_available;
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
}
