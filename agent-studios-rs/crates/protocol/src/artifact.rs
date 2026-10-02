use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::id::{AgentId, ArtifactId, StudioId, TaskId};

/// Category of an artifact generated during task execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    File,
    Patch,
    Log,
    Report,
    Plan,
    Other,
}

/// Metadata record for an artifact managed by Agent Studios.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRecord {
    pub id: ArtifactId,
    pub studio_id: StudioId,
    pub task_id: TaskId,
    pub producer_agent_id: AgentId,
    pub kind: ArtifactKind,
    pub logical_name: String,
    pub content_hash: Option<String>,
    pub location: String,
    pub created_at: DateTime<Utc>,
}

impl ArtifactRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        studio_id: StudioId,
        task_id: TaskId,
        producer_agent_id: AgentId,
        kind: ArtifactKind,
        logical_name: impl Into<String>,
        content_hash: Option<String>,
        location: impl Into<String>,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id: ArtifactId::new(),
            studio_id,
            task_id,
            producer_agent_id,
            kind,
            logical_name: logical_name.into(),
            content_hash,
            location: location.into(),
            created_at,
        }
    }
}
