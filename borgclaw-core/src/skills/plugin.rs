//! Plugin SDK - WASM plugin loading and execution

use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Deserialize)]
pub enum WasmPermission {
    FileRead,
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
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            sandbox: crate::security::WasmSandbox::new(10),
        }
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
                WasmPermission::FileRead => {
                    tracing::debug!(
                        plugin = %manifest.name,
                        permission = "FileRead",
                        "Plugin requesting file read access"
                    );
                }
                WasmPermission::FileWrite(path) => {
                    tracing::info!(
                        plugin = %manifest.name,
                        permission = "FileWrite",
                        path = %path.display(),
                        "Plugin requesting file write access"
                    );
                    if path.exists() && !path.starts_with(std::env::temp_dir()) {
                        tracing::warn!(
                            plugin = %manifest.name,
                            path = %path.display(),
                            "Plugin attempting to write outside temp directory"
                        );
                    }
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
            permissions.push(WasmPermission::FileRead);
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
            .any(|permission| matches!(permission, WasmPermission::FileRead)));
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
}
