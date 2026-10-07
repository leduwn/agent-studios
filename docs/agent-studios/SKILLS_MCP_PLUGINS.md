# Agent Studios — Skills, MCP & Plugins Architecture

> **Status**: Planned Architecture Specification (Target: Milestone M16)
> **Current Reality**: Upstream Codex (`codex-rs`) natively supports Skills, MCP, and `AGENTS.md`. Surfacing full discovery, GitHub installation, and management within the Agent Studios UI is **PLANNED** for Milestone M16.
> **Precedence**: Subservient to `MASTER_VISION.md` and `PRODUCT_PRINCIPLES.md`.

---

## 1. Architectural Role & Heritage

Agent Studios builds upon the extensible capabilities of OpenAI Codex:
- **Zero Reimplementation**: We do not build a proprietary tool or skill system. We leverage upstream Codex's native implementation of MCP (Model Context Protocol), Skills (`SKILL.md`), and memory hierarchy (`AGENTS.md`).
- **Surface Elevation**: Milestone M16 exposes these powerful capabilities through intuitive Agent Studios UI controls, CLI commands, and project configurations.

---

## 2. Skills System (`SKILL.md`)

Skills are modular, reusable instruction and tool packages defined in markdown.

### A. Format & Compatibility
- Full compatibility with the upstream OpenAI Codex `SKILL.md` format.
- Frontmatter defines name, description, parameters, tools, and triggers.

### B. Scopes
Skills are discovered and loaded across three hierarchical scopes:
1. **Workspace Scope**: Checked into `.agent-studios/skills/` or `.codex/skills/` within the repository. Project-specific and shared via Git.
2. **User Scope**: Stored in `~/.agent-studios/skills/` or `~/.codex/skills/`. Available across all projects on the developer's machine.
3. **Built-in Scope**: Packaged directly with Agent Studios (standard refactoring, test generation, git operations).

### C. Installation via GitHub
Developers can install shared community skills directly via chat or command palette:
```text
/skill install <owner>/<repo>
/skill install <owner>/<repo>@<branch-or-tag>
```
Installs the repository into the user or workspace skill store.

### D. Progressive Loading
To preserve context window capacity, skills are **progressively loaded**:
- Only skill summaries and triggers reside in the baseline system prompt.
- Full instructions and specialized prompts load into the context window only when the skill is invoked by user intent or Coordinator routing.

### E. Skill Security & Provenance
Because skills can suggest shell commands and execute tools:
- **Provenance Tracking**: Installed skills record source repository, commit SHA, installation timestamp, and signature.
- **Security Warning on Install**: Untrusted external skills require explicit user confirmation.
- **Budget Gating**: Skill executions remain subject to the Control Plane's pre-execution tool budgets and approvals.

---

## 3. Model Context Protocol (MCP) Integration

Agent Studios connects to local and remote MCP servers through Codex's native MCP client.

### MCP Scopes
1. **Workspace MCP**: Configured in `.agent-studios/mcp.json`. Enables project-specific servers (e.g., local database query server, dev server inspector).
2. **Global MCP**: Configured in `~/.agent-studios/mcp.json`. Developers connect personal tools (GitHub, Linear, Slack, Sentry, Brave Search).

---

## 4. Plugins & Lifecycle Hooks

- **Plugins**: Bundles that package together Skills, MCP server definitions, and configuration presets into a single distributable archive.
- **Lifecycle Hooks**: Deterministic shell or script hooks triggered at key agent events:
  - `pre_turn`: Runs before an agent turn begins.
  - `post_task`: Runs after a task completes (e.g., auto-formatting, linter check).
  - `on_reconciliation`: Runs integration validation tests after patch reconciliation.

---

## 5. Hierarchical Repository Guidelines: `AGENTS.md`

Agent Studios inherits Codex's recursive discovery of repository instructions:

```text
my-repo/
├── AGENTS.md                  <-- Global project architecture & conventions
├── backend/
│   ├── AGENTS.md              <-- Backend-specific rules (Rust, cargo, DB)
│   └── src/
└── frontend/
    ├── AGENTS.md              <-- Frontend-specific rules (React, Tailwind)
    └── src/
```

- When an agent runs inside `backend/src/`, the engine merges root `AGENTS.md` and `backend/AGENTS.md`.
- Instructions are strictly scoped to the agent's allocated directory and task focus.
