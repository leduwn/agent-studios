# Official AGS CLI & TUI Architecture Specification
**Milestone:** M15.1-A — Backend and CLI Consolidation  
**Target Crates:** `agent-studios-rs/crates/{cli, cli-client, tui}`  
**Ported From:** `feat/provider-management-v1` (PR #20)  
**Binary Output:** `target/debug/ags` (`ags.exe`)  
**Date:** 2026-11-06  
**Auditor:** Claude 1 (Principal Rust Architect & CLI/TUI Systems Engineer)

---

## 1. Executive Summary

The official **Agent Studios Developer CLI (`ags`)** provides an asynchronous, developer-first terminal interface and autonomous multi-agent cockpit.

Ported from PR #20 into the canonical `agent-studios-rs` workspace, `ags` adopts a clean three-tier separation of concerns:
1. **`agent-studios-cli`**: Clap-derived CLI command dispatcher, JSON serialization, and subcommands.
2. **`agent-studios-cli-client`**: Abstract client trait (`Client`) decoupling user interfaces from execution backends, supporting both in-process `EmbeddedClient` and daemon-connected `IpcClient`.
3. **`agent-studios-tui`**: High-polish terminal user interface built on Ratatui 0.30.2 and Crossterm, featuring responsive layouts, interactive widgets, and Unicode NFC tone mark protection.

---

## 2. Crates Topology & Architectural Flow

```text
  +-----------------------------------------------------------------------+
  |                           ags CLI / ags TUI                          |
  +-----------------------------------------------------------------------+
                                      |
                                      v
  +-----------------------------------------------------------------------+
  |                  agent-studios-cli-client (Trait)                     |
  |  - submit_goal()   - list_agents()   - resolve_approval()             |
  |  - subscribe_events() - get_task_graph() - reconcile_worktree()       |
  +-----------------------------------------------------------------------+
               |                                              |
               v (Standalone Mode)                            v (Daemon Mode)
  +-------------------------------+             +-------------------------------+
  |        EmbeddedClient         |             |           IpcClient           |
  |  (In-process ControlPlane     |             |  (Native Named Pipe on Win,   |
  |   + SQLite / InMemoryStore)   |             |   Unix Domain Socket on POSIX)|
  +-------------------------------+             +-------------------------------+
               |                                              |
               |                                              v
               |                                +-------------------------------+
               |                                |        agsd Daemon IPC        |
               +------------------------------->|  (Authoritative Control Plane |
                                                |   Multi-Agent Orchestrator)   |
                                                +-------------------------------+
```

---

## 3. Command Specifications

### 3.1 Global Flags & Options
- `ags --help`: Comprehensive usage, subcommands, and options.
- `ags --version`: Crate semantic version (`0.1.0`).
- `ags --preview`: Launches offline interactive visual design previewer (themes, modals, diff viewer).
- `ags --demo`: Launches in simulated demo mode without requiring remote provider API credentials.

### 3.2 System Doctor (`ags doctor`)
Runs an 8-stage diagnostic suite validating host environment readiness:
1. **Platform:** Operating system and architecture (`windows (x86_64, windows)`).
2. **Git Executable:** Verifies Git binary presence and version (`git version 2.56.0+`).
3. **Git Repository & Worktree:** Identifies repo root and working directory dirty state.
4. **Terminal Geometry:** Measures column/row dimensions against minimum standard (`>= 100x28`).
5. **Color & ANSI Capabilities:** Probes TrueColor (`COLORTERM=truecolor`), 256-color, Windows Terminal VT, or `NO_COLOR` monochrome fallback.
6. **Unicode NFC & Vietnamese Tone Pipeline:** Validates 69 grapheme clusters containing complex Vietnamese diacritics, ensuring tone marks render intact without clipping.
7. **Control Plane Engine:** Bootstraps an in-process deterministic test engine with default agent roles.
8. **Daemon Service (`agsd`) Probe:** Checks native named pipe (`\\.\pipe\agent-studios-default`) or Unix socket (`/tmp/agent-studios.sock`). When offline, gracefully reports `INFO` and activates `EmbeddedClient` standalone fallback.

Supports `--json` flag producing machine-readable structured output for CI and telemetry.

### 3.3 Provider Management (`ags provider`)
- `ags provider list [--format table|json]`: Lists all configured provider instances with protocol, auth type, model count, and default status.
- `ags provider get <id>`: Displays comprehensive details for a specific provider instance.
- `ags provider add`: Registers a new provider instance with endpoint and protocol binding.
- `ags provider remove <id>`: Removes an instance (safely fails if models depend on it).
- `ags provider import <path> [--format codex|opencode|snapshot] [--dry-run]`: Imports provider configurations with conflict resolution.

### 3.4 Model Catalog Management (`ags model`)
- `ags model list [--provider <id>]`: Lists all registered models with context window, output limits, and capabilities.
- `ags model get <id>`: Inspects full model descriptor.
- `ags model add`: Manually registers a model descriptor under a provider instance.
- `ags model remove <id>`: Removes a model descriptor from catalog.

---

## 4. TUI Component Architecture (`agent-studios-tui`)

### 4.1 Terminal Layer
- **Patched Crossterm Dependency:** Uses `crossterm = { git = "https://github.com/openai-oss-forks/crossterm", rev = "efa177859fd9623d57b9fe7ae9bf491ae1ac6ec4" }` for robust bracketed paste and Windows console compatibility.
- **Title Sanitization:** Neutralizes ANSI escape sequences and Trojan Source bi-directional Unicode overrides in terminal titles.

### 4.2 Responsive Layout & Breakpoints
Adapts layout dynamically based on terminal dimensions:
- **Compact (`< 100 cols`):** Single-column stacked panes, collapsed agent sidebar.
- **Standard (`100–140 cols`):** Two-pane layout with task timeline and event feed.
- **Wide (`> 140 cols`):** Three-pane layout with agent roster, task graph DAG, and live diff stream.

### 4.3 Widget Inventory
- **`ProviderPanel`:** Interactive provider selection, instance switching, and default assignment.
- **`DiffViewer`:** Unified diff rendering with syntax highlighting and line numbers.
- **`ApprovalModal`:** Dual-key approval modal for tool execution and branch reconciliation.
- **`SlashMenu`:** Slash command palette (`/help`, `/doctor`, `/provider`, `/model`, `/clear`, `/quit`).
- **`StatusBar`:** Live token usage, active agent status, and connection state.
