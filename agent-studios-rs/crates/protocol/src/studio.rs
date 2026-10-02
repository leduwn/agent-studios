use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::id::StudioId;

/// Top-level orchestration scope for Agent Studios.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Studio {
    pub id: StudioId,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

impl Studio {
    pub fn new(name: impl Into<String>, created_at: DateTime<Utc>) -> Self {
        Self {
            id: StudioId::new(),
            name: name.into(),
            created_at,
        }
    }

    pub fn with_id(id: StudioId, name: impl Into<String>, created_at: DateTime<Utc>) -> Self {
        Self {
            id,
            name: name.into(),
            created_at,
        }
    }
}
