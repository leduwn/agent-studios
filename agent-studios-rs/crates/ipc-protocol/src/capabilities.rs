use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::envelope::ProtocolVersion;
use crate::error::ProtocolError;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    WorktreeIsolation,
    ApprovalRouting,
    EventStreaming,
    EventReplay,
    SecretRedaction,
    HmacAuth,
    ToolExecution,
    DiffInspection,
    #[serde(untagged)]
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityNegotiationRequest {
    pub client_id: String,
    pub supported_versions: Vec<ProtocolVersion>,
    pub required_capabilities: Vec<Capability>,
    pub optional_capabilities: Vec<Capability>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityNegotiationResponse {
    pub selected_version: ProtocolVersion,
    pub agreed_capabilities: Vec<Capability>,
    pub max_payload_bytes: usize,
    pub server_features: Vec<String>,
}

pub struct ServerCapabilityNegotiator {
    pub server_versions: Vec<ProtocolVersion>,
    pub supported_capabilities: HashSet<Capability>,
    pub max_payload_bytes: usize,
}

impl ServerCapabilityNegotiator {
    pub fn new(
        server_versions: Vec<ProtocolVersion>,
        supported_capabilities: HashSet<Capability>,
        max_payload_bytes: usize,
    ) -> Self {
        Self {
            server_versions,
            supported_capabilities,
            max_payload_bytes,
        }
    }

    pub fn default_server() -> Self {
        let mut caps = HashSet::new();
        caps.insert(Capability::WorktreeIsolation);
        caps.insert(Capability::ApprovalRouting);
        caps.insert(Capability::EventStreaming);
        caps.insert(Capability::EventReplay);
        caps.insert(Capability::SecretRedaction);
        caps.insert(Capability::HmacAuth);
        caps.insert(Capability::ToolExecution);
        caps.insert(Capability::DiffInspection);

        Self {
            server_versions: vec![ProtocolVersion::V1_0],
            supported_capabilities: caps,
            max_payload_bytes: 16 * 1024 * 1024, // 16 MiB default
        }
    }

    pub fn negotiate(
        &self,
        request: &CapabilityNegotiationRequest,
    ) -> Result<CapabilityNegotiationResponse, ProtocolError> {
        // 1. Verify version compatibility
        let mut best_version = None;
        for client_v in &request.supported_versions {
            for server_v in &self.server_versions {
                if client_v.major == server_v.major && client_v.minor <= server_v.minor {
                    if let Some(current_best) = best_version {
                        if client_v > &current_best {
                            best_version = Some(*client_v);
                        }
                    } else {
                        best_version = Some(*client_v);
                    }
                }
            }
        }

        let selected_version = best_version.ok_or_else(|| ProtocolError::VersionMismatch {
            expected: format!("{:?}", self.server_versions),
            received: format!("{:?}", request.supported_versions),
        })?;

        // 2. Verify all required capabilities are supported
        for req_cap in &request.required_capabilities {
            if !self.supported_capabilities.contains(req_cap) {
                return Err(ProtocolError::CapabilityMissing(format!(
                    "Server does not support required capability: {:?}",
                    req_cap
                )));
            }
        }

        // 3. Assemble agreed capabilities
        let mut agreed = Vec::new();
        for req_cap in &request.required_capabilities {
            agreed.push(req_cap.clone());
        }
        for opt_cap in &request.optional_capabilities {
            if self.supported_capabilities.contains(opt_cap) && !agreed.contains(opt_cap) {
                agreed.push(opt_cap.clone());
            }
        }

        Ok(CapabilityNegotiationResponse {
            selected_version,
            agreed_capabilities: agreed,
            max_payload_bytes: self.max_payload_bytes,
            server_features: vec!["m13_ipc_foundation".to_string()],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_successful_capability_negotiation() {
        let server = ServerCapabilityNegotiator::default_server();
        let req = CapabilityNegotiationRequest {
            client_id: "client-01".to_string(),
            supported_versions: vec![ProtocolVersion::new(1, 0, 0)],
            required_capabilities: vec![Capability::WorktreeIsolation, Capability::ApprovalRouting],
            optional_capabilities: vec![
                Capability::EventReplay,
                Capability::Custom("unsupported_extension".to_string()),
            ],
        };

        let res = server.negotiate(&req).expect("negotiate must succeed");
        assert_eq!(res.selected_version, ProtocolVersion::new(1, 0, 0));
        assert!(
            res.agreed_capabilities
                .contains(&Capability::WorktreeIsolation)
        );
        assert!(
            res.agreed_capabilities
                .contains(&Capability::ApprovalRouting)
        );
        assert!(res.agreed_capabilities.contains(&Capability::EventReplay));
        assert!(
            !res.agreed_capabilities
                .contains(&Capability::Custom("unsupported_extension".to_string()))
        );
    }

    #[test]
    fn test_missing_required_capability_fails() {
        let server = ServerCapabilityNegotiator::default_server();
        let req = CapabilityNegotiationRequest {
            client_id: "client-01".to_string(),
            supported_versions: vec![ProtocolVersion::new(1, 0, 0)],
            required_capabilities: vec![Capability::Custom("mandatory_missing_cap".to_string())],
            optional_capabilities: vec![],
        };

        let err = server.negotiate(&req).unwrap_err();
        assert!(err.to_string().contains("Required capability missing"));
    }

    #[test]
    fn test_incompatible_version_fails() {
        let server = ServerCapabilityNegotiator::default_server();
        let req = CapabilityNegotiationRequest {
            client_id: "client-01".to_string(),
            supported_versions: vec![ProtocolVersion::new(2, 0, 0)],
            required_capabilities: vec![],
            optional_capabilities: vec![],
        };

        let err = server.negotiate(&req).unwrap_err();
        assert!(err.to_string().contains("version mismatch"));
    }
}
