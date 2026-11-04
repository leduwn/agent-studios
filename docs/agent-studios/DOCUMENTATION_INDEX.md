# Agent Studios — Master Documentation Hub & Index

> **Status**: Authoritative Documentation Directory & Reading Guide  
> **Milestone**: M15 Consolidation & Canonical Master Reconstruction  
> **Target Branch**: `integration/m15-unified-codebase`  
> **Precedence Hierarchy**: Governed by Section 1 below

---

## 1. Documentation Precedence Hierarchy

When questions or discrepancies arise between documents, resolve them according to this strict normative hierarchy:

```text
┌────────────────────────────────────────────────────────────────────────┐
│                   NORMATIVE ARCHITECTURAL PRECEDENCE                   │
├────────────────────────────────────────────────────────────────────────┤
│ 1. VISION_AND_ORIGINAL_REQUIREMENTS.md (Highest Authority / Vision)    │
├────────────────────────────────────────────────────────────────────────┤
│ 2. PRODUCT_REQUIREMENTS.md (Core PRD & Operational Rules)              │
├────────────────────────────────────────────────────────────────────────┤
│ 3. ROADMAP_TO_STABLE_RELEASE.md (Milestone Order & Target Scope)       │
├────────────────────────────────────────────────────────────────────────┤
│ 4. Subsystem Specifications (Architecture & Invariants)                │
│    - MULTI_AGENT_ORCHESTRATION.md                                      │
│    - AGENT_AND_IDE_EXPERIENCE.md                                       │
│    - RUNTIME_PROVIDER_AND_MODEL.md                                     │
│    - SECURITY_AND_EXECUTION_POLICY.md                                  │
│    - MISSION_CONTROL_AND_UI.md                                         │
│    - AGS_CLI_TUI_SPEC.md                                               │
├────────────────────────────────────────────────────────────────────────┤
│ 5. Operational Guides & History                                        │
│    - CURRENT_IMPLEMENTATION_STATUS.md                                  │
│    - MILESTONE_HISTORY.md                                              │
│    - REMAINING_GAPS_AND_BACKLOG.md                                     │
│    - DEVELOPMENT_WORKFLOW.md                                           │
│    - UNIFIED_REPOSITORY_WORKSPACE.md                                   │
│    - UPSTREAM_RESEARCH_AND_IDEAS.md                                    │
│    - PROJECT_HANDOFF.md                                                │
└────────────────────────────────────────────────────────────────────────┘
```

*Rule*: Documentation precedence defines architectural intent. Current code reality is determined by `Reviewed Source Code + Automated Tests + Verified Git SHA`. Discrepancies must be surfaced explicitly rather than silently rewriting canonical vision.

---

## 2. Master Deliverables Directory (The 17 Core Documents)

| # | Document File | Primary Subject & Description |
|:---:|---|---|
| 1 | [`VISION_AND_ORIGINAL_REQUIREMENTS.md`](VISION_AND_ORIGINAL_REQUIREMENTS.md) | Official product statement, formula, 30 canonical invariants, persona definitions. |
| 2 | [`PRODUCT_REQUIREMENTS.md`](PRODUCT_REQUIREMENTS.md) | Comprehensive PRD, zero-loss switching contract, Activity Bar rule, localization. |
| 3 | [`AGENT_AND_IDE_EXPERIENCE.md`](AGENT_AND_IDE_EXPERIENCE.md) | Deep UX architecture for Agent Mode, IDE Mode, and shared session kernel. |
| 4 | [`MULTI_AGENT_ORCHESTRATION.md`](MULTI_AGENT_ORCHESTRATION.md) | Cognitive Coordinator vs. Control Plane, Kahn DAG scheduling, worktrees, 3-way patches. |
| 5 | [`RUNTIME_PROVIDER_AND_MODEL.md`](RUNTIME_PROVIDER_AND_MODEL.md) | Provider vs. Runtime decoupling, `ModelRef`, secret zeroization, external runtimes. |
| 6 | [`AGS_CLI_TUI_SPEC.md`](AGS_CLI_TUI_SPEC.md) | Official `ags` CLI/TUI specification, headless CI mode, dynamic model discovery. |
| 7 | [`MISSION_CONTROL_AND_UI.md`](MISSION_CONTROL_AND_UI.md) | 9-tab observability workbench specification, 7 core inquiries, truthful telemetry. |
| 8 | [`SECURITY_AND_EXECUTION_POLICY.md`](SECURITY_AND_EXECUTION_POLICY.md) | Win32 Job Object sandboxing, 2PC stdin approval delivery, HMAC IPC, $0 token gate. |
| 9 | [`CURRENT_IMPLEMENTATION_STATUS.md`](CURRENT_IMPLEMENTATION_STATUS.md) | Ground truth at unified HEAD: 15 Rust crates, 339+ tests, 735 TS tests, CI matrix. |
| 10 | [`MILESTONE_HISTORY.md`](MILESTONE_HISTORY.md) | Comprehensive chronicle of Milestones M01 to M15, PRs, commits, review closures. |
| 11 | [`REMAINING_GAPS_AND_BACKLOG.md`](REMAINING_GAPS_AND_BACKLOG.md) | Critical gap audit: operator decision gates vs. technical implementation backlog. |
| 12 | [`ROADMAP_TO_STABLE_RELEASE.md`](ROADMAP_TO_STABLE_RELEASE.md) | Authoritative roadmap from M15 to M19 GA, Windows MSI installer, auto-updater. |
| 13 | [`DEVELOPMENT_WORKFLOW.md`](DEVELOPMENT_WORKFLOW.md) | Contributor setup, build commands, offline test invariants, coding standards. |
| 14 | [`UNIFIED_REPOSITORY_WORKSPACE.md`](UNIFIED_REPOSITORY_WORKSPACE.md) | Local 12-directory audit, canonical migration plan, post-approval cleanup matrix. |
| 15 | [`UPSTREAM_RESEARCH_AND_IDEAS.md`](UPSTREAM_RESEARCH_AND_IDEAS.md) | Analysis of Codex, OpenCode, Claude Code, Code-OSS, and Agent Studios synthesis. |
| 16 | [`PROJECT_HANDOFF.md`](PROJECT_HANDOFF.md) | Master executive handoff, 20-question onboarding FAQ, automated Resume Prompt. |
| 17 | [`DOCUMENTATION_INDEX.md`](DOCUMENTATION_INDEX.md) | This document: reading paths, provenance tag taxonomy, cross-references. |

---

## 3. Recommended Reading Paths by Persona

### Path A: The Operator / Decision Maker
*Focus: Product vision, risk management, release readiness, and governance.*
1. [`VISION_AND_ORIGINAL_REQUIREMENTS.md`](VISION_AND_ORIGINAL_REQUIREMENTS.md) (Product identity & 30 invariants)
2. [`PROJECT_HANDOFF.md`](PROJECT_HANDOFF.md) (20-question FAQ and operational models)
3. [`REMAINING_GAPS_AND_BACKLOG.md`](REMAINING_GAPS_AND_BACKLOG.md) (Operator decision gates)
4. [`ROADMAP_TO_STABLE_RELEASE.md`](ROADMAP_TO_STABLE_RELEASE.md) (Timeline to GA v1.0.0)

### Path B: The System Architect
*Focus: End-to-end topology, state machines, isolation boundaries, and protocol contracts.*
1. [`PRODUCT_REQUIREMENTS.md`](PRODUCT_REQUIREMENTS.md) (Zero-loss mode switching contract)
2. [`MULTI_AGENT_ORCHESTRATION.md`](MULTI_AGENT_ORCHESTRATION.md) (Control plane actor & worktree isolation)
3. [`RUNTIME_PROVIDER_AND_MODEL.md`](RUNTIME_PROVIDER_AND_MODEL.md) (Provider/runtime decoupling & `ModelRef`)
4. [`SECURITY_AND_EXECUTION_POLICY.md`](SECURITY_AND_EXECUTION_POLICY.md) (Sandboxing, 2PC approvals, IPC auth)
5. [`UPSTREAM_RESEARCH_AND_IDEAS.md`](UPSTREAM_RESEARCH_AND_IDEAS.md) (Codex & Claude Code synthesis)

### Path C: The Backend Rust Engineer
*Focus: Crates, control plane actor, daemon lifecycle, and runtime adapters.*
1. [`DEVELOPMENT_WORKFLOW.md`](DEVELOPMENT_WORKFLOW.md) (Prerequisites, build commands, test suites)
2. [`CURRENT_IMPLEMENTATION_STATUS.md`](CURRENT_IMPLEMENTATION_STATUS.md) (15-crate inventory & test counts)
3. [`MULTI_AGENT_ORCHESTRATION.md`](MULTI_AGENT_ORCHESTRATION.md) (Worktree leases & Kahn DAG scheduling)
4. [`AGS_CLI_TUI_SPEC.md`](AGS_CLI_TUI_SPEC.md) (`ags` binary, dynamic discovery, headless execution)
5. [`SECURITY_AND_EXECUTION_POLICY.md`](SECURITY_AND_EXECUTION_POLICY.md) (Win32 Job Objects & 2PC stdin protocol)

### Path D: The Frontend VS Code Extension Engineer
*Focus: Code-OSS integration, Mission Control webview, Activity Bar ergonomics, and IPC client.*
1. [`AGENT_AND_IDE_EXPERIENCE.md`](AGENT_AND_IDE_EXPERIENCE.md) (Agent Mode vs IDE Mode UX specifications)
2. [`MISSION_CONTROL_AND_UI.md`](MISSION_CONTROL_AND_UI.md) (9-tab workbench layout & SVG DAG renderer)
3. [`PRODUCT_REQUIREMENTS.md`](PRODUCT_REQUIREMENTS.md) (Single Activity Bar item rule)
4. [`DEVELOPMENT_WORKFLOW.md`](DEVELOPMENT_WORKFLOW.md) (Launching Extension Development Host)

---

## 4. Provenance Tag Taxonomy & Legend

Throughout the Agent Studios documentation hub, specifications carry standardized provenance tags to maintain strict distinction between requirements, decisions, implementations, and proposals:

- **`[USER_DIRECT_REQUIREMENT]`**: Explicitly mandated by the human project lead or operator. Takes precedence over default behavior.
- **`[EXISTING_CANONICAL_DECISION]`**: Previously reviewed and formally recorded in canonical architecture documents (`MASTER_VISION.md`, `PRODUCT_PRINCIPLES.md`). Immutable without explicit architectural governance review.
- **`[IMPLEMENTED_BEHAVIOR]`**: Backed by reviewed, committed, and passing source code and automated tests at the verified Git SHA.
- **`[ASSISTANT_OR_CLAUDE_PROPOSAL]`**: Introduced or proposed by Claude Code or assistant agents to resolve technical defects or improve developer ergonomics. Requires operator sign-off for canonical adoption.
- **`[UPSTREAM_INSPIRED_IDEA]`**: Concept, pattern, or primitive derived from upstream open-source projects (OpenAI Codex, OpenCode, Claude Code, Code-OSS).
- **`[OPEN_DECISION]`**: Unresolved architectural trade-off or implementation fork requiring operator review or empirical benchmarking.
