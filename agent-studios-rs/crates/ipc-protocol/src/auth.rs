use chrono::Utc;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Mutex;
use subtle::ConstantTimeEq;

use crate::envelope::ProtocolVersion;
use crate::error::ProtocolError;
use crate::id::SessionId;

type HmacSha256 = Hmac<Sha256>;

/// Typed verification flow binding tying together message parameters, identity, and cryptographic context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthBinding<'a> {
    pub key_id: String,
    pub session_id: SessionId,
    pub nonce: String,
    pub timestamp_millis: i64,
    pub payload: &'a [u8],
    pub protocol_version: ProtocolVersion,
    pub capabilities: Option<Vec<String>>,
}

pub type AuthVerificationFlow<'a> = AuthBinding<'a>;
pub type AuthVerificationBinding<'a> = AuthBinding<'a>;
pub type AuthVerificationContext<'a> = AuthBinding<'a>;

impl<'a> AuthBinding<'a> {
    pub fn new(
        key_id: impl Into<String>,
        session_id: SessionId,
        nonce: impl Into<String>,
        timestamp_millis: i64,
        payload: &'a [u8],
        protocol_version: ProtocolVersion,
        capabilities: Option<Vec<String>>,
    ) -> Self {
        Self {
            key_id: key_id.into(),
            session_id,
            nonce: nonce.into(),
            timestamp_millis,
            payload,
            protocol_version,
            capabilities,
        }
    }

    pub fn canonical_digest(&self) -> Vec<u8> {
        HmacSigner::compute_canonical_digest(
            &self.protocol_version,
            &self.key_id,
            &self.session_id,
            &self.nonce,
            self.timestamp_millis,
            self.capabilities.as_deref(),
            self.payload,
        )
    }

    pub fn compute_canonical_digest(&self) -> Vec<u8> {
        self.canonical_digest()
    }
}

/// Authentication header transmitted in envelope metadata containing identity and HMAC proof.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HmacAuthHeader {
    pub protocol_version: ProtocolVersion,
    pub key_id: String,
    pub session_id: SessionId,
    pub nonce: String,
    pub timestamp_millis: i64,
    pub signature: String,
}

impl HmacAuthHeader {
    pub fn new(
        protocol_version: ProtocolVersion,
        key_id: impl Into<String>,
        session_id: SessionId,
        nonce: impl Into<String>,
        timestamp_millis: i64,
        signature: impl Into<String>,
    ) -> Self {
        Self {
            protocol_version,
            key_id: key_id.into(),
            session_id,
            nonce: nonce.into(),
            timestamp_millis,
            signature: signature.into(),
        }
    }

    pub fn to_binding<'a>(
        &self,
        payload: &'a [u8],
        capabilities: Option<Vec<String>>,
    ) -> AuthBinding<'a> {
        AuthBinding {
            key_id: self.key_id.clone(),
            session_id: self.session_id.clone(),
            nonce: self.nonce.clone(),
            timestamp_millis: self.timestamp_millis,
            payload,
            protocol_version: self.protocol_version,
            capabilities,
        }
    }
}

pub struct HmacSigner;

impl HmacSigner {
    /// Canonical digest format:
    /// `{protocol_version}:{key_id}:{session_id}:{nonce}:{timestamp_millis}:{capabilities_str}:{payload_hash}`
    pub fn compute_canonical_digest(
        protocol_version: &ProtocolVersion,
        key_id: &str,
        session_id: &SessionId,
        nonce: &str,
        timestamp_millis: i64,
        capabilities: Option<&[String]>,
        payload: &[u8],
    ) -> Vec<u8> {
        let payload_hash = hex::encode(Sha256::digest(payload));
        let capabilities_str = match capabilities {
            Some(caps) => {
                let mut sorted: Vec<String> = caps.to_vec();
                sorted.sort();
                sorted.join(",")
            }
            None => String::new(),
        };

        format!("{protocol_version}:{key_id}:{session_id}:{nonce}:{timestamp_millis}:{capabilities_str}:{payload_hash}").into_bytes()
    }

    pub fn compute_canonical_digest_for_binding(binding: &AuthBinding) -> Vec<u8> {
        binding.canonical_digest()
    }

    /// Sign an authentication binding to generate a signed `HmacAuthHeader`.
    pub fn sign(secret: &[u8], binding: &AuthBinding) -> Result<HmacAuthHeader, ProtocolError> {
        let canonical = binding.canonical_digest();
        let mut mac = HmacSha256::new_from_slice(secret)
            .map_err(|e| ProtocolError::AuthenticationFailed(format!("Invalid secret key: {e}")))?;
        mac.update(&canonical);
        let signature = hex::encode(mac.finalize().into_bytes());

        Ok(HmacAuthHeader {
            protocol_version: binding.protocol_version,
            key_id: binding.key_id.clone(),
            session_id: binding.session_id.clone(),
            nonce: binding.nonce.clone(),
            timestamp_millis: binding.timestamp_millis,
            signature,
        })
    }

    /// Sign individual parameters into a `HmacAuthHeader`.
    #[allow(clippy::too_many_arguments)]
    pub fn sign_parts(
        secret: &[u8],
        protocol_version: ProtocolVersion,
        key_id: &str,
        session_id: &SessionId,
        nonce: &str,
        timestamp_millis: i64,
        payload: &[u8],
        capabilities: Option<Vec<String>>,
    ) -> Result<HmacAuthHeader, ProtocolError> {
        let binding = AuthBinding::new(
            key_id,
            session_id.clone(),
            nonce,
            timestamp_millis,
            payload,
            protocol_version,
            capabilities,
        );
        Self::sign(secret, &binding)
    }

    /// Verify authentication header against payload and capabilities.
    ///
    /// Flow:
    /// 1. Constant-time verification (`subtle::ConstantTimeEq`) of HMAC-SHA256 signature FIRST.
    /// 2. Timestamp skew validation (`abs(now - timestamp) <= max_skew_millis`).
    /// 3. Atomically check and consume nonce in `NonceTracker` within identity scope `(key_id, session_id)`.
    pub fn verify(
        secret: &[u8],
        header: &HmacAuthHeader,
        payload: &[u8],
        capabilities: Option<&[String]>,
        max_skew_millis: i64,
        nonce_tracker: &NonceTracker,
    ) -> Result<(), ProtocolError> {
        Self::verify_internal(
            secret,
            header,
            None,
            payload,
            capabilities,
            max_skew_millis,
            nonce_tracker,
        )
    }

    /// Verify authentication header with explicit session ID check.
    pub fn verify_with_session(
        secret: &[u8],
        header: &HmacAuthHeader,
        expected_session_id: &SessionId,
        payload: &[u8],
        capabilities: Option<&[String]>,
        max_skew_millis: i64,
        nonce_tracker: &NonceTracker,
    ) -> Result<(), ProtocolError> {
        Self::verify_internal(
            secret,
            header,
            Some(expected_session_id),
            payload,
            capabilities,
            max_skew_millis,
            nonce_tracker,
        )
    }

    /// Verify an `AuthBinding` directly with signature.
    pub fn verify_binding(
        secret: &[u8],
        binding: &AuthBinding,
        signature: &str,
        max_skew_millis: i64,
        nonce_tracker: &NonceTracker,
    ) -> Result<(), ProtocolError> {
        let header = HmacAuthHeader {
            protocol_version: binding.protocol_version,
            key_id: binding.key_id.clone(),
            session_id: binding.session_id.clone(),
            nonce: binding.nonce.clone(),
            timestamp_millis: binding.timestamp_millis,
            signature: signature.to_string(),
        };
        Self::verify_internal(
            secret,
            &header,
            Some(&binding.session_id),
            binding.payload,
            binding.capabilities.as_deref(),
            max_skew_millis,
            nonce_tracker,
        )
    }

    fn verify_internal(
        secret: &[u8],
        header: &HmacAuthHeader,
        expected_session_id: Option<&SessionId>,
        payload: &[u8],
        capabilities: Option<&[String]>,
        max_skew_millis: i64,
        nonce_tracker: &NonceTracker,
    ) -> Result<(), ProtocolError> {
        // 1. Constant-time verification of HMAC-SHA256 signature FIRST
        let provided_sig = hex::decode(&header.signature).map_err(|e| {
            ProtocolError::AuthenticationFailed(format!("Invalid signature hex format: {e}"))
        })?;

        let canonical = Self::compute_canonical_digest(
            &header.protocol_version,
            &header.key_id,
            &header.session_id,
            &header.nonce,
            header.timestamp_millis,
            capabilities,
            payload,
        );

        let mut mac = HmacSha256::new_from_slice(secret)
            .map_err(|e| ProtocolError::AuthenticationFailed(format!("Invalid secret key: {e}")))?;
        mac.update(&canonical);
        let expected_sig = mac.finalize().into_bytes();

        if !bool::from(expected_sig.ct_eq(&provided_sig[..])) {
            return Err(ProtocolError::AuthenticationFailed(
                "HMAC signature mismatch".to_string(),
            ));
        }

        // 2. Validate session ID if expected session specified
        if expected_session_id.is_some_and(|expected| header.session_id != *expected) {
            let expected = expected_session_id.unwrap();
            return Err(ProtocolError::AuthenticationFailed(format!(
                "Session ID mismatch: expected {expected}, received {}",
                header.session_id
            )));
        }

        // 3. Timestamp skew validation
        let now = Utc::now().timestamp_millis();
        let skew = (now - header.timestamp_millis).abs();
        if skew > max_skew_millis {
            return Err(ProtocolError::AuthenticationFailed(format!(
                "Timestamp skew {skew}ms exceeds maximum allowed {max_skew_millis}ms"
            )));
        }

        // 4. Atomically check and consume the nonce in NonceTracker within authenticated identity scope
        nonce_tracker.check_and_record(
            &header.key_id,
            &header.session_id,
            &header.nonce,
            header.timestamp_millis,
        )?;

        Ok(())
    }
}

/// Memory-bounded replay cache tracking nonces scoped by `(key_id, session_id, nonce)`.
pub struct NonceTracker {
    seen: Mutex<HashMap<(String, SessionId, String), i64>>,
    ttl_millis: i64,
    max_capacity: usize,
}

impl NonceTracker {
    pub fn new(ttl_millis: i64, max_capacity: usize) -> Self {
        Self {
            seen: Mutex::new(HashMap::with_capacity(max_capacity.min(1024))),
            ttl_millis,
            max_capacity,
        }
    }

    /// Atomically check if the scoped nonce exists and record it.
    /// Prunes expired nonces according to TTL, then enforces `max_capacity` by evicting oldest entry.
    pub fn check_and_record(
        &self,
        key_id: &str,
        session_id: &SessionId,
        nonce: &str,
        timestamp_millis: i64,
    ) -> Result<(), ProtocolError> {
        let now = Utc::now().timestamp_millis();
        let mut lock = self.seen.lock().unwrap_or_else(|e| e.into_inner());

        // 1. Prune expired entries based on TTL
        let ttl = self.ttl_millis;
        lock.retain(|_, &mut ts| now.saturating_sub(ts) < ttl);

        // 2. Key nonces by scoped identity: (key_id, session_id, nonce)
        let key = (key_id.to_string(), session_id.clone(), nonce.to_string());
        if lock.contains_key(&key) {
            return Err(ProtocolError::AuthenticationFailed(
                "Replay attack detected: nonce has already been processed".to_string(),
            ));
        }

        // 3. Enforce max_capacity: if capacity is zero, fail closed
        if self.max_capacity == 0 {
            return Err(ProtocolError::AuthenticationFailed(
                "Nonce tracker capacity exceeded: capacity is zero".to_string(),
            ));
        }

        // 4. If size reaches capacity after pruning, evict oldest entry
        if lock.len() >= self.max_capacity {
            let oldest_key = lock
                .iter()
                .min_by_key(|(_, ts)| *ts)
                .map(|(k, _)| k.clone());
            if let Some(key) = oldest_key {
                lock.remove(&key);
            }
        }

        lock.insert(key, timestamp_millis);
        Ok(())
    }

    pub fn check_and_record_binding(&self, binding: &AuthBinding) -> Result<(), ProtocolError> {
        self.check_and_record(
            &binding.key_id,
            &binding.session_id,
            &binding.nonce,
            binding.timestamp_millis,
        )
    }

    pub fn len(&self) -> usize {
        let lock = self.seen.lock().unwrap_or_else(|e| e.into_inner());
        lock.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn contains(&self, key_id: &str, session_id: &SessionId, nonce: &str) -> bool {
        let lock = self.seen.lock().unwrap_or_else(|e| e.into_inner());
        let key = (key_id.to_string(), session_id.clone(), nonce.to_string());
        lock.contains_key(&key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};
    use std::thread;

    #[test]
    fn test_hmac_sign_and_verify_success() {
        let secret = b"super-secure-shared-key-1234567890";
        let key_id = "key-agent-01";
        let session_id = SessionId::generate();
        let nonce = "nonce-abc-123";
        let now = Utc::now().timestamp_millis();
        let payload = b"{\"command\": \"submit_goal\"}";
        let caps = vec!["read".to_string(), "write".to_string()];

        let binding = AuthBinding::new(
            key_id,
            session_id.clone(),
            nonce,
            now,
            payload,
            ProtocolVersion::CURRENT,
            Some(caps.clone()),
        );

        let header = HmacSigner::sign(secret, &binding).expect("sign failed");
        assert_eq!(header.protocol_version, ProtocolVersion::CURRENT);
        assert_eq!(header.key_id, key_id);
        assert_eq!(header.session_id, session_id);
        assert_eq!(header.nonce, nonce);
        assert_eq!(header.timestamp_millis, now);

        let tracker = NonceTracker::new(10_000, 100);

        // Verify valid signature and consume nonce
        let verify_result =
            HmacSigner::verify(secret, &header, payload, Some(&caps[..]), 5000, &tracker);
        assert!(verify_result.is_ok());

        // Verify nonce was recorded
        assert!(tracker.contains(key_id, &session_id, nonce));
    }

    #[test]
    fn test_hmac_verify_duplicate_request_rejection() {
        let secret = b"super-secure-shared-key-1234567890";
        let key_id = "key-agent-01";
        let session_id = SessionId::generate();
        let nonce = "nonce-duplicate-123";
        let now = Utc::now().timestamp_millis();
        let payload = b"{\"command\": \"ping\"}";

        let binding = AuthBinding::new(
            key_id,
            session_id.clone(),
            nonce,
            now,
            payload,
            ProtocolVersion::CURRENT,
            None,
        );

        let header = HmacSigner::sign(secret, &binding).expect("sign failed");
        let tracker = NonceTracker::new(10_000, 100);

        // First verification succeeds
        assert!(HmacSigner::verify(secret, &header, payload, None, 5000, &tracker).is_ok());

        // Second verification with identical header fails
        let replay_err = HmacSigner::verify(secret, &header, payload, None, 5000, &tracker);
        assert!(replay_err.is_err());
        let err_msg = replay_err.unwrap_err().to_string();
        assert!(err_msg.contains("Replay attack detected"));
        // Untrusted nonce must not be in error message
        assert!(!err_msg.contains(nonce));
    }

    #[test]
    fn test_hmac_concurrent_replay_attempts() {
        let secret = b"super-secure-shared-key-1234567890";
        let key_id = "key-agent-concurrent";
        let session_id = SessionId::generate();
        let nonce = "nonce-concurrent-456";
        let now = Utc::now().timestamp_millis();
        let payload = b"{\"command\": \"execute_step\"}";

        let binding = AuthBinding::new(
            key_id,
            session_id.clone(),
            nonce,
            now,
            payload,
            ProtocolVersion::CURRENT,
            None,
        );

        let header = Arc::new(HmacSigner::sign(secret, &binding).expect("sign failed"));
        let tracker = Arc::new(NonceTracker::new(10_000, 100));

        let num_threads = 20;
        let barrier = Arc::new(Barrier::new(num_threads));
        let mut handles = Vec::new();

        for _ in 0..num_threads {
            let h = Arc::clone(&header);
            let t = Arc::clone(&tracker);
            let b = Arc::clone(&barrier);

            handles.push(thread::spawn(move || {
                b.wait();
                HmacSigner::verify(secret, &h, payload, None, 5000, &t)
            }));
        }

        let mut successes = 0;
        let mut failures = 0;

        for handle in handles {
            let res = handle.join().expect("thread panicked");
            match res {
                Ok(()) => successes += 1,
                Err(e) => {
                    assert!(e.to_string().contains("Replay attack detected"));
                    failures += 1;
                }
            }
        }

        assert_eq!(successes, 1, "Exactly one thread must succeed");
        assert_eq!(
            failures,
            num_threads - 1,
            "All other concurrent attempts must be rejected"
        );
    }

    #[test]
    fn test_hmac_verify_stale_and_future_timestamp_rejection() {
        let secret = b"super-secure-shared-key-1234567890";
        let key_id = "key-agent-skew";
        let session_id = SessionId::generate();
        let tracker = NonceTracker::new(10_000, 100);
        let payload = b"{\"command\": \"ping\"}";

        // Stale timestamp (> 5s skew)
        let stale_time = Utc::now().timestamp_millis() - 60_000;
        let stale_binding = AuthBinding::new(
            key_id,
            session_id.clone(),
            "nonce-stale",
            stale_time,
            payload,
            ProtocolVersion::CURRENT,
            None,
        );
        let stale_header = HmacSigner::sign(secret, &stale_binding).expect("sign");
        let stale_err = HmacSigner::verify(secret, &stale_header, payload, None, 5000, &tracker);
        assert!(stale_err.is_err());
        assert!(
            stale_err
                .unwrap_err()
                .to_string()
                .contains("exceeds maximum allowed")
        );

        // Future timestamp (> 5s skew)
        let future_time = Utc::now().timestamp_millis() + 60_000;
        let future_binding = AuthBinding::new(
            key_id,
            session_id.clone(),
            "nonce-future",
            future_time,
            payload,
            ProtocolVersion::CURRENT,
            None,
        );
        let future_header = HmacSigner::sign(secret, &future_binding).expect("sign");
        let future_err = HmacSigner::verify(secret, &future_header, payload, None, 5000, &tracker);
        assert!(future_err.is_err());
        assert!(
            future_err
                .unwrap_err()
                .to_string()
                .contains("exceeds maximum allowed")
        );
    }

    #[test]
    fn test_hmac_verify_malformed_signature_hex_rejection() {
        let secret = b"super-secure-shared-key-1234567890";
        let key_id = "key-agent-01";
        let session_id = SessionId::generate();
        let tracker = NonceTracker::new(10_000, 100);
        let now = Utc::now().timestamp_millis();
        let payload = b"{\"command\": \"ping\"}";

        let mut header = HmacSigner::sign_parts(
            secret,
            ProtocolVersion::CURRENT,
            key_id,
            &session_id,
            "nonce-malformed",
            now,
            payload,
            None,
        )
        .expect("sign");

        header.signature = "this-is-not-valid-hex-zz!".to_string();

        let res = HmacSigner::verify(secret, &header, payload, None, 5000, &tracker);
        assert!(res.is_err());
        assert!(
            res.unwrap_err()
                .to_string()
                .contains("Invalid signature hex format")
        );
    }

    #[test]
    fn test_hmac_verify_tampered_payload_signature_mismatch() {
        let secret = b"super-secure-shared-key-1234567890";
        let key_id = "key-agent-01";
        let session_id = SessionId::generate();
        let tracker = NonceTracker::new(10_000, 100);
        let now = Utc::now().timestamp_millis();
        let payload = b"{\"command\": \"submit_goal\"}";
        let tampered_payload = b"{\"command\": \"submit_goal\", \"evil\": true}";

        let header = HmacSigner::sign_parts(
            secret,
            ProtocolVersion::CURRENT,
            key_id,
            &session_id,
            "nonce-tamper",
            now,
            payload,
            None,
        )
        .expect("sign");

        let res = HmacSigner::verify(secret, &header, tampered_payload, None, 5000, &tracker);
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("signature mismatch"));
    }

    #[test]
    fn test_hmac_verify_wrong_key_rejection() {
        let secret = b"super-secure-shared-key-1234567890";
        let wrong_secret = b"wrong-secret-key-09876543210987654";
        let key_id = "key-agent-01";
        let session_id = SessionId::generate();
        let tracker = NonceTracker::new(10_000, 100);
        let now = Utc::now().timestamp_millis();
        let payload = b"{\"command\": \"submit_goal\"}";

        let header = HmacSigner::sign_parts(
            secret,
            ProtocolVersion::CURRENT,
            key_id,
            &session_id,
            "nonce-key",
            now,
            payload,
            None,
        )
        .expect("sign");

        let res = HmacSigner::verify(wrong_secret, &header, payload, None, 5000, &tracker);
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("signature mismatch"));
    }

    #[test]
    fn test_hmac_verify_session_id_mismatch_rejection() {
        let secret = b"super-secure-shared-key-1234567890";
        let key_id = "key-agent-01";
        let session_1 = SessionId::generate();
        let session_2 = SessionId::generate();
        let tracker = NonceTracker::new(10_000, 100);
        let now = Utc::now().timestamp_millis();
        let payload = b"{\"command\": \"submit_goal\"}";

        let header = HmacSigner::sign_parts(
            secret,
            ProtocolVersion::CURRENT,
            key_id,
            &session_1,
            "nonce-session",
            now,
            payload,
            None,
        )
        .expect("sign");

        // Verification against expected session_2 fails session ID check
        let mismatch_res = HmacSigner::verify_with_session(
            secret, &header, &session_2, payload, None, 5000, &tracker,
        );
        assert!(mismatch_res.is_err());
        assert!(
            mismatch_res
                .unwrap_err()
                .to_string()
                .contains("Session ID mismatch")
        );

        // If attacker mutates header.session_id, HMAC signature check fails first
        let mut tampered_header = header.clone();
        tampered_header.session_id = session_2.clone();
        let tampered_res =
            HmacSigner::verify(secret, &tampered_header, payload, None, 5000, &tracker);
        assert!(tampered_res.is_err());
        assert!(
            tampered_res
                .unwrap_err()
                .to_string()
                .contains("signature mismatch")
        );
    }

    #[test]
    fn test_nonce_tracker_capacity_bound_enforcement() {
        let key_id = "key-agent-capacity";
        let session_id = SessionId::generate();
        let now = Utc::now().timestamp_millis();

        // Capacity 3
        let tracker = NonceTracker::new(10_000, 3);
        assert_eq!(tracker.len(), 0);

        // Insert 3 items
        tracker
            .check_and_record(key_id, &session_id, "nonce-1", now)
            .unwrap();
        tracker
            .check_and_record(key_id, &session_id, "nonce-2", now + 1)
            .unwrap();
        tracker
            .check_and_record(key_id, &session_id, "nonce-3", now + 2)
            .unwrap();
        assert_eq!(tracker.len(), 3);

        // Insert 4th item -> evicts oldest ("nonce-1")
        tracker
            .check_and_record(key_id, &session_id, "nonce-4", now + 3)
            .unwrap();
        assert_eq!(tracker.len(), 3);
        assert!(!tracker.contains(key_id, &session_id, "nonce-1"));
        assert!(tracker.contains(key_id, &session_id, "nonce-2"));
        assert!(tracker.contains(key_id, &session_id, "nonce-3"));
        assert!(tracker.contains(key_id, &session_id, "nonce-4"));

        // Insert 5th item -> evicts next oldest ("nonce-2")
        tracker
            .check_and_record(key_id, &session_id, "nonce-5", now + 4)
            .unwrap();
        assert_eq!(tracker.len(), 3);
        assert!(!tracker.contains(key_id, &session_id, "nonce-2"));

        // Zero capacity fails closed
        let zero_tracker = NonceTracker::new(10_000, 0);
        let zero_res = zero_tracker.check_and_record(key_id, &session_id, "nonce-z", now);
        assert!(zero_res.is_err());
        assert!(
            zero_res
                .unwrap_err()
                .to_string()
                .contains("capacity is zero")
        );

        // TTL pruning
        let ttl_tracker = NonceTracker::new(100, 10);
        ttl_tracker
            .check_and_record(key_id, &session_id, "nonce-expired", now - 200)
            .unwrap();
        assert_eq!(ttl_tracker.len(), 1);
        ttl_tracker
            .check_and_record(key_id, &session_id, "nonce-fresh", now)
            .unwrap();
        // Expired entry was pruned
        assert_eq!(ttl_tracker.len(), 1);
        assert!(!ttl_tracker.contains(key_id, &session_id, "nonce-expired"));
        assert!(ttl_tracker.contains(key_id, &session_id, "nonce-fresh"));
    }

    #[test]
    fn test_nonce_tracker_scoped_identity_isolation() {
        let tracker = NonceTracker::new(10_000, 100);
        let key_1 = "key-01";
        let key_2 = "key-02";
        let session_1 = SessionId::generate();
        let session_2 = SessionId::generate();
        let nonce = "nonce-shared";
        let now = Utc::now().timestamp_millis();

        // Same nonce in session 1 is valid
        assert!(
            tracker
                .check_and_record(key_1, &session_1, nonce, now)
                .is_ok()
        );

        // Same nonce in session 2 is also valid (isolated by session)
        assert!(
            tracker
                .check_and_record(key_1, &session_2, nonce, now)
                .is_ok()
        );

        // Same nonce under key 2 is also valid (isolated by key)
        assert!(
            tracker
                .check_and_record(key_2, &session_1, nonce, now)
                .is_ok()
        );

        // Replay in same scope (key_1, session_1, nonce) fails
        assert!(
            tracker
                .check_and_record(key_1, &session_1, nonce, now)
                .is_err()
        );
    }

    #[test]
    fn test_canonical_digest_sorted_capabilities() {
        let session_id = SessionId::generate();
        let version = ProtocolVersion::CURRENT;
        let payload = b"canonical payload";

        // Unsorted vs sorted capability lists must yield exact same digest
        let caps_unsorted = vec!["write".to_string(), "admin".to_string(), "read".to_string()];
        let caps_sorted = vec!["admin".to_string(), "read".to_string(), "write".to_string()];

        let digest_1 = HmacSigner::compute_canonical_digest(
            &version,
            "key-1",
            &session_id,
            "nonce-1",
            1000,
            Some(&caps_unsorted),
            payload,
        );

        let digest_2 = HmacSigner::compute_canonical_digest(
            &version,
            "key-1",
            &session_id,
            "nonce-1",
            1000,
            Some(&caps_sorted),
            payload,
        );

        assert_eq!(digest_1, digest_2);

        // None capabilities vs empty vector yield identical digest
        let digest_none = HmacSigner::compute_canonical_digest(
            &version,
            "key-1",
            &session_id,
            "nonce-1",
            1000,
            None,
            payload,
        );

        let empty_caps: Vec<String> = vec![];
        let digest_empty = HmacSigner::compute_canonical_digest(
            &version,
            "key-1",
            &session_id,
            "nonce-1",
            1000,
            Some(&empty_caps),
            payload,
        );

        assert_eq!(digest_none, digest_empty);
    }
}
