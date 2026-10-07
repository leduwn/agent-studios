# Agent Studios — Upstream References & Architectural Lineage

> **Status**: Core Architecture & Governance Specification
> **Precedence**: Subservient to `MASTER_VISION.md` and `PRODUCT_PRINCIPLES.md`.

---

## 1. Upstream References Philosophy

Agent Studios stands on the shoulders of giants. We believe in engineering honesty: we explicitly acknowledge, document, and study upstream open-source projects rather than obscuring our heritage behind marketing claims.

At the same time, **Agent Studios maintains a completely distinct and independent product identity**:
- We are not a clone, wrapper, or superficial rebranding of any single upstream project.
- We synthesize best-in-class ideas from multiple software engineering domains into a cohesive, production-grade development environment.

---

## 2. Upstream Project Roles & Relationships

```text
┌─────────────────────────────────────────────────────────────────┐
│                        UPSTREAM LINEAGE                         │
├─────────────────┬───────────────────┬───────────────────────────┤
│ Upstream        │ Primary Role      │ Architectural Influence   │
├─────────────────┼───────────────────┼───────────────────────────┤
│ OpenAI Codex    │ Execution Engine  │ Agent loop, tools, sandbox│
│ OpenCode        │ Open Inspiration  │ BYOK, providers, desktop  │
│ AgentTeams      │ Coordination Lens │ Manager-worker, approvals │
│ Code-OSS        │ IDE Foundation    │ Professional workbench    │
└─────────────────┴───────────────────┴───────────────────────────┘
```

---

## 3. OpenAI Codex (`codex-rs`)

**Role: Core Internal Agent Execution Engine**

OpenAI Codex is the foundational execution engine powering internal agent threads:
- **What We Inherit**:
  - The portable core agent loop, tool dispatch (`exec_command`, `apply_patch`), and interactive approvals.
  - Native sandbox containment and execution monitoring.
  - Context history management and turn compaction algorithms.
  - Native support for Model Context Protocol (MCP), Skills (`SKILL.md`), Plugins, hooks, and `AGENTS.md`.
  - Hierarchical sub-agent spawning via `AgentControl`.
- **What We Add**:
  - Open multi-provider protocol adapters (Anthropic Messages, Google Gemini, OpenAI Chat Completions, OpenRouter, local models).
  - Deterministic multi-agent Control Plane with DAG validation (Kahn's algorithm).
  - True parallel physical isolation via dedicated Git worktrees.
  - Ephemeral index change capture, content-addressed artifact storage, and patch reconciliation.
- **Maintenance Invariant**: Upstream modifications to `codex-rs/` are strictly minimized to generic, provider-neutral seams (documented in `CODEX_PATCHES.md`).

---

## 4. OpenCode (`opencode-ai`)

**Role: Reference for Provider Openness & Desktop Ergonomics**

OpenCode demonstrates that AI coding tools can be open, flexible, and model-agnostic:
- **What We Learn from OpenCode**:
  - First-class Bring-Your-Own-Key (BYOK) without mandatory proprietary account logins.
  - First-class treatment of local and self-hosted models (Ollama, LM Studio, vLLM).
  - Fast, responsive, keyboard-driven desktop developer ergonomics.
- **How We Differ**:
  - OpenCode is a TypeScript/Node.js desktop application; Agent Studios is a native Rust-powered Control Plane integrated with Code-OSS.
  - Agent Studios provides deterministic DAG scheduling, physical worktree isolation, and artifact reconciliation.
  - OpenCode is planned as an **external runtime adapter** (Milestone M11), not our internal execution engine.

---

## 5. AgentTeams

**Role: Reference for Manager-Worker & Human-in-the-Loop Coordination**

AgentTeams explores multi-agent collaboration in software engineering:
- **What We Learn from AgentTeams**:
  - Clear manager-worker task delegation patterns.
  - The necessity of human-in-the-loop review at critical execution checkpoints.
  - Specialized agent roles (planner, coder, reviewer, tester).
- **How We Differ**:
  - Agent Studios rejects turning the IDE into a social-network simulation of bots chattering in chat rooms.
  - Agent Studios roots all agent coordination in **native Git primitives and compiler feedback**, not conversational consensus.

---

## 6. Code-OSS

**Role: Permanent Professional IDE Foundation**

Code-OSS (the open-source core of VS Code) provides our developer workbench:
- **What We Learn from Code-OSS**:
  - Developers demand professional, reliable IDE capabilities (Monaco editor, LSP, DAP debugging, terminal multiplexing, Git source control, extensions).
  - Custom webview or Monaco-wrapper text editors consistently fail to match developer expectations.
  - Open extensions ecosystem.
- **Our Integration**:
  - Code-OSS is our permanent IDE foundation (Milestones M13–M15).
  - Integrated via a single `Agents` Activity Bar entry, preserving familiar VS Code ergonomics.

---

## 7. Attribution & Independent Branding

- **License Compliance**: Agent Studios strictly respects the open-source licenses of all upstream dependencies (Apache 2.0, MIT). Original copyright notices and license headers are preserved.
- **Independent Identity**: Agent Studios is independently branded and developed. It is not sponsored, endorsed, or affiliated with OpenAI, Microsoft, or Anthropic.
