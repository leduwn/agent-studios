use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

use crate::error::IdParseError;

macro_rules! define_id {
    ($name:ident, $doc:expr) => {
        #[doc = $doc]
        #[derive(
            Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            /// Generates a new unique identifier.
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            /// Creates an identifier wrapping the provided UUID.
            pub const fn from_uuid(uuid: Uuid) -> Self {
                Self(uuid)
            }

            /// Returns a reference to the underlying UUID.
            pub const fn as_uuid(&self) -> &Uuid {
                &self.0
            }

            /// Consumes the wrapper and returns the underlying UUID.
            pub const fn into_inner(self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl FromStr for $name {
            type Err = IdParseError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let uuid = Uuid::parse_str(s).map_err(|err| IdParseError {
                    type_name: stringify!($name),
                    source: err,
                })?;
                Ok(Self(uuid))
            }
        }

        impl From<Uuid> for $name {
            fn from(uuid: Uuid) -> Self {
                Self(uuid)
            }
        }

        impl From<$name> for Uuid {
            fn from(id: $name) -> Self {
                id.0
            }
        }
    };
}

define_id!(StudioId, "Strongly typed identifier for a Studio.");
define_id!(AgentId, "Strongly typed identifier for an Agent.");
define_id!(TaskId, "Strongly typed identifier for a Task.");
define_id!(RunId, "Strongly typed identifier for a Task Run attempt.");
define_id!(ArtifactId, "Strongly typed identifier for an Artifact.");
define_id!(
    ApprovalId,
    "Strongly typed identifier for an Approval Request."
);
define_id!(
    EventId,
    "Strongly typed identifier for a Control Plane Event."
);
define_id!(WorktreeId, "Strongly typed identifier for a Worktree.");
define_id!(
    ReconciliationId,
    "Strongly typed identifier for a Reconciliation operation."
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_id_generation_and_uniqueness() {
        let id1 = TaskId::new();
        let id2 = TaskId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_id_display_and_from_str() {
        let id = AgentId::new();
        let id_str = id.to_string();
        let parsed: AgentId = id_str.parse().expect("valid UUID string");
        assert_eq!(id, parsed);
    }

    #[test]
    fn test_id_serde_roundtrip() {
        let id = StudioId::new();
        let json = serde_json::to_string(&id).expect("serialize id");
        let deserialized: StudioId = serde_json::from_str(&json).expect("deserialize id");
        assert_eq!(id, deserialized);
    }
}
