use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowState {
    Pending,
    AwaitingApproval,
    Running,
    Completed,
    Failed,
}

impl std::fmt::Display for WorkflowState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Pending => "pending",
            Self::AwaitingApproval => "awaiting_approval",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
        };
        write!(f, "{}", s)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRequest {
    pub task: String,
    pub session_id: String,
    pub require_approval: bool,
    pub audit_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStatus {
    pub workflow_id: String,
    pub status: WorkflowState,
    pub task: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStartResponse {
    pub workflow_id: String,
    pub status: WorkflowState,
}
