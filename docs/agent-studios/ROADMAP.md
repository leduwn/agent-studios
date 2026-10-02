# Agent Studios Architectural Roadmap

This roadmap defines the strategic progression for building Agent Studios upon the Codex upstream baseline. Milestones reflect architectural progression; uncompleted milestones remain prospective and directional.

---

### M01: Bootstrap Codex Upstream (Completed)
- Import Codex baseline as a clean source snapshot (`d25c114d494ddb693290b76bf5e5f64ecbdb38fc`, 2026-10-02) with clean Git root history.
- Record exact upstream baseline SHA in machine-readable `upstream/codex.lock.json`.
- Maintain required Codex license notices and attribution in `LICENSE`, `NOTICE`, and `UPSTREAM.md`.
- Validate Windows native baseline compilation (`cargo build -p codex-cli`).
- Establish snapshot-based upstream synchronization architecture and maintenance workflows.

### M02: Agent Studios Control-Plane Foundation (Completed)
- Scaffold core control plane library crate (`agent-studios-control-plane`) and protocol types (`agent-studios-protocol`).
- Define deterministic state structures: TaskGraph DAG engine, Agent lifecycle, Run execution, Approvals, Artifacts, and Event envelopes.
- Implement staged transactional pipeline (`commit_transaction`) and atomic batch persistence (ALL-or-ZERO) with sequence continuity and regression protection.
- Strict event replay engine with full domain validation and corruption rejection (19 corruption test cases + duplicate entity guardrails).
- Hierarchical cancellation propagation and pre-execution dependency readiness checks.
- Eliminated all mutable store/state escape hatches (`store_mut` removed).

### M03: Provider Core & Model Registry (Completed on `feat/provider-core`)
- Establish `agent-studios-provider` crate.
- Decouple `ProviderDefinition` (vendor/gateway metadata) from `ProviderInstance` (concrete configured endpoint + credentials).
- Establish wire protocol decoupling via `ProtocolFamily` (`OpenAiResponses`, `OpenAiChatCompletions`, `AnthropicMessages`, `GeminiGenerateContent`, `Custom`).
- Implement zero-plaintext secret architecture using `SecretReference` and `SecretBackend` (EnvironmentVariable, OsCredentialStore, External).
- Enforce strict endpoint security: forbid URL userinfo/credentials, forbid URL fragments, reject sensitive static headers (`Authorization`, `x-api-key`, etc.).
- Introduce tristate `CapabilitySupport` (`Unknown`, `Unsupported`, `Supported`) across 10 distinct model capabilities to prevent false-negative capability assumptions.
- Provide `ModelDescriptor`, non-zero `ModelLimits`, and composite `ModelRef` (`provider_instance_id` + `model_id`) for deterministic provider-neutral routing.
- Implement `ProviderRegistry`, `ModelRegistry`, and coordinating facade `ProviderCatalog` with safe removal invariants (rejection on in-use instances/definitions).
- Implement exportable `ProviderCatalogSnapshot` with atomic, staged import validation.
- *(Note: Provider Core models metadata and configuration only; network requests and LLM drivers arrive in subsequent milestones.)*

### M04: Codex Runtime ↔ Provider Core Bridge / OpenAI Responses Integration (Next Milestone)
- Integrate Codex upstream Responses API client with the provider registry.
- Bridge `ModelRef` and `ProviderInstance` into Codex runtime client options.
- Resolve secrets via `SecretReference` at runtime without exposing credentials in configuration.
- Support configurable custom endpoint base URLs (enabling 9Router, local proxies, and third-party gateways).

### M05: Chat Completions Protocol Adapter
- Implement OpenAI Chat Completions protocol translation adapter.
- Map streaming chunks, tool call formats, and system instructions between internal abstractions and standard `/v1/chat/completions`.

### M06: Anthropic Messages Protocol Adapter
- Implement Anthropic Messages protocol translation adapter.
- Provide token caching markers, thinking parameter mapping, and tool call serialization.

### M07: Google Gemini Protocol Adapter
- Implement Google Gemini `generateContent` adapter.
- Support multimodal payloads and Gemini-specific function calling conventions.

### M08: Internal Multi-Agent Extension
- Enhance internal agent runner to instantiate multiple isolated agent configurations.
- Allow per-agent role specifications, provider assignments, reasoning depths, and tool budgets.

### M09: Worktree, Task & Artifact Orchestration
- Extend Git worktree isolation for concurrent workers based on upstream Codex worktree infrastructure.
- Implement deterministic artifact tracking, versioning, and reconciliation workflows.

### M10: External Runtime Interface
- Define standard `AgentRuntime` interface (lifecycle, communication, supervision, capabilities).
- Establish process-level process containment and IPC protocol.

### M11: OpenCode Runtime Adapter
- Implement runtime adapter for OpenCode CLI sessions.
- Map input/output streams and tool interactions to Agent Studios control plane events.

### M12: Claude Code Runtime Adapter
- Implement runtime adapter wrapping Claude Code CLI.
- Handle session resumption, approval delegation, and output streaming.

### M13: Desktop Shell
- Scaffold Windows desktop application container (Windows x64).
- Integrate native IPC channel with the Agent Studios control plane backend.

### M14: Agent Mode UX
- Implement high-level conversational interface.
- Provide task timeline, tool execution inspect panels, interactive approvals, and diff visualizer.

### M15: IDE Mode UX
- Integrate Monaco Editor and file tree project navigation.
- Embed xterm.js terminal emulator, git diff panes, and side-by-side agent collaborator panels.

### M16: Skills, MCP & Plugins UX
- Build management UI for discovering, configuring, and toggling `AGENTS.md`, `SKILL.md`, and MCP servers.
- Provide secure credential input targeting Windows Credential Manager.

### M17: Packaging, Installer & Auto-Updater
- Configure Windows native installers (MSI / NSIS / WiX).
- Implement background update checks and secure patching channels.

### M18: Hardening & End-to-End Integration Testing
- Comprehensive multi-agent stress tests, worktree conflict benchmarks, and recovery drills.
- Windows security audit and memory profiling.

### M19: Windows Stable Release
- Final polish, user documentation, and initial Windows x64 public distribution.
