# GitHub Branch & Pull Request Governance Framework

**Milestone:** M14-UI-02  
**Run ID:** RUN-20261030-M14-UI-02  
**Date:** 2026-10-30  
**Scope:** Authoritative remote audit, branch taxonomy, lifecycle state gates, stacked PR ancestry, and cleanup hygiene.

---

## 1. Authoritative Audit of Open Pull Requests

An audit against the remote repository `leduwn/agent-studios` reveals 7 open pull requests across three distinct development workstreams:

| PR # | Title | Author | Base Branch | Head Branch | Status | Workstream / Domain | Stacked Dependency |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **#21** | feat(vscode): deliver M14-UI-01 chat-first assistant & prompt composer | Duwn | `main` | `feat/m14-codeoss-agents-workbench` | Open (Draft) | VS Code Extension (UI-00 + UI-01) | Base of M14 UI track |
| **#20** | feat(runtime): complete M11 OpenCode runtime adapter and OS process sandboxing | Duwn | `main` | `feat/m11-opencode-runtime-adapter` | Open | Runtime / Sandboxing | Standalone milestone |
| **#19** | feat(protocol): deliver M10 AG-UI protocol and agent control-plane foundation | Duwn | `main` | `feat/m10-ag-ui-protocol` | Open | Protocol / Control Plane | Base for daemon IPC |
| **#18** | feat(daemon): deliver M09 core daemon event-bus and multi-agent supervisor | Duwn | `main` | `feat/m09-core-daemon-supervisor` | Open | Daemon Core | Predecessor to M10/M11 |
| **#17** | feat(runtime): complete M08 OpenCode adapter and multi-agent coordination foundation | Duwn | `main` | `feat/m08-opencode-adapter` | Open | Runtime | Predecessor to M11 |
| **#16** | feat(agent-system): complete M06 agent system foundation and core contracts | Duwn | `main` | `feat/m06-agent-system-foundation` | Open | Core System Contracts | Root contract foundation |
| **#15** | feat(security): complete M04 security baseline and permission boundary enforcement | Duwn | `main` | `feat/m04-security-baseline` | Open | Security Architecture | Base security foundation |

### Workstream Separation Analysis:
1. **Parallel Active Rust Workstream:** PRs #16, #18, #19, and active development in `codex-rs/` represent the native backend daemon, IPC-03 server, and model provider subsystem. This workstream MUST proceed completely undisturbed.
2. **VS Code Extension Workstream:** PR #21 contains M14-UI-00 and M14-UI-01. The current feature branch `feat/m14-vscode-extension-host-hardening` forms a stacked PR targeting `feat/m14-codeoss-agents-workbench` (#21).

---

## 2. Standardized Branch Naming Conventions

All future branches must adhere strictly to the following semantic structure:

`<type>/<milestone-id>-<short-descriptive-slug>`

### Allowed Types:
- `feat/`: Milestone features and product deliverables (e.g., `feat/m14-vscode-extension-host-hardening`).
- `fix/`: Bug fixes and defect remediations (e.g., `fix/m14-ipc-reconnect-leak`).
- `refactor/`: Code reorganization with zero behavior alteration.
- `docs/`: Documentation reviews, architecture specifications, and governance records.
- `chore/`: CI/CD workflows, build scripts, toolchain dependency upgrades.

### Strict Formatting Rules:
- Lowercase alphanumeric characters, dashes (`-`), and forward slashes (`/`) only.
- Milestone identifier required for feature branches: `m01` through `m14`.
- Maximum branch name length: 50 characters.

---

## 3. Branch Lifecycle State Machine & Evidence Gates

```
[ Concept ] ──► [ Draft / Spike ] ──► [ In Development ] ──► [ Review Ready ] ──► [ Merged ] ──► [ Cleaned ]
      │                   │                     │                     │
      └───────────────────┴─────────────────────┴─────────────────────► [ Abandoned ]
```

### Gate 1: Concept to In Development
- **Requirement:** Associated Milestone ticket or issue documented in repo.
- **Evidence:** Clean branching from verified remote base branch (`git fetch origin`).

### Gate 2: In Development to Review Ready (PR Opened)
- **Requirement 1 (Code Cleanliness):** `npm run lint` and `npm run typecheck` pass with zero warnings or errors.
- **Requirement 2 (Unit Test Baseline):** 100% pass rate on unit test suite (`npm run test:unit`). Zero regressions against predecessor.
- **Requirement 3 (Host Integration Verification):** All Extension Development Host integration tests pass inside real Electron environment (`npm run test:integration`).
- **Requirement 4 (VSIX Packaging Integrity):** Lean production packaging verified with `@vscode/vsce package --no-dependencies`.
- **Requirement 5 (Security Baseline):** Fail-closed guards verified against prototype pollution, path breakout, and workspace untrusted states.
- **Requirement 6 (Evidence Documentation):** Comprehensive review dossier authored under `docs/reviews/<MILESTONE>/<RUN_ID>/`.

### Gate 3: Review Ready to Merged
- **Requirement:** Formal peer review approval.
- **Requirement:** Cross-platform CI matrix passes green on all runners (`ubuntu-latest`, `windows-latest`).
- **Enforcement:** Strictly no self-merging without explicit human maintainer sign-off.

---

## 4. Stacked Pull Request Ancestry & Retargeting Protocol

When multiple dependent features are developed prior to merge of earlier PRs, stacked PRs must follow this strict choreography:

```
main
  ▲
  │ (PR #21: feat/m14-codeoss-agents-workbench)
  ├─────────────────────────────────────────────────┐
                                                    ▲
                                                    │ (New Stacked PR: feat/m14-vscode-extension-host-hardening)
                                                    └──────────────────────────────────────────────
```

### Invariant Rules:
1. **Targeting:** A stacked PR MUST target its parent feature branch (`feat/m14-codeoss-agents-workbench`), NOT `main`.
2. **Diff Isolation:** Targeting the parent ensures the GitHub PR diff displays ONLY changes unique to M14-UI-02, without duplicating commits from M14-UI-00 and M14-UI-01.
3. **Rebasing / Retargeting Cascade:**
   - If parent PR #21 is squashed and merged into `main`, child branch `feat/m14-vscode-extension-host-hardening` must be retargeted to `main` via `gh pr edit --base main`.
   - If commits are rebased on the parent, child branch must be rebased onto parent with `--onto`.
4. **Draft Designation:** Stacked PRs remain in `Draft` state until parent PR achieves formal approval and merge readiness.

---

## 5. Stale & Merged Branch Inventory (Manual Hygiene Only)

*Note: In compliance with non-destructive governance principles, NO automated branch deletion is performed. Branches are inventoried below for maintainer evaluation:*

| Remote Branch | Associated PR | State | Recommended Action |
| :--- | :--- | :--- | :--- |
| `origin/feat/m04-security-baseline` | PR #15 | Active Open PR | Retain until PR #15 merged |
| `origin/feat/m06-agent-system-foundation` | PR #16 | Active Open PR | Retain until PR #16 merged |
| `origin/feat/m08-opencode-adapter` | PR #17 | Active Open PR | Retain until PR #17 merged |
| `origin/feat/m09-core-daemon-supervisor` | PR #18 | Active Open PR | Retain until PR #18 merged |
| `origin/feat/m10-ag-ui-protocol` | PR #19 | Active Open PR | Retain until PR #19 merged |
| `origin/feat/m11-opencode-runtime-adapter` | PR #20 | Active Open PR | Retain until PR #20 merged |
| `origin/feat/m14-codeoss-agents-workbench` | PR #21 | Active Open PR | Retain (Parent branch for UI track) |
| `origin/feat/opencode-runtime-adapter` | PR #14 | Merged | Candidate for manual maintainer pruning |

---

## 6. Recommended Branch Protection Rules

To protect repository stability while supporting multi-agent parallel development, the following rules should be configured in GitHub repository settings:

1. **Branch Protection for `main`:**
   - Require pull request before merging (minimum 1 approval).
   - Require status checks to pass before merging:
     - `Build & Test (ubuntu-latest)`
     - `Build & Test (windows-latest)`
     - `Validate VSIX packaging`
   - Require linear commit history or squash merging.
   - Do not allow bypass by administrators.

2. **Branch Protection for Milestone Staging Branches (`feat/m*`):**
   - Require linear history.
   - Require status checks to pass on push.
   - Block force-pushes (`git push --force` forbidden).
