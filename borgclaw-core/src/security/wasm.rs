//! WASM Sandbox module - isolated tool execution using wasmtime

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct WasmSandbox {
    modules: Arc<RwLock<HashMap<String, Vec<u8>>>>,
    max_instances: usize,
}

impl WasmSandbox {
    pub fn new(max_instances: usize) -> Self {
        Self {
            modules: Arc::new(RwLock::new(HashMap::new())),
            max_instances,
        }
    }
    
    pub async fn register_module(&self, name: &str, wasm_bytes: Vec<u8>) -> Result<(), super::SecurityError> {
        let mut modules = self.modules.write().await;
        modules.insert(name.to_string(), wasm_bytes);
        Ok(())
    }
    
    pub async fn execute(
        &self,
        module_name: &str,
        function: &str,
        input: &str,
    ) -> Result<String, super::SecurityError> {
        let modules = self.modules.read().await;
        
        let wasm_bytes = modules
            .get(module_name)
            .ok_or_else(|| super::SecurityError::WasmError("Module not found".to_string()))?;
        
        let wasm_bytes = wasm_bytes.clone();
        drop(modules);
        
        let module_name = module_name.to_string();
        let function = function.to_string();
        let input = input.to_string();
        
        tokio::task::spawn_blocking(move || {
            Self::execute_wasm_sync(&wasm_bytes, &function, &input)
        })
        .await
        .map_err(|e| super::SecurityError::WasmError(format!("Task join error: {}", e)))?
    }
    
    fn execute_wasm_sync(wasm_bytes: &[u8], function: &str, input: &str) -> Result<String, super::SecurityError> {
        use wasmtime::*;
        
        let mut config = Config::new();
        config.wasm_backtrace_details(WasmBacktraceDetails::Enable);
        config.cranelift_opt_level(OptLevel::Speed);
        
        let engine = Engine::new(&config)
            .map_err(|e| super::SecurityError::WasmError(format!("Engine creation failed: {}", e)))?;
        
        let module = Module::new(&engine, wasm_bytes)
            .map_err(|e| super::SecurityError::WasmError(format!("Module compilation failed: {}", e)))?;
        
        let mut linker = Linker::new(&engine);
        
        let mut store = Store::new(&engine, ());
        
        let instance = linker.instantiate(&mut store, &module)
            .map_err(|e| super::SecurityError::WasmError(format!("Instantiation failed: {}", e)))?;
        
        let func = instance
            .get_export(&mut store, function)
            .and_then(|e| e.into_func())
            .ok_or_else(|| super::SecurityError::WasmError(format!("Function '{}' not found", function)))?;
        
        let memory = instance
            .get_export(&mut store, "memory")
            .and_then(|e| e.into_memory())
            .ok_or_else(|| super::SecurityError::WasmError("Memory export not found".to_string()))?;
        
        let input_bytes = input.as_bytes();
        let input_len = input_bytes.len() as i32;
        
        let input_ptr = if let Some(alloc_export) = instance.get_export(&mut store, "alloc") {
            if let Some(alloc_func) = alloc_export.into_func() {
                if let Ok(typed_alloc) = alloc_func.typed::<i32, i32>(&store) {
                    typed_alloc
                        .call(&mut store, input_len)
                        .map_err(|e| super::SecurityError::WasmError(format!("Allocation failed: {}", e)))?
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
            .map_err(|e| super::SecurityError::WasmError(format!("Memory write failed: {}", e)))?;
        
        let typed_func = func
            .typed::<(i32, i32), i32>(&store)
            .map_err(|e| super::SecurityError::WasmError(format!("Function signature mismatch: {}", e)))?;
        
        let result_ptr = typed_func
            .call(&mut store, (input_ptr, input_len))
            .map_err(|e| super::SecurityError::WasmError(format!("Execution failed: {}", e)))?;
        
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
                return Err(super::SecurityError::WasmError("Output exceeds 1MB limit".to_string()));
            }
        }
        
        String::from_utf8(output)
            .map_err(|e| super::SecurityError::WasmError(format!("Invalid UTF-8 output: {}", e)))
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

pub struct WasmTool {
    pub module_name: String,
    pub function_name: String,
    pub description: String,
    pub input_schema: super::super::agent::ToolSchema,
}

impl WasmTool {
    pub fn new(
        module_name: impl Into<String>,
        function_name: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            module_name: module_name.into(),
            function_name: function_name.into(),
            description: description.into(),
            input_schema: super::super::agent::ToolSchema::default(),
        }
    }
}
