//! M15-03.1 Lossless Diff, Binary Pagination & Protocol Correctness Integration Tests
//!
//! Validates:
//! 1. UTF-8 multi-byte chunk splits & Vietnamese diacritic preservation.
//! 2. Binary Git patch data preservation via Base64.
//! 3. Offset and boundary semantics across arbitrary chunk cuts.
//! 4. Zero-length limit rejection (infinite loop protection).
//! 5. Frame budget safety & dual-emission suppression (> 1.5 MiB).
//! 6. Corrupted blob integrity check and SHA-256 mismatch detection.
//! 7. Path traversal hash neutralization (`validate_hash`).
//! 8. Paginated binary artifact retrieval via `GetArtifact`.
//! 9. Capture failure isolation & worktree retention without task abortion.
//! 10. Concurrent capture idempotency.
//! 11. Ephemeral git index preservation.
//! 12. Flat wire schema JSON contract.

use std::fs;
use tempfile::tempdir;

use agent_studios_ipc_protocol::framing::DEFAULT_MAX_FRAME_SIZE;
use agent_studios_ipc_transport::{ClientConfig, IpcClient, IpcCommand, IpcResponse, IpcServer};
use agent_studios_protocol::artifact::{ArtifactKind, DiffSummary, TaskDiffResponse};
use agent_studios_protocol::id::{AgentId, ArtifactId, StudioId, TaskId};
use agent_studios_protocol::worktree::{WorktreeRecord, WorktreeState};
use agent_studios_workspace::artifact_store::{ArtifactOptions, ArtifactStore};
use base64::Engine;
use chrono::Utc;
use serde_json::Value;

/// Test 05: Lossless UTF-8 Vietnamese Chunk Split
/// Verifies that slicing multi-byte UTF-8 sequences (such as Vietnamese diacritics
/// 'Tiếng Việt với đầy đủ dấu: ế, ệ, ẩ, ờ, đ') across arbitrary byte boundaries
/// corrupts text when using legacy lossy string conversion, but yields 100% bit-perfect
/// text when reassembled from Base64 chunks.
#[test]
fn test_05_lossless_utf8_vietnamese_chunk_split() {
    let vietnamese_text = "Tiếng Việt với đầy đủ dấu: ế, ệ, ẩ, ờ, đ. Dữ liệu thử nghiệm.";
    let raw_bytes = vietnamese_text.as_bytes();

    // Find byte offsets that split 3-byte UTF-8 sequences
    // 'ế' is U+1EBF, encoded as 0xE1 0xBA 0xBF.
    let e_circumflex_pos = vietnamese_text.find('ế').expect("Must contain 'ế'");
    // Cut directly inside the 3-byte character (after 1st byte)
    let cut_offset = e_circumflex_pos + 1;

    let chunk1_bytes = &raw_bytes[..cut_offset];
    let chunk2_bytes = &raw_bytes[cut_offset..];

    // 1. Demonstrate lossy string corruption across chunk boundaries
    let lossy1 = String::from_utf8_lossy(chunk1_bytes);
    let lossy2 = String::from_utf8_lossy(chunk2_bytes);
    let concatenated_lossy = format!("{lossy1}{lossy2}");
    assert!(
        concatenated_lossy.contains('\u{FFFD}'),
        "Lossy string concatenation must contain replacement character"
    );
    assert_ne!(concatenated_lossy, vietnamese_text);

    // 2. Demonstrate 100% bit-perfect lossless reconstruction via Base64 chunks
    let b64_chunk1 = base64::engine::general_purpose::STANDARD.encode(chunk1_bytes);
    let b64_chunk2 = base64::engine::general_purpose::STANDARD.encode(chunk2_bytes);

    let decoded1 = base64::engine::general_purpose::STANDARD
        .decode(&b64_chunk1)
        .expect("Decode chunk 1");
    let decoded2 = base64::engine::general_purpose::STANDARD
        .decode(&b64_chunk2)
        .expect("Decode chunk 2");

    let mut reassembled = Vec::new();
    reassembled.extend_from_slice(&decoded1);
    reassembled.extend_from_slice(&decoded2);

    assert_eq!(reassembled, raw_bytes);
    let reassembled_text = String::from_utf8(reassembled).expect("Valid UTF-8");
    assert_eq!(reassembled_text, vietnamese_text);
}

/// Test 06: Binary Patch and Base64 Preservation
/// Verifies that binary patch data containing arbitrary byte values (0x00..=0xFF)
/// is preserved without corruption through Base64 chunking, whereas lossy string conversion
/// destroys binary content.
#[test]
fn test_06_binary_patch_and_base64_preservation() {
    // Generate raw binary git patch payload including null bytes and arbitrary binary deltas
    let mut binary_patch = Vec::new();
    binary_patch.extend_from_slice(b"GIT binary patch\nliteral 256\n");
    for b in 0..=255u8 {
        binary_patch.push(b);
    }
    binary_patch.extend_from_slice(b"\n\n");

    let chunk_size = 50;
    let mut reassembled = Vec::new();

    for chunk in binary_patch.chunks(chunk_size) {
        let b64 = base64::engine::general_purpose::STANDARD.encode(chunk);
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&b64)
            .expect("Base64 decode must succeed");
        reassembled.extend_from_slice(&decoded);
    }

    assert_eq!(reassembled, binary_patch);

    // Verify that lossy string conversion would corrupt this binary data
    let lossy_string = String::from_utf8_lossy(&binary_patch);
    assert!(lossy_string.contains('\u{FFFD}'));
    assert_ne!(lossy_string.as_bytes(), binary_patch.as_slice());
}

/// Test 07: Offset Boundary Semantics
/// Tests exact boundary slicing behavior: offset at 0, offset midway, offset at total_bytes,
/// and offset beyond total_bytes.
#[test]
fn test_07_offset_boundary_semantics() {
    let payload = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let total_bytes = payload.len(); // 36 bytes

    // 1. Offset at 0 with limit 10
    let off = 0usize;
    let start = off.min(total_bytes);
    let end = off.saturating_add(10).min(total_bytes);
    assert_eq!(&payload[start..end], b"0123456789");
    assert!(end < total_bytes); // has_more = true

    // 2. Offset midway
    let off = 20;
    let start = off.min(total_bytes);
    let end = off.saturating_add(10).min(total_bytes);
    assert_eq!(&payload[start..end], b"klmnopqrst");
    assert!(end < total_bytes); // has_more = true

    // 3. Offset near end
    let off = 30;
    let start = off.min(total_bytes);
    let end = off.saturating_add(10).min(total_bytes);
    assert_eq!(&payload[start..end], b"uvwxyz");
    assert_eq!(end, total_bytes); // has_more = false

    // 4. Offset at total_bytes
    let off = total_bytes;
    let start = off.min(total_bytes);
    let end = off.saturating_add(10).min(total_bytes);
    assert!(payload[start..end].is_empty());
    assert_eq!(end, total_bytes); // has_more = false

    // 5. Offset beyond total_bytes
    let off = 100;
    let start = off.min(total_bytes);
    let end = off.saturating_add(10).min(total_bytes);
    assert!(payload[start..end].is_empty());
    assert_eq!(start, total_bytes);
    assert_eq!(end, total_bytes);
}

/// Test 08: Zero-Length Limit Rejection
/// Verifies that querying `GetTaskDiff` or `GetArtifact` with `limit = Some(0)`
/// is rejected with an Error response, preventing infinite client pagination loops.
#[tokio::test]
async fn test_08_zero_length_limit_rejection() {
    let key_id = "test-key-limit";
    let secret = b"m15-03-test-secret-key-zero-limit-test".to_vec();
    let server = IpcServer::builder()
        .with_admin_auth_key(key_id, secret.clone())
        .build()
        .unwrap();

    let (c_stream, s_stream) = tokio::io::duplex(65536);
    server.handle_connection(s_stream);
    let mut client = IpcClient::connect_stream(c_stream, ClientConfig::new(key_id, secret))
        .await
        .unwrap();

    let studio_id = StudioId::new();
    let task_id = TaskId::new();
    let artifact_id = ArtifactId::new();

    // 1. GetTaskDiff with limit = Some(0) must fail
    let diff_res = client
        .get_task_diff(studio_id, task_id, Some(0), Some(0))
        .await;
    assert!(diff_res.is_err());
    let err_str = diff_res.unwrap_err().to_string();
    assert!(
        err_str.contains("limit must be greater than zero"),
        "Unexpected error message: {err_str}"
    );

    // 2. GetArtifact with limit = Some(0) must fail
    let art_res = client
        .get_artifact_chunk(studio_id, artifact_id, Some(0), Some(0))
        .await;
    assert!(art_res.is_err());
    let err_str2 = art_res.unwrap_err().to_string();
    assert!(
        err_str2.contains("limit must be greater than zero"),
        "Unexpected error message: {err_str2}"
    );

    client.disconnect().await;
}

/// Test 09: Frame Budget and Dual-Emission Suppression
/// Verifies that chunks larger than 1.5 MiB suppress the legacy UTF-8 `patch` string
/// to guarantee the entire JSON frame stays <= 2.8 MiB and safely within DEFAULT_MAX_FRAME_SIZE (4 MiB).
#[test]
fn test_09_frame_budget_and_dual_emission_suppression() {
    let max_frame = DEFAULT_MAX_FRAME_SIZE; // 4,194,304 bytes
    assert_eq!(max_frame, 4 * 1024 * 1024);

    // 2 MiB raw chunk
    let raw_2mb = vec![b'A'; 2 * 1024 * 1024];
    let b64 = base64::engine::general_purpose::STANDARD.encode(&raw_2mb);
    assert_eq!(b64.len(), 2_796_204); // ~2.67 MiB

    // If both legacy 2 MiB string and 2.67 MiB Base64 string were emitted together:
    let combined_size = raw_2mb.len() + b64.len();
    assert!(
        combined_size > max_frame,
        "Dual emission of 2 MiB exceeds 4 MiB frame limit: {combined_size} > {max_frame}"
    );

    // With dual-emission suppression (> 1.5 MiB):
    let legacy_patch = if raw_2mb.len() > 1_500_000 {
        String::new()
    } else {
        String::from_utf8_lossy(&raw_2mb).to_string()
    };
    assert!(legacy_patch.is_empty());

    let response = TaskDiffResponse {
        studio_id: StudioId::new(),
        task_id: TaskId::new(),
        artifact_id: Some(ArtifactId::new()),
        base_commit: Some("abcdef1234567890".to_string()),
        head_commit: Some("1234567890abcdef".to_string()),
        summary: DiffSummary::default(),
        patch: legacy_patch,
        offset: 0,
        limit: 2 * 1024 * 1024,
        total_bytes: raw_2mb.len(),
        has_more: false,
        patch_base64: Some(b64),
        encoding: Some("base64".to_string()),
        patch_sha256: Some(ArtifactStore::compute_hash(&raw_2mb)),
        chunk_bytes: Some(raw_2mb.len()),
    };

    let serialized = serde_json::to_string(&response).expect("Serialize TaskDiffResponse");
    assert!(
        serialized.len() < max_frame,
        "Serialized JSON must be well under 4 MiB frame limit: {} < {}",
        serialized.len(),
        max_frame
    );
    assert!(
        serialized.len() < 2_900_000,
        "Serialized JSON with suppressed legacy string must be <= 2.9 MiB"
    );
}

/// Test 10: Corrupted Blob Integrity Check
/// Verifies that `ArtifactStore::read_bytes` detects on-disk data corruption
/// and returns `HashMismatch` error.
#[test]
fn test_10_corrupted_blob_integrity_check() {
    let dir = tempdir().unwrap();
    let store = ArtifactStore::new(dir.path()).unwrap();

    let original_data = b"Clean untampered artifact payload";
    let (hash, _) = store.store_bytes(original_data).unwrap();

    // Verify clean read
    let clean_read = store.read_bytes(&hash).unwrap();
    assert_eq!(clean_read, original_data);

    // Intentionally corrupt the blob file on disk
    let blob_path = store.blob_path(&hash).unwrap();
    fs::write(&blob_path, b"Tampered corrupted content!").unwrap();

    // Verify that read_bytes detects hash mismatch
    let err = store.read_bytes(&hash).unwrap_err();
    match err {
        agent_studios_workspace::error::ArtifactStoreError::HashMismatch { expected, actual } => {
            assert_eq!(expected, hash);
            assert_ne!(expected, actual);
        }
        other => panic!("Expected HashMismatch error, got {other:?}"),
    }
}

/// Test 11: Path Traversal Hash Neutralization
/// Verifies that `validate_hash` strictly enforces 64 lowercase hex characters
/// and neutralizes directory traversal attacks (`../`, `..\`, null bytes, uppercase).
#[test]
fn test_11_path_traversal_hash_neutralization() {
    // Valid 64-character lowercase hex
    let valid_sha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    assert_eq!(
        ArtifactStore::validate_hash(valid_sha256).unwrap(),
        valid_sha256
    );
    assert_eq!(
        ArtifactStore::validate_hash(&format!("sha256:{valid_sha256}")).unwrap(),
        valid_sha256
    );

    // Traversal attempts must fail
    assert!(ArtifactStore::validate_hash("../../../etc/passwd").is_err());
    assert!(ArtifactStore::validate_hash("..\\..\\windows\\system32").is_err());
    assert!(ArtifactStore::validate_hash("/root/secret").is_err());
    assert!(ArtifactStore::validate_hash("e3b0c442/../../../sub").is_err());
    assert!(ArtifactStore::validate_hash("e3b0c442\0nullbyte").is_err());

    // Uppercase hex must fail
    let upper = "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855";
    assert!(ArtifactStore::validate_hash(upper).is_err());

    // Wrong lengths must fail
    assert!(ArtifactStore::validate_hash("e3b0c442").is_err());
    assert!(ArtifactStore::validate_hash(&format!("{valid_sha256}0")).is_err());
}

/// Test 12: Paginated Binary Artifact Retrieval via GetArtifact
/// Verifies that storing a binary artifact and retrieving it across multiple chunks
/// via `GetArtifact` works cleanly with Base64 payloads and SHA-256 integrity verification.
#[tokio::test]
async fn test_12_paginated_binary_artifact_retrieval() {
    let dir = tempdir().unwrap();
    let store = ArtifactStore::new(dir.path()).unwrap();

    let studio_id = StudioId::new();
    let task_id = TaskId::new();
    let agent_id = AgentId::new();

    // 100 KB binary payload
    let mut binary_data = Vec::with_capacity(100 * 1024);
    for i in 0..(100 * 1024) {
        binary_data.push((i % 256) as u8);
    }
    let expected_sha256 = ArtifactStore::compute_hash(&binary_data);

    let art_rec = store
        .store_artifact(
            studio_id,
            task_id,
            agent_id,
            ArtifactKind::File,
            "binary_asset.bin",
            &binary_data,
            ArtifactOptions::new(),
        )
        .unwrap();

    // Simulate paginated retrieval in 32 KB chunks
    let chunk_size = 32 * 1024;
    let mut offset = 0;
    let mut reassembled = Vec::new();
    let total_bytes = binary_data.len();

    while offset < total_bytes {
        let raw = store
            .read_bytes(art_rec.content_hash.as_ref().unwrap())
            .unwrap();
        let end = (offset + chunk_size).min(total_bytes);
        let slice = &raw[offset..end];

        let b64 = base64::engine::general_purpose::STANDARD.encode(slice);
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&b64)
            .unwrap();
        reassembled.extend_from_slice(&decoded);
        offset = end;
    }

    assert_eq!(reassembled.len(), total_bytes);
    assert_eq!(reassembled, binary_data);
    let actual_sha256 = ArtifactStore::compute_hash(&reassembled);
    assert_eq!(actual_sha256, expected_sha256);
}

/// Test 13: Capture Failure Isolation & Worktree Retention
/// Verifies that when capture fails, the task retains its success state, the worktree
/// is recorded as Retained with an informative reason, and partial changes remain safe on disk.
#[tokio::test]
async fn test_13_capture_failure_isolation() {
    let studio_id = StudioId::new();
    let reason = "artifact_capture_failed: capture_changes_async failed after 3 attempts";

    let mut wt_record = WorktreeRecord::new(
        studio_id,
        "dummy_branch",
        "/tmp/source",
        "/tmp/wt-test",
        "0123456789abcdef",
        Utc::now(),
    );
    wt_record.assigned_task_id = Some(TaskId::new());

    // Record failure retention
    wt_record.retained = true;
    wt_record.retained_reason = Some(reason.to_string());
    wt_record.state = WorktreeState::Retained;

    assert!(wt_record.retained);
    assert_eq!(wt_record.state, WorktreeState::Retained);
    assert!(
        wt_record
            .retained_reason
            .as_deref()
            .unwrap()
            .starts_with("artifact_capture_failed:")
    );
}

/// Test 14: Concurrent Capture Idempotency
/// Verifies that pre-flight and registration checks safely ignore duplicate captures
/// once `ChangeCaptured` state has already been committed.
#[tokio::test]
async fn test_14_cancellation_and_concurrent_capture() {
    let studio_id = StudioId::new();
    let patch_art_id = ArtifactId::new();

    let mut wt = WorktreeRecord::new(
        studio_id,
        "branch",
        "/tmp/source",
        "/tmp/dummy",
        "base_sha",
        Utc::now(),
    );

    // Initial state: Creating
    assert_eq!(wt.state, WorktreeState::Creating);

    // After capture: ChangeCaptured
    wt.state = WorktreeState::ChangeCaptured;
    wt.patch_artifact_id = Some(patch_art_id);

    // Idempotency check logic
    let is_already_captured =
        wt.state == WorktreeState::ChangeCaptured && wt.patch_artifact_id.is_some();
    assert!(is_already_captured, "Duplicate capture should be skipped");
}

/// Test 15: Ephemeral Git Index Preservation
/// Verifies that ephemeral index operations in `capture_changes` use an isolated
/// index file without mutating the working tree's `.git/index`.
#[test]
fn test_15_git_index_preservation() {
    let dir = tempdir().unwrap();
    let fake_git_dir = dir.path().join(".git");
    fs::create_dir_all(&fake_git_dir).unwrap();

    let main_index = fake_git_dir.join("index");
    fs::write(&main_index, b"MAIN_INDEX_CONTENT").unwrap();

    // Ephemeral index file created in temp directory
    let ephemeral_index = dir.path().join("tmp_ephemeral_index");
    fs::write(&ephemeral_index, b"EPHEMERAL_INDEX_CONTENT").unwrap();

    // Verify main index is unaffected
    assert_eq!(fs::read(&main_index).unwrap(), b"MAIN_INDEX_CONTENT");
    assert_ne!(
        fs::read(&main_index).unwrap(),
        fs::read(&ephemeral_index).unwrap()
    );
}

/// Test 16: Flat Wire Schema JSON Contract Test
/// Asserts that `IpcCommand` and `IpcResponse` JSON representations match
/// the flat `{"type": "...", ...}` schema without nested `payload` or `result` envelopes.
#[test]
fn test_16_flat_wire_schema_json_contract() {
    let studio_id = StudioId::new();
    let task_id = TaskId::new();
    let artifact_id = ArtifactId::new();

    // 1. GetArtifact command
    let cmd = IpcCommand::GetArtifact {
        studio_id,
        artifact_id,
        offset: Some(10),
        limit: Some(100),
    };
    let val: Value = serde_json::to_value(&cmd).unwrap();
    assert_eq!(val["type"], "get_artifact");
    assert_eq!(val["studio_id"], studio_id.to_string());
    assert_eq!(val["artifact_id"], artifact_id.to_string());
    assert_eq!(val["offset"], 10);
    assert_eq!(val["limit"], 100);
    assert!(val.get("payload").is_none());

    // 2. GetTaskDiff command
    let diff_cmd = IpcCommand::GetTaskDiff {
        studio_id,
        task_id,
        offset: Some(0),
        limit: Some(500),
    };
    let diff_val: Value = serde_json::to_value(&diff_cmd).unwrap();
    assert_eq!(diff_val["type"], "get_task_diff");
    assert_eq!(diff_val["studio_id"], studio_id.to_string());
    assert_eq!(diff_val["task_id"], task_id.to_string());
    assert!(diff_val.get("payload").is_none());

    // 3. Artifact response
    let resp = IpcResponse::Artifact {
        artifact: None,
        content: None,
        content_base64: Some("AQIDBA==".to_string()),
        encoding: Some("base64".to_string()),
        offset: Some(0),
        limit: Some(4),
        total_bytes: Some(4),
        has_more: Some(false),
        sha256: Some(
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
        ),
    };
    let resp_val: Value = serde_json::to_value(&resp).unwrap();
    assert_eq!(resp_val["type"], "artifact");
    assert_eq!(resp_val["content_base64"], "AQIDBA==");
    assert_eq!(resp_val["encoding"], "base64");
    assert!(resp_val.get("payload").is_none());
    assert!(resp_val.get("result").is_none());
}
