# M15 UI-01: Final Handoff & Pull Request Specification

**Document ID**: `M15-UI-01-DOC-07`  
**Milestone**: M15 UI-01 Interactive Agent Execution Workbench  
**Author**: Principal VS Code Extension Engineer  
**Date**: 2026-10-31  
**Status**: Ready for Pull Request  

---

## 1. Milestone Release Summary

Milestone M15 UI-01 successfully delivers the interactive agent execution workbench in VS Code, establishing a production-grade UI for managing real AI coding agent executions without compromising developer trust or system safety.

### Key Deliverables Completed:
1. **Chat-First Execution Workbench**: Integrated toolbar with Runtime selection (Claude Code native CLI default; external Codex disabled with M16 tooltip; custom shell disabled), Studio selection, and Workspace status badge.
2. **Execution Target Summary Card**: Displays configuration metadata (Runtime, Model, Workspace, Host-Owned Confirmation Policy) prior to task launch.
3. **Multiline Markdown Composer**: Auto-expanding input with `Ctrl+Enter` submit and `Shift+Enter` newline shortcuts, with proactive wire frame length warnings (>100 KB warning banner, >4 MiB hard error rejection).
4. **Trusted Native Operator Modal Gate**: Host-owned modal dialog (`vscode.window.showWarningMessage` with `{ modal: true }`) bound to cryptographic HMAC-SHA256 challenge tokens.
5. **Live Execution Streaming Pane**: Real-time status badge, elapsed timer, tool activity banner with spinner and status indicators, monospace streaming console output, and cancel action.
6. **Truthful Telemetry Metrics**: Renders `Metrics: Pending daemon telemetry` with informational tooltip when daemon has not emitted metrics; zero fabricated tokens or cost.
7. **Unified Task Execution Store**: Shared singleton state tracking execution status and timeline events across Chat and Mission Control views.
8. **Comprehensive Test Suite**: 469 passing tests, 0 compilation errors, 0 lint errors, 0 Rust modifications, 0 billable AI calls.

---

## 2. Pull Request Specification

- **Title**: `feat(m15-ui): interactive agent execution workbench with host operator confirmation`
- **Source Branch**: `feat/m15-real-agent-execution-ui`
- **Target Branch**: `feat/m14-mission-control-connected-integration`
- **Base Commit**: `a13230ee93afd101f8b0f0c5b6b0e3d61013c9af` (PR #25 HEAD)

### Golden Rules Audit

| Invariant | Status | Audit Verification |
|---|---|---|
| **Base Commit Parity** | **VERIFIED** | Branch created from M14 HEAD `a13230ee93` |
| **Zero Rust Edits** | **VERIFIED** | `git status -- codex-rs/` clean, 0 files modified |
| **Zero Billable AI Calls** | **VERIFIED** | Unit test suite runs 100% offline with zero external network calls |
| **No Fabricated Telemetry** | **VERIFIED** | Metrics render pending notice unless emitted by daemon |
| **Host-Owned Confirmation** | **VERIFIED** | Cryptographic HMAC-SHA256 tokens in Extension Host memory |
| **Fail-Closed Security** | **VERIFIED** | Workspace mismatch & disabled capabilities block execution |

---

## 3. Next Steps (M16 Roadmap)

1. Review and merge Draft PR into `feat/m14-mission-control-connected-integration`.
2. Proceed to **Milestone M16: External Codex Process Runner Support** to enable running external coding agents alongside native Claude Code.
