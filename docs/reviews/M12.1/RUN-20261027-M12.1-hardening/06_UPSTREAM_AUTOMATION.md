# Milestone M12.1 — Upstream Intelligence & CI Automation Hardening

- **Milestone**: `M12.1 — Claude Code Runtime Hardening & Protocol Corrections`
- **Execution Run**: `RUN-20261027-M12.1-hardening`
- **Date**: `2026-10-27T23:59:00+07:00`
- **Lead Owner**: Agent D (Upstream Intelligence) & Principal Engineering Lead
- **Target Repository**: `https://github.com/leduwn/agent-studios`
- **Impacted Files**:
  - `.github/workflows/upstream-intelligence.yml`
  - `.github/upstream-state.json`

---

## 1. Technical Audit of M12-REV-008: Upstream Intelligence Reliability

### 1.1 Defect Mechanism

The initial M12 workflow `.github/workflows/upstream-intelligence.yml`:

1. **Incomplete Scope**: Monitored only 4 repositories (`openai/codex`, `anomalyco/opencode`, `anthropics/claude-code`, `microsoft/vscode`), omitting `777genius/agent-teams-ai` and `agentscope-ai/AgentTeams`.
2. **Issue Flooding / Duplication**: On every scheduled run where upstream differed from the hardcoded target, `await github.rest.issues.create()` created a new GitHub issue without checking for existing open issues, resulting in alert fatigue and spam.
3. **Static Targets**: Target versions were hardcoded directly in bash script variables instead of maintaining a trackable, committed state file.
4. **Schedule Discrepancy**: Ran weekly (`0 2 * * 1`) instead of daily, causing latency in detecting upstream breaking changes.

---

## 2. Remediated Architecture

### 2.1 State Management via `.github/upstream-state.json`

A structured version state file `.github/upstream-state.json` tracks known baselines:

```json
{
  "last_updated": "2026-10-27T23:59:00Z",
  "repositories": {
    "openai/codex": {
      "target_version": "rust-v0.164.0",
      "notes": "Upstream codex base; patches 001-004 in CODEX_PATCHES.md"
    },
    "anomalyco/opencode": {
      "target_version": "v1.19.4",
      "notes": "OpenCode CLI runtime adapter (M11)"
    },
    "anthropics/claude-code": {
      "target_version": "v2.1.311",
      "notes": "Claude Code CLI runtime adapter (M12/M12.1)"
    },
    "microsoft/vscode": {
      "target_version": "1.143.1",
      "notes": "Code-OSS Shell Integration foundation (M13)"
    },
    "777genius/agent-teams-ai": {
      "target_version": "v2.18.4",
      "notes": "AGPL-3.0 clean-room visual UX reference only (zero code copy)"
    },
    "agentscope-ai/AgentTeams": {
      "target_version": "v1.2.5",
      "notes": "Multi-agent framework reference"
    }
  }
}
```

### 2.2 Deduplicated Issue Management

In `.github/workflows/upstream-intelligence.yml`:

- The script queries GitHub API for each repository's latest release or latest tag (fallback if releases do not exist).
- If drift is detected against `target_version`, the script checks for existing open issues with the label `upstream-intelligence` or matching title:
  ```javascript
  const existingIssues = await github.rest.issues.listForRepo({
    owner: context.repo.owner,
    repo: context.repo.repo,
    state: "open",
    labels: "upstream-intelligence",
  });
  ```
- If an open issue exists, it appends a comment or updates the issue body with the latest detection timestamp, preventing duplicate issue creation.
- If no open issue exists, a new alert issue is created.

### 2.3 Schedule & Execution Guarantees

- Schedule: Daily at 02:00 UTC (`0 2 * * *`).
- Permissions: `contents: read`, `issues: write`.
- Read-Only Guarantee: Zero automated code pulling, zero automatic merging, zero rebasing.
