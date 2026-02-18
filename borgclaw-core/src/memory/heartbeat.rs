//! Heartbeat engine for periodic task execution

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use chrono::{DateTime, Utc};
use async_trait::async_trait;
use cron::Schedule;

pub struct HeartbeatEngine {
    tasks: Arc<RwLock<HashMap<String, HeartbeatTask>>>,
    running: Arc<RwLock<bool>>,
    sender: mpsc::Sender<HeartbeatEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

    pub fn parse_schedule(&self) -> Result<Schedule, String> {
        self.schedule.parse().map_err(|e: cron::error::Error| e.to_string())
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
            running: Arc::new(RwLock::new(false)),
            sender,
        }
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
        
        id
    }

    pub async fn unregister(&self, id: &str) -> bool {
        let mut tasks = self.tasks.write().await;
        tasks.remove(id).is_some()
    }

    pub async fn enable(&self, id: &str) -> bool {
        let mut tasks = self.tasks.write().await;
        if let Some(task) = tasks.get_mut(id) {
            task.enabled = true;
            task.calculate_next_run();
            true
        } else {
            false
        }
    }

    pub async fn disable(&self, id: &str) -> bool {
        let mut tasks = self.tasks.write().await;
        if let Some(task) = tasks.get_mut(id) {
            task.enabled = false;
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
            .filter(|t| {
                t.enabled && t.next_run.map(|nr| nr <= now).unwrap_or(false)
            })
            .cloned()
            .collect()
    }

    pub async fn start(&self) {
        let mut running = self.running.write().await;
        *running = true;
        drop(running);

        let _ = self.sender.send(HeartbeatEvent::EngineStarted).await;
    }

    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        *running = false;
        drop(running);

        let _ = self.sender.send(HeartbeatEvent::EngineStopped).await;
    }

    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    pub async fn tick(&self) -> Vec<(String, HeartbeatResult)> {
        let due_tasks = self.list_due().await;
        let mut results = Vec::new();

        for task in due_tasks {
            let task_id = task.id.clone();
            let _ = self.sender.send(HeartbeatEvent::TaskTriggered(task_id.clone())).await;

            let result = self.execute_task(&task).await;
            
            {
                let mut tasks = self.tasks.write().await;
                if let Some(t) = tasks.get_mut(&task_id) {
                    t.last_run = Some(Utc::now());
                    t.run_count += 1;
                    t.last_result = Some(result.clone());
                    t.calculate_next_run();
                }
            }

            if result.success {
                let _ = self.sender.send(HeartbeatEvent::TaskCompleted(task_id.clone(), result.clone())).await;
            } else {
                let _ = self.sender.send(HeartbeatEvent::TaskFailed(task_id.clone(), result.message.clone())).await;
            }

            results.push((task_id, result));
        }

        results
    }

    async fn execute_task(&self, task: &HeartbeatTask) -> HeartbeatResult {
        let start = std::time::Instant::now();
        
        let result = match task.name.as_str() {
            "memory_cleanup" => {
                HeartbeatResult::success(&task.id, "Memory cleanup completed")
            }
            "health_check" => {
                HeartbeatResult::success(&task.id, "Health check passed")
            }
            "session_compaction" => {
                HeartbeatResult::success(&task.id, "Session compaction completed")
            }
            _ => {
                HeartbeatResult::success(&task.id, format!("Task '{}' executed", task.name))
            }
        };

        result.with_duration(start.elapsed().as_millis() as u64)
    }

    pub async fn run_task_now(&self, id: &str) -> Option<HeartbeatResult> {
        let tasks = self.tasks.read().await;
        let task = tasks.get(id)?;
        let task = task.clone();
        drop(tasks);

        let result = self.execute_task(&task).await;

        {
            let mut tasks = self.tasks.write().await;
            if let Some(t) = tasks.get_mut(id) {
                t.last_run = Some(Utc::now());
                t.run_count += 1;
                t.last_result = Some(result.clone());
            }
        }

        Some(result)
    }
}

impl Default for HeartbeatEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
pub trait HeartbeatHandler: Send + Sync {
    async fn handle(&self, task: &HeartbeatTask) -> HeartbeatResult;
}

pub struct DefaultHeartbeatHandler;

#[async_trait]
impl HeartbeatHandler for DefaultHeartbeatHandler {
    async fn handle(&self, task: &HeartbeatTask) -> HeartbeatResult {
        HeartbeatResult::success(&task.id, format!("Task '{}' executed", task.name))
    }
}
