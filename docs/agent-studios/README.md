# Agent Studios Documentation Hub

> **The Canonical Architectural Memory of Agent Studios**
> All architectural specifications, master vision documents, product principles, and subsystem guides reside here.

---

## ⚠️ Mandatory Instruction for Future Coding Agents

```markdown
<!-- CANONICAL INSTRUCTION FOR ALL CODING AGENTS -->
For every future Agent Studios milestone:

Before implementation, read:
- docs/agent-studios/MASTER_VISION.md
- docs/agent-studios/PRODUCT_PRINCIPLES.md
- docs/agent-studios/ROADMAP.md

Then read the subsystem-specific documents relevant to the milestone.

If the requested implementation conflicts with MASTER_VISION.md or an established
architectural invariant, stop and report the conflict before changing architecture.
```

---

## 1. Master Foundations

| Document | Title & Role |
| :--- | :--- |
| [`MASTER_VISION.md`](MASTER_VISION.md) | **Master Product Constitution**: Official product statement, formula, short identity, what Agent Studios is NOT, high-level architecture, progressive disclosure UX, cognitive vs. control plane boundary. |
| [`PRODUCT_PRINCIPLES.md`](PRODUCT_PRINCIPLES.md) | **Product Principles & Ruleset**: The 27 immutable architectural principles governing code, design, and execution. |
| [`ROADMAP.md`](ROADMAP.md) | **Master Architectural Roadmap**: Complete status of Milestones M01 through M19 (M01–M08 complete, M09 under final review, M10–M19 planned). |
| [`UPSTREAM_REFERENCES.md`](UPSTREAM_REFERENCES.md) | **Upstream References & Heritage**: Roles of OpenAI Codex, OpenCode, AgentTeams, and Code-OSS. |

---

## 2. Product Architecture & User Experience

| Document | Title & Role |
| :--- | :--- |
| [`PRODUCT_ARCHITECTURE.md`](PRODUCT_ARCHITECTURE.md) | **System Architecture**: High-level topology, desktop process IPC, shared session semantics, Rust crate layout, and status index. |
| [`AGENT_MODE_AND_IDE_MODE.md`](AGENT_MODE_AND_IDE_MODE.md) | **Dual Presentation UX**: Agent-first conversational startup, progressive disclosure flow, Task Graph interactive spec, Code-OSS Activity Bar single `Agents` entry, and lossless mode toggling. |
| [`ORCHESTRATION_OBSERVABILITY.md`](ORCHESTRATION_OBSERVABILITY.md) | **Observability Specification**: Event-sourced read models, interactive DAG UX, subtle connectors, Approval Inbox, structured activity feed, Context Inspector with zero secrets. |
| [`LOCALIZATION.md`](LOCALIZATION.md) | **Localization Architecture**: English & Vietnamese day-one parity, display language vs. agent language decoupling, fallback hierarchy, RTL readiness. |

---

## 3. Core Engine & Runtimes

| Document | Title & Role | Status |
| :--- | :--- | :--- |
| [`CONTROL_PLANE.md`](CONTROL_PLANE.md) | **Deterministic Control Plane**: Single-writer actor (`ControlPlaneActor`), commit-before-broadcast rule, event families, state machines (`Blocked != Cancelled`), and event replay. | Implemented |
| [`PROVIDERS_AND_MODELS.md`](PROVIDERS_AND_MODELS.md) | **Providers & Models**: BYOK open philosophy, Provider vs. Runtime distinction, composite `ModelRef`, provider domain metadata, and SSE transport. | Implemented |
| [`INTERNAL_MULTI_AGENT.md`](INTERNAL_MULTI_AGENT.md) | **Internal Multi-Agent Runtime**: Real Codex agent tree (`AgentControl` spawn), workspace access arbitrator, pre-execution tool budgets, and supervisor failure policies. | Implemented |
| [`WORKTREE_TASK_ARTIFACT.md`](WORKTREE_TASK_ARTIFACT.md) | **Worktrees, Artifacts & Reconciliation**: Dedicated Git worktrees, thread/cwd/worktree invariant, safe retention, ephemeral index change capture, SHA-256 artifacts, reconciliation engine. | **Under Final Review (M09)** |
| [`EXTERNAL_RUNTIMES.md`](EXTERNAL_RUNTIMES.md) | **External Runtimes Architecture**: Standardized `AgentRuntime` lifecycle interface, process containment, planned adapters for OpenCode, Claude Code, and Codex CLI. | **Planned (M10–M12)** |

---

## 4. Platform Integration & Extensions

| Document | Title & Role | Status |
| :--- | :--- | :--- |
| [`CODE_OSS_INTEGRATION.md`](CODE_OSS_INTEGRATION.md) | **Code-OSS Integration**: Code-OSS as permanent IDE foundation, Activity Bar single `Agents` entry, Agents sidebar, built-in AI extension, and shared session toggling. | **Planned (M13–M15)** |
| [`SKILLS_MCP_PLUGINS.md`](SKILLS_MCP_PLUGINS.md) | **Skills, MCP & Plugins**: Codex feature inheritance, GitHub skill installation (`/skill install`), progressive loading, security provenance, MCP scopes, and `AGENTS.md` hierarchy. | **Planned (M16)** |
| [`SECURITY_MODEL.md`](SECURITY_MODEL.md) | **Security & Trust Model**: Layered defense-in-depth, zero plaintext secrets, execution boundary enforcement, sandboxing, and OS credential stores. | Core Specification |

---

## 5. Architectural Invariants Index

Every pull request and modification must preserve these 12 critical invariants:

1. **Codex Capability Preservation**: We inherit upstream Codex capabilities (`codex-rs`), not simplified rewrites.
2. **Control Plane Determinism**: State transitions, DAG acyclicity, and event sequencing are mathematically deterministic; LLMs propose actions, Control Plane enforces state.
3. **Git Remains Git**: Real Git primitives (branches, diffs, worktrees, commits), never mock databases pretending to be VCS.
4. **Physical Worktree Isolation**: Mutating workers execute in dedicated Git worktrees to prevent concurrent write collisions.
5. **UI as Reactive Projection**: UI never owns orchestration truth or scrapes terminal strings.
6. **Provider $\neq$ Runtime**: API wire transport (Anthropic provider) is distinct from execution environment (Claude Code runtime).
7. **`ModelRef` Identity**: Model target is strictly `ModelRef(provider_instance_id, model_id)`, never bare model slugs.
8. **Thread / CWD / Worktree Binding**: Threads do not teleport between worktrees during execution.
9. **Safe Worktree Retention**: Failed or dirty worktrees are never destructively cleaned with `git reset --hard` or `git clean -fdx`.
10. **`Blocked != Cancelled`**: Unfulfilled dependencies or patch conflicts mark tasks as `Blocked`, never `Cancelled`.
11. **Single Activity Bar Entry**: Exactly one primary entry added to Code-OSS Activity Bar: `Agents`.
12. **Zero Plaintext Secrets**: Credentials never touch disk, persistent logs, or inspectable UI trees in plaintext.
