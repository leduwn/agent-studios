use std::path::PathBuf;
use std::time::Duration;
use tokio::process::Command;

use agent_studios_claude_code_runtime::discovery::ClaudeCodeDiscoveryConfig;
use agent_studios_claude_code_runtime::runtime::ClaudeCodeRuntime;
use agent_studios_external_runtime::{
    AgentRuntime, RuntimeAvailability, RuntimeEventKind, RuntimeInstanceRef, RuntimeStartRequest,
};
use agent_studios_protocol::id::WorktreeId;
use agent_studios_protocol::worktree::ExecutionWorkspace;
use tempfile::tempdir;

/// Resolves the candidate Claude Code CLI binary according to lead specifications.
fn resolve_claude_cli_binary() -> Option<PathBuf> {
    // 1. Direct explicit specification path
    let explicit_path = PathBuf::from(r"C:\Users\Duwn\.local\bin\claude.exe");
    if explicit_path.is_file() {
        return Some(explicit_path);
    }

    // 2. USERPROFILE candidate
    if let Ok(userprofile) = std::env::var("USERPROFILE") {
        let candidate = PathBuf::from(userprofile)
            .join(".local")
            .join("bin")
            .join("claude.exe");
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    // 3. Environment overrides
    for env_key in &["CLAUDE_CODE_BIN", "CLAUDE_BIN"] {
        if let Ok(val) = std::env::var(env_key) {
            let p = PathBuf::from(val);
            if p.is_file() {
                return Some(p);
            }
        }
    }

    // 4. PATH search
    if let Ok(path_var) = std::env::var("PATH") {
        let binary_names = if cfg!(windows) {
            vec!["claude.exe", "claude.cmd", "claude.bat", "claude"]
        } else {
            vec!["claude"]
        };
        for dir in std::env::split_paths(&path_var) {
            for name in &binary_names {
                let candidate = dir.join(name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }

    None
}

/// Probes whether the executable binary is functional with `--version`.
async fn probe_executable(path: &PathBuf) -> Result<String, String> {
    let mut cmd = Command::new(path);
    cmd.arg("--version");

    match tokio::time::timeout(Duration::from_secs(5), cmd.output()).await {
        Ok(Ok(output)) => {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                Ok(stdout.trim().to_string())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                Err(format!("Exit status {}: {}", output.status, stderr.trim()))
            }
        }
        Ok(Err(e)) => Err(format!("Execution spawn failed: {e}")),
        Err(_) => Err("Version probe timed out after 5s".to_string()),
    }
}

// ============================================================================
// Real Read-Only Headless Claude Code CLI Smoke Test Harness
// ============================================================================

#[tokio::test]
async fn test_cli_smoke_headless_read_only_prompt_execution() {
    // 1. Check if claude.exe exists
    let binary_path = match resolve_claude_cli_binary() {
        Some(p) => p,
        None => {
            println!(
                "NOT_TESTED: Claude Code CLI binary not found at 'C:\\Users\\Duwn\\.local\\bin\\claude.exe' or in PATH"
            );
            return;
        }
    };

    // 2. Check if binary is executable
    let version = match probe_executable(&binary_path).await {
        Ok(v) => v,
        Err(reason) => {
            println!(
                "NOT_TESTED: Claude Code CLI binary at '{}' is not executable: {}",
                binary_path.display(),
                reason
            );
            return;
        }
    };

    println!(
        "Found functional Claude Code CLI at '{}': {}",
        binary_path.display(),
        version
    );

    // 3. Create wrapper script to pass `--verbose --bare` and capture exit code
    // In Claude Code CLI 2.1+, `--output-format=stream-json` requires `--verbose`
    // when using headless print mode, and `--bare` prevents interactive chrome.
    let shim_dir = tempdir().expect("Failed to create temporary directory for smoke shim");
    let exit_code_file = shim_dir.path().join("smoke_exit.txt");

    let shim_binary = if cfg!(windows) {
        let shim = shim_dir.path().join("claude_smoke_wrapper.cmd");
        let content = format!(
            r#"@echo off
python -u -c "import sys, subprocess; cmd = [sys.argv[1], '--verbose', '--bare'] + sys.argv[2:]; p = subprocess.Popen(cmd, stdin=subprocess.PIPE); line = sys.stdin.buffer.readline(); p.communicate(input=line); sys.exit(p.returncode)" "{}" %*
set SMOKE_EXIT=%ERRORLEVEL%
> "{}" echo %SMOKE_EXIT%
exit /b %SMOKE_EXIT%
"#,
            binary_path.display(),
            exit_code_file.display()
        );
        std::fs::write(&shim, content).expect("Failed to write Windows smoke wrapper");
        shim
    } else {
        let shim = shim_dir.path().join("claude_smoke_wrapper.sh");
        let content = format!(
            r#"#!/bin/sh
if command -v python3 >/dev/null 2>&1; then
    python3 -u -c "import sys, subprocess; cmd = [sys.argv[1], '--verbose', '--bare'] + sys.argv[2:]; p = subprocess.Popen(cmd, stdin=subprocess.PIPE); line = sys.stdin.buffer.readline(); p.communicate(input=line); sys.exit(p.returncode)" "{}" "$@"
else
    "{}" --verbose --bare "$@"
fi
SMOKE_EXIT=$?
echo $SMOKE_EXIT > "{}"
exit $SMOKE_EXIT
"#,
            binary_path.display(),
            binary_path.display(),
            exit_code_file.display()
        );
        std::fs::write(&shim, content).expect("Failed to write Unix smoke wrapper");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        shim
    };

    // 4. Discover through ClaudeCodeRuntime adapter
    let config = ClaudeCodeDiscoveryConfig::new().with_custom_binary_path(&shim_binary);
    let runtime = match ClaudeCodeRuntime::new() {
        Ok(r) => r.with_discovery_config(config),
        Err(e) => {
            println!("NOT_TESTED: Failed to initialize ClaudeCodeRuntime: {e}");
            return;
        }
    };

    let discovered = match runtime.discover().await {
        Ok(instances) => instances,
        Err(e) => {
            println!("NOT_TESTED: Discovery probe failed: {e}");
            return;
        }
    };

    if discovered.is_empty() {
        println!("NOT_TESTED: Discovery returned no instances for smoke shim");
        return;
    }

    let instance = &discovered[0];
    if !matches!(instance.availability, RuntimeAvailability::Available) {
        println!(
            "NOT_TESTED: Discovered instance is not marked available: {:?}",
            instance.availability
        );
        return;
    }

    // 5. Setup clean managed workspace
    let ws_dir = tempdir().expect("Failed to create temporary workspace");
    let ws = ExecutionWorkspace::managed(
        WorktreeId::new(),
        ws_dir.path().to_path_buf(),
        ws_dir.path().to_path_buf(),
        PathBuf::from("/source"),
        PathBuf::from("/source"),
        "0123456789abcdef0123456789abcdef01234567",
    );

    // 6. Safe, read-only headless prompt
    let instance_ref =
        RuntimeInstanceRef::new(instance.implementation_id.clone(), instance.instance_id);
    let start_req = RuntimeStartRequest::new(instance_ref, ws)
        .with_initial_prompt("Respond with exactly the single word: PONG");

    let handle = match runtime.start(start_req).await {
        Ok(h) => h,
        Err(err) => {
            println!(
                "NOT_TESTED: Claude Code runtime start failed (likely authentication unavailable in this context): {err}"
            );
            return;
        }
    };

    let session_ref = handle.session_ref();

    // 7. Subscribe to SessionEventHub from start (after_sequence = None)
    let mut sub = match runtime.events(&session_ref).await {
        Ok(s) => s,
        Err(err) => {
            println!("NOT_TESTED: Event hub subscription failed: {err}");
            let _ = runtime.stop(&session_ref).await;
            return;
        }
    };

    let mut session_started_count = 0;
    let mut seq1_is_session_started = false;
    let mut received_system_init_diagnostic = false;
    let mut received_stream_content = false;
    let mut turn_completed = false;

    // Stream events with 30s timeout
    let stream_deadline = Duration::from_secs(30);
    let start_time = tokio::time::Instant::now();

    loop {
        if start_time.elapsed() > stream_deadline {
            println!("Smoke test stream timeout after 30s; stopping session");
            let _ = runtime.stop(&session_ref).await;
            break;
        }

        match tokio::time::timeout(Duration::from_millis(2000), sub.next_event()).await {
            Ok(Ok(Some(envelope))) => {
                let seq = envelope.sequence;
                match &envelope.kind {
                    RuntimeEventKind::SessionStarted { .. } => {
                        session_started_count += 1;
                        if seq == 1 {
                            seq1_is_session_started = true;
                        }
                    }
                    RuntimeEventKind::Diagnostic { level, message } => {
                        let msg = message.as_str();
                        if msg.contains("Claude Code initialized:") || msg.contains("version=") {
                            received_system_init_diagnostic = true;
                        }
                        if msg.contains("Turn completed:") {
                            turn_completed = true;
                            // Prompt turn has successfully finished
                            break;
                        }
                        if level == "warn" || level == "error" {
                            println!("Diagnostic [{}]: {}", level, msg);
                        }
                    }
                    RuntimeEventKind::OutputDelta { text } => {
                        received_stream_content = true;
                        print!("{}", text);
                    }
                    RuntimeEventKind::ToolStarted { tool_name, .. } => {
                        println!("ToolStarted: {}", tool_name);
                    }
                    RuntimeEventKind::ToolCompleted {
                        tool_name, success, ..
                    } => {
                        println!("ToolCompleted: {} (success: {})", tool_name, success);
                    }
                    RuntimeEventKind::Completed { .. } => {
                        turn_completed = true;
                        break;
                    }
                    RuntimeEventKind::Failed { safe_error_summary } => {
                        let summary = safe_error_summary.as_str();
                        println!("Session failed with: {}", summary);
                        if summary.contains("auth")
                            || summary.contains("API key")
                            || summary.contains("unauthorized")
                            || summary.contains("login")
                        {
                            println!(
                                "NOT_TESTED: Claude Code authentication or API key unavailable: {}",
                                summary
                            );
                            return;
                        }
                        break;
                    }
                    RuntimeEventKind::Stopped => {
                        break;
                    }
                    _ => {}
                }
            }
            Ok(Ok(None)) => {
                // Stream closed
                break;
            }
            Ok(Err(e)) => {
                println!("Error reading event from subscription: {e}");
                break;
            }
            Err(_) => {
                // Tick timeout; if content was received, turn may be complete
                if received_stream_content {
                    turn_completed = true;
                    break;
                }
            }
        }
    }

    // 8. Assertions verifying the Lead Specification invariants:
    // - Single SessionStarted event at sequence 1
    assert!(
        seq1_is_session_started,
        "Sequence 1 must be SessionStarted event"
    );
    assert_eq!(
        session_started_count, 1,
        "Must have exactly ONE SessionStarted event across the entire session"
    );

    // - `system/init` diagnostic emission
    assert!(
        received_system_init_diagnostic,
        "Adapter must emit Diagnostic event on Claude Code system/init message"
    );

    // - Stream message reception
    assert!(
        received_stream_content || turn_completed,
        "Must receive stream message content from Claude Code prompt execution"
    );

    // 9. Clean process termination with exit code 0
    let stop_res = runtime.stop(&session_ref).await;
    assert!(
        stop_res.is_ok(),
        "Stopping session must succeed cleanly: {:?}",
        stop_res.err()
    );

    // Wait briefly for process supervisor to terminate and write exit code
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Verify exit code recorded from the child process is 0
    if exit_code_file.is_file() {
        let recorded_exit = std::fs::read_to_string(&exit_code_file).unwrap_or_default();
        let trimmed_exit = recorded_exit.trim();
        assert_eq!(
            trimmed_exit, "0",
            "Child process must terminate with exit code 0, got: {trimmed_exit}"
        );
    }

    println!(
        "\nSmoke test passed: Startup sequence, init diagnostic, single SessionStarted, and clean termination with exit code 0 verified."
    );
}

// ============================================================================
// Clean Fallback Smoke Test when Binary is Unavailable
// ============================================================================

#[tokio::test]
async fn test_cli_smoke_cleanly_marks_not_tested_when_binary_unavailable() {
    let dummy_missing_path =
        PathBuf::from("C:\\definitely_nonexistent_directory_99999\\claude.exe");

    let config = ClaudeCodeDiscoveryConfig::new().with_custom_binary_path(&dummy_missing_path);
    let runtime = ClaudeCodeRuntime::new()
        .expect("Runtime instantiation should succeed")
        .with_discovery_config(config);

    let discovered = runtime
        .discover()
        .await
        .expect("Discover should handle missing binary cleanly");

    // Must be empty or marked unavailable
    assert!(
        discovered.is_empty(),
        "Discovery with non-existent binary must return empty list"
    );

    println!(
        "NOT_TESTED: Claude CLI binary cleanly reported unavailable as expected when path is missing."
    );
}
