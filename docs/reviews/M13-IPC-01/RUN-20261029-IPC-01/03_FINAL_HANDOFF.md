# Milestone M13 IPC-01 — Final Handoff & Release Sign-Off

- **Milestone**: `M13 IPC-01 Native Authenticated Transport Vertical Slice`
- **Execution Run**: `RUN-20261029-IPC-01`
- **Date**: `2026-10-29T11:30:00+07:00`
- **Branch**: `feat/m13-ipc-native-transport`
- **Base Branch**: `main`
- **Status**: **READY FOR PULL REQUEST / STRICT NO SELF-MERGE**

---

## 1. Scope of Completed Deliverables

1. **New Crate `agent-studios-ipc-transport`**:
   - `Cargo.toml`: Registered in workspace; dependencies on `agent-studios-ipc-protocol`, `tokio`, `bytes`, `clap`, `windows-sys`, `libc`.
   - `error.rs`: Typed `TransportError` taxonomy covering I/O, protocol, authentication, invalid state, framing, security descriptors, and permissions.
   - `framing.rs`: Async 4-byte big-endian length-delimited framing engine with pre-allocation bounds checking (`FrameTooLarge`).
   - `windows/`: User-only Windows Security Descriptor via `windows-sys` APIs; `NamedPipeListener` and `NamedPipeStream` with retry backoff.
   - `unix/`: Permissions enforcement (`0700` dir, `0600` socket) with XDG and UID fallback; `UnixSocketListener` and `UnixSocketStream` with clean non-unix mock fallback.
   - `connection.rs`: 4-state lifecycle (`Connected`, `Authenticating`, `Ready`, `Disconnected`) with fail-closed unauthenticated command rejection; Ping, GetHealth, SubscribeEvents (replay buffer backfill), RequestApproval, ResolveApproval, Shutdown.
   - `server.rs`: `IpcServer` with multi-client accept loop and event broadcast publisher.
   - `client.rs`: `IpcClient` with automatic handshake, typed commands, event subscription stream, and automatic sequence watermark recovery.
   - `daemon.rs` & `bin/agsd.rs`: Configurable `agsd` daemon binary with CLI argument parsing and OS signal handling (`SIGINT`/Ctrl-C).
   - `tests/e2e_tests.rs`: Comprehensive 8-test end-to-end integration test suite.

2. **Automated Cross-Platform CI**:
   - Updated `.github/workflows/agent-studios-m13-ipc-ci.yml` to run formatting check, cargo check, clippy (-D warnings), and test suites for both `agent-studios-ipc-protocol` and `agent-studios-ipc-transport` across Ubuntu and Windows runners.

---

## 2. Release & Governance Checklist

- [x] All 80 unit and integration tests passing locally (51 protocol, 21 transport unit, 8 E2E).
- [x] Clippy clean across all targets with zero warnings under `-D warnings`.
- [x] Rustfmt clean across all crates.
- [x] Smoke verified: `cargo run --bin agsd -- --help` exits code 0.
- [x] Remote CI passing across matrix: Ubuntu (`ubuntu-latest`) & Windows (`windows-latest`) for PR #18 (Run ID `40912363071`).
- [x] Draft PR #18 opened against `main` (`https://github.com/leduwn/agent-studios/pull/18`).
- [x] Subtree `codex-rs/` 100% untouched.
- [x] Zero bearer secrets placed in git repositories or fixtures.
- [x] No self-merge rule strictly observed across all PRs.

