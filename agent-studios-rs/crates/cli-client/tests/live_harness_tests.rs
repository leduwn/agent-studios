use std::fs;

use agent_studios_cli_client::live_harness::{
    LiveAcceptanceHarness, LiveBudgetTracker, LiveHarnessConfig, LiveHarnessError, SandboxWorkspace,
};

#[test]
fn test_live_harness_enforces_zero_billable_opt_in_gate_by_default() {
    let cfg = LiveHarnessConfig {
        env_opt_in_var: "AGENT_STUDIOS_NONEXISTENT_OPT_IN_FOR_TEST".to_string(),
        ..LiveHarnessConfig::default()
    };
    let harness = LiveAcceptanceHarness::new(cfg).expect("valid config");
    assert!(!harness.is_live_opt_in_enabled());

    let report = harness.run_preflight_or_skip().expect("preflight");
    assert!(!report.executed);
    assert!(report.opt_in_missing);
    assert_eq!(report.spend_usd, 0.0);
    assert_eq!(report.tokens_used, 0);
    assert!(!report.kill_switch_triggered);
}

#[test]
fn test_live_harness_budget_kill_switch_trips_on_overrun() {
    let mut tracker = LiveBudgetTracker::new(0.10, 1000);
    tracker.record_usage(400, 0.05).expect("within cap");
    let err = tracker
        .record_usage(400, 0.08)
        .expect_err("must trip USD kill switch");
    assert!(matches!(err, LiveHarnessError::KillSwitchTriggered(_)));
    assert!(tracker.kill_switch_triggered);

    let mut token_tracker = LiveBudgetTracker::new(10.0, 500);
    let tok_err = token_tracker
        .record_usage(600, 0.01)
        .expect_err("must trip token kill switch");
    assert!(matches!(tok_err, LiveHarnessError::KillSwitchTriggered(_)));
}

#[test]
fn test_live_harness_sandbox_git_diff_capture() {
    let sandbox = SandboxWorkspace::initialize_with_file("src/main.rs", "fn main() {}\n")
        .expect("init sandbox repo");
    fs::write(
        sandbox.path().join("src/main.rs"),
        "fn main() { println!(\"updated\"); }\n",
    )
    .expect("modify seed");
    let diff = sandbox.capture_git_diff_stat().expect("git diff stat");
    assert!(diff.contains("src/main.rs"));
}
