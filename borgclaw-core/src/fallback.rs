//! Structured fallback deliverables for failed or stuck background work.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FallbackDeliverable {
    pub title: String,
    pub summary: String,
    pub failure_kind: String,
    pub suggested_actions: Vec<String>,
    #[serde(default)]
    pub context: HashMap<String, String>,
}

impl FallbackDeliverable {
    pub fn new(
        title: impl Into<String>,
        summary: impl Into<String>,
        failure_kind: impl Into<String>,
    ) -> Self {
        let failure_kind = failure_kind.into();
        Self {
            title: title.into(),
            summary: summary.into(),
            suggested_actions: default_suggested_actions(&failure_kind),
            failure_kind,
            context: HashMap::new(),
        }
    }

    pub fn with_context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.insert(key.into(), value.into());
        self
    }
}

pub fn background_failure_deliverable(
    kind: &str,
    subject: &str,
    summary: impl Into<String>,
) -> FallbackDeliverable {
    FallbackDeliverable::new(
        format!("{} fallback deliverable", subject),
        summary,
        kind.to_string(),
    )
    .with_context("subject", subject.to_string())
}

fn default_suggested_actions(failure_kind: &str) -> Vec<String> {
    match failure_kind {
        "timeout" => vec![
            "Reduce the task scope or split it into smaller work units.".to_string(),
            "Increase the timeout if the task is expected to run longer.".to_string(),
            "Inspect runtime logs or persisted state before retrying.".to_string(),
        ],
        "approval" => vec![
            "Run the task from an interactive context that can grant approval.".to_string(),
            "Lower the approval requirement only if the action is genuinely safe.".to_string(),
        ],
        _ => vec![
            "Inspect the persisted error details and retry only after correcting the input or dependency issue.".to_string(),
            "Use the operator status surfaces to confirm the runtime prerequisites are available.".to_string(),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_deliverable_includes_expected_actions() {
        let deliverable = background_failure_deliverable("timeout", "scheduler job", "timed out");
        assert_eq!(deliverable.failure_kind, "timeout");
        assert!(deliverable
            .suggested_actions
            .iter()
            .any(|item| item.contains("Increase the timeout")));
        assert_eq!(
            deliverable.context.get("subject").map(String::as_str),
            Some("scheduler job")
        );
    }
}
