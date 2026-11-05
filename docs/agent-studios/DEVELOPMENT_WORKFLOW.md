# Agent Studios — Development Workflow & Contributor Guide

> **Status**: Authoritative Developer & Contributor Guide  
> **Milestone**: M15 Consolidation & Canonical Master Reconstruction  
> **Target Branch**: `integration/m15-unified-codebase`  
> **Reference**: Implements `BUILD_BASELINE.md`, `DEVELOPMENT.md`, and CI Matrix Standards

---

## 1. System Prerequisites & Environment Setup

Agent Studios is developed as a dual-language monorepo (Rust backend + TypeScript/VS Code extension).

### Required Toolchains
- **Rust Toolchain**: Rust 1.80.0+ (stable). Install via `rustup`:
  ```bash
  rustup default stable
  rustup component add clippy rustfmt
  ```
- **Node.js Environment**: Node.js 22.x LTS and npm 10.x+. Install via `nvm` or official installer.
- **Git**: Git 2.40+ with support for `git worktree`.
- **Platform Compilers**:
  - **Windows (Tier 1)**: Visual Studio 2022 Build Tools with C++ core workload (`x86_64-pc-windows-msvc`).
  - **Linux (Tier 1)**: `build-essential`, `pkg-config`, `libssl-dev`, `xvfb` (for headless extension tests).
  - **macOS (Tier 2)**: Xcode Command Line Tools.

---

## 2. Repository Architecture & Directory Layout

```text
agent-studios/
├── agent-studios-rs/           # Backend Cargo Monorepo Workspace
│   ├── Cargo.toml              # Workspace root (15 active crates)
│   ├── Cargo.lock              # Pinned dependency graph
│   ├── crates/
│   │   ├── protocol/           # Wire domain models & strong IDs
│   │   ├── control-plane/      # Deterministic state machine & SQLite WAL
│   │   ├── provider/           # Provider registry & dynamic discovery
│   │   ├── external-runtime/   # Abstract AgentRuntime traits
│   │   ├── claude-code-runtime/# Claude Code CLI adapter & Job Object sandbox
│   │   ├── opencode-runtime/   # OpenCode runtime adapter
│   │   ├── workspace/          # Git worktree isolation & diff streaming
│   │   ├── ipc-protocol/       # Length-prefixed framing & HMAC signing
│   │   └── ipc-transport/      # Native agsd daemon binary & server
│   └── target/                 # Shared build output directory
│
├── apps/
│   └── agent-studios-vscode/   # Code-OSS AI Extension
│       ├── package.json        # Extension manifest & commands
│       ├── src/
│       │   ├── bridge/         # Native IPC client & transports
│       │   ├── mission-control/# 9-tab Mission Control webview
│       │   ├── chat/           # Activity Bar prompt composer
│       │   └── security/       # 2PC confirmation modals & path guards
│       └── out/                # Compiled JavaScript output
│
├── docs/                       # Authoritative Documentation Hub
│   ├── agent-studios/          # Master vision, PRD, architecture, and roadmap
│   └── reviews/                # Milestone audit reports & evidence bundles
│
└── .github/
    └── workflows/
        └── agent-studios-unified-ci.yml # Cross-platform Ubuntu & Windows CI
```

---

## 3. Build, Lint & Automated Verification Commands

Always run these commands locally before submitting code or opening a pull request:

### 3.1 Rust Backend Quality Gates
Execute from `agent-studios-rs/`:

```bash
# 1. Format check
cargo fmt --check

# 2. Workspace compilation check (all targets: bins, tests, benches)
cargo check --workspace --all-targets

# 3. Strict Clippy linting (warnings treated as fatal errors)
cargo clippy --workspace --all-targets -- -D warnings

# 4. Workspace automated test suite (339+ tests)
cargo test --workspace

# 5. Compile native agsd daemon binary (Debug configuration)
cargo build --bin agsd -p agent-studios-ipc-transport

# 6. Compile optimized release binary
cargo build --release --bin agsd -p agent-studios-ipc-transport
```
*Binary output*: `agent-studios-rs/target/debug/agsd.exe` (or `target/debug/agsd` on Linux).

### 3.2 VS Code Extension Quality Gates
Execute from `apps/agent-studios-vscode/`:

```bash
# 1. Clean dependency installation
npm ci

# 2. ESLint verification (0 errors required)
npm run lint

# 3. TypeScript compilation & strict typecheck (tsc --noEmit)
npm run typecheck

# 4. Compile extension bundle
npm run build

# 5. Unit test execution (657 tests in ~4s)
npm run test:unit

# 6. Real cross-process daemon E2E execution (78 tests in ~12s)
# Point AGSD_BIN_PATH to the compiled daemon:
export AGSD_BIN_PATH="$(pwd)/../../agent-studios-rs/target/debug/agsd"
# On Windows PowerShell:
# $env:AGSD_BIN_PATH = Resolve-Path "../../agent-studios-rs/target/debug/agsd.exe"
npm run test:e2e

# 7. Package standalone distribution VSIX
npx @vscode/vsce package --no-dependencies
```

---

## 4. Running & Debugging in VS Code

### 4.1 Launching the Extension Development Host
1. Open the repository root in VS Code / Code-OSS.
2. Ensure `agsd` is compiled: `cargo build --bin agsd -p agent-studios-ipc-transport`.
3. Press `F5` or select **Run Extension** in the Debug menu.
4. A new **Extension Development Host** window opens with Agent Studios activated.

### 4.2 Interactive Verification
- Check the Activity Bar: Exactly **one** `Agents` icon should appear.
- Open Mission Control: Run Command Palette (`Ctrl+Shift+P`), select **Agent Studios: Open Mission Control Workbench**.
- Verify IPC Connection: Navigate to Tab 9 (`Diagnostics`) to confirm `agsd` socket connection, PID, and protocol handshake.

---

## 5. Offline Testing & Zero Billable Invariant ($0.00)

`[USER_DIRECT_REQUIREMENT]` & `[EXISTING_CANONICAL_DECISION]`
- **Zero Cloud API Spend**: All automated tests MUST run with zero token expenditure.
- **Mock Harvester**: Tests use WireMock HTTP servers or non-billable CLI metadata inspection (`claude --version`).
- **Never Hardcode Secrets**: Test files must never contain real API tokens. Use `InMemorySecretStore` with dummy values (`"test-token-12345"`).

---

## 6. Coding Conventions & Commit Attribution

### Rust Guidelines
- Edition: **2024 Edition**.
- Strong Typing: Use newtypes for domain identifiers (`TaskId`, `SessionId`, `RunId`). Avoid raw strings.
- Error Handling: Use `thiserror` for library crates and `anyhow` for binaries.
- Concurrency: Rely on `tokio` primitives; avoid blocking calls on async worker threads.
- Cross-Platform Safety: Always guard OS-specific code (e.g. `#[cfg(windows)] JobObjectGuard`).

### TypeScript Guidelines
- Strict Mode: Always enabled (`noImplicitAny: true`, `strictNullChecks: true`).
- UI Aesthetics: Follow the **Useful / Native / Quiet** monochrome palette. Use standard Codicons.

### Git Attribution Standards
Every git commit and PR created during automated agent assistance MUST include the co-author attribution lines:

- Commit messages must end with:
  ```text
  Co-Authored-By: Claude Code <noreply@anthropic.com>
  ```
- Pull request descriptions must end with:
  ```text
  🤖 Generated with [Claude Code](https://claude.com/claude-code)
  ```
