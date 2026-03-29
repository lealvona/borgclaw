//! WASM Sandbox module - isolated tool execution using wasmtime

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, Semaphore};

pub struct WasmSandbox {
    modules: Arc<RwLock<HashMap<String, Vec<u8>>>>,
    instance_semaphore: Arc<Semaphore>,
}

impl WasmSandbox {
    pub fn new(max_instances: usize) -> Self {
        Self {
            modules: Arc::new(RwLock::new(HashMap::new())),
            instance_semaphore: Arc::new(Semaphore::new(max_instances)),
        }
    }

    pub async fn register_module(
        &self,
        name: &str,
        wasm_bytes: Vec<u8>,
    ) -> Result<(), crate::security::SecurityError> {
        let mut modules = self.modules.write().await;
        modules.insert(name.to_string(), wasm_bytes);
        Ok(())
    }

    pub async fn execute(
        &self,
        module_name: &str,
        function: &str,
        input: &str,
    ) -> Result<String, crate::security::SecurityError> {
        // Acquire permit to limit concurrent executions
        let _permit = self.instance_semaphore.acquire().await.map_err(|e| {
            crate::security::SecurityError::WasmError(format!("Semaphore error: {}", e))
        })?;

        let modules = self.modules.read().await;

        let wasm_bytes = modules.get(module_name).ok_or_else(|| {
            crate::security::SecurityError::WasmError("Module not found".to_string())
        })?;

        let wasm_bytes = wasm_bytes.clone();
        drop(modules);

        let function = function.to_string();
        let input = input.to_string();

        tokio::task::spawn_blocking(move || Self::execute_wasm_sync(&wasm_bytes, &function, &input))
            .await
            .map_err(|e| {
                crate::security::SecurityError::WasmError(format!("Task join error: {}", e))
            })?
    }

    fn execute_wasm_sync(
        wasm_bytes: &[u8],
        function: &str,
        input: &str,
    ) -> Result<String, crate::security::SecurityError> {
        use wasmtime::*;

        let mut config = Config::new();
        config.wasm_backtrace_details(WasmBacktraceDetails::Enable);
        config.cranelift_opt_level(OptLevel::Speed);

        let engine = Engine::new(&config).map_err(|e| {
            crate::security::SecurityError::WasmError(format!("Engine creation failed: {}", e))
        })?;

        let module = Module::new(&engine, wasm_bytes).map_err(|e| {
            crate::security::SecurityError::WasmError(format!("Module compilation failed: {}", e))
        })?;

        let linker = Linker::new(&engine);

        let mut store = Store::new(&engine, ());

        let instance = linker.instantiate(&mut store, &module).map_err(|e| {
            crate::security::SecurityError::WasmError(format!("Instantiation failed: {}", e))
        })?;

        let func = instance
            .get_export(&mut store, function)
            .and_then(|e| e.into_func())
            .ok_or_else(|| {
                crate::security::SecurityError::WasmError(format!(
                    "Function '{}' not found",
                    function
                ))
            })?;

        let memory = instance
            .get_export(&mut store, "memory")
            .and_then(|e| e.into_memory())
            .ok_or_else(|| {
                crate::security::SecurityError::WasmError("Memory export not found".to_string())
            })?;

        let input_bytes = input.as_bytes();
        let input_len = input_bytes.len() as i32;

        let input_ptr = if let Some(alloc_export) = instance.get_export(&mut store, "alloc") {
            if let Some(alloc_func) = alloc_export.into_func() {
                if let Ok(typed_alloc) = alloc_func.typed::<i32, i32>(&store) {
                    typed_alloc.call(&mut store, input_len).map_err(|e| {
                        crate::security::SecurityError::WasmError(format!(
                            "Allocation failed: {}",
                            e
                        ))
                    })?
                } else {
                    0
                }
            } else {
                0
            }
        } else {
            0
        };

        memory
            .write(&mut store, input_ptr as usize, input_bytes)
            .map_err(|e| {
                crate::security::SecurityError::WasmError(format!("Memory write failed: {}", e))
            })?;

        let typed_func = func.typed::<(i32, i32), i32>(&store).map_err(|e| {
            crate::security::SecurityError::WasmError(format!("Function signature mismatch: {}", e))
        })?;

        let result_ptr = typed_func
            .call(&mut store, (input_ptr, input_len))
            .map_err(|e| {
                crate::security::SecurityError::WasmError(format!("Execution failed: {}", e))
            })?;

        let mut output = Vec::new();
        let data = memory.data(&store);
        let mut offset = result_ptr as usize;

        while offset < data.len() {
            let byte = data[offset];
            if byte == 0 {
                break;
            }
            output.push(byte);
            offset += 1;

            if output.len() > 1024 * 1024 {
                return Err(crate::security::SecurityError::WasmError(
                    "Output exceeds 1MB limit".to_string(),
                ));
            }
        }

        String::from_utf8(output).map_err(|e| {
            crate::security::SecurityError::WasmError(format!("Invalid UTF-8 output: {}", e))
        })
    }

    pub async fn list_modules(&self) -> Vec<String> {
        let modules = self.modules.read().await;
        modules.keys().cloned().collect()
    }

    pub async fn has_module(&self, name: &str) -> bool {
        let modules = self.modules.read().await;
        modules.contains_key(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wasm_sandbox_new() {
        let sandbox = WasmSandbox::new(10);
        assert_eq!(sandbox.instance_semaphore.available_permits(), 10);
    }

    #[tokio::test]
    async fn wasm_sandbox_register_module() {
        let sandbox = WasmSandbox::new(5);
        let wasm_bytes = vec![0x00, 0x61, 0x73, 0x6d]; // Invalid WASM magic

        let result = sandbox.register_module("test_module", wasm_bytes).await;
        assert!(result.is_ok());

        // Module should be registered
        assert!(sandbox.has_module("test_module").await);
    }

    #[tokio::test]
    async fn wasm_sandbox_has_module_returns_false_for_missing() {
        let sandbox = WasmSandbox::new(5);

        assert!(!sandbox.has_module("nonexistent").await);
    }

    #[tokio::test]
    async fn wasm_sandbox_list_modules_empty() {
        let sandbox = WasmSandbox::new(5);

        let modules = sandbox.list_modules().await;
        assert!(modules.is_empty());
    }

    #[tokio::test]
    async fn wasm_sandbox_list_modules_with_modules() {
        let sandbox = WasmSandbox::new(5);

        sandbox
            .register_module("module1", vec![0x00])
            .await
            .unwrap();
        sandbox
            .register_module("module2", vec![0x01])
            .await
            .unwrap();
        sandbox
            .register_module("module3", vec![0x02])
            .await
            .unwrap();

        let mut modules = sandbox.list_modules().await;
        modules.sort(); // Order may vary due to HashMap

        assert_eq!(modules.len(), 3);
        assert_eq!(modules, vec!["module1", "module2", "module3"]);
    }

    #[tokio::test]
    async fn wasm_sandbox_register_module_overwrites() {
        let sandbox = WasmSandbox::new(5);

        sandbox.register_module("test", vec![0x00]).await.unwrap();
        sandbox
            .register_module("test", vec![0x01, 0x02])
            .await
            .unwrap();

        assert!(sandbox.has_module("test").await);
        let modules = sandbox.list_modules().await;
        assert_eq!(modules.len(), 1);
    }

    #[tokio::test]
    async fn wasm_sandbox_execute_missing_module_fails() {
        let sandbox = WasmSandbox::new(5);

        let result = sandbox.execute("missing", "func", "input").await;
        assert!(result.is_err());

        match result {
            Err(crate::security::SecurityError::WasmError(msg)) => {
                assert!(msg.contains("Module not found"));
            }
            _ => panic!("Expected WasmError for missing module"),
        }
    }

    #[tokio::test]
    async fn wasm_sandbox_execute_invalid_wasm_fails() {
        let sandbox = WasmSandbox::new(5);
        let invalid_wasm = vec![0x00, 0x01, 0x02, 0x03]; // Not valid WASM

        sandbox
            .register_module("invalid", invalid_wasm)
            .await
            .unwrap();

        let result = sandbox.execute("invalid", "func", "input").await;
        assert!(result.is_err());
    }

    #[test]
    fn wasm_sandbox_with_zero_instances() {
        // Edge case: semaphore with 0 permits
        let sandbox = WasmSandbox::new(0);
        assert_eq!(sandbox.instance_semaphore.available_permits(), 0);
    }

    #[test]
    fn wasm_sandbox_with_large_instance_count() {
        let sandbox = WasmSandbox::new(10000);
        assert_eq!(sandbox.instance_semaphore.available_permits(), 10000);
    }

    #[tokio::test]
    async fn wasm_sandbox_multiple_modules_independent() {
        let sandbox = WasmSandbox::new(10);

        sandbox.register_module("mod1", vec![0x00]).await.unwrap();
        sandbox.register_module("mod2", vec![0x01]).await.unwrap();

        assert!(sandbox.has_module("mod1").await);
        assert!(sandbox.has_module("mod2").await);

        // Removing one shouldn't affect the other
        // (Note: we don't have remove, but we can verify independence)
        let modules = sandbox.list_modules().await;
        assert_eq!(modules.len(), 2);
    }
}
