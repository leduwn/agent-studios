use serde::{Deserialize, Serialize};

use crate::approval::ApprovalDecision;
use crate::capabilities::CapabilityNegotiationRequest;
use crate::error::ProtocolError;
use crate::id::{ApprovalId, SubscriptionId};

pub const DEFAULT_MAX_FRAME_SIZE: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessMode {
    ReadOnly,
    Mutating,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "payload")]
pub enum IpcCommand {
    Ping,
    Pong,
    Negotiate(CapabilityNegotiationRequest),
    SubmitGoal {
        goal: String,
        access_mode: AccessMode,
        workspace_root: String,
        options: serde_json::Value,
    },
    CancelTask {
        #[serde(default)]
        studio_id: Option<String>,
        task_id: String,
        reason: Option<String>,
    },
    ResolveApproval {
        approval_id: ApprovalId,
        decision: ApprovalDecision,
    },
    SubscribeEvents {
        from_sequence: Option<u64>,
    },
    UnsubscribeEvents {
        subscription_id: SubscriptionId,
    },
    ReplayEvents {
        from_sequence: u64,
        limit: Option<usize>,
    },
    ListStudios,
    GetStudio {
        studio_id: String,
    },
    SubmitTask {
        studio_id: String,
        task_spec: serde_json::Value,
    },
    ListTasks {
        studio_id: String,
    },
    GetTask {
        studio_id: String,
        task_id: String,
    },
    ListRuns {
        studio_id: String,
    },
    GetRun {
        studio_id: String,
        run_id: String,
    },
    ListProviders,
    GetProvider {
        provider_id: String,
    },
    ListModels {
        #[serde(default)]
        instance_id: Option<String>,
    },
    Custom {
        action: String,
        payload: serde_json::Value,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidRequest,
    Unauthorized,
    Forbidden,
    NotFound,
    VersionMismatch,
    CapabilityMissing,
    InternalError,
    Timeout,
    ApprovalExpired,
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcErrorResponse {
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    pub retryable: bool,
}

impl IpcErrorResponse {
    pub fn new(code: ErrorCode, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
            retryable,
        }
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum IpcResponse {
    Success {
        result: serde_json::Value,
    },
    Error {
        #[serde(flatten)]
        error: IpcErrorResponse,
    },
}

pub struct FrameCodec;

impl FrameCodec {
    pub const DEFAULT_MAX_FRAME_SIZE: usize = DEFAULT_MAX_FRAME_SIZE;

    pub fn encode_frame(payload: &[u8], max_size: usize) -> Result<Vec<u8>, ProtocolError> {
        if payload.len() > max_size {
            return Err(ProtocolError::FrameTooLarge {
                size: payload.len(),
                max: max_size,
            });
        }

        let len_u32 = u32::try_from(payload.len()).map_err(|_| ProtocolError::FrameTooLarge {
            size: payload.len(),
            max: max_size,
        })?;

        let mut frame = Vec::with_capacity(4 + payload.len());
        frame.extend_from_slice(&len_u32.to_be_bytes());
        frame.extend_from_slice(payload);
        Ok(frame)
    }

    pub fn decode_frame(
        buffer: &mut Vec<u8>,
        max_size: usize,
    ) -> Result<Option<Vec<u8>>, ProtocolError> {
        if buffer.len() < 4 {
            return Ok(None);
        }

        let frame_len = u32::from_be_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]) as usize;
        if frame_len > max_size {
            return Err(ProtocolError::FrameTooLarge {
                size: frame_len,
                max: max_size,
            });
        }

        if buffer.len() < 4 + frame_len {
            return Ok(None);
        }

        let payload = buffer[4..4 + frame_len].to_vec();
        buffer.drain(..4 + frame_len);
        Ok(Some(payload))
    }
}

pub fn encode_frame(payload: &[u8], max_size: usize) -> Result<Vec<u8>, ProtocolError> {
    FrameCodec::encode_frame(payload, max_size)
}

pub fn decode_frame(
    buffer: &mut Vec<u8>,
    max_size: usize,
) -> Result<Option<Vec<u8>>, ProtocolError> {
    FrameCodec::decode_frame(buffer, max_size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipc_command_serde() {
        let cmd = IpcCommand::Ping;
        let json = serde_json::to_string(&cmd).expect("serialize");
        let decoded: IpcCommand = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(cmd, decoded);

        let submit = IpcCommand::SubmitGoal {
            goal: "refactor module".to_string(),
            access_mode: AccessMode::Mutating,
            workspace_root: "/repo".to_string(),
            options: serde_json::json!({ "auto_commit": false }),
        };
        let json = serde_json::to_string(&submit).expect("serialize");
        let decoded: IpcCommand = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(submit, decoded);

        let resolve = IpcCommand::ResolveApproval {
            approval_id: ApprovalId::generate(),
            decision: ApprovalDecision::Approved {
                modified_details: Some(serde_json::json!({ "override": true })),
                user_note: Some("approved by lead".to_string()),
            },
        };
        let json = serde_json::to_string(&resolve).expect("serialize");
        let decoded: IpcCommand = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(resolve, decoded);

        let list_studios = IpcCommand::ListStudios;
        let json = serde_json::to_string(&list_studios).expect("serialize");
        let decoded: IpcCommand = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(list_studios, decoded);

        let submit_task = IpcCommand::SubmitTask {
            studio_id: "studio-1".to_string(),
            task_spec: serde_json::json!({ "key": "spec_1", "title": "Spec Task" }),
        };
        let json = serde_json::to_string(&submit_task).expect("serialize");
        let decoded: IpcCommand = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(submit_task, decoded);

        let cancel_task = IpcCommand::CancelTask {
            studio_id: Some("studio-1".to_string()),
            task_id: "task-1".to_string(),
            reason: Some("cancelled by user".to_string()),
        };
        let json = serde_json::to_string(&cancel_task).expect("serialize");
        let decoded: IpcCommand = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(cancel_task, decoded);

        let list_models = IpcCommand::ListModels {
            instance_id: Some("prov-inst-1".to_string()),
        };
        let json = serde_json::to_string(&list_models).expect("serialize");
        let decoded: IpcCommand = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(list_models, decoded);
    }

    #[test]
    fn test_ipc_response_serde() {
        let success = IpcResponse::Success {
            result: serde_json::json!({ "status": "ok", "task_id": "task_1" }),
        };
        let json = serde_json::to_string(&success).expect("serialize");
        let decoded: IpcResponse = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(success, decoded);

        let err = IpcResponse::Error {
            error: IpcErrorResponse::new(ErrorCode::Unauthorized, "Missing auth token", false),
        };
        let json = serde_json::to_string(&err).expect("serialize");
        let decoded: IpcResponse = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(err, decoded);
    }

    #[test]
    fn test_frame_codec_roundtrip() {
        let payload = b"hello agent studios frame codec";
        let encoded = FrameCodec::encode_frame(payload, DEFAULT_MAX_FRAME_SIZE).unwrap();

        assert_eq!(encoded.len(), 4 + payload.len());
        let expected_len = (payload.len() as u32).to_be_bytes();
        assert_eq!(&encoded[0..4], &expected_len);
        assert_eq!(&encoded[4..], payload);

        let mut buffer = encoded;
        let decoded = FrameCodec::decode_frame(&mut buffer, DEFAULT_MAX_FRAME_SIZE).unwrap();
        assert_eq!(decoded, Some(payload.to_vec()));
        assert!(buffer.is_empty());

        // Back-to-back multiple frames
        let frame1 = FrameCodec::encode_frame(b"frame 1", DEFAULT_MAX_FRAME_SIZE).unwrap();
        let frame2 = FrameCodec::encode_frame(b"frame 2", DEFAULT_MAX_FRAME_SIZE).unwrap();
        let mut stream = Vec::new();
        stream.extend_from_slice(&frame1);
        stream.extend_from_slice(&frame2);

        let dec1 = FrameCodec::decode_frame(&mut stream, DEFAULT_MAX_FRAME_SIZE).unwrap();
        assert_eq!(dec1, Some(b"frame 1".to_vec()));
        assert_eq!(stream.len(), frame2.len());

        let dec2 = FrameCodec::decode_frame(&mut stream, DEFAULT_MAX_FRAME_SIZE).unwrap();
        assert_eq!(dec2, Some(b"frame 2".to_vec()));
        assert!(stream.is_empty());
    }

    #[test]
    fn test_frame_codec_exceeds_max_size_rejected_before_allocation() {
        let max_size = 32;
        let large_payload = vec![0x42; 64];

        // Encoding oversized frame fails
        let encode_err = FrameCodec::encode_frame(&large_payload, max_size);
        assert_eq!(
            encode_err.unwrap_err(),
            ProtocolError::FrameTooLarge {
                size: 64,
                max: max_size,
            }
        );

        // Decoding header that claims size > max_size rejected immediately without allocating
        let mut buffer = Vec::new();
        let declared_size = 100u32;
        buffer.extend_from_slice(&declared_size.to_be_bytes());
        buffer.extend_from_slice(b"small partial data");

        let decode_err = FrameCodec::decode_frame(&mut buffer, max_size);
        assert_eq!(
            decode_err.unwrap_err(),
            ProtocolError::FrameTooLarge {
                size: 100,
                max: max_size,
            }
        );
        // Buffer remains intact
        assert_eq!(buffer.len(), 4 + b"small partial data".len());
    }

    #[test]
    fn test_frame_codec_truncated_partial_frame() {
        let max_size = 1024;

        // Empty buffer
        let mut empty_buf = Vec::new();
        assert_eq!(
            FrameCodec::decode_frame(&mut empty_buf, max_size).unwrap(),
            None
        );

        // Fewer than 4 bytes (partial header)
        for len in 1..4 {
            let mut buf = vec![0u8; len];
            assert_eq!(FrameCodec::decode_frame(&mut buf, max_size).unwrap(), None);
            assert_eq!(buf.len(), len);
        }

        // Header says 10 bytes payload, but only 5 available
        let mut partial = Vec::new();
        partial.extend_from_slice(&(10u32).to_be_bytes());
        partial.extend_from_slice(b"12345");
        assert_eq!(
            FrameCodec::decode_frame(&mut partial, max_size).unwrap(),
            None
        );
        assert_eq!(partial.len(), 9);

        // Supply the remaining 5 bytes
        partial.extend_from_slice(b"67890");
        let decoded = FrameCodec::decode_frame(&mut partial, max_size).unwrap();
        assert_eq!(decoded, Some(b"1234567890".to_vec()));
        assert!(partial.is_empty());
    }

    #[test]
    fn test_frame_codec_malformed_length_prefix() {
        let max_size = 100;

        // Max possible u32 length prefix
        let mut malformed = Vec::new();
        malformed.extend_from_slice(&u32::MAX.to_be_bytes());
        let err = FrameCodec::decode_frame(&mut malformed, max_size);
        assert_eq!(
            err.unwrap_err(),
            ProtocolError::FrameTooLarge {
                size: u32::MAX as usize,
                max: max_size,
            }
        );

        // Exact boundary: max_size frame accepted
        let exact_payload = vec![1u8; max_size];
        let mut exact_frame = FrameCodec::encode_frame(&exact_payload, max_size).unwrap();
        let decoded = FrameCodec::decode_frame(&mut exact_frame, max_size).unwrap();
        assert_eq!(decoded, Some(exact_payload));

        // Exact boundary + 1 rejected
        let too_big_payload = vec![1u8; max_size + 1];
        let err2 = FrameCodec::encode_frame(&too_big_payload, max_size);
        assert_eq!(
            err2.unwrap_err(),
            ProtocolError::FrameTooLarge {
                size: max_size + 1,
                max: max_size,
            }
        );
    }

    #[test]
    fn test_frame_codec_invalid_json_payload_handling() {
        // Frame codec handles raw binary payloads without UTF-8 / JSON assumptions
        let non_utf8_payload = vec![0xFF, 0xFE, 0xFD, 0x00, 0xAA];
        let mut frame =
            FrameCodec::encode_frame(&non_utf8_payload, DEFAULT_MAX_FRAME_SIZE).unwrap();
        let decoded = FrameCodec::decode_frame(&mut frame, DEFAULT_MAX_FRAME_SIZE)
            .unwrap()
            .unwrap();
        assert_eq!(decoded, non_utf8_payload);

        // Deserializing this raw frame into an IpcCommand fails at the JSON layer
        let parse_result: Result<IpcCommand, _> = serde_json::from_slice(&decoded);
        assert!(parse_result.is_err());

        // Valid IpcCommand framed and parsed succeeds
        let cmd = IpcCommand::SubmitGoal {
            goal: "test goal".to_string(),
            access_mode: AccessMode::ReadOnly,
            workspace_root: "/test".to_string(),
            options: serde_json::json!({}),
        };
        let cmd_bytes = serde_json::to_vec(&cmd).unwrap();
        let mut cmd_frame = FrameCodec::encode_frame(&cmd_bytes, DEFAULT_MAX_FRAME_SIZE).unwrap();
        let recovered_bytes = FrameCodec::decode_frame(&mut cmd_frame, DEFAULT_MAX_FRAME_SIZE)
            .unwrap()
            .unwrap();
        let recovered_cmd: IpcCommand = serde_json::from_slice(&recovered_bytes).unwrap();
        assert_eq!(cmd, recovered_cmd);
    }
}
