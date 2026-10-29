# Milestone M13 IPC-00.1 — Test Execution & Evidence Ledger

- **Milestone**: `M13 IPC-00.1 Security Hardening & Protocol Correctness`
- **Execution Run**: `RUN-20261029-IPC-00.1`
- **Date**: `2026-10-29T10:15:00+07:00`
- **Branch**: `feat/m13-ipc-protocol-foundation`
- **Platform**: `Windows 11 Home Single Language 10.0.26200 (x86_64-pc-windows-msvc)`
- **Toolchain**: `stable-x86_64-pc-windows-msvc (rustc 1.85.0 / 2024 edition)`
- **Status**: **ALL TESTS GREEN (100% PASSING / 51 TESTS)**

---

## 1. Test Suite Verification Summary

| Test Suite / Target | Command | Result | Pass/Fail Count | Execution Duration |
|---|---|:---:|:---:|:---:|
| **IPC Protocol Full Suite** | `cargo test -p agent-studios-ipc-protocol --all-targets` | **PASS** | 51 / 51 | 0.01s |
| **Authentication Module** | `cargo test -p agent-studios-ipc-protocol --lib auth` | **PASS** | 11 / 11 | <0.01s |
| **Approval Lifecycle** | `cargo test -p agent-studios-ipc-protocol --lib approval` | **PASS** | 7 / 7 | <0.01s |
| **Redaction & Diagnostics** | `cargo test -p agent-studios-ipc-protocol --lib redaction` | **PASS** | 11 / 11 | <0.01s |
| **Replay Buffer & Gaps** | `cargo test -p agent-studios-ipc-protocol --lib replay` | **PASS** | 7 / 7 | <0.01s |
| **Framing & FrameCodec** | `cargo test -p agent-studios-ipc-protocol --lib framing` | **PASS** | 7 / 7 | <0.01s |
| **Envelope & Versioning** | `cargo test -p agent-studios-ipc-protocol --lib envelope` | **PASS** | 3 / 3 | <0.01s |
| **Capabilities Negotiation** | `cargo test -p agent-studios-ipc-protocol --lib capabilities` | **PASS** | 3 / 3 | <0.01s |
| **ID System & Prefixes** | `cargo test -p agent-studios-ipc-protocol --lib id` | **PASS** | 2 / 2 | <0.01s |
| **Clippy Linter** | `cargo clippy -p agent-studios-ipc-protocol --all-targets -- -D warnings` | **PASS** | 0 warnings | 1.94s |
| **Rustfmt Check** | `cargo fmt -p agent-studios-ipc-protocol -- --check` | **PASS** | Clean format | <0.1s |

---

## 2. Detailed Execution Outputs

### 2.1. Complete Test Suite Run
```text
running 51 tests
test approval::tests::test_approval_creation_and_resolution ... ok
test approval::tests::test_approval_expiry_fails_closed ... ok
test approval::tests::test_correlation_failures_fail_closed ... ok
test approval::tests::test_illegal_state_transitions ... ok
test approval::tests::test_transition_expiry_fails_closed ... ok
test auth::tests::test_canonical_digest_sorted_capabilities ... ok
test auth::tests::test_hmac_verify_malformed_signature_hex_rejection ... ok
test auth::tests::test_hmac_sign_and_verify_success ... ok
test auth::tests::test_hmac_verify_duplicate_request_rejection ... ok
test auth::tests::test_hmac_verify_wrong_key_rejection ... ok
test auth::tests::test_nonce_tracker_scoped_identity_isolation ... ok
test approval::tests::test_happy_path_lifecycle ... ok
test capabilities::tests::test_incompatible_version_fails ... ok
test auth::tests::test_hmac_verify_session_id_mismatch_rejection ... ok
test auth::tests::test_hmac_verify_stale_and_future_timestamp_rejection ... ok
test auth::tests::test_hmac_verify_tampered_payload_signature_mismatch ... ok
test auth::tests::test_nonce_tracker_capacity_bound_enforcement ... ok
test capabilities::tests::test_missing_required_capability_fails ... ok
test capabilities::tests::test_successful_capability_negotiation ... ok
test approval::tests::test_concurrent_state_transitions ... ok
test auth::tests::test_hmac_concurrent_replay_attempts ... ok
test envelope::tests::test_envelope_version_validation ... ok
test envelope::tests::test_envelope_creation_and_serialization ... ok
test envelope::tests::test_version_compatibility ... ok
test framing::tests::test_frame_codec_exceeds_max_size_rejected_before_allocation ... ok
test framing::tests::test_frame_codec_malformed_length_prefix ... ok
test framing::tests::test_frame_codec_invalid_json_payload_handling ... ok
test framing::tests::test_frame_codec_roundtrip ... ok
test framing::tests::test_frame_codec_truncated_partial_frame ... ok
test framing::tests::test_ipc_command_serde ... ok
test framing::tests::test_ipc_response_serde ... ok
test id::tests::test_id_generation_and_prefix ... ok
test id::tests::test_id_serde ... ok
test redaction::tests::test_redacted_security_contract_and_safe_telemetry ... ok
test redaction::tests::test_redacted_type_debug_display ... ok
test replay::tests::test_boundary_capacity_and_overflow ... ok
test replay::tests::test_contiguous_vs_non_contiguous_gaps ... ok
test replay::tests::test_non_monotonic_sequence_rejected ... ok
test replay::tests::test_replay_buffer_capacity_eviction_and_unrecoverable_gap ... ok
test replay::tests::test_replay_buffer_reject_zero_capacity ... ok
test replay::tests::test_replay_buffer_push_and_replay ... ok
test replay::tests::test_replay_gap_unrecoverable_vs_recoverable ... ok
test redaction::tests::test_redact_aws_key ... ok
test redaction::tests::test_redact_json_sensitive_keys ... ok
test redaction::tests::test_redact_anthropic_key ... ok
test redaction::tests::test_redact_bearer_token ... ok
test redaction::tests::test_redact_deep_nested_json_depth_5_plus ... ok
test redaction::tests::test_redact_openai_key ... ok
test redaction::tests::test_redact_github_tokens ... ok
test redaction::tests::test_redact_arrays_of_objects_and_mixed_keys ... ok
test redaction::tests::test_redact_pem_private_keys ... ok

test result: ok. 51 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

---

## 3. Remote GitHub Actions CI Verification

- **Workflow**: `Agent Studios M13 IPC CI` (`.github/workflows/agent-studios-m13-ipc-ci.yml`)
- **Target Commit**: `2b17aafe293adbfa377d3d274fb10e3f26a6d38b`
- **Pull Request**: PR #17 (`https://github.com/leduwn/agent-studios/pull/17`)
- **PR Run ID**: `40905882354` (`https://github.com/leduwn/agent-studios/actions/runs/40905882354`)
- **Push Run ID**: `40905873455` (`https://github.com/leduwn/agent-studios/actions/runs/40905873455`)
- **Overall Status**: **TOTAL SUCCESS (100% GREEN ACROSS UBUNTU & WINDOWS)**

### 3.1. Matrix Job Results (Run 40905882354)

| Job | OS Platform | Status | Conclusion | Key Verified Steps |
|---|---|:---:|:---:|---|
| **M13 IPC Quality Gate (ubuntu-latest)** | `ubuntu-latest` | completed | **success** | Check Formatting, Cargo Check, Cargo Clippy, Cargo Test |
| **M13 IPC Quality Gate (windows-latest)** | `windows-latest` | completed | **success** | Cargo Check, Cargo Clippy, Cargo Test |

### 3.2. Matrix Job Results (Run 40905873455)

| Job | OS Platform | Status | Conclusion | Key Verified Steps |
|---|---|:---:|:---:|---|
| **M13 IPC Quality Gate (ubuntu-latest)** | `ubuntu-latest` | completed | **success** | Check Formatting, Cargo Check, Cargo Clippy, Cargo Test |
| **M13 IPC Quality Gate (windows-latest)** | `windows-latest` | completed | **success** | Cargo Check, Cargo Clippy, Cargo Test |

