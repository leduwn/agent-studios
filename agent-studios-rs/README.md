# Agent Studios Rust Workspace (`agent-studios-rs`)

This workspace contains the core Rust implementation of the **Agent Studios** platform.

## Architectural Boundary & Relationship to Codex

Agent Studios builds on top of baseline concepts from OpenAI Codex, but keeps a **strict, decoupled architectural boundary**:
- Upstream Codex runtime source is located in `codex-rs/` and is kept as close to upstream snapshot commits as possible.
- Agent Studios specific domain models, orchestration logic, and control plane foundations reside exclusively in this workspace (`agent-studios-rs/`).
- Crates in `agent-studios-rs` do not directly depend on internal private crates of `codex-rs`. Any future integration with Codex CLI or Codex server runtimes must be achieved via clean adapter crates and protocol boundaries.

## Workspace Crates

| Crate | Path | Responsibility |
|---|---|---|
| `agent-studios-protocol` | `crates/protocol` | Strongly typed UUID identifiers, state machines, enums, domain models, and serializable event envelopes. Zero external business dependencies. |
| `agent-studios-control-plane` | `crates/control-plane` | Control plane state engine, iterative DAG task graph with cycle detection, clock abstraction, monotonic event store, hierarchical cancellation, and event replay. |

## Prerequisites

- Rust 1.85+ (Edition 2024 support)
- Cargo

## Development & Verification Commands

All commands can be executed from the repository root or within `agent-studios-rs/`:

```bash
# Check compilation across all targets
cargo check --manifest-path agent-studios-rs/Cargo.toml --workspace --all-targets

# Run the test suite (unit tests and integration tests)
cargo test --manifest-path agent-studios-rs/Cargo.toml --workspace

# Lint with Clippy (warnings treated as errors)
cargo clippy --manifest-path agent-studios-rs/Cargo.toml --workspace --all-targets -- -D warnings

# Verify rustfmt compliance
cargo fmt --manifest-path agent-studios-rs/Cargo.toml --all -- --check
```

## Documentation

Comprehensive architecture guides and specifications are located in `docs/agent-studios/`:
- `ARCHITECTURE.md` — High-level platform architecture and dual-mode UX model.
- `CONTROL_PLANE.md` — Detailed control plane engine, state machines, DAG validation, and event-sourcing replay.
- `ROADMAP.md` — Strategic roadmap from foundation to production.
- `UPSTREAM.md` — Snapshot synchronization strategy and upstream lock maintenance.
