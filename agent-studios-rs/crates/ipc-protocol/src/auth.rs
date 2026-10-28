use chrono::Utc;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Mutex;
use subtle::ConstantTimeEq;

use crate::error::ProtocolError;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HmacAuthHeader {
    pub key_id: String,
    pub nonce: String,
    pub timestamp_millis: i64,
    pub signature: String,
}

impl HmacAuthHeader {
    pub fn new(key_id: String, nonce: String, timestamp_millis: i64, signature: String) -> Self {
        Self {
            key_id,
            nonce,
            timestamp_millis,
            signature,
        }
    }
}

pub struct HmacSigner;

impl HmacSigner {
    pub fn compute_canonical_digest(
        key_id: &str,
        nonce: &str,
        timestamp_millis: i64,
        payload: &[u8],
    ) -> Vec<u8> {
        let payload_hash = hex::encode(Sha256::digest(payload));
        format!("{key_id}:{nonce}:{timestamp_millis}:{payload_hash}").into_bytes()
    }

    pub fn sign(
        secret: &[u8],
        key_id: &str,
        nonce: &str,
        timestamp_millis: i64,
        payload: &[u8],
    ) -> Result<HmacAuthHeader, ProtocolError> {
        let canonical = Self::compute_canonical_digest(key_id, nonce, timestamp_millis, payload);
        let mut mac = HmacSha256::new_from_slice(secret)
            .map_err(|e| ProtocolError::AuthenticationFailed(format!("Invalid secret key: {e}")))?;
        mac.update(&canonical);
        let signature = hex::encode(mac.finalize().into_bytes());

        Ok(HmacAuthHeader {
            key_id: key_id.to_string(),
            nonce: nonce.to_string(),
            timestamp_millis,
            signature,
        })
    }

    pub fn verify(
        secret: &[u8],
        header: &HmacAuthHeader,
        payload: &[u8],
        max_skew_millis: i64,
    ) -> Result<(), ProtocolError> {
        let now = Utc::now().timestamp_millis();
        let skew = (now - header.timestamp_millis).abs();
        if skew > max_skew_millis {
            return Err(ProtocolError::AuthenticationFailed(format!(
                "Timestamp skew {skew}ms exceeds maximum allowed {max_skew_millis}ms"
            )));
        }

        let canonical = Self::compute_canonical_digest(
            &header.key_id,
            &header.nonce,
            header.timestamp_millis,
            payload,
        );

        let mut mac = HmacSha256::new_from_slice(secret)
            .map_err(|e| ProtocolError::AuthenticationFailed(format!("Invalid secret key: {e}")))?;
        mac.update(&canonical);
        let expected_sig = mac.finalize().into_bytes();

        let provided_sig = hex::decode(&header.signature).map_err(|e| {
            ProtocolError::AuthenticationFailed(format!("Invalid signature hex format: {e}"))
        })?;

        if expected_sig.ct_eq(&provided_sig).into() {
            Ok(())
        } else {
            Err(ProtocolError::AuthenticationFailed(
                "HMAC signature mismatch".to_string(),
            ))
        }
    }
}

pub struct NonceTracker {
    seen: Mutex<HashMap<String, i64>>,
    ttl_millis: i64,
}

impl NonceTracker {
    pub fn new(ttl_millis: i64) -> Self {
        Self {
            seen: Mutex::new(HashMap::new()),
            ttl_millis,
        }
    }

    pub fn check_and_record(
        &self,
        nonce: &str,
        timestamp_millis: i64,
    ) -> Result<(), ProtocolError> {
        let now = Utc::now().timestamp_millis();
        let mut lock = self.seen.lock().unwrap();

        // Prune expired entries
        let ttl = self.ttl_millis;
        lock.retain(|_, &mut ts| now - ts < ttl);

        if lock.contains_key(nonce) {
            return Err(ProtocolError::AuthenticationFailed(format!(
                "Replay attack detected: nonce '{nonce}' has already been processed"
            )));
        }

        lock.insert(nonce.to_string(), timestamp_millis);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hmac_sign_and_verify_success() {
        let secret = b"super-secure-shared-key-1234567890";
        let key_id = "key-agent-01";
        let nonce = "nonce-abc-123";
        let now = Utc::now().timestamp_millis();
        let payload = b"{\"command\": \"submit_goal\"}";

        let header = HmacSigner::sign(secret, key_id, nonce, now, payload).unwrap();
        assert_eq!(header.key_id, key_id);
        assert_eq!(header.nonce, nonce);

        // Verify valid
        let verify_result = HmacSigner::verify(secret, &header, payload, 5000);
        assert!(verify_result.is_ok());
    }

    #[test]
    fn test_hmac_verify_tampered_payload_fails() {
        let secret = b"super-secure-shared-key-1234567890";
        let key_id = "key-agent-01";
        let nonce = "nonce-abc-123";
        let now = Utc::now().timestamp_millis();
        let payload = b"{\"command\": \"submit_goal\"}";

        let header = HmacSigner::sign(secret, key_id, nonce, now, payload).unwrap();
        let tampered_payload = b"{\"command\": \"submit_goal\", \"evil\": true}";

        let result = HmacSigner::verify(secret, &header, tampered_payload, 5000);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("signature mismatch")
        );
    }

    #[test]
    fn test_hmac_verify_skew_timeout_fails() {
        let secret = b"super-secure-shared-key-1234567890";
        let key_id = "key-agent-01";
        let nonce = "nonce-abc-123";
        let stale_time = Utc::now().timestamp_millis() - 60_000;
        let payload = b"{\"command\": \"ping\"}";

        let header = HmacSigner::sign(secret, key_id, nonce, stale_time, payload).unwrap();

        // 5s max skew should reject 60s old timestamp
        let result = HmacSigner::verify(secret, &header, payload, 5000);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("exceeds maximum allowed")
        );
    }

    #[test]
    fn test_nonce_replay_protection() {
        let tracker = NonceTracker::new(10_000);
        let now = Utc::now().timestamp_millis();

        // First use is ok
        assert!(tracker.check_and_record("nonce-1", now).is_ok());

        // Replay of same nonce is rejected
        let replay_err = tracker.check_and_record("nonce-1", now);
        assert!(replay_err.is_err());
        assert!(
            replay_err
                .unwrap_err()
                .to_string()
                .contains("Replay attack detected")
        );

        // Different nonce is ok
        assert!(tracker.check_and_record("nonce-2", now).is_ok());
    }
}
