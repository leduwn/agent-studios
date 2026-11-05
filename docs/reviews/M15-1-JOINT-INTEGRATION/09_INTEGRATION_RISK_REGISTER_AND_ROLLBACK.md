# Agent Studios — Milestone M15.1 Joint Integration Preflight
## Document 09: Integration Risk Register, Mitigations & Rollback Playbooks

**Milestone:** M15.1 (Joint Integration Preflight & Cross-Platform Release Gate)  
**Date:** 2026-11-06  
**Role:** Principal Integration Engineer & Release Architect  
**Validation Branch:** `integration/m15-1-joint-validation`  
**Status:** RISKS AUDITED & MITIGATED  

---

### 1. Risk Register & Mitigation Matrix

| ID | Risk Description | Severity | Likelihood | Mitigation Strategy | Verification Status |
| :--- | :--- | :---: | :---: | :--- | :---: |
| **R-01** | **Linux CI Sandbox Fail-Closed Panics**<br>Tests exercising child process spawning fail on Linux runners lacking Win32 Job Objects. | **HIGH** | **HIGH** | Configured `SandboxPolicy::unconstrained()` for mock child process tests on `not(windows)`. Production continues to fail closed. | **MITIGATED**<br>(PR #33 commit `b0696ace58`) |
| **R-02** | **Stale Binary Provenance in CI**<br>CI runner evaluates obsolete daemon binary from historical worktrees (`agent-studios-m15-04`). | **CRITICAL** | **MED** | `AgsdBinaryResolver.ts` enforces checked-out workspace build boundaries under `CI=true` and rejects obsolete worktree regex patterns. | **MITIGATED**<br>(PR #34 / 13 unit tests pass) |
| **R-03** | **Premature Baseline Contamination**<br>Unreviewed code merged directly into `integration/m15-unified-codebase` or `main`. | **CRITICAL** | **LOW** | Dedicated validation branch (`integration/m15-1-joint-validation`) and isolated worktree. Zero merge into baseline until full sign-off. | **PREVENTED**<br>(Enforced by Charter) |
| **R-04** | **Disk Space Exhaustion on Windows**<br>Multiple active worktrees with heavy `node_modules` and `target/` exhaust host disk space. | **HIGH** | **HIGH** | Utilized Windows directory junctions (`mklink /J`) for `node_modules`. Reused shared Cargo build cache. | **MITIGATED**<br>(Worktree operational) |
| **R-05** | **Accidental Billable Inference**<br>Automated test suite executes live LLM API calls incurring billable token charges. | **HIGH** | **LOW** | Enforced zero-dollar invariant. In-memory mock transports, offline CLI flags (`--demo`, `--preview`), dummy keys. | **VERIFIED**<br>($0.00 spent across all runs) |
| **R-06** | **IPC Sequence Gap on Daemon Crash**<br>Network drop or SIGKILL leaves frontend state projection out of sync with backend events. | **HIGH** | **MED** | Monotonic sequence watermarking (`since_sequence`) and SQLite WAL replay backfill. Reconnect restores full history without task resubmission. | **VERIFIED**<br>(84/84 E2E tests pass) |

---

### 2. Rollback & Remediation Playbooks

#### 2.1 Playbook A: Joint Validation Branch Disposal (Zero Risk)
If joint integration preflight encounters an insurmountable blocker, the validation branch and worktree can be cleanly discarded with zero impact to the primary repository:
```bash
# 1. Navigate to primary repository
cd D:/Projects/agent-studios

# 2. Remove dedicated validation worktree
git worktree remove D:/Projects/agent-studios-active/m15-1-joint-validation --force

# 3. Delete local validation branch
git branch -D integration/m15-1-joint-validation
```
*Outcome:* Baseline `integration/m15-unified-codebase` and PR head branches remain 100% untouched.

#### 2.2 Playbook B: Individual PR Repair Isolation
If a defect is isolated to one specific PR:
1. Do not apply patches directly to `integration/m15-unified-codebase`.
2. Apply the fix strictly to the PR's designated branch (`feat/m15-1-backend-cli-consolidation`, `fix/m15-unified-cross-platform-ci`, or `feat/m15-ui-06-agent-ide-continuity`).
3. Re-merge the repaired commit into `integration/m15-1-joint-validation` for fresh re-testing.

#### 2.3 Playbook C: Baseline Pin Recovery
In the event of accidental baseline modification:
```bash
git checkout integration/m15-unified-codebase
git reset --hard ec2058256a0001f869b968f4d787b7378e413eb8
git clean -fd
```
Restores the exact, uncompromised baseline commit.
