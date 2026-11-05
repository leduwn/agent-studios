# Milestone M13 IPC-00 — Test Execution & Evidence Ledger

- **Milestone**: `M13 IPC-00 Protocol Foundation`
- **Execution Run**: `RUN-20261028-M13-IPC-00`
- **Date**: `2026-10-28T16:16:00+07:00`
- **Branch**: `feat/m13-ipc-protocol-foundation`
- **Platform**: `Windows 11 Home Single Language 10.0.26200 (x86_64-pc-windows-msvc)`
- **Toolchain**: `stable-x86_64-pc-windows-msvc`
- **Status**: **ALL TESTS GREEN (23/23 PASSING)**

---

## 1. Test Execution Evidence

```text
running 23 tests
test approval::tests::test_approval_creation_and_resolution ... ok
test auth::tests::test_hmac_sign_and_verify_success ... ok
test approval::tests::test_approval_expiry_fails_closed ... ok
test auth::tests::test_hmac_verify_tampered_payload_fails ... ok
test auth::tests::test_hmac_verify_skew_timeout_fails ... ok
test capabilities::tests::test_missing_required_capability_fails ... ok
test capabilities::tests::test_incompatible_version_fails ... ok
test envelope::tests::test_version_compatibility ... ok
test envelope::tests::test_envelope_creation_and_serialization ... ok
test envelope::tests::test_envelope_version_validation ... ok
test auth::tests::test_nonce_replay_protection ... ok
test capabilities::tests::test_successful_capability_negotiation ... ok
test framing::tests::test_ipc_command_serde ... ok
test framing::tests::test_ipc_response_serde ... ok
test id::tests::test_id_generation_and_prefix ... ok
test id::tests::test_id_serde ... ok
test replay::tests::test_replay_buffer_push_and_replay ... ok
test replay::tests::test_non_monotonic_sequence_rejected ... ok
test redaction::tests::test_redacted_type_debug_display ... ok
test replay::tests::test_replay_buffer_capacity_eviction_and_unrecoverable_gap ... ok
test redaction::tests::test_redact_bearer_token ... ok
test redaction::tests::test_redact_json_sensitive_keys ... ok
test redaction::tests::test_redact_anthropic_key ... ok

test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
```

---

## 2. Clippy & Code Quality Checks

```bash
cargo clippy -p agent-studios-ipc-protocol --all-targets -- -D warnings
```
**Exit Code**: 0  
**Output**: Clean (0 warnings, 0 errors).

```bash
rustfmt --edition 2024 --check crates/ipc-protocol/src/lib.rs
```
**Exit Code**: 0  
**Output**: Clean formatting across all module files.
