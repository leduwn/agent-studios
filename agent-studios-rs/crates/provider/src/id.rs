use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

use crate::error::ProviderError;

/// Strongly-typed identifier for a ProviderDefinition (e.g. "openai", "anthropic", "custom-openai").
#[derive(Clone, Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
pub struct ProviderId(String);

impl ProviderId {
    /// Creates a validated ProviderId. Rejects empty, whitespace, or invalid characters.
    pub fn new(s: impl Into<String>) -> Result<Self, ProviderError> {
        let s = s.into();
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err(ProviderError::InvalidProviderId(
                "ProviderId cannot be empty".to_string(),
            ));
        }

        // Validate character set: ASCII alphanumeric, '-', '_', '.'
        if !trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        {
            return Err(ProviderError::InvalidProviderId(format!(
                "ProviderId '{trimmed}' contains invalid characters (allowed: [a-zA-Z0-9._-])"
            )));
        }

        Ok(Self(trimmed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProviderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for ProviderId {
    type Err = ProviderError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl Serialize for ProviderId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ProviderId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::new(s).map_err(serde::de::Error::custom)
    }
}

/// Unique UUID-backed identifier for a configured ProviderInstance.
#[derive(Clone, Copy, Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
pub struct ProviderInstanceId(Uuid);

impl ProviderInstanceId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for ProviderInstanceId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ProviderInstanceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for ProviderInstanceId {
    type Err = ProviderError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(s)
            .map(Self)
            .map_err(|e| ProviderError::InvalidProviderInstanceId(e.to_string()))
    }
}

impl Serialize for ProviderInstanceId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ProviderInstanceId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let uuid = Uuid::deserialize(deserializer)?;
        Ok(Self(uuid))
    }
}

/// Opaque provider-native model identifier (e.g. "gpt-5.6", "claude-sonnet-4-5", "meta-llama-3").
#[derive(Clone, Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
pub struct ModelId(String);

impl ModelId {
    /// Creates a validated ModelId. Rejects empty or whitespace-only identifiers.
    pub fn new(s: impl Into<String>) -> Result<Self, ProviderError> {
        let s = s.into();
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err(ProviderError::InvalidModelId(
                "ModelId cannot be empty".to_string(),
            ));
        }
        Ok(Self(trimmed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ModelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for ModelId {
    type Err = ProviderError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl Serialize for ModelId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ModelId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::new(s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_id_valid() {
        let p = ProviderId::new("openai").unwrap();
        assert_eq!(p.as_str(), "openai");
        assert_eq!(p.to_string(), "openai");

        let p2 = ProviderId::new("custom-openai_v2.0").unwrap();
        assert_eq!(p2.as_str(), "custom-openai_v2.0");
    }

    #[test]
    fn test_provider_id_invalid() {
        assert!(ProviderId::new("").is_err());
        assert!(ProviderId::new("   ").is_err());
        assert!(ProviderId::new("openai with spaces").is_err());
        assert!(ProviderId::new("openai/v1").is_err());
    }

    #[test]
    fn test_model_id_valid() {
        let m = ModelId::new("gpt-5.6").unwrap();
        assert_eq!(m.as_str(), "gpt-5.6");
        assert_eq!(m.to_string(), "gpt-5.6");

        let m2 = ModelId::new("anthropic/claude-sonnet-4-5:latest").unwrap();
        assert_eq!(m2.as_str(), "anthropic/claude-sonnet-4-5:latest");
    }

    #[test]
    fn test_model_id_invalid() {
        assert!(ModelId::new("").is_err());
        assert!(ModelId::new("   ").is_err());
    }

    #[test]
    fn test_provider_instance_id() {
        let id1 = ProviderInstanceId::new();
        let s = id1.to_string();
        let id2: ProviderInstanceId = s.parse().unwrap();
        assert_eq!(id1, id2);
    }
}
