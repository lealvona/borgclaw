# Memory Systems

BorgClaw provides a comprehensive memory system with hybrid search, session management, and solution patterns.

## Architecture

```
┌─────────────────────────────────────────────┐
│               Memory Layer                  │
├─────────────────────────────────────────────┤
│                                             │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐ │
│  │ Storage  │  │ Session  │  │ Solution │ │
│  │ (SQLite) │  │ (Compact)│  │(Patterns)│ │
│  └──────────┘  └──────────┘  └──────────┘ │
│                                             │
│  ┌──────────┐  ┌──────────┐               │
│  │Heartbeat │  │Sub-Agent │               │
│  │ (Cron)   │  │(Parallel)│               │
│  └──────────┘  └──────────┘               │
│                                             │
└─────────────────────────────────────────────┘
```

## Storage (SQLite + FTS5)

### Features

- **Full-text search** via SQLite FTS5
- **Per-group isolation** - Separate memory per conversation
- **Importance scoring** - Weighted recall
- **Access tracking** - Frequency and recency

### Configuration

```toml
[memory]
database_path = ".local/data/memory.db"
hybrid_search = true
session_max_entries = 100
```

### API

```rust
// Store
memory.store(MemoryEntry {
    key: "project_deadlines".to_string(),
    content: "Q1 report due Feb 15...".to_string(),
    group_id: Some("work".to_string()),
    importance: 0.8,
    ..Default::default()
}).await?;

// Recall
let results = memory.recall(MemoryQuery {
    query: "deadlines report".to_string(),
    limit: 10,
    min_score: 0.3,
    group_id: Some("work".to_string()),
}).await?;

// List keys
let keys = memory.keys().await?;

// Clear group
memory.clear_group("work").await?;
```

### Memory Entry

```rust
pub struct MemoryEntry {
    pub id: String,
    pub key: String,           // Topic/subject
    pub content: String,       // Full content
    pub metadata: HashMap<String, String>,
    pub created_at: DateTime<Utc>,
    pub accessed_at: DateTime<Utc>,
    pub access_count: u32,
    pub importance: f32,       // 0.0 - 1.0
    pub group_id: Option<String>,
}
```

## Session Memory

### Auto-Compaction

Sessions automatically compact when exceeding thresholds:

```toml
[memory]
session_max_entries = 100
session_keep_recent = 20
session_keep_important = true
```

### Session Compactor

```rust
let compactor = SessionCompactor::new()
    .keep_recent(20)
    .keep_important(true);

let compacted = compactor.compact(entries);
```

### Session ID

Each conversation has a unique session ID for isolation:

```rust
let session_id = SessionId::new("telegram", "user123", Some("group456"));
```

## Solution Memory

Store and recall reusable solution patterns:

```rust
// Store a solution
memory.store_solution(Solution {
    problem: "Parse JSON from API response".to_string(),
    solution: "Use serde_json::from_str with error handling...".to_string(),
    tags: vec!["json", "api", "parsing"],
    success_count: 5,
    ..Default::default()
}).await?;

// Find similar solutions
let solutions = memory.find_solutions("parse json api", 5).await?;
```

### Solution Pattern

```rust
pub struct Solution {
    pub id: String,
    pub problem: String,
    pub solution: String,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub last_used: DateTime<Utc>,
    pub success_count: u32,
}

pub struct SolutionPattern {
    pub pattern_type: String,
    pub pattern: String,
    pub template: String,
    pub examples: Vec<String>,
}
```

## Heartbeat Engine

Scheduled background tasks with cron expressions:

```rust
let mut engine = HeartbeatEngine::new();

// Add scheduled task
engine.add_task(HeartbeatTask {
    id: "daily_summary".to_string(),
    schedule: "0 9 * * *".to_string(),  // 9 AM daily
    handler: Box::new(|ctx| async move {
        // Generate daily summary
        Ok(())
    }),
}).await;

// Start engine
engine.start().await?;
```

### Heartbeat Task

```rust
pub struct HeartbeatTask {
    pub id: String,
    pub name: String,
    pub schedule: String,       // Cron expression
    pub enabled: bool,
    pub last_run: Option<DateTime<Utc>>,
    pub next_run: Option<DateTime<Utc>>,
    pub run_count: u32,
    pub max_retries: u32,
    pub retry_count: u32,
    pub retry_delay_seconds: u64,
    pub dead_lettered_at: Option<DateTime<Utc>>,
    pub last_result: Option<HeartbeatResult>,
    pub handler: Box<dyn HeartbeatHandler>,
}
```

Retry behavior:
- Failed heartbeat tasks can be configured with retry attempts and retry backoff.
- Exhausted heartbeat tasks are dead-lettered by disabling the task and recording `dead_lettered_at`.

### Cron Examples

| Expression | Schedule |
|------------|----------|
| `* * * * *` | Every minute |
| `0 * * * *` | Every hour |
| `0 9 * * *` | Daily at 9 AM |
| `0 9 * * 1` | Every Monday 9 AM |
| `*/15 * * * *` | Every 15 minutes |

## Sub-Agent Coordinator

Run background tasks in parallel:

```rust
let coordinator = SubAgentCoordinator::new(agent_config);

// Spawn background task
let task_id = coordinator.spawn(
    "Analyze logs",
    AgentContext { /* ... */ },
    Priority::Low,
).await?;

// Check status
let status = coordinator.status(&task_id).await;

// Get result
if let SubAgentStatus::Completed(result) = status {
    println!("Result: {}", result);
}
```

### Task Priority

```rust
pub enum Priority {
    High,    // Immediate processing
    Normal,  // Default
    Low,     // Background, when idle
}
```

### Task Status

```rust
pub enum SubAgentStatus {
    Pending,
    Running,
    Completed(AgentResponse),
    Failed(String),
    Cancelled,
    Timeout(String),
}
```

Retry behavior:
- Failed or timed-out sub-agent tasks can re-enter `Pending` with retry backoff.
- Exhausted retries leave the task in terminal `Failed` or `Timeout` state with dead-letter metadata preserved on the task record.

## Configuration

```toml
[memory]
database_path = ".local/data/memory.db"
hybrid_search = true
session_max_entries = 100
session_keep_recent = 20
session_keep_important = true

[heartbeat]
enabled = true
check_interval_seconds = 60
```

## Best Practices

### Key Naming

Use consistent key prefixes:
- `project:` - Project information
- `user:` - User preferences
- `task:` - Task context
- `temp:` - Temporary data

### Importance Scoring

- `0.0-0.3` - Low (temporary, procedural)
- `0.4-0.6` - Normal (facts, information)
- `0.7-0.9` - High (critical, rarely changes)
- `1.0` - Permanent

### Group Isolation

Always set `group_id` for multi-tenant scenarios:
- Telegram groups → group_id from chat ID
- Webhooks → group_id from header or path
- Users → group_id from user ID
