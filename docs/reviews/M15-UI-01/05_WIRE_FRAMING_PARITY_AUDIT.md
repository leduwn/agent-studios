# M15 UI-01: Rust Wire Framing Parity & IPC Command Audit

**Document ID**: `M15-UI-01-DOC-05`  
**Milestone**: M15 UI-01 Interactive Agent Execution Workbench  
**Author**: Principal VS Code Extension Engineer  
**Date**: 2026-10-31  
**Status**: Completed & Verified  

---

## 1. Rust Wire Contract Parity (`connection.rs:1758-1827`)

The client implementation in `ExecutionClient.ts` maintains exact wire contract parity with the Agent Studios Rust daemon:

1. **Multiline Markdown Wire Representation**:
   - In `connection.rs` (lines 1758-1769), task submission expects:
     ```rust
     task_spec: TaskSpec {
         title: req.prompt.clone(),       // Authoritative multiline Markdown prompt
         description: String::new(),      // Kept empty; title contains full prompt
         dependencies: Vec::new(),
     }
     ```
   - `ExecutionClient.ts` and `SubmitTaskAdapter.ts` preserve prompt formatting verbatim without truncation, stripping, or single-line flattening.

2. **Rust-Assigned UUID Binding**:
   - The daemon assigns authoritative UUIDs (`task_id`, `run_id`).
   - The Extension Host never invents synthetic task IDs; all store entities and UI views bind to the UUID returned in `CommandResponse::TaskSubmitted { task_id }`.

3. **Serde Tagged Enums**:
   - Commands are encoded using Serde externally tagged format:
     ```json
     {
       "SubmitTask": {
         "studio_id": "default-studio",
         "task_spec": {
           "title": "Prompt text...",
           "description": "",
           "dependencies": []
         }
       }
     }
     ```
     and
     ```json
     {
       "CancelTask": {
         "task_id": "9b1deb4d-3b7d-4bad-9bdd-2b0d7b3dcb6d",
         "reason": "Cancelled by operator"
       }
     }
     ```

---

## 2. Wire Frame Boundary Bounds

Length boundaries are enforced at three layers:

| Layer | Threshold | Behavior | Location |
|---|---|---|---|
| **Webview Composer** | `> 100 KB` | Renders yellow warning banner (`#composerWarning`) informing operator that payload is approaching frame limits. | `ChatHtmlBuilder.ts` |
| **Webview Composer** | `> 4 MiB` | Renders red error banner (`#composerError`), disables `#sendBtn`, prevents submission. | `ChatHtmlBuilder.ts` |
| **Security Guard** | `> 4 MiB` | Rejects webview message fail-closed (`Prompt exceeds 4 MiB wire frame limit`). | `ChatSecurityGuards.ts` |
| **IPC Codec** | `> 4 MiB` | `RustIpcCodec.encodeCommand` throws `Error: Payload exceeds maximum frame size (4194304 bytes)`. | `RustIpcCodec.ts` |

---

## 3. Rust Codebase Integrity

- **Repository Verification**: `git status -- codex-rs/` returns `nothing to commit, working tree clean`.
- **Rust Edits**: **Zero lines of Rust code modified**. All functionality achieved strictly within the VS Code Extension Host.
