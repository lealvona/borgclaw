//! Plugin SDK - WASM plugin loading and execution

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::Deserialize;

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
        toml::from_str(content).map_err(|e| PluginError::ParseFailed(e.to_string()))
    }
}

#[derive(Debug, Clone)]
pub struct WasmPlugin {
    pub manifest: PluginManifest,
    pub bytes: Vec<u8>,
}

impl WasmPlugin {
    pub fn load(wasm_path: &PathBuf) -> Result<Self, PluginError> {
        let bytes = std::fs::read(wasm_path)
            .map_err(|e| PluginError::IoError(e.to_string()))?;

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

        if !plugin.manifest.permissions.is_empty() {
            return Err(PluginError::PermissionDenied(
                "Plugin has permissions but enforcement not implemented".to_string(),
            ));
        }

        self.sandbox
            .execute(plugin_name, function, input_json)
            .await
            .map_err(|e| PluginError::WasmError(e.to_string()))
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
        self.plugins.read().await.get(name).map(|p| p.manifest.clone())
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
