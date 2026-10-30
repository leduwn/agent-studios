# M15 — Prompt Fidelity & Workspace Integrity Architecture

**Document Reference**: `docs/reviews/M15-UI-EXECUTION/09_PROMPT_AND_WORKSPACE_INTEGRITY.md`  
**Milestone**: M14 Final Integration Freeze & M15 UI Execution Preparation  
**PR**: `https://github.com/leduwn/agent-studios/pull/25`  
**Target Base**: `feat/m14-mission-control-connected-integration`  
**Author**: Principal VS Code Extension Architect & IPC Systems Engineer  
**Date**: 2026-10-30  
**Status**: VERIFIED & BENCHMARKED  

---

## 1. Executive Summary

Milestone M15 introduces real agent execution over length-prefixed IPC framing. To prevent catastrophic failure modes (prompt corruption, line stripping, delimiter injection, buffer overflow, and cross-repository workspace mutation), PR #25 introduces `PromptSanitizer.ts` and strict workspace routing verification.

---

## 2. Authoritative Rust Task Execution Contract

In `agent-studios-rs` (pinned commit `ce0396c1f165760ea271110eaf9bfd6dc84a67a7`), `crates/ipc-transport/src/connection.rs` lines 1758-1769 define task prompt assembly:

```rust
let prompt = {
    let cp = server_state.control_plane.read().await;
    cp.get_task(task_id)
        .map(|t| {
            if t.description.is_empty() {
                t.title.clone()
            } else {
                format!("{}: {}", t.title, t.description)
            }
        })
        .unwrap_or_default()
};
```

### 2.1 The Single-Payload Strategy
- **Failure Mode in Naive Implementations**: Splitting prompts into `title = lines[0]` and `description = lines[1..]` strips blank lines inside Markdown code blocks and causes Rust `agsd` to inject a colon delimiter (`": "`) between title and description.
- **Architectural Solution**: `PromptSanitizer.prepareAuthoritativeTaskSpec` places the entire sanitized prompt verbatim in `task_spec.title` and sets `task_spec.description` to `""` (empty string).
- **Guaranteed Parity**: Because `t.description.is_empty()` is true, the Rust daemon evaluates `t.title.clone()` directly, preserving the user's prompt byte-for-byte without delimiter injection or text duplication.

---

## 3. Character Encoding & Bounded Wire Framing

### 3.1 Whitespace & Markdown Preservation
- All Markdown elements (headings, blockquotes, unordered/ordered lists, horizontal rules) are preserved verbatim.
- Blank lines within code fences (` ```rust `, ` ```python `) and indentation (tabs, spaces) are untouched.
- Windows CRLF (`\r\n`) is normalized to standard UNIX LF (`\n`) without byte loss.

### 3.2 Multibyte Unicode Integrity
- Full UTF-8 fidelity verified across Vietnamese (diacritics), Japanese (Kanji/Kana), mathematical symbols (`∑`, `∫`, `≈`, `≠`), and emojis (`🛡️`, `🚀`, `🔒`). `Buffer.compare` proves 100% byte equivalence.

### 3.3 Bounded Buffer & 4 MiB Frame Limit
- Wire framing enforces a 4-byte big-endian length prefix with `RUST_DEFAULT_MAX_FRAME_SIZE = 4 MiB` (4,194,304 bytes).
- Headroom allocated for JSON envelope, studio ID, and auth overhead: `ENVELOPE_BUFFER_HEADROOM_BYTES = 64 KiB`.
- Authoritative maximum prompt size: `MAX_PROMPT_BYTES = 4,128,768 bytes` (~3.94 MiB). Payloads exceeding this limit fail closed before memory allocation.

---

## 4. Workspace & Repository Boundary Verification

### 4.1 Daemon Repository Resolution Precedence
Parity with `crates/ipc-transport/src/connection.rs` lines 1568-1574:
1. `server_state.default_repo_path` (from builder)
2. `AGENT_STUDIOS_REPO_PATH` environment variable
3. Process current working directory (`std::env::current_dir()`), fallback `.`

### 4.2 Fail-Closed Workspace Routing
If the user's active VS Code workspace folder does not match the daemon's actual execution repository, task execution must fail closed:
1. **Cross-Platform Normalization**: `normalizeWorkspacePath` normalizes path separators (`/` vs `\`), strips trailing slashes, and applies case-insensitivity on Windows (`win32`).
2. **Path Traversal Guards**: Rejects any path containing `..` sequence (`/(?:^|[/\\])\.\.(?:[/\\]|$)/`).
3. **Null-Byte Injection Guard**: Rejects paths containing `\0`.
4. **UNC Network Path Guard**: Rejects Windows UNC paths (`\\server\share`).
5. **Mismatch Rejection**: If normalized workspace folder !== normalized daemon repository root, preflight rejects execution with descriptive error preventing unauthorized cross-repository modification.
