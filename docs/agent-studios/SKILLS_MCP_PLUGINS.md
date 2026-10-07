# Agent Studios — Skills, MCP & Plugins Architecture

> **Status**: Planned Architecture Specification (Target: Milestone M16)
>
> **Current Reality**: Upstream Codex (`codex-rs`) natively supports Skills, MCP, and `AGENTS.md`. Surfacing full discovery, GitHub installation, and management within the Agent Studios UI is **PLANNED** for Milestone M16.
>
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
- **Full Standard Compatibility**: Native compatibility with OpenAI Codex `SKILL.md`, Claude Code `SKILL.md`, and Agent Studios native skills.
- Frontmatter defines name, description, parameters, tools, and triggers.

### B. Orthogonal Taxonomy: Scope vs. Source/Distribution

Agent Studios strictly decouples **where/for whom a skill is active** (Scope) from **where the skill came from** (Source / Distribution Provenance):

```text
Skill
├── Scope (Where / for whom the skill is active)
│   ├── Global  (~/.agent-studios/skills/)
│   ├── Project (<repo>/.agent-studios/skills/)
│   └── Agent   (<workspace>/.agent-studios/agents/<agent>/skills/)
│
└── Source / Distribution (Where the skill came from)
    ├── GitHub
    ├── Local Folder
    ├── Registry
    └── Bundled / Built-in (and future source types)
```

#### 1. Canonical Skill Scopes (Activation Domain)
Skills are discovered and activated across three canonical hierarchical scopes:
- **Global Scope**:
  - Location: `~/.agent-studios/skills/`
  - Available across all projects and sessions on the developer's machine.
- **Project Scope**:
  - Location: `<repo>/.agent-studios/skills/`
  - Project-specific and shared with the team via Git.
- **Agent Scope**:
  - Location: `<workspace>/.agent-studios/agents/<agent>/skills/`
  - Scoped specifically to a specialized agent role within a studio or team.

#### 2. Source / Distribution Provenance
Source provenance describes origin, distribution, and installation channel:
- **GitHub**: Installed directly from public or private Git repositories.
- **Local Folder**: Linked or imported from a local filesystem directory.
- **Registry**: Community or organizational skill catalogs.
- **Bundled / Built-in**: Core standard capabilities packaged with the Agent Studios distribution.

*(Architectural Invariant: Built-in is a distribution provenance, not a scope. We do **not** define "Built-in = Global Scope" as a canonical invariant. How bundled skills are surfaced into effective scopes—whether global defaults, project overrides, or role presets—is an implementation and policy decision to be formalized during M16 design.)*

### C. Installation via GitHub (Planned Milestone M16)
The canonical contract is installing standard `SKILL.md`-compatible skills from ordinary GitHub repositories:

```text
/skill install https://github.com/user/repo
```

Optional convenience shorthand for future tooling (explicitly planned/optional):
```text
/skill install <owner>/<repo>
/skill install <owner>/<repo>@<branch-or-tag>
```

#### UI Concept (Planned Milestone M16)
```text
Install Skill
├── GitHub URL (e.g., https://github.com/user/repo)
├── Local Folder
└── Registry (Future community catalog)
```

Target compatibility across formats:
- Codex `SKILL.md`
- Claude Code `SKILL.md`
- Agent Studios native skill

*(Note: GitHub skill installation and management UI are explicitly **PLANNED** for Milestone M16; this is not currently implemented in existing code.)*

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
1. **Workspace / Project MCP**: Configured in `.agent-studios/mcp.json`. Enables project-specific servers (e.g., local database query server, dev server inspector).
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
