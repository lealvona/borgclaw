# Cut Content

This file preserves superseded contract text removed from the live BorgClaw docs during the March 29, 2026 contract repair.

The live docs now describe the implemented surfaces. The snippets below are retained only as historical context.

## Removed: Obsolete Custom Channel Example

Previously documented in `docs/channels.md` and `docs/integrations.md`:

```rust
use borgclaw_core::channel::{Channel, ChannelSender, InboundMessage, OutboundMessage};

struct MyChannel;

#[async_trait]
impl Channel for MyChannel {
    async fn start(&mut self, sender: ChannelSender) -> Result<(), ChannelError> {
        // Listen for messages, send via sender
    }

    async fn send(&self, msg: OutboundMessage) -> Result<(), ChannelError> {
        // Send message to channel
    }

    async fn stop(&mut self) -> Result<(), ChannelError> {
        // Cleanup
    }
}
```

Reason cut:
- The current `Channel` trait uses `init`, `start_receiving`, `send`, `status`, and `shutdown`.
- The older `start/send/stop` shape no longer matches the runtime.

## Removed: Obsolete Custom Tool Trait Example

Previously documented in `docs/integrations.md`:

```rust
use borgclaw_core::agent::*;

pub struct MyTool;

impl Tool for MyTool {
    fn name(&self) -> &str {
        "my_tool"
    }

    fn description(&self) -> &str {
        "Does something useful"
    }

    fn input_schema(&self) -> ToolSchema {
        ToolSchema {
            properties: vec![
                ("input".to_string(), PropertyDef {
                    property_type: "string".to_string(),
                    description: Some("Input to process".to_string()),
                    required: true,
                }),
            ],
        }
    }

    async fn execute(&self, input: serde_json::Value) -> Result<ToolResult, ToolError> {
        let input: String = serde_json::from_value(input)?;
        Ok(ToolResult::success(format!("Result: {}", input)))
    }
}
```

Reason cut:
- BorgClaw does not currently expose a public `Tool` trait for third-party runtime extension.
- The current runtime uses a `Tool` definition struct and built-in/plugin-integrated execution paths.

## Removed: Overstated Postgres Memory Example

Previously documented in `docs/integrations.md`:

```toml
[memory]
backend = "postgres"
connection_string = "postgres://user:pass@localhost/borgclaw"
hybrid_search = true
vector_provider = "postgres"
```

Reason cut:
- The `vector_provider = "postgres"` wording implied a `pgvector`-style backend contract that the live runtime does not expose as a separate documented requirement.
- The current contract is `backend = "postgres"` with `connection_string`; hybrid recall remains supported without promising a specific Postgres extension.

## Removed: Gateway Response Event Wording

Previously documented in `docs/gateway.md`:

- "Receive agent responses as `message` events"
- Message types listed `message` as the server-to-client chat event

Reason cut:
- The current gateway emits agent replies as `response` events.
- Client-to-server chat messages still use `type = "message"`.

## Removed: Stale Deployment Config Snippet

Previously documented in `skills/deployment-onboarding.md`:

```toml
[security]
wasm_sandbox = true
docker_sandbox = true
command_blocklist = true
secrets_encryption = true

[memory]
database_path = ".borgclaw/memory"
hybrid_search = true
vector_provider = "sqlite"

[channels.telegram]
enabled = true
bot_token = "${TELEGRAM_BOT_TOKEN}"
```

Reason cut:
- `docker_sandbox` is not part of the live config contract.
- Telegram uses `token`, not `bot_token`.
- Memory backend selection is now documented with `backend`.
