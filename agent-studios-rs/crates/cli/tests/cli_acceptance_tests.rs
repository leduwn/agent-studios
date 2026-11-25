//! CLI End-to-End Acceptance Tests for `ags` Binary
//!
//! Invokes the compiled `ags` executable directly and verifies exit codes,
//! JSON contracts, `--headless` NDJSON streaming, `--stdin` input, and `daemon status`.

use std::io::Write;
use std::process::{Command, Stdio};
use tempfile::tempdir;

fn init_isolated_git_repo(dir: &std::path::Path) {
    let run = |args: &[&str]| {
        let out = Command::new("git")
            .current_dir(dir)
            .args(args)
            .output()
            .expect("run git");
        assert!(out.status.success(), "git {:?} failed", args);
    };
    run(&["init"]);
    run(&["config", "user.name", "CLI Test"]);
    run(&["config", "user.email", "cli@agent-studios.internal"]);
    run(&["config", "commit.gpgsign", "false"]);
    std::fs::write(dir.join("README.md"), "# CLI Acceptance\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-m", "init"]);
}

fn ags_bin() -> &'static str {
    env!("CARGO_BIN_EXE_ags")
}

#[test]
fn test_ags_help_and_version_exit_zero() {
    let help_out = Command::new(ags_bin())
        .arg("--help")
        .output()
        .expect("execute ags --help");
    assert!(help_out.status.success(), "ags --help must exit 0");
    let help_stdout = String::from_utf8_lossy(&help_out.stdout);
    assert!(help_stdout.contains("doctor"));
    assert!(help_stdout.contains("provider"));
    assert!(help_stdout.contains("model"));
    assert!(help_stdout.contains("run"));
    assert!(help_stdout.contains("daemon"));

    let ver_out = Command::new(ags_bin())
        .arg("--version")
        .output()
        .expect("execute ags --version");
    assert!(ver_out.status.success(), "ags --version must exit 0");
    let ver_stdout = String::from_utf8_lossy(&ver_out.stdout);
    assert!(ver_stdout.contains("ags"));
}

#[test]
fn test_ags_doctor_human_and_json_contract() {
    let json_out = Command::new(ags_bin())
        .args(["doctor", "--json"])
        .output()
        .expect("execute ags doctor --json");
    assert!(json_out.status.success(), "ags doctor --json must exit 0");
    let parsed: serde_json::Value =
        serde_json::from_slice(&json_out.stdout).expect("doctor --json must emit valid JSON");
    assert!(parsed.get("cli_version").is_some());
    let checks = parsed["checks"]
        .as_array()
        .expect("checks array in doctor output");
    assert_eq!(checks.len(), 8, "doctor must report all 8 checks");

    let human_out = Command::new(ags_bin())
        .arg("doctor")
        .output()
        .expect("execute ags doctor");
    assert!(human_out.status.success(), "ags doctor must exit 0");
}

#[test]
fn test_ags_provider_and_model_list_json() {
    let temp_home = tempdir().expect("tempdir");

    let prov_out = Command::new(ags_bin())
        .env("AGENT_STUDIOS_HOME", temp_home.path())
        .args(["provider", "list", "--format", "json"])
        .output()
        .expect("execute ags provider list");
    assert!(prov_out.status.success());
    let prov_val: serde_json::Value =
        serde_json::from_slice(&prov_out.stdout).expect("provider list JSON");
    assert!(prov_val.is_array());

    let model_out = Command::new(ags_bin())
        .env("AGENT_STUDIOS_HOME", temp_home.path())
        .args(["model", "list", "--format", "json"])
        .output()
        .expect("execute ags model list");
    assert!(model_out.status.success());
    let model_val: serde_json::Value =
        serde_json::from_slice(&model_out.stdout).expect("model list JSON");
    assert!(model_val.is_array());
}

#[test]
fn test_ags_run_headless_demo_emits_ndjson_and_exits_zero() {
    let temp_home = tempdir().expect("tempdir");
    let repo_dir = tempdir().expect("repo_dir");
    init_isolated_git_repo(repo_dir.path());

    let out = Command::new(ags_bin())
        .current_dir(repo_dir.path())
        .env("AGENT_STUDIOS_HOME", temp_home.path())
        .args([
            "run",
            "--headless",
            "--demo",
            "Verify deterministic headless CLI execution",
        ])
        .output()
        .expect("execute ags run --headless --demo");

    assert_eq!(
        out.status.code(),
        Some(0),
        "ags run --headless --demo stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout_str = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout_str
        .lines()
        .filter(|l| !l.trim().is_empty())
        .collect();
    assert!(
        !lines.is_empty(),
        "Headless run must emit NDJSON event lines"
    );

    let mut saw_succeeded = false;
    for line in lines {
        let parsed: serde_json::Value =
            serde_json::from_str(line).expect("Each NDJSON line must be valid JSON");
        if parsed["event"]["kind"] == "task_state_changed"
            && parsed["event"]["data"]["new_state"] == "succeeded"
        {
            saw_succeeded = true;
        }
    }
    assert!(
        saw_succeeded,
        "NDJSON stream must include TaskStateChanged -> succeeded"
    );
}

#[test]
fn test_ags_run_stdin_headless_demo() {
    let temp_home = tempdir().expect("tempdir");
    let repo_dir = tempdir().expect("repo_dir");
    init_isolated_git_repo(repo_dir.path());

    let mut child = Command::new(ags_bin())
        .current_dir(repo_dir.path())
        .env("AGENT_STUDIOS_HOME", temp_home.path())
        .args(["run", "--stdin", "--headless", "--demo"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn ags run --stdin");

    {
        let stdin = child.stdin.as_mut().expect("stdin handle");
        stdin
            .write_all(b"Execute piped stdin goal in headless mode\n")
            .expect("write stdin");
    }

    let out = child.wait_with_output().expect("wait ags run --stdin");
    assert_eq!(
        out.status.code(),
        Some(0),
        "ags run --stdin stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout_str = String::from_utf8_lossy(&out.stdout);
    assert!(stdout_str.contains("Execute piped stdin goal"));
}

#[test]
fn test_ags_run_missing_goal_exits_three() {
    let out = Command::new(ags_bin())
        .args(["run", "--headless", "--demo"])
        .output()
        .expect("execute ags run without goal");
    assert_eq!(
        out.status.code(),
        Some(3),
        "Missing goal must return contract exit code 3"
    );
}

#[test]
fn test_ags_daemon_status_json_offline_graceful() {
    let out = Command::new(ags_bin())
        .args(["daemon", "status", "--json"])
        .output()
        .expect("execute ags daemon status --json");
    assert!(out.status.success());
    let parsed: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("daemon status JSON");
    assert!(parsed.get("online").is_some());
    assert!(parsed.get("bootstrap_path").is_some());
}
