use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

use crate::auth::HmacAuthHeader;
use crate::error::ProtocolError;
use crate::id::{CorrelationId, RequestId};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ProtocolVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl ProtocolVersion {
    pub const V1_0: Self = Self {
        major: 1,
        minor: 0,
        patch: 0,
    };

    pub const CURRENT: Self = Self::V1_0;

    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    pub fn is_compatible_with(&self, other: &Self) -> bool {
        self.major == other.major && self.minor <= other.minor
    }
}

impl fmt::Display for ProtocolVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl fmt::Debug for ProtocolVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ProtocolVersion({})", self)
    }
}

impl FromStr for ProtocolVersion {
    type Err = ProtocolError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 3 {
            return Err(ProtocolError::InvalidEnvelope(format!(
                "Invalid semver version string: '{s}'"
            )));
        }
        let major = parts[0]
            .parse::<u32>()
            .map_err(|e| ProtocolError::InvalidEnvelope(format!("Invalid major version: {e}")))?;
        let minor = parts[1]
            .parse::<u32>()
            .map_err(|e| ProtocolError::InvalidEnvelope(format!("Invalid minor version: {e}")))?;
        let patch = parts[2]
            .parse::<u32>()
            .map_err(|e| ProtocolError::InvalidEnvelope(format!("Invalid patch version: {e}")))?;
        Ok(Self {
            major,
            minor,
            patch,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcEnvelope<T> {
    pub version: ProtocolVersion,
    pub id: RequestId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<CorrelationId>,
    pub timestamp: DateTime<Utc>,
    pub payload: T,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<HmacAuthHeader>,
}

impl<T> IpcEnvelope<T> {
    pub fn new(payload: T) -> Self {
        Self {
            version: ProtocolVersion::CURRENT,
            id: RequestId::generate(),
            correlation_id: None,
            timestamp: Utc::now(),
            payload,
            auth: None,
        }
    }

    pub fn with_correlation(mut self, correlation_id: CorrelationId) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }

    pub fn with_auth(mut self, auth: HmacAuthHeader) -> Self {
        self.auth = Some(auth);
        self
    }

    pub fn validate_version(&self, supported_major: u32) -> Result<(), ProtocolError> {
        if self.version.major != supported_major {
            return Err(ProtocolError::VersionMismatch {
                expected: format!("{supported_major}.x.x"),
                received: self.version.to_string(),
            });
        }
        Ok(())
    }
}

impl<T: Serialize> IpcEnvelope<T> {
    pub fn to_bytes(&self) -> Result<Vec<u8>, ProtocolError> {
        serde_json::to_vec(self).map_err(|e| ProtocolError::Serialization(e.to_string()))
    }
}

impl<T: for<'de> Deserialize<'de>> IpcEnvelope<T> {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ProtocolError> {
        serde_json::from_slice(bytes).map_err(|e| ProtocolError::Serialization(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_envelope_creation_and_serialization() {
        let envelope = IpcEnvelope::new("test_payload".to_string())
            .with_correlation(CorrelationId::generate());

        assert_eq!(envelope.version, ProtocolVersion::CURRENT);
        assert!(envelope.id.as_str().starts_with("req_"));
        assert!(envelope.correlation_id.is_some());

        let bytes = envelope.to_bytes().expect("serialize");
        let decoded: IpcEnvelope<String> = IpcEnvelope::from_bytes(&bytes).expect("deserialize");
        assert_eq!(envelope.id, decoded.id);
        assert_eq!(envelope.payload, decoded.payload);
        assert_eq!(envelope.correlation_id, decoded.correlation_id);
    }

    #[test]
    fn test_version_compatibility() {
        let v1_0 = ProtocolVersion::new(1, 0, 0);
        let v1_2 = ProtocolVersion::new(1, 2, 0);
        let v2_0 = ProtocolVersion::new(2, 0, 0);

        assert!(v1_0.is_compatible_with(&v1_2));
        assert!(!v1_2.is_compatible_with(&v1_0));
        assert!(!v1_0.is_compatible_with(&v2_0));
    }

    #[test]
    fn test_envelope_version_validation() {
        let envelope = IpcEnvelope::new("payload".to_string());
        assert!(envelope.validate_version(1).is_ok());
        assert!(envelope.validate_version(2).is_err());
    }
}
