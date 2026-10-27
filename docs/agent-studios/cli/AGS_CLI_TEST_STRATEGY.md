# Agent Studios CLI (`ags`) — Comprehensive Test Strategy & Verification Architecture

> **Document**: `AGS_CLI_TEST_STRATEGY.md`  
> **Status**: Canonical CLI & TUI Test Strategy Specification  
> **Target Subsystem**: `ags` Test Suite / `agent-studios-cli-client` / Ratatui `TestBackend` / Integration Tests  
> **Precedence**: Aligned with `PRODUCT_PRINCIPLES.md` (Principle #20: Exhaustive Verification) and `AGS_CLI_PRODUCT_VISION.md`.

---

## 1. Executive Summary & Quality Gates

The Agent Studios CLI (`ags`) operates directly on production software repositories, mutating files, executing compilers, and orchestrating autonomous multi-agent teams. Consequently, its test suite must be exhaustive, deterministic, and resistant to flakiness.

```
┌────────────────────────────────────────────────────────────────────────┐
│                        Testing Pyramid for ags                         │
├────────────────────────────────────────────────────────────────────────┤
│                                                                        │
│                    ▲                                                   │
│                   / \       Level 4: Headless CLI E2E Tests            │
│                  /   \      (ags run, stdout streams, exit codes)      │
│                 /─────\                                                │
│                /       \     Level 3: Client & Event Pipeline Tests    │
│               /         \    (Embedded & Connected IPC, Resync)        │
│              /───────────\                                             │
│             /             \   Level 2: Ratatui Widget Snapshot Tests   │
│            /               \  (TestBackend, NFC Unicode, Cell Buffers) │
│           /─────────────────\                                          │
│          /                   \ Level 1: Unit & Protocol Invariant Tests│
│         /─────────────────────\(Clap, Hunk Parser, Approvals, State)   │
│                                                                        │
└────────────────────────────────────────────────────────────────────────┘
```

### Quality Invariants
1. **Zero Flakiness**: All async tests utilize deterministic clocks or controlled virtual time where applicable. Network calls are strictly mocked.
2. **Headless Verifiability**: 100% of UI components and layouts must be verifiable without an attached physical TTY/terminal using Ratatui's `TestBackend`.
3. **Cross-Platform Parity**: Windows 11 x64 (Named Pipes, CRLF, PowerShell paths) and Linux/macOS (Unix Domain Sockets, LF, Bash paths) are co-equal Tier-1 test environments.
4. **Clean Terminal Cleanup**: Tests must assert that terminal raw mode and alternate screen buffers are restored under normal exit, `Ctrl+C`, and worker panics.

---

## 2. Level 1: Unit Tests & Protocol Invariants

Unit tests verify standalone functions, CLI parsing, diff parsing, and security state machines in isolation.

### 2.1 Test Coverage Inventory
- **Clap CLI Argument Parsing**: Verifies flag combinations (`--provider`, `--model`, `--runtime`, `--auto-approve`, `--lang`, `--theme`), aliases, defaults, and conflict errors.
- **Slash Command Parser**: Tests autocompletion tokens, argument splitting, and unknown command suggestions.
- **Unified Diff Hunk Parser**: Tests parsing of multi-file diffs, hunk line counts, binary file markers, and syntax highlighting spans.
- **Approval State Transitions**: Tests that `ApprovalState` strictly follows the lifecycle state machine and that terminal states reject further transitions.
- **Session Whitelist Matching**: Tests command prefix and regex matching, path prefix checking, and escaping edge cases.

### 2.2 Example Unit Test: Approval State Invariant

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use agent_studios_protocol::approval::ApprovalState;

    #[test]
    fn test_approval_state_terminal_immutability() {
        let terminal_states = [
            ApprovalState::Approved,
            ApprovalState::Denied,
            ApprovalState::Cancelled,
            ApprovalState::Expired,
        ];

        for state in terminal_states {
            assert!(state.is_terminal());
            // Attempting transition from terminal to any other state must fail
            assert!(state.validate_transition_to(ApprovalState::Pending).is_err());
            assert!(state.validate_transition_to(ApprovalState::Approved).is_err());
        }
    }
}
```

---

## 3. Level 2: Ratatui Widget & Layout Snapshot Tests

Widget tests verify visual rendering accuracy, color token mapping, responsive breakpoint adaptations, and Unicode handling using `ratatui::backend::TestBackend`.

### 3.1 The `TestBackend` Harness

```rust
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;

pub fn setup_test_terminal(width: u16, height: u16) -> Terminal<TestBackend> {
    let backend = TestBackend::new(width, height);
    Terminal::new(backend).expect("Failed to initialize TestBackend")
}

pub fn assert_buffer_contains(buffer: &Buffer, expected: &str) {
    let mut found = false;
    for y in 0..buffer.area.height {
        let line: String = (0..buffer.area.width)
            .map(|x| buffer.get(x, y).symbol())
            .collect();
        if line.contains(expected) {
            found = true;
            break;
        }
    }
    assert!(found, "Expected string '{expected}' not found in terminal buffer");
}
```

### 3.2 Responsive Breakpoint Tests

Tests verify that widgets gracefully adjust layout when terminal dimensions change:
- **HeaderWidget**:
  * Width $< 90$: Renders compressed single-line header (`ags ~/auth · feat* · claude-3-7`).
  * Width $\ge 90$: Renders full metadata row with individual badges for Provider, Model, and Runtime.
- **ViewportContainer**:
  * Width $< 120$: Transcript takes 100% width; Task Graph and Diff modals overlay on top.
  * Width $\ge 120$: Two-column split view (Transcript 60%, Task Graph / Live Diff 40%).

### 3.3 Vietnamese Diacritic & NFC Unicode Invariant Tests

Per Product Principle #26 and `LOCALIZATION.md`, Vietnamese diacritics must never cause visual corruption:

```rust
#[test]
fn test_vietnamese_diacritic_rendering_no_clipping() {
    let mut terminal = setup_test_terminal(80, 10);
    let sample_vietnamese = "YÊU CẦU PHÊ DUYỆT: Lập trình viên đang thực thi tác vụ";

    terminal.draw(|f| {
        let widget = ratatui::widgets::Paragraph::new(sample_vietnamese);
        f.render_widget(widget, f.size());
    }).unwrap();

    let buffer = terminal.backend().buffer();
    assert_buffer_contains(buffer, "YÊU CẦU PHÊ DUYỆT");
    // Verify width calculation matches unicode-width
    assert_eq!(
        unicode_width::UnicodeWidthStr::width(sample_vietnamese),
        unicode_width::UnicodeWidthStr::width_cjk(sample_vietnamese)
    );
}
```

---

## 4. Level 3: Client & Event Pipeline Integration Tests

Integration tests verify communication between `agent-studios-cli-client`, the Control Plane actor, and the event streaming pipeline.

### 4.1 `MockControlPlaneClient` for Fast Test Loops

A mock client implementation enables testing the complete UI state projection without spinning up full backend threads or disk databases:

```rust
use async_trait::async_trait;
use tokio::sync::{broadcast, mpsc};
use agent_studios_protocol::id::{StudioId, TaskId};
use agent_studios_protocol::event::EventEnvelope;

pub struct MockControlPlaneClient {
    pub studio_id: StudioId,
    pub event_tx: broadcast::Sender<EventEnvelope>,
    pub task_graph_snapshot: Arc<tokio::sync::RwLock<TaskGraphSnapshot>>,
}

#[async_trait]
impl AgentStudiosClient for MockControlPlaneClient {
    fn studio_id(&self) -> StudioId { self.studio_id }
    fn execution_mode(&self) -> ExecutionMode { ExecutionMode::Embedded }
    async fn shutdown(&self) -> Result<(), ClientError> { Ok(()) }

    async fn subscribe_events(&self, from_sequence: u64) -> Result<ClientSubscription, ClientError> {
        let (tx, rx) = mpsc::channel(128);
        let mut bcast_rx = self.event_tx.subscribe();
        tokio::spawn(async move {
            while let Ok(env) = bcast_rx.recv().await {
                if tx.send(env).await.is_err() { break; }
            }
        });
        Ok(ClientSubscription { backfill: vec![], stream: rx })
    }

    async fn submit_goal(&self, _p: &str, _o: GoalSubmissionOptions) -> Result<TaskId, ClientError> {
        Ok(TaskId::new())
    }
    // ... remaining trait methods stubbed
}
```

### 4.2 Sequence Gap Detection & Reconnection Test

```rust
#[tokio::test]
async fn test_pipeline_detects_sequence_gap() {
    let mut tracker = SequenceTracker::new();
    let studio_id = StudioId::new();

    let env1 = EventEnvelope::new(studio_id, 1, chrono::Utc::now(), dummy_event());
    let env3 = EventEnvelope::new(studio_id, 3, chrono::Utc::now(), dummy_event()); // Skipped seq 2!

    assert!(tracker.observe(&env1).is_ok());
    let res = tracker.observe(&env3);
    assert!(matches!(res, Err(SequenceError::GapOrOutdated { expected: 2, actual: 3 })));
}
```

---

## 5. Level 4: Headless CLI End-to-End Tests

Headless E2E tests execute the compiled `ags` binary in subprocess mode against temporary Git repositories created via `tempfile`.

### 5.1 Temporary Git Repository Test Fixture

```rust
use std::process::Command;
use tempfile::TempDir;

pub struct TestRepo {
    pub dir: TempDir,
}

impl TestRepo {
    pub fn new() -> Self {
        let dir = TempDir::new().expect("Failed to create tempdir");
        let path = dir.path();

        // Initialize Git repo
        Command::new("git").current_dir(path).args(["init"]).output().unwrap();
        Command::new("git").current_dir(path).args(["config", "user.name", "Test"]).output().unwrap();
        Command::new("git").current_dir(path).args(["config", "user.email", "test@test.com"]).output().unwrap();

        // Commit initial seed file
        std::fs::write(path.join("README.md"), "# Test Project\n").unwrap();
        Command::new("git").current_dir(path).args(["add", "."]).output().unwrap();
        Command::new("git").current_dir(path).args(["commit", "-m", "initial"]).output().unwrap();

        Self { dir }
    }

    pub fn path(&self) -> &std::path::Path {
        self.dir.path()
    }
}
```

### 5.2 Subcommand Execution Tests

```rust
#[test]
fn test_ags_run_headless_execution() {
    let repo = TestRepo::new();

    let output = Command::new(cargo_bin("ags"))
        .current_dir(repo.path())
        .args(["run", "Add hello.txt with greeting", "--auto-approve", "all"])
        .output()
        .expect("Failed to execute ags run");

    assert!(output.status.success(), "ags run exited with non-zero code");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Completed") || stdout.contains("Success"));
}

#[test]
fn test_ags_worktree_list_clean() {
    let repo = TestRepo::new();

    let output = Command::new(cargo_bin("ags"))
        .current_dir(repo.path())
        .args(["worktree", "list"])
        .output()
        .expect("Failed to execute ags worktree list");

    assert!(output.status.success());
}
```

---

## 6. Continuous Integration & Automation Matrix

The test suite runs automatically on GitHub Actions CI across the following matrix:

| OS Environment | Target Architecture | Toolchain | Key Scenarios Tested |
| :--- | :--- | :--- | :--- |
| **Windows Server 2022** | `x86_64-pc-windows-msvc` | Stable (2024 edition) | Named Pipes, Job Objects, PowerShell paths, CRLF, Windows Terminal ANSI |
| **Ubuntu 24.04 LTS** | `x86_64-unknown-linux-gnu` | Stable (2024 edition) | Unix Domain Sockets, cgroups, Bash pipes, UTF-8 locale |
| **macOS 14 Sonoma** | `aarch64-apple-darwin` | Stable (2024 edition) | Unix Domain Sockets, Apple Silicon, Terminal.app raw mode |

### CI Verification Commands
```bash
# 1. Check workspace formatting & linting
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings

# 2. Run fast unit and widget snapshot tests
cargo test --workspace --lib

# 3. Run client and integration tests
cargo test --package agent-studios-cli-client --test "*"

# 4. Run CLI binary end-to-end tests
cargo test --package ags --test "*"
```
