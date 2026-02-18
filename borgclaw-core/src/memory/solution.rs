//! Solution memory for storing and retrieving problem-solving patterns

use super::{MemoryError, MemoryEntry, MemoryQuery, MemoryResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use chrono::{DateTime, Utc};
use async_trait::async_trait;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Solution {
    pub id: String,
    pub problem_type: String,
    pub problem_description: String,
    pub solution_steps: Vec<SolutionStep>,
    pub tags: Vec<String>,
    pub success_rate: f32,
    pub usage_count: u32,
    pub created_at: DateTime<Utc>,
    pub last_used: Option<DateTime<Utc>>,
    pub metadata: HashMap<String, String>,
}

impl Solution {
    pub fn new(problem_type: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            problem_type: problem_type.into(),
            problem_description: description.into(),
            solution_steps: Vec::new(),
            tags: Vec::new(),
            success_rate: 0.0,
            usage_count: 0,
            created_at: Utc::now(),
            last_used: None,
            metadata: HashMap::new(),
        }
    }

    pub fn with_step(mut self, step: SolutionStep) -> Self {
        self.solution_steps.push(step);
        self
    }

    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    pub fn record_use(&mut self, success: bool) {
        self.usage_count += 1;
        self.last_used = Some(Utc::now());
        
        let prev_weight = (self.usage_count - 1) as f32;
        let curr_weight = 1.0;
        let total_weight = prev_weight + curr_weight;
        
        self.success_rate = if total_weight > 0.0 {
            (self.success_rate * prev_weight + if success { 1.0 } else { 0.0 } * curr_weight) / total_weight
        } else {
            if success { 1.0 } else { 0.0 }
        };
    }

    pub fn to_entry(&self) -> MemoryEntry {
        MemoryEntry {
            id: self.id.clone(),
            key: format!("solution:{}", self.problem_type),
            content: serde_json::to_string(self).unwrap_or_default(),
            metadata: self.metadata.clone(),
            created_at: self.created_at,
            accessed_at: self.last_used.unwrap_or(self.created_at),
            access_count: self.usage_count,
            importance: self.success_rate,
            group_id: Some("solutions".to_string()),
        }
    }

    pub fn from_entry(entry: &MemoryEntry) -> Option<Self> {
        if !entry.key.starts_with("solution:") {
            return None;
        }
        
        serde_json::from_str(&entry.content).ok()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolutionStep {
    pub order: u32,
    pub description: String,
    pub command: Option<String>,
    pub expected_result: Option<String>,
    pub notes: Option<String>,
}

impl SolutionStep {
    pub fn new(order: u32, description: impl Into<String>) -> Self {
        Self {
            order,
            description: description.into(),
            command: None,
            expected_result: None,
            notes: None,
        }
    }

    pub fn with_command(mut self, command: impl Into<String>) -> Self {
        self.command = Some(command.into());
        self
    }

    pub fn with_expected_result(mut self, result: impl Into<String>) -> Self {
        self.expected_result = Some(result.into());
        self
    }

    pub fn with_notes(mut self, notes: impl Into<String>) -> Self {
        self.notes = Some(notes.into());
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolutionPattern {
    pub pattern_id: String,
    pub name: String,
    pub description: String,
    pub triggers: Vec<String>,
    pub solution_template: String,
    pub variables: HashMap<String, String>,
}

impl SolutionPattern {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            pattern_id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            description: description.into(),
            triggers: Vec::new(),
            solution_template: String::new(),
            variables: HashMap::new(),
        }
    }

    pub fn with_trigger(mut self, trigger: impl Into<String>) -> Self {
        self.triggers.push(trigger.into());
        self
    }

    pub fn with_template(mut self, template: impl Into<String>) -> Self {
        self.solution_template = template.into();
        self
    }

    pub fn with_variable(mut self, name: impl Into<String>, default: impl Into<String>) -> Self {
        self.variables.insert(name.into(), default.into());
        self
    }

    pub fn matches(&self, text: &str) -> bool {
        let text_lower = text.to_lowercase();
        self.triggers.iter().any(|t| {
            let trigger_lower = t.to_lowercase();
            text_lower.contains(&trigger_lower)
        })
    }

    pub fn instantiate(&self, variables: &HashMap<String, String>) -> String {
        let mut result = self.solution_template.clone();
        for (key, default) in &self.variables {
            let value = variables.get(key).unwrap_or(default);
            result = result.replace(&format!("{{{}}}", key), value);
        }
        result
    }
}

pub struct SolutionMemory {
    solutions: HashMap<String, Solution>,
    patterns: Vec<SolutionPattern>,
}

impl SolutionMemory {
    pub fn new() -> Self {
        Self {
            solutions: HashMap::new(),
            patterns: Vec::new(),
        }
    }

    pub fn store(&mut self, solution: Solution) {
        self.solutions.insert(solution.id.clone(), solution);
    }

    pub fn get(&self, id: &str) -> Option<&Solution> {
        self.solutions.get(id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Solution> {
        self.solutions.get_mut(id)
    }

    pub fn remove(&mut self, id: &str) -> Option<Solution> {
        self.solutions.remove(id)
    }

    pub fn add_pattern(&mut self, pattern: SolutionPattern) {
        self.patterns.push(pattern);
    }

    pub fn find_by_type(&self, problem_type: &str) -> Vec<&Solution> {
        self.solutions
            .values()
            .filter(|s| s.problem_type == problem_type)
            .collect()
    }

    pub fn find_by_tag(&self, tag: &str) -> Vec<&Solution> {
        self.solutions
            .values()
            .filter(|s| s.tags.contains(&tag.to_string()))
            .collect()
    }

    pub fn find_matching_pattern(&self, text: &str) -> Option<&SolutionPattern> {
        self.patterns.iter().find(|p| p.matches(text))
    }

    pub fn search(&self, query: &str) -> Vec<&Solution> {
        let query_lower = query.to_lowercase();
        self.solutions
            .values()
            .filter(|s| {
                s.problem_description.to_lowercase().contains(&query_lower)
                    || s.tags.iter().any(|t| t.to_lowercase().contains(&query_lower))
                    || s.problem_type.to_lowercase().contains(&query_lower)
            })
            .collect()
    }

    pub fn top_solutions(&self, limit: usize) -> Vec<&Solution> {
        let mut solutions: Vec<&Solution> = self.solutions.values().collect();
        solutions.sort_by(|a, b| {
            b.success_rate.partial_cmp(&a.success_rate)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.usage_count.cmp(&a.usage_count))
        });
        solutions.truncate(limit);
        solutions
    }

    pub fn all_solutions(&self) -> impl Iterator<Item = &Solution> {
        self.solutions.values()
    }

    pub fn all_patterns(&self) -> impl Iterator<Item = &SolutionPattern> {
        self.patterns.iter()
    }
}

impl Default for SolutionMemory {
    fn default() -> Self {
        Self::new()
    }
}
