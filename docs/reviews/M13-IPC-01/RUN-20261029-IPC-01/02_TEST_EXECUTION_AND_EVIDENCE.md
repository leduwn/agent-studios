# Milestone M13 IPC-01 — Test Execution & Evidence Ledger

- **Milestone**: `M13 IPC-01 Native Authenticated Transport Vertical Slice`
- **Execution Run**: `RUN-20261029-IPC-01`
- **Date**: `2026-10-29T11:15:00+07:00`
- **Branch**: `feat/m13-ipc-native-transport`
- **Platform**: `Windows 11 Home Single Language 10.0.26200 (x86_64-pc-windows-msvc)`
- **Toolchain**: `stable-x86_64-pc-windows-msvc (rustc 1.85.0 / 2024 edition)`
- **Status**: **ALL TESTS GREEN (100% PASSING / 80 TESTS)**

---

## 1. Test Suite Verification Summary

| Target / Suite | Command | Result | Pass/Fail Count | Execution Duration |
|---|---|:---:|:---:|:---:|
| **IPC Protocol Suite** | `cargo test -p agent-studios-ipc-protocol --all-targets` | **PASS** | 51 / 51 | 0.02s |
| **IPC Transport Unit Tests** | `cargo test -p agent-studios-ipc-transport --lib` | **PASS** | 21 / 21 | 0.07s |
| **IPC Transport E2E Integration Suite** | `cargo test -p agent-studios-ipc-transport --test e2e_tests` | **PASS** | 8 / 8 | 0.07s |
| **Combined M13 Suite** | `cargo test -p agent-studios-ipc-protocol -p agent-studios-ipc-transport --all-targets` | **PASS** | 80 / 80 | 0.16s |
| **Clippy Linter** | `cargo clippy -p agent-studios-ipc-protocol -p agent-studios-ipc-transport --all-targets -- -D warnings` | **PASS** | 0 warnings | 1.20s |
| **Rustfmt Check** | `cargo fmt -p agent-studios-ipc-protocol -p agent-studios-ipc-transport -- --check` | **PASS** | Clean format | <0.1s |
| **AGSD Binary Help Smoke** | `cargo run --bin agsd -- --help` | **PASS** | Exit code 0 | <0.5s |

---

## 2. End-to-End Integration Test Ledger (`tests/e2e_tests.rs`)

| Test Name | Verified Behavior | Status |
|---|---|:---:|
| `test_e2e_connect_authenticate_and_ping` | In-memory duplex connection; two-phase capability negotiation + HMAC-SHA256 authentication; Ping/Pong roundtrip with latency calculation | **PASS** |
| `test_e2e_unauthenticated_command_rejected` | Connection state machine fail-closed check; unauthenticated `GetHealth` command rejected and dropped | **PASS** |
| `test_e2e_wrong_auth_key_rejected` | Client with invalid HMAC secret rejected with `TransportError::AuthenticationFailed` | **PASS** |
| `test_e2e_event_subscription_and_live_broadcast` | Client subscribes to events; server publishes 5 broadcast events; client receives all 5 events in monotonic order (seq 1..=5) | **PASS** |
| `test_e2e_disconnect_reconnect_with_watermark_recovery` | Client receives seq 1..=3 and disconnects; server emits seq 4..=8; second client connects with `from_sequence = 4`; receives backfilled events 4..=8 with zero loss | **PASS** |
| `test_e2e_reconnect_stream_auto_watermark_recovery` | Client uses `reconnect_stream` with automatic watermark resumption; seamlessly receives remaining stream | **PASS** |
| `test_e2e_graceful_shutdown` | Client with shutdown capability sends `Shutdown` command; server token cancelled; accept loop terminates gracefully | **PASS** |
| `test_e2e_windows_named_pipe_full_lifecycle` | OS-native Windows Named Pipe (`\\.\pipe\agent-studios-test-e2e-...`) with user-only Security Descriptor; server accept loop, client connect, auth, ping, health, event subscription, and clean shutdown | **PASS** |

---

## 3. Raw Execution Output
```text
running 8 tests
test test_e2e_unauthenticated_command_rejected ... ok
test test_e2e_wrong_auth_key_rejected ... ok
test test_e2e_graceful_shutdown ... ok
test test_e2e_connect_authenticate_and_ping ... ok
test test_e2e_event_subscription_and_live_broadcast ... ok
test test_e2e_reconnect_stream_auto_watermark_recovery ... ok
test test_e2e_disconnect_reconnect_with_watermark_recovery ... ok
test test_e2e_windows_named_pipe_full_lifecycle ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.07s
```
