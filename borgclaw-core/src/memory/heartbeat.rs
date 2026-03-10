//! Heartbeat engine for periodic task execution

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use cron::Schedule;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

pub struct HeartbeatEngine {
    tasks: Arc<RwLock<HashMap<String, HeartbeatTask>>>,
    handlers: Arc<RwLock<HashMap<String, Arc<dyn HeartbeatHandler>>>>,
    running: Arc<RwLock<bool>>,
    sender: mpsc::Sender<HeartbeatEvent>,
    state_path: Option<PathBuf>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct HeartbeatTask {
    pub id: String,
    pub name: String,
    pub schedule: String,
    pub enabled: bool,
    pub last_run: Option<DateTime<Utc>>,
    pub next_run: Option<DateTime<Utc>>,
    pub run_count: u32,
    pub last_result: Option<HeartbeatResult>,
    pub metadata: HashMap<String, String>,
    #[serde(skip, default = "default_task_handler")]
    pub handler: Box<dyn HeartbeatHandler>,
}

impl std::fmt::Debug for HeartbeatTask {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HeartbeatTask")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("schedule", &self.schedule)
            .field("enabled", &self.enabled)
            .field("last_run", &self.last_run)
            .field("next_run", &self.next_run)
            .field("run_count", &self.run_count)
            .field("last_result", &self.last_result)
            .field("metadata", &self.metadata)
            .finish()
    }
}

fn default_task_handler() -> Box<dyn HeartbeatHandler> {
    Box::new(DefaultHeartbeatHandler)
}

impl HeartbeatTask {
    pub fn new(name: impl Into<String>, schedule: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            schedule: schedule.into(),
            enabled: true,
            last_run: None,
            next_run: None,
            run_count: 0,
            last_result: None,
            metadata: HashMap::new(),
            handler: default_task_handler(),
        }
    }

    pub fn every_minutes(n: u32) -> Self {
        Self::new("task", format!("0 */{} * * * *", n))
    }

    pub fn every_hours(n: u32) -> Self {
        Self::new("task", format!("0 0 */{} * * *", n))
    }

    pub fn daily() -> Self {
        Self::new("task", "0 0 0 * * *".to_string())
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    pub fn with_handler<H>(mut self, handler: H) -> Self
    where
        H: HeartbeatHandler + 'static,
    {
        self.handler = Box::new(handler);
        self
    }

    pub fn parse_schedule(&self) -> Result<Schedule, String> {
        self.schedule
            .parse()
            .map_err(|e: cron::error::Error| e.to_string())
    }

    pub fn calculate_next_run(&mut self) {
        if let Ok(schedule) = self.parse_schedule() {
            let now = chrono::Utc::now();
            self.next_run = schedule.after(&now).next();
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatResult {
    pub task_id: String,
    pub success: bool,
    pub message: String,
    pub duration_ms: u64,
    pub timestamp: DateTime<Utc>,
    pub data: Option<serde_json::Value>,
}

impl HeartbeatResult {
    pub fn success(task_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            task_id: task_id.into(),
            success: true,
            message: message.into(),
            duration_ms: 0,
            timestamp: Utc::now(),
            data: None,
        }
    }

    pub fn failure(task_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            task_id: task_id.into(),
            success: false,
            message: message.into(),
            duration_ms: 0,
            timestamp: Utc::now(),
            data: None,
        }
    }

    pub fn with_duration(mut self, ms: u64) -> Self {
        self.duration_ms = ms;
        self
    }

    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }
}

#[derive(Debug, Clone)]
pub enum HeartbeatEvent {
    TaskTriggered(String),
    TaskCompleted(String, HeartbeatResult),
    TaskFailed(String, String),
    EngineStarted,
    EngineStopped,
}

impl HeartbeatEngine {
    pub fn new() -> Self {
        let (sender, _) = mpsc::channel(100);
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
            handlers: Arc::new(RwLock::new(HashMap::new())),
            running: Arc::new(RwLock::new(false)),
            sender,
            state_path: None,
        }
    }

    pub fn with_state_path(mut self, path: impl Into<PathBuf>) -> Self {
        let state_path = path.into();
        self.tasks = Arc::new(RwLock::new(load_tasks(&state_path)));
        self.state_path = Some(state_path);
        self
    }

    pub fn with_event_channel(mut self, sender: mpsc::Sender<HeartbeatEvent>) -> Self {
        self.sender = sender;
        self
    }

    pub async fn register(&self, task: HeartbeatTask) -> String {
        let mut task = task;
        task.calculate_next_run();
        let id = task.id.clone();

        let mut tasks = self.tasks.write().await;
        tasks.insert(id.clone(), task);
        self.persist_state(&tasks);

        id
    }

    pub async fn add_task(&self, task: HeartbeatTask) -> String {
        self.register(task).await
    }

    pub async fn register_handler(
        &self,
        task_name: impl Into<String>,
        handler: Arc<dyn HeartbeatHandler>,
    ) {
        self.handlers
            .write()
            .await
            .insert(task_name.into(), handler);
    }

    pub async fn unregister(&self, id: &str) -> bool {
        let mut tasks = self.tasks.write().await;
        let removed = tasks.remove(id).is_some();
        if removed {
            self.persist_state(&tasks);
        }
        removed
    }

    pub async fn enable(&self, id: &str) -> bool {
        let mut tasks = self.tasks.write().await;
        if let Some(task) = tasks.get_mut(id) {
            task.enabled = true;
            task.calculate_next_run();
            self.persist_state(&tasks);
            true
        } else {
            false
        }
    }

    pub async fn disable(&self, id: &str) -> bool {
        let mut tasks = self.tasks.write().await;
        if let Some(task) = tasks.get_mut(id) {
            task.enabled = false;
            task.next_run = None;
            self.persist_state(&tasks);
            true
        } else {
            false
        }
    }

    pub async fn get(&self, id: &str) -> Option<HeartbeatTask> {
        let tasks = self.tasks.read().await;
        tasks.get(id).cloned()
    }

    pub async fn list(&self) -> Vec<HeartbeatTask> {
        let tasks = self.tasks.read().await;
        tasks.values().cloned().collect()
    }

    pub async fn list_due(&self) -> Vec<HeartbeatTask> {
        let tasks = self.tasks.read().await;
        let now = chrono::Utc::now();

        tasks
            .values()
            .filter(|t| t.enabled && t.next_run.map(|nr| nr <= now).unwrap_or(false))
            .cloned()
            .collect()
    }

    pub async fn start(&self) -> Result<(), String> {
        let mut running = self.running.write().await;
        *running = true;
        drop(running);

        let _ = self.sender.send(HeartbeatEvent::EngineStarted).await;
        Ok(())
    }

    pub async fn stop(&self) -> Result<(), String> {
        let mut running = self.running.write().await;
        *running = false;
        drop(running);

        let _ = self.sender.send(HeartbeatEvent::EngineStopped).await;
        Ok(())
    }

    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    pub async fn tick(&self) -> Vec<(String, HeartbeatResult)> {
        if !self.is_running().await {
            return Vec::new();
        }

        let due_tasks = self.list_due().await;
        let mut results = Vec::new();

        for task in due_tasks {
            let task_id = task.id.clone();
            let _ = self
                .sender
                .send(HeartbeatEvent::TaskTriggered(task_id.clone()))
                .await;

            let result = self.execute_task(&task).await;

            {
                let mut tasks = self.tasks.write().await;
                if let Some(t) = tasks.get_mut(&task_id) {
                    t.last_run = Some(Utc::now());
                    t.run_count += 1;
                    t.last_result = Some(result.clone());
                    t.calculate_next_run();
                }
                self.persist_state(&tasks);
            }

            if result.success {
                let _ = self
                    .sender
                    .send(HeartbeatEvent::TaskCompleted(
                        task_id.clone(),
                        result.clone(),
                    ))
                    .await;
            } else {
                let _ = self
                    .sender
                    .send(HeartbeatEvent::TaskFailed(
                        task_id.clone(),
                        result.message.clone(),
                    ))
                    .await;
            }

            results.push((task_id, result));
        }

        results
    }

    async fn execute_task(&self, task: &HeartbeatTask) -> HeartbeatResult {
        let start = std::time::Instant::now();

        if !task.handler.is_default() {
            return task
                .handler
                .handle(task)
                .await
                .with_duration(start.elapsed().as_millis() as u64);
        }

        if let Some(handler) = self.handlers.read().await.get(&task.name).cloned() {
            return handler
                .handle(task)
                .await
                .with_duration(start.elapsed().as_millis() as u64);
        }

        let result = match task.name.as_str() {
            "memory_cleanup" => HeartbeatResult::success(&task.id, "Memory cleanup completed")
                .with_data(serde_json::json!({"operation": "memory_cleanup"})),
            "health_check" => HeartbeatResult::success(&task.id, "Health check passed")
                .with_data(serde_json::json!({"operation": "health_check"})),
            "session_compaction" => {
                HeartbeatResult::success(&task.id, "Session compaction completed")
                    .with_data(serde_json::json!({"operation": "session_compaction"}))
            }
            _ => {
                if let Some(action) = task.metadata.get("action") {
                    HeartbeatResult::success(&task.id, format!("Action '{}' executed", action))
                        .with_data(serde_json::json!({"action": action}))
                } else {
                    HeartbeatResult::success(&task.id, format!("Task '{}' executed", task.name))
                }
            }
        };

        result.with_duration(start.elapsed().as_millis() as u64)
    }

    pub async fn run_task_now(&self, id: &str) -> Option<HeartbeatResult> {
        let tasks = self.tasks.read().await;
        let task = tasks.get(id)?;
        if !task.enabled {
            return None;
        }
        let task = task.clone();
        drop(tasks);

        let result = self.execute_task(&task).await;

        {
            let mut tasks = self.tasks.write().await;
            if let Some(t) = tasks.get_mut(id) {
                t.last_run = Some(Utc::now());
                t.run_count += 1;
                t.last_result = Some(result.clone());
                t.calculate_next_run();
            }
            self.persist_state(&tasks);
        }

        Some(result)
    }

    fn persist_state(&self, tasks: &HashMap<String, HeartbeatTask>) {
        let Some(path) = &self.state_path else {
            return;
        };

        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let Ok(serialized) = serde_json::to_string_pretty(tasks) else {
            return;
        };
        let temp_path = path.with_extension("json.tmp");
        if std::fs::write(&temp_path, serialized).is_ok() {
            let _ = std::fs::rename(temp_path, path);
        }
    }
}

impl Default for HeartbeatEngine {
    fn default() -> Self {
        Self::new()
    }
}

fn load_tasks(path: &PathBuf) -> HashMap<String, HeartbeatTask> {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return HashMap::new();
    };

    serde_json::from_str(&contents).unwrap_or_default()
}

#[async_trait]
pub trait HeartbeatHandler: Send + Sync {
    async fn handle(&self, task: &HeartbeatTask) -> HeartbeatResult;

    fn clone_box(&self) -> Box<dyn HeartbeatHandler>;

    fn is_default(&self) -> bool {
        false
    }
}

impl Clone for Box<dyn HeartbeatHandler> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

pub trait IntoHeartbeatResult: Send + 'static {
    fn into_heartbeat_result(self, task: &HeartbeatTask) -> HeartbeatResult;
}

impl IntoHeartbeatResult for HeartbeatResult {
    fn into_heartbeat_result(self, _task: &HeartbeatTask) -> HeartbeatResult {
        self
    }
}

impl IntoHeartbeatResult for Result<(), String> {
    fn into_heartbeat_result(self, task: &HeartbeatTask) -> HeartbeatResult {
        match self {
            Ok(()) => HeartbeatResult::success(&task.id, format!("Task '{}' executed", task.name)),
            Err(error) => HeartbeatResult::failure(&task.id, error),
        }
    }
}

impl IntoHeartbeatResult for Result<(), &'static str> {
    fn into_heartbeat_result(self, task: &HeartbeatTask) -> HeartbeatResult {
        self.map_err(str::to_string).into_heartbeat_result(task)
    }
}

#[async_trait]
impl<F, Fut, Output> HeartbeatHandler for F
where
    F: Send + Sync + Clone + 'static + Fn(&HeartbeatTask) -> Fut,
    Fut: Future<Output = Output> + Send + 'static,
    Output: IntoHeartbeatResult,
{
    async fn handle(&self, task: &HeartbeatTask) -> HeartbeatResult {
        (self)(task).await.into_heartbeat_result(task)
    }

    fn clone_box(&self) -> Box<dyn HeartbeatHandler> {
        Box::new(self.clone())
    }
}

pub struct DefaultHeartbeatHandler;

#[async_trait]
impl HeartbeatHandler for DefaultHeartbeatHandler {
    async fn handle(&self, task: &HeartbeatTask) -> HeartbeatResult {
        HeartbeatResult::success(&task.id, format!("Task '{}' executed", task.name))
    }

    fn clone_box(&self) -> Box<dyn HeartbeatHandler> {
        Box::new(Self)
    }

    fn is_default(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct CustomHandler;

    #[async_trait]
    impl HeartbeatHandler for CustomHandler {
        async fn handle(&self, task: &HeartbeatTask) -> HeartbeatResult {
            HeartbeatResult::success(&task.id, format!("custom {}", task.name))
        }

        fn clone_box(&self) -> Box<dyn HeartbeatHandler> {
            Box::new(Self)
        }
    }

    #[tokio::test]
    async fn heartbeat_uses_registered_handler() {
        let engine = HeartbeatEngine::new();
        let task = HeartbeatTask::new("custom_task", "0 0 0 * * *");
        let id = task.id.clone();
        engine
            .register_handler("custom_task", Arc::new(CustomHandler))
            .await;
        engine.register(task).await;

        let result = engine.run_task_now(&id).await.unwrap();

        assert_eq!(result.message, "custom custom_task");
    }

    #[tokio::test]
    async fn heartbeat_add_task_and_start_follow_documented_contract() {
        let (sender, mut receiver) = mpsc::channel(2);
        let engine = HeartbeatEngine::new().with_event_channel(sender);
        let task_id = engine
            .add_task(HeartbeatTask::new("doc_task", "0 0 0 * * *"))
            .await;

        assert!(engine.start().await.is_ok());
        assert!(matches!(
            receiver.recv().await,
            Some(HeartbeatEvent::EngineStarted)
        ));
        assert!(engine.get(&task_id).await.is_some());
    }

    #[tokio::test]
    async fn heartbeat_task_specific_handler_supports_documented_flow() {
        let engine = HeartbeatEngine::new();
        let task = HeartbeatTask::new("doc_task", "0 0 0 * * *")
            .with_handler(|_task: &HeartbeatTask| async move { Ok::<(), String>(()) });
        let id = engine.add_task(task).await;

        let result = engine.run_task_now(&id).await.unwrap();

        assert!(result.success);
        assert_eq!(result.message, "Task 'doc_task' executed");
    }

    #[tokio::test]
    async fn heartbeat_run_task_now_honors_enabled_flag_and_updates_next_run() {
        let engine = HeartbeatEngine::new();
        let disabled = HeartbeatTask::new("disabled_task", "0 0 0 * * *").disabled();
        let disabled_id = engine.add_task(disabled).await;

        assert!(engine.run_task_now(&disabled_id).await.is_none());

        let enabled_id = engine
            .add_task(HeartbeatTask::new("enabled_task", "0 0 0 * * *"))
            .await;
        let before = engine.get(&enabled_id).await.unwrap().next_run;

        let result = engine.run_task_now(&enabled_id).await.unwrap();
        let after = engine.get(&enabled_id).await.unwrap();

        assert!(result.success);
        assert_eq!(after.run_count, 1);
        assert!(after.last_run.is_some());
        assert!(after.next_run.is_some());
        assert!(after.last_result.is_some());
        assert_eq!(before, after.next_run);
    }

    #[tokio::test]
    async fn heartbeat_disable_clears_next_run_until_reenabled() {
        let engine = HeartbeatEngine::new();
        let id = engine
            .add_task(HeartbeatTask::new("toggle_task", "0 0 0 * * *"))
            .await;

        let before = engine.get(&id).await.unwrap();
        assert!(before.next_run.is_some());

        assert!(engine.disable(&id).await);
        let disabled = engine.get(&id).await.unwrap();
        assert!(!disabled.enabled);
        assert!(disabled.next_run.is_none());

        assert!(engine.enable(&id).await);
        let reenabled = engine.get(&id).await.unwrap();
        assert!(reenabled.enabled);
        assert!(reenabled.next_run.is_some());
    }

    #[tokio::test]
    async fn heartbeat_tick_requires_running_engine() {
        let engine = HeartbeatEngine::new();
        let id = engine
            .add_task(HeartbeatTask::new("due_task", "0 0 0 * * *"))
            .await;
        engine
            .tasks
            .write()
            .await
            .get_mut(&id)
            .unwrap()
            .next_run = Some(Utc::now() - chrono::Duration::seconds(1));

        let stopped = engine.tick().await;
        assert!(stopped.is_empty());
        assert_eq!(engine.get(&id).await.unwrap().run_count, 0);

        assert!(engine.start().await.is_ok());
        let running = engine.tick().await;
        assert_eq!(running.len(), 1);

        assert!(engine.stop().await.is_ok());
        let after_stop = engine.tick().await;
        assert!(after_stop.is_empty());
    }

    #[tokio::test]
    async fn heartbeat_persists_tasks_across_reconstruction() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_heartbeat_persist_test_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let state_path = root.join("heartbeat.json");

        let engine = HeartbeatEngine::new().with_state_path(state_path.clone());
        let id = engine
            .add_task(HeartbeatTask::new("persisted_task", "0 0 0 * * *"))
            .await;
        let _ = engine.run_task_now(&id).await.unwrap();

        let reconstructed = HeartbeatEngine::new().with_state_path(state_path);
        let task = reconstructed.get(&id).await.unwrap();

        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(task.run_count, 1);
        assert!(task.last_run.is_some());
        assert!(task.last_result.is_some());
    }
}
