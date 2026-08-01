use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::{
    AssignmentId, ConflictKind, MilestoneId, ProductError, ProductResult, ProductRevision,
    ProjectId, TaskId, WorkflowId,
};

/// Product Task is not an Agent Todo. Agent todos must be explicitly promoted.
pub const AGENT_TODO_PROMOTION_REQUIRED: &str = "agent_todo_must_be_promoted";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductTaskStatus {
    Draft,
    Waiting,
    Running,
    Blocked,
    Done,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductTaskPriority {
    Low,
    Normal,
    High,
    Urgent,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductTask {
    pub id: TaskId,
    pub project_id: Option<ProjectId>,
    pub title: String,
    pub description: Option<String>,
    pub status: ProductTaskStatus,
    pub priority: ProductTaskPriority,
    pub assignment_id: Option<AssignmentId>,
    pub completion_criteria: Vec<String>,
    pub milestone_id: Option<MilestoneId>,
    pub workflow_id: Option<WorkflowId>,
    pub agent_profile_id: Option<String>,
    pub blocked_reason: Option<String>,
    pub depends_on: Vec<TaskId>,
    pub parent_id: Option<TaskId>,
    pub pinned: bool,
    pub sort_order: i64,
    pub archived: bool,
    pub tags: Vec<String>,
    /// Unix epoch milliseconds. Hosts set this through their clock port.
    #[serde(default)]
    pub created_at: i64,
    /// Unix epoch milliseconds. Updated by the application command boundary.
    #[serde(default)]
    pub updated_at: i64,
    pub revision: ProductRevision,
    /// Provenance only — never a Claude/Codex brand field on the core model.
    pub legacy_source: Option<String>,
}

impl ProductTask {
    pub fn new(
        id: TaskId,
        project_id: Option<ProjectId>,
        title: impl Into<String>,
    ) -> ProductResult<Self> {
        let title = title.into();
        if title.trim().is_empty() {
            return Err(ProductError::InvalidInput {
                field: "title".into(),
                message: "task title must not be empty".into(),
            });
        }
        Ok(Self {
            id,
            project_id,
            title,
            description: None,
            status: ProductTaskStatus::Draft,
            priority: ProductTaskPriority::Normal,
            assignment_id: None,
            completion_criteria: Vec::new(),
            milestone_id: None,
            workflow_id: None,
            agent_profile_id: None,
            blocked_reason: None,
            depends_on: Vec::new(),
            parent_id: None,
            pinned: false,
            sort_order: 0,
            archived: false,
            tags: Vec::new(),
            created_at: 0,
            updated_at: 0,
            revision: ProductRevision::INITIAL,
            legacy_source: None,
        })
    }
}

/// Pure dependency graph rules used by application services (no SQLite).
#[derive(Clone, Debug, Default)]
pub struct TaskDependencyGraph {
    /// task_id → dependency ids
    edges: HashMap<String, Vec<String>>,
    project_of: HashMap<String, Option<String>>,
}

impl TaskDependencyGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_task(
        &mut self,
        task_id: &TaskId,
        project_id: Option<&ProjectId>,
        depends_on: &[TaskId],
    ) {
        self.project_of.insert(
            task_id.as_str().to_string(),
            project_id.map(|id| id.as_str().to_string()),
        );
        self.edges.insert(
            task_id.as_str().to_string(),
            depends_on
                .iter()
                .map(|id| id.as_str().to_string())
                .collect(),
        );
    }

    pub fn validate_dependencies(
        &self,
        task_id: &TaskId,
        project_id: Option<&ProjectId>,
        depends_on: &[TaskId],
    ) -> ProductResult<Vec<TaskId>> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for dep in depends_on {
            let key = dep.as_str();
            if key.is_empty() || !seen.insert(key.to_string()) {
                continue;
            }
            if key == task_id.as_str() {
                return Err(ProductError::InvalidInput {
                    field: "depends_on".into(),
                    message: "task cannot depend on itself".into(),
                });
            }
            let dep_project = self
                .project_of
                .get(key)
                .ok_or_else(|| ProductError::NotFound {
                    entity: "task".into(),
                    id: key.to_string(),
                })?;
            let expected = project_id.map(|id| id.as_str().to_string());
            if dep_project.as_deref() != expected.as_deref() {
                return Err(ProductError::InvalidInput {
                    field: "depends_on".into(),
                    message: "dependency must belong to the same project".into(),
                });
            }
            if self.would_create_cycle(task_id.as_str(), key) {
                return Err(ProductError::Conflict {
                    conflict: ConflictKind::DependencyCycle,
                    message: "dependency relationship cannot form a cycle".into(),
                });
            }
            out.push(dep.clone());
        }
        Ok(out)
    }

    fn would_create_cycle(&self, task_id: &str, dependency_id: &str) -> bool {
        let mut stack = vec![dependency_id.to_string()];
        let mut seen = HashSet::new();
        while let Some(current) = stack.pop() {
            if current == task_id {
                return true;
            }
            if !seen.insert(current.clone()) {
                continue;
            }
            if let Some(next) = self.edges.get(&current) {
                stack.extend(next.iter().cloned());
            }
        }
        false
    }
}

/// Marker type documenting that Agent Todo elevation is an explicit product command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TaskDependencyRule;

impl TaskDependencyRule {
    pub fn agent_todo_is_not_product_task() -> &'static str {
        AGENT_TODO_PROMOTION_REQUIRED
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_dependency_cycles() {
        let a = TaskId::new("a").unwrap();
        let b = TaskId::new("b").unwrap();
        let project = ProjectId::new("p1").unwrap();
        let mut graph = TaskDependencyGraph::new();
        graph.register_task(&a, Some(&project), &[]);
        graph.register_task(&b, Some(&project), std::slice::from_ref(&a));
        let err = graph
            .validate_dependencies(&a, Some(&project), std::slice::from_ref(&b))
            .unwrap_err();
        assert!(matches!(
            err,
            ProductError::Conflict {
                conflict: ConflictKind::DependencyCycle,
                ..
            }
        ));
    }

    #[test]
    fn allows_acyclic_dependencies() {
        let a = TaskId::new("a").unwrap();
        let b = TaskId::new("b").unwrap();
        let project = ProjectId::new("p1").unwrap();
        let mut graph = TaskDependencyGraph::new();
        graph.register_task(&a, Some(&project), &[]);
        graph.register_task(&b, Some(&project), &[]);
        let deps = graph
            .validate_dependencies(&b, Some(&project), std::slice::from_ref(&a))
            .unwrap();
        assert_eq!(deps, vec![a]);
    }
}
