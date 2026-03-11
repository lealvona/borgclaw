//! Plugin SDK - WASM plugin loading and execution

use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Deserialize)]
pub enum WasmPermission {
    FileRead(Vec<PathBuf>),
    FileWrite(PathBuf),
    Network(Vec<String>),
    Memory,
    Shell,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: Option<String>,
    pub permissions: Vec<WasmPermission>,
    pub exports: Vec<String>,
    pub entry_point: String,
}

impl PluginManifest {
    pub fn from_toml(content: &str) -> Result<Self, PluginError> {
        if let Ok(manifest) = toml::from_str::<PluginManifest>(content) {
            return Ok(manifest);
        }

        let documented: DocumentedPluginManifest =
            toml::from_str(content).map_err(|e| PluginError::ParseFailed(e.to_string()))?;

        let entry_point = documented
            .entry_point
            .clone()
            .unwrap_or_else(|| "invoke".to_string());
        let mut exports = documented.exports.unwrap_or_default();
        if exports.is_empty() {
            exports.push(entry_point.clone());
        } else if !exports.iter().any(|export| export == &entry_point) {
            exports.push(entry_point.clone());
        }

        Ok(Self {
            name: documented.name,
            version: documented.version,
            description: documented.description,
            author: documented.author,
            permissions: documented
                .permissions
                .map(DocumentedPermissions::into_permissions)
                .unwrap_or_default(),
            exports,
            entry_point,
        })
    }
}

#[derive(Debug, Clone)]
pub struct WasmPlugin {
    pub manifest: PluginManifest,
    pub bytes: Vec<u8>,
}

impl WasmPlugin {
    pub fn load(wasm_path: &PathBuf) -> Result<Self, PluginError> {
        let bytes = std::fs::read(wasm_path).map_err(|e| PluginError::IoError(e.to_string()))?;

        let manifest_path = wasm_path.with_extension("toml");
        let manifest = if manifest_path.exists() {
            let content = std::fs::read_to_string(&manifest_path)
                .map_err(|e| PluginError::IoError(e.to_string()))?;
            PluginManifest::from_toml(&content)?
        } else {
            let name = wasm_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown");
            PluginManifest {
                name: name.to_string(),
                version: "0.1.0".to_string(),
                description: "WASM plugin".to_string(),
                author: None,
                permissions: vec![],
                exports: vec![],
                entry_point: "invoke".to_string(),
            }
        };

        Ok(Self { manifest, bytes })
    }
}

pub struct PluginRegistry {
    plugins: Arc<RwLock<HashMap<String, WasmPlugin>>>,
    sandbox: crate::security::WasmSandbox,
    workspace_root: PathBuf,
    workspace_policy: crate::config::WorkspacePolicyConfig,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            sandbox: crate::security::WasmSandbox::new(10),
            workspace_root: PathBuf::from("."),
            workspace_policy: crate::config::WorkspacePolicyConfig::default(),
        }
    }

    pub fn with_workspace_policy(
        mut self,
        workspace_root: PathBuf,
        workspace_policy: crate::config::WorkspacePolicyConfig,
    ) -> Self {
        self.workspace_root = workspace_root;
        self.workspace_policy = workspace_policy;
        self
    }

    pub async fn load_from_dir(&self, dir: &PathBuf) -> Result<(), PluginError> {
        if !dir.exists() {
            return Ok(());
        }

        for entry in std::fs::read_dir(dir).map_err(|e| PluginError::IoError(e.to_string()))? {
            let entry = entry.map_err(|e| PluginError::IoError(e.to_string()))?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("wasm") {
                let plugin = WasmPlugin::load(&path)?;

                self.sandbox
                    .register_module(&plugin.manifest.name, plugin.bytes.clone())
                    .await
                    .map_err(|e| PluginError::WasmError(e.to_string()))?;

                self.plugins
                    .write()
                    .await
                    .insert(plugin.manifest.name.clone(), plugin);
            }
        }

        Ok(())
    }

    pub async fn invoke(
        &self,
        plugin_name: &str,
        function: &str,
        input_json: &str,
    ) -> Result<String, PluginError> {
        let plugins = self.plugins.read().await;
        let plugin = plugins
            .get(plugin_name)
            .ok_or(PluginError::NotFound(plugin_name.to_string()))?;

        self.validate_permissions(&plugin.manifest, function, input_json)?;

        self.sandbox
            .execute(plugin_name, function, input_json)
            .await
            .map_err(|e| PluginError::WasmError(e.to_string()))
    }

    fn validate_permissions(
        &self,
        manifest: &PluginManifest,
        function: &str,
        input: &str,
    ) -> Result<(), PluginError> {
        if !manifest.exports.is_empty() && !manifest.exports.iter().any(|export| export == function)
        {
            return Err(PluginError::PermissionDenied(format!(
                "function '{}' is not exported by plugin '{}'",
                function, manifest.name
            )));
        }

        for permission in &manifest.permissions {
            match permission {
                WasmPermission::FileRead(paths) => {
                    tracing::debug!(
                        plugin = %manifest.name,
                        permission = "FileRead",
                        paths = ?paths,
                        "Plugin requesting file read access"
                    );
                    if paths.is_empty() {
                        return Err(PluginError::PermissionDenied(
                            "FileRead permission requires at least one allowed path".to_string(),
                        ));
                    }
                    for path in paths {
                        self.validate_workspace_path(path)?;
                    }
                }
                WasmPermission::FileWrite(path) => {
                    tracing::info!(
                        plugin = %manifest.name,
                        permission = "FileWrite",
                        path = %path.display(),
                        "Plugin requesting file write access"
                    );
                    self.validate_workspace_path(path)?;
                }
                WasmPermission::Network(hosts) => {
                    tracing::info!(
                        plugin = %manifest.name,
                        permission = "Network",
                        hosts = ?hosts,
                        "Plugin requesting network access"
                    );
                    if hosts.is_empty() {
                        return Err(PluginError::PermissionDenied(
                            "Network permission requires at least one allowed host".to_string(),
                        ));
                    }
                    if hosts
                        .iter()
                        .any(|host| host.trim().is_empty() || host.contains("://"))
                    {
                        return Err(PluginError::PermissionDenied(
                            "Network permission entries must be bare host[:port] values"
                                .to_string(),
                        ));
                    }
                }
                WasmPermission::Memory => {
                    tracing::debug!(
                        plugin = %manifest.name,
                        permission = "Memory",
                        "Plugin requesting extended memory access"
                    );
                }
                WasmPermission::Shell => {
                    tracing::warn!(
                        plugin = %manifest.name,
                        permission = "Shell",
                        function = %function,
                        "Plugin requesting shell access - elevated risk"
                    );
                    if input.contains("rm ") || input.contains("del ") || input.contains("format ")
                    {
                        return Err(PluginError::PermissionDenied(
                            "Shell permission does not allow destructive commands".to_string(),
                        ));
                    }
                }
            }
        }

        Ok(())
    }

    fn validate_workspace_path(&self, path: &Path) -> Result<(), PluginError> {
        let resolved = resolve_policy_path(&self.workspace_root, path).map_err(|err| {
            PluginError::PermissionDenied(format!("invalid plugin path permission: {}", err))
        })?;

        let mut allowed_roots = vec![self.workspace_root.clone(), std::env::temp_dir()];
        if !self.workspace_policy.workspace_only {
            for root in &self.workspace_policy.allowed_roots {
                if let Ok(resolved_root) = resolve_policy_path(&self.workspace_root, root) {
                    allowed_roots.push(resolved_root);
                }
            }
        }

        if !allowed_roots.iter().any(|root| resolved.starts_with(root)) {
            return Err(PluginError::PermissionDenied(format!(
                "plugin path permission escapes allowed roots: {}",
                path.display()
            )));
        }

        for forbidden in &self.workspace_policy.forbidden_paths {
            let resolved_forbidden = resolve_policy_path(&self.workspace_root, forbidden)
                .map_err(|err| PluginError::PermissionDenied(err.to_string()))?;
            if resolved.starts_with(&resolved_forbidden) {
                return Err(PluginError::PermissionDenied(format!(
                    "plugin path permission blocked by workspace policy: {}",
                    forbidden.display()
                )));
            }
        }

        Ok(())
    }

    pub async fn list(&self) -> Vec<PluginManifest> {
        self.plugins
            .read()
            .await
            .values()
            .map(|p| p.manifest.clone())
            .collect()
    }

    pub async fn get(&self, name: &str) -> Option<PluginManifest> {
        self.plugins
            .read()
            .await
            .get(name)
            .map(|p| p.manifest.clone())
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("IO error: {0}")]
    IoError(String),

    #[error("Parse failed: {0}")]
    ParseFailed(String),

    #[error("Plugin not found: {0}")]
    NotFound(String),

    #[error("WASM error: {0}")]
    WasmError(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),
}

#[derive(Debug, Clone, Deserialize)]
struct DocumentedPluginManifest {
    name: String,
    version: String,
    description: String,
    author: Option<String>,
    permissions: Option<DocumentedPermissions>,
    exports: Option<Vec<String>>,
    entry_point: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct DocumentedPermissions {
    #[serde(default)]
    file_read: Vec<PathBuf>,
    #[serde(default)]
    file_write: Vec<PathBuf>,
    #[serde(default)]
    network: Vec<String>,
    #[serde(default)]
    memory: bool,
    #[serde(default)]
    shell: bool,
}

impl DocumentedPermissions {
    fn into_permissions(self) -> Vec<WasmPermission> {
        let mut permissions = Vec::new();
        if !self.file_read.is_empty() {
            permissions.push(WasmPermission::FileRead(self.file_read));
        }
        permissions.extend(self.file_write.into_iter().map(WasmPermission::FileWrite));
        if !self.network.is_empty() {
            permissions.push(WasmPermission::Network(self.network));
        }
        if self.memory {
            permissions.push(WasmPermission::Memory);
        }
        if self.shell {
            permissions.push(WasmPermission::Shell);
        }
        permissions
    }
}

fn resolve_policy_path(workspace_root: &Path, path: &Path) -> Result<PathBuf, String> {
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        workspace_root.join(path)
    };

    std::fs::canonicalize(&candidate)
        .or_else(|_| {
            candidate
                .parent()
                .ok_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::NotFound, "path has no parent")
                })
                .and_then(|parent| {
                    std::fs::canonicalize(parent)
                        .map(|canon| canon.join(candidate.file_name().unwrap_or_default()))
                })
        })
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_manifest_parses_documented_permissions_table() {
        let manifest = PluginManifest::from_toml(
            r#"
name = "my-plugin"
version = "1.0.0"
description = "My custom plugin"
author = "Developer"
entry_point = "main"

[permissions]
file_read = ["/workspace"]
file_write = ["/tmp"]
network = ["api.example.com"]
memory = true
shell = false
"#,
        )
        .unwrap();

        assert_eq!(manifest.name, "my-plugin");
        assert_eq!(manifest.entry_point, "main");
        assert!(manifest.exports.iter().any(|export| export == "main"));
        assert!(manifest
            .permissions
            .iter()
            .any(|permission| matches!(permission, WasmPermission::FileRead(paths) if paths == &vec![PathBuf::from("/workspace")])));
        assert!(manifest.permissions.iter().any(
            |permission| matches!(permission, WasmPermission::FileWrite(path) if path == &PathBuf::from("/tmp"))
        ));
        assert!(manifest.permissions.iter().any(
            |permission| matches!(permission, WasmPermission::Network(hosts) if hosts == &vec!["api.example.com".to_string()])
        ));
        assert!(manifest
            .permissions
            .iter()
            .any(|permission| matches!(permission, WasmPermission::Memory)));
        assert!(!manifest
            .permissions
            .iter()
            .any(|permission| matches!(permission, WasmPermission::Shell)));
    }

    #[test]
    fn plugin_registry_rejects_unexported_function() {
        let registry = PluginRegistry::new();
        let manifest = PluginManifest {
            name: "example".to_string(),
            version: "1.0.0".to_string(),
            description: "Example".to_string(),
            author: None,
            permissions: vec![],
            exports: vec!["main".to_string()],
            entry_point: "main".to_string(),
        };

        let err = registry
            .validate_permissions(&manifest, "other", "{}")
            .unwrap_err();

        assert!(matches!(err, PluginError::PermissionDenied(_)));
        assert!(err.to_string().contains("not exported"));
    }

    #[test]
    fn plugin_registry_rejects_file_write_outside_workspace_policy() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_plugin_policy_test_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();

        let registry = PluginRegistry::new().with_workspace_policy(
            root.clone(),
            crate::config::WorkspacePolicyConfig::default(),
        );
        let manifest = PluginManifest {
            name: "example".to_string(),
            version: "1.0.0".to_string(),
            description: "Example".to_string(),
            author: None,
            permissions: vec![WasmPermission::FileWrite(PathBuf::from("/etc"))],
            exports: vec!["main".to_string()],
            entry_point: "main".to_string(),
        };

        let err = registry
            .validate_permissions(&manifest, "main", "{}")
            .unwrap_err();

        std::fs::remove_dir_all(&root).unwrap();
        assert!(matches!(err, PluginError::PermissionDenied(_)));
        assert!(err.to_string().contains("escapes allowed roots"));
    }

    #[test]
    fn plugin_registry_rejects_network_entries_with_scheme() {
        let registry = PluginRegistry::new();
        let manifest = PluginManifest {
            name: "example".to_string(),
            version: "1.0.0".to_string(),
            description: "Example".to_string(),
            author: None,
            permissions: vec![WasmPermission::Network(vec![
                "https://api.example.com".to_string()
            ])],
            exports: vec!["main".to_string()],
            entry_point: "main".to_string(),
        };

        let err = registry
            .validate_permissions(&manifest, "main", "{}")
            .unwrap_err();

        assert!(matches!(err, PluginError::PermissionDenied(_)));
        assert!(err.to_string().contains("bare host[:port]"));
    }
}
