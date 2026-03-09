//! Solution memory for storing and retrieving problem-solving patterns

use super::{MemoryEntry, MemoryError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Solution {
    pub id: String,
    pub problem: String,
    pub solution: String,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub last_used: DateTime<Utc>,
    pub success_count: u32,
    pub metadata: HashMap<String, String>,

    // Legacy compatibility fields retained so older serialized entries still round-trip.
    pub problem_type: String,
    pub problem_description: String,
    pub solution_steps: Vec<SolutionStep>,
    pub success_rate: f32,
    pub usage_count: u32,
}

impl Default for Solution {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            problem: String::new(),
            solution: String::new(),
            tags: Vec::new(),
            created_at: now,
            last_used: now,
            success_count: 0,
            metadata: HashMap::new(),
            problem_type: String::new(),
            problem_description: String::new(),
            solution_steps: Vec::new(),
            success_rate: 0.0,
            usage_count: 0,
        }
    }
}

impl Solution {
    pub fn new(problem: impl Into<String>, solution: impl Into<String>) -> Self {
        let problem = problem.into();
        let solution = solution.into();
        Self {
            problem: problem.clone(),
            solution: solution.clone(),
            problem_type: problem.clone(),
            problem_description: problem,
            ..Default::default()
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
        self.last_used = Utc::now();
        if success {
            self.success_count += 1;
        }

        let prev_weight = (self.usage_count - 1) as f32;
        let curr_weight = 1.0;
        let total_weight = prev_weight + curr_weight;

        self.success_rate = if total_weight > 0.0 {
            (self.success_rate * prev_weight + if success { 1.0 } else { 0.0 } * curr_weight)
                / total_weight
        } else {
            if success {
                1.0
            } else {
                0.0
            }
        };
    }

    pub fn to_entry(&self) -> MemoryEntry {
        MemoryEntry {
            id: self.id.clone(),
            key: format!("solution:{}", self.problem),
            content: serde_json::to_string(self).unwrap_or_default(),
            metadata: self.metadata.clone(),
            created_at: self.created_at,
            accessed_at: self.last_used,
            access_count: self.success_count,
            importance: (self.success_count as f32).max(self.success_rate),
            group_id: Some("solutions".to_string()),
        }
    }

    pub fn from_entry(entry: &MemoryEntry) -> Option<Self> {
        if !entry.key.starts_with("solution:") {
            return None;
        }

        let mut solution: Solution = serde_json::from_str(&entry.content).ok()?;
        if solution.problem.is_empty() {
            solution.problem = if !solution.problem_description.is_empty() {
                solution.problem_description.clone()
            } else {
                solution.problem_type.clone()
            };
        }
        if solution.solution.is_empty() {
            solution.solution = solution
                .solution_steps
                .iter()
                .map(|step| step.description.clone())
                .collect::<Vec<_>>()
                .join("\n");
        }
        if solution.problem_type.is_empty() {
            solution.problem_type = solution.problem.clone();
        }
        if solution.problem_description.is_empty() {
            solution.problem_description = solution.problem.clone();
        }
        Some(solution)
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
#[serde(default)]
pub struct SolutionPattern {
    pub pattern_type: String,
    pub pattern: String,
    pub template: String,
    pub examples: Vec<String>,

    // Legacy compatibility fields retained for older callers and stored entries.
    pub pattern_id: String,
    pub name: String,
    pub description: String,
    pub triggers: Vec<String>,
    pub solution_template: String,
    pub variables: HashMap<String, String>,
}

impl Default for SolutionPattern {
    fn default() -> Self {
        Self {
            pattern_type: String::new(),
            pattern: String::new(),
            template: String::new(),
            examples: Vec::new(),
            pattern_id: uuid::Uuid::new_v4().to_string(),
            name: String::new(),
            description: String::new(),
            triggers: Vec::new(),
            solution_template: String::new(),
            variables: HashMap::new(),
        }
    }
}

impl SolutionPattern {
    pub fn new(pattern_type: impl Into<String>, pattern: impl Into<String>) -> Self {
        let pattern_type = pattern_type.into();
        let pattern = pattern.into();
        Self {
            pattern_type: pattern_type.clone(),
            pattern: pattern.clone(),
            name: pattern_type,
            description: pattern,
            ..Default::default()
        }
    }

    pub fn with_trigger(mut self, trigger: impl Into<String>) -> Self {
        let trigger = trigger.into();
        self.triggers.push(trigger.clone());
        self.examples.push(trigger);
        self
    }

    pub fn with_template(mut self, template: impl Into<String>) -> Self {
        let template = template.into();
        self.template = template.clone();
        self.solution_template = template;
        self
    }

    pub fn with_variable(mut self, name: impl Into<String>, default: impl Into<String>) -> Self {
        self.variables.insert(name.into(), default.into());
        self
    }

    pub fn matches(&self, text: &str) -> bool {
        let text_lower = text.to_lowercase();
        self.triggers
            .iter()
            .chain(self.examples.iter())
            .chain(std::iter::once(&self.pattern))
            .any(|t| {
                let trigger_lower = t.to_lowercase();
                text_lower.contains(&trigger_lower)
            })
    }

    pub fn instantiate(&self, variables: &HashMap<String, String>) -> String {
        let mut result = if self.template.is_empty() {
            self.solution_template.clone()
        } else {
            self.template.clone()
        };
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

    pub async fn store_solution(&mut self, solution: Solution) -> Result<(), MemoryError> {
        self.store(solution);
        Ok(())
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
        let terms = query
            .split_whitespace()
            .map(|term| term.to_lowercase())
            .collect::<Vec<_>>();
        self.solutions
            .values()
            .filter(|s| {
                if terms.is_empty() {
                    return true;
                }

                let haystack = format!(
                    "{} {} {} {} {}",
                    s.problem,
                    s.solution,
                    s.problem_description,
                    s.problem_type,
                    s.tags.join(" ")
                )
                .to_lowercase();

                terms.iter().all(|term| haystack.contains(term))
            })
            .collect()
    }

    pub async fn find_solutions(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<Solution>, MemoryError> {
        let mut results = self.search(query);
        results.sort_by(|a, b| {
            b.success_count
                .cmp(&a.success_count)
                .then_with(|| b.last_used.cmp(&a.last_used))
        });
        results.truncate(limit);
        Ok(results.into_iter().cloned().collect())
    }

    pub fn top_solutions(&self, limit: usize) -> Vec<&Solution> {
        let mut solutions: Vec<&Solution> = self.solutions.values().collect();
        solutions.sort_by(|a, b| {
            b.success_count
                .cmp(&a.success_count)
                .then_with(|| b.last_used.cmp(&a.last_used))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn documented_solution_api_stores_and_finds_solutions() {
        let mut memory = SolutionMemory::new();
        memory
            .store_solution(Solution {
                problem: "Parse JSON from API response".to_string(),
                solution: "Use serde_json::from_str with error handling".to_string(),
                tags: vec!["json".to_string(), "api".to_string()],
                success_count: 5,
                ..Default::default()
            })
            .await
            .unwrap();

        let results = memory.find_solutions("parse json api", 5).await.unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].problem, "Parse JSON from API response");
    }

    #[test]
    fn solution_pattern_matches_documented_shape() {
        let pattern = SolutionPattern {
            pattern_type: "parser".to_string(),
            pattern: "json parse".to_string(),
            template: "Use {crate}".to_string(),
            examples: vec!["json parse failure".to_string()],
            ..Default::default()
        };

        assert!(pattern.matches("json parse failure"));
        assert_eq!(
            pattern.instantiate(&HashMap::from([(
                "crate".to_string(),
                "serde_json".to_string()
            )])),
            "Use {crate}"
        );
    }
}
