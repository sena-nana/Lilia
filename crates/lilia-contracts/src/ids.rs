use serde::{Deserialize, Serialize};

/// Stable product identity helpers. Empty / whitespace-only values are rejected.
macro_rules! product_id {
    ($name:ident, $label:literal) => {
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, crate::ProductError> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err(crate::ProductError::InvalidInput {
                        field: $label.into(),
                        message: format!("{label} must not be empty", label = $label),
                    });
                }
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub fn into_inner(self) -> String {
                self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

product_id!(ProjectId, "project_id");
product_id!(TaskId, "task_id");
product_id!(ConversationId, "conversation_id");
product_id!(MilestoneId, "milestone_id");
product_id!(WorkflowId, "workflow_id");
product_id!(WorkflowRunId, "workflow_run_id");
product_id!(AssignmentId, "assignment_id");
product_id!(ArtifactId, "artifact_id");
product_id!(ProjectAssetId, "project_asset_id");
product_id!(BindingId, "binding_id");
