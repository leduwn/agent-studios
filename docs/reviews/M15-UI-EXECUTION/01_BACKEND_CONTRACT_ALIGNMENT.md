# M15 — Backend Contract Alignment & IPC Protocol Specification

**Document Reference**: `docs/reviews/M15-UI-EXECUTION/01_BACKEND_CONTRACT_ALIGNMENT.md`  
**Milestone**: M14 Final Integration Freeze & M15 UI Execution Preparation  
**Repository**: `https://github.com/leduwn/agent-studios`  
**Approved Pinned Backend SHA**: `ce0396c1f165760ea271110eaf9bfd6dc84a67a7` (PR #19)  
**Author**: Principal VS Code Extension Engineer & Rust/TypeScript Integration Architect  
**Date**: 2026-10-30  
**Status**: APPROVED & FROZEN FOR M15 UI EXECUTION  

---

## 1. Executive Summary

This specification establishes the authoritative inter-process communication (IPC) protocol contract between the TypeScript Code-OSS VS Code extension (`agent-studios-vscode`) and the native Rust daemon (`agsd` from `agent-studios-rs`).

All contract definitions are verified against the approved Rust backend commit `ce0396c1f165760ea271110eaf9bfd6dc84a67a7` from backend PR #19 (`feat/m13-ipc-02-control-plane`). Zero modifications were introduced to the Rust codebase. All alignment is enforced client-side via type-safe TypeScript codecs, HMAC signers, and authoritative golden fixtures.

---

## 2. Framing & Wire Format Specification

### 2.1 Length-Prefixed Binary Framing
Communication over OS IPC (Windows Named Pipes: `\\.\pipe\agsd-{key_id}` and POSIX Unix Domain Sockets: `/tmp/agsd-{key_id}.sock`) uses a 4-byte big-endian length prefix followed by UTF-8 encoded JSON.

```
+--------------------------------+---------------------------------------------+
| Length Prefix (4 bytes, BE)    | Payload Data (JSON, UTF-8 encoded)          |
| uint32_be (N bytes)            | {"type": "negotiate_capabilities", ...}     |
+--------------------------------+---------------------------------------------+
```

- **Header Size**: `RUST_FRAME_HEADER_SIZE = 4` bytes.
- **Maximum Frame Size**: `DEFAULT_MAX_FRAME_SIZE = 4 MiB` (4,194,304 bytes). Any frame with a length prefix exceeding 4,194,304 bytes triggers an immediate `RangeError` and terminates the connection to prevent memory exhaustion attacks.
- **Stream Accumulation Guard**: Partial frame buffer accumulation is strictly bounded at `RUST_MAX_ACCUMULATED_BUFFER_SIZE = 16 MiB` (16,777,216 bytes).

### 2.2 Serde Internally-Tagged Wire Contract
All command and response payloads use Serde's internal tagging format: `{"type": "<tag_name>", ...}`.

- **Strict Tagging Invariant**: Payloads lacking a `"type"` string property are rejected before transmission or upon receipt.
- **Fail-Closed Foreign Contract Handling**: Any payload containing foreign JSON-RPC fields (`"jsonrpc": "2.0"`, `"method"`, `"params"`) fails closed immediately, preventing protocol confusion or unauthenticated execution.

---

## 3. Cryptographic Authentication & Canonical Digest

### 3.1 Two-Phase Handshake Lifecycle
Every IPC connection executes a two-phase handshake prior to issuing domain commands:
1. `IpcCommand::NegotiateCapabilities` $\rightarrow$ `IpcResponse::CapabilitiesNegotiated`
2. `IpcCommand::Authenticate` $\rightarrow$ `IpcResponse::Authenticated`

### 3.2 Canonical Digest Formula
Authentication headers (`HmacAuthHeader`) require an HMAC-SHA256 signature computed over an exact canonical digest string:

$$\text{Digest} = \text{version} : \text{key\_id} : \text{session\_id} : \text{nonce} : \text{timestamp\_millis} : \text{sorted\_capabilities} : \text{payload\_sha256}$$

Where:
- `version`: Format `{major}.{minor}.{patch}` (e.g., `1.0.0`).
- `key_id`: Client identifier string matching bootstrap credentials (e.g., `test-client-1`).
- `session_id`: Unique session UUID (e.g., `sess-9a7c0000-0000-0000-0000-000000000001`).
- `nonce`: Cryptographically random string (e.g., `nonce-abcd-1234`).
- `timestamp_millis`: UTC timestamp in integer milliseconds. Handshake fails closed if clock skew exceeds $\pm 60,000$ ms.
- `sorted_capabilities`: Agreed capabilities sorted lexicographically in ascending ASCII order and joined by commas (e.g., `event_streaming,linearizable_replay`). If empty, rendered as an empty string.
- `payload_sha256`: Lowercase hexadecimal SHA-256 hash of the command payload. When payload is empty or null, the canonical empty SHA-256 hash is required:
  `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`

### 3.3 Golden Digest Verification Vector
Verified in `RustIpcContractFixtures.test.ts` against Rust vector in `crates/ipc-protocol/src/auth.rs`:
```
1.0.0:test-client-1:sess-9a7c0000-0000-0000-0000-000000000001:nonce-abcd-1234:1793400000000:event_streaming,linearizable_replay:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
```

---

## 4. Hermetic CI Backend Pinning

To guarantee reproducible, hermetic testing without silent branch drift, `.github/workflows/agent-studios-m14-ui-ci.yml` implements a hard SHA fetch and provenance assertion:

```yaml
- name: Checkout pinned Rust backend (M13 IPC agsd)
  working-directory: ${{ github.workspace }}
  shell: bash
  run: |
    PINNED_SHA="ce0396c1f165760ea271110eaf9bfd6dc84a67a7"
    echo "Fetching pinned backend commit: $PINNED_SHA"
    git fetch --depth=1 --force origin "$PINNED_SHA":backend-pin || {
      echo "::error::Failed to fetch pinned backend commit $PINNED_SHA. Branch fallback disabled."
      exit 1
    }
    ACTUAL_SHA=$(git rev-parse backend-pin)
    echo "Verified fetched backend commit: $ACTUAL_SHA"
    if [ "$ACTUAL_SHA" != "$PINNED_SHA" ]; then
      echo "::error::Backend commit mismatch: expected $PINNED_SHA but got $ACTUAL_SHA"
      exit 1
    fi
    rm -rf agent-studios-rs
    git checkout backend-pin -- agent-studios-rs/
    echo "Successfully extracted agent-studios-rs/ from pinned commit $ACTUAL_SHA"
```

Any attempt to fall back to a moving branch (such as `main` or `feat/m13-ipc-02-control-plane`) has been removed; CI fails immediately if the exact approved SHA is unreachable or modified.
