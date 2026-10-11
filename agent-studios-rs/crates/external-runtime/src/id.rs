use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

use crate::error::RuntimeError;

/// Strongly-typed identifier for a runtime implementation class/type (e.g. "fake-test", "opencode", "claude-code").
#[derive(Clone, Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
pub struct RuntimeImplementationId(String);

impl RuntimeImplementationId {
    pub fn new(s: impl Into<String>) -> Result<Self, RuntimeError> {
        let s = s.into();
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err(RuntimeError::invalid_id(
                "RuntimeImplementationId cannot be empty",
            ));
        }

        // Validate character set: ASCII alphanumeric, '-', '_'
        if !trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(RuntimeError::invalid_id(format!(
                "RuntimeImplementationId '{trimmed}' contains invalid characters (allowed: [a-zA-Z0-9_-])"
            )));
        }

        Ok(Self(trimmed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RuntimeImplementationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for RuntimeImplementationId {
    type Err = RuntimeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl Serialize for RuntimeImplementationId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for RuntimeImplementationId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::new(s).map_err(serde::de::Error::custom)
    }
}

/// Strongly-typed unique identifier for a configured runtime instance/installation.
/// Formatted with prefix "rt-inst-" to guarantee zero wire collision with ProviderInstanceId or raw UUIDs.
#[derive(Clone, Copy, Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
pub struct RuntimeInstanceId(Uuid);

const INSTANCE_PREFIX: &str = "rt-inst-";

impl RuntimeInstanceId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn generate() -> Self {
        Self::new()
    }

    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub const fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for RuntimeInstanceId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for RuntimeInstanceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", INSTANCE_PREFIX, self.0)
    }
}

impl FromStr for RuntimeInstanceId {
    type Err = RuntimeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let raw = s.strip_prefix(INSTANCE_PREFIX).ok_or_else(|| {
            RuntimeError::invalid_id(format!(
                "RuntimeInstanceId '{s}' must start with prefix '{INSTANCE_PREFIX}'"
            ))
        })?;
        Uuid::parse_str(raw)
            .map(Self)
            .map_err(|e| RuntimeError::invalid_id(format!("Invalid RuntimeInstanceId '{s}': {e}")))
    }
}

impl Serialize for RuntimeInstanceId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for RuntimeInstanceId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(serde::de::Error::custom)
    }
}

/// Strongly-typed unique identifier for an active runtime execution session.
/// Formatted with prefix "rt-sess-" to guarantee unambiguous session correlation.
#[derive(Clone, Copy, Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
pub struct RuntimeSessionId(Uuid);

const SESSION_PREFIX: &str = "rt-sess-";

impl RuntimeSessionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn generate() -> Self {
        Self::new()
    }

    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub const fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for RuntimeSessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for RuntimeSessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", SESSION_PREFIX, self.0)
    }
}

impl FromStr for RuntimeSessionId {
    type Err = RuntimeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let raw = s.strip_prefix(SESSION_PREFIX).ok_or_else(|| {
            RuntimeError::invalid_id(format!(
                "RuntimeSessionId '{s}' must start with prefix '{SESSION_PREFIX}'"
            ))
        })?;
        Uuid::parse_str(raw)
            .map(Self)
            .map_err(|e| RuntimeError::invalid_id(format!("Invalid RuntimeSessionId '{s}': {e}")))
    }
}

impl Serialize for RuntimeSessionId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for RuntimeSessionId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(serde::de::Error::custom)
    }
}

/// Opaque typed reference for an external runtime configuration document or identifier.
#[derive(Clone, Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
pub struct RuntimeConfigRef(String);

impl RuntimeConfigRef {
    pub fn new(s: impl Into<String>) -> Result<Self, RuntimeError> {
        let s = s.into();
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err(RuntimeError::InvalidConfiguration {
                reason: crate::error::SanitizedRuntimeMessage::new(
                    "RuntimeConfigRef cannot be empty",
                ),
            });
        }
        Ok(Self(trimmed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RuntimeConfigRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for RuntimeConfigRef {
    type Err = RuntimeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl Serialize for RuntimeConfigRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for RuntimeConfigRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::new(s).map_err(serde::de::Error::custom)
    }
}
