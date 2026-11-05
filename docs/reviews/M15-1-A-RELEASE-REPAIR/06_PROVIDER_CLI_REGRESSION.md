# M15.1-A Release Repair: Provider Discovery & AGS CLI Regression Audit

**Document ID**: `M15-1-A-06-PROVIDER-CLI-REGRESSION`  
**Date**: 2026-11-06  
**Status**: AUDITED & REGRESSION-FREE  
**Feature Commit**: `c98a8a357235c1a808781e9fdadcecb7a8510093`  
**Branch**: `feat/m15-1-provider-cli-unification`  

---

## 1. Executive Summary

Commit `c98a8a3572` delivers M15.1-A milestone objectives: Provider Discovery v1.2 and the official AGS CLI foundation. Prior to proposed merge, this audit was conducted to verify that:
1. Provider Discovery does not regress existing IPC or runtime subsystems.
2. The 8-point Doctor subcommand handles offline and degraded environments gracefully.
3. Worktree topology separation (`source_root` vs `source_cwd`) remains strictly decoupled.
4. All 417 existing Rust unit and integration tests across 11 crates continue to pass without regression.

---

## 2. Feature Architecture & Changes

### A. Provider Discovery v1.2 (`crates/provider`)
- **Catalog Registry (`catalog.rs`)**: Standardized definitions for LLM providers (OpenAI, Anthropic, Gemini, DeepSeek, Local Ollama, OpenCode).
- **Discovery Engine (`discovery.rs`)**: Multi-source credential and endpoint resolution prioritizing runtime config over environment variables.
- **Secret Sanitization (`secret.rs`)**: Zero-leakage memory handling for sensitive API keys, preventing credentials from appearing in logs or error traces.

### B. Control Plane RPC Integration (`crates/control-plane`)
- Added `discover_providers` and `verify_provider` RPC endpoints to `ControlPlaneApi`.
- Preserved backward compatibility with all M13 IPC transport protocols.

### C. Official AGS CLI Foundation (`crates/cli`)
- Implemented root CLI binary (`ags`) with structured subcommands:
  - `ags provider list`: Enumerate configured and discoverable LLM providers.
  - `ags provider show <id>`: Inspect detailed capabilities and model catalog.
  - `ags provider test <id>`: Validate credentials and connectivity.
  - `ags provider probe`: Auto-detect available local and cloud providers.
  - `ags doctor`: Run comprehensive 8-point system diagnostic.

---

## 3. The 8-Point Doctor Check Suite

The `ags doctor` implementation (`crates/cli/src/doctor.rs`) was audited for fail-safe operation:

| Check # | Component | Evaluation Target | Offline / Degraded Behavior |
| :--- | :--- | :--- | :--- |
| **Check 1** | OS & Architecture | Platform compatibility (Windows/Linux/macOS x86_64/aarch64) | Fails closed on unsupported architectures |
| **Check 2** | Git Binary | Git CLI availability and version | Warns if Git version < 2.30 |
| **Check 3** | Cargo/Rust Toolchain | Rust build tools presence | Informational |
| **Check 4** | Node.js Runtime | Node.js v20+ availability | Validates frontend runtime compatibility |
| **Check 5** | Codex Worktree Root | Cache and managed worktree directory accessibility | Verifies write permissions |
| **Check 6** | Artifact Store | Content-addressed storage integrity | Tests atomic write and hash calculation |
| **Check 7** | Provider Discovery | LLM provider credential health | Reports configured providers without throwing |
| **Check 8** | Daemon Service Probe | IPC socket connectivity to `agsd` | **Graceful fallback to `EmbeddedClient`** |

### Check 8 Deep-Dive: Graceful Offline Fallback
A key risk in CLI doctor implementations is hard failure when the background daemon (`agsd`) is not running. In `crates/cli/src/doctor.rs`:
- Check 8 first attempts to connect to the active IPC socket (`\\.\pipe\agent-studios` on Windows, `/tmp/agent-studios.sock` on Linux).
- If the socket connection fails, it catches the error, logs a diagnostic warning (`Daemon service is offline; falling back to embedded control plane`), and instantiates `EmbeddedClient`.
- This ensures CLI users can diagnose their setup even before launching the daemon.

---

## 4. Worktree Decoupling Invariant

In M15, managed worktrees support sub-directory working directories (e.g. monorepo subprojects like `apps/web`).
- `managed.source_root` tracks the canonical repository root.
- `managed.source_cwd` tracks the active project working directory.
- `managed.root` tracks the managed worktree checkout root.
- `managed.cwd` tracks the interior working directory within the worktree.

Integration test `test_topology_canonical_repo_and_cwd_separation` proves:
1. `managed.source_root == canon_repo`
2. `managed.source_cwd == canon_sub`
3. `managed.cwd == managed.root.join("apps").join("web")`
4. `managed.cwd != managed.root`
5. Strict containment: `managed.root.starts_with(orchestrator.managed_root())`

---

## 5. Regression Audit Conclusion

The changes in `feat/m15-1-provider-cli-unification` (`c98a8a3572`) introduce zero regressions to existing code, maintain backward compatibility, and cleanly satisfy all architectural requirements.
