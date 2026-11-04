use agent_studios_claude_code_runtime::{
    ClaudeCodeError, ProcessContainment, SandboxLevel, SandboxPolicy,
};

#[test]
#[cfg(windows)]
fn test_sandbox_strict_fails_closed_on_windows() {
    let policy = SandboxPolicy::strict();
    assert_eq!(policy.level, SandboxLevel::Strict);
    assert!(policy.requires_os_containment());
    assert!(policy.enforce_job_object);
    assert!(policy.kill_on_close);

    let result = ProcessContainment::new(policy);
    assert!(
        result.is_err(),
        "SandboxLevel::Strict must fail closed on Windows without Hyper-V container backing"
    );

    match result.err().unwrap() {
        ClaudeCodeError::SandboxPolicyViolation(reason) => {
            assert!(
                reason.contains("Strict")
                    && (reason.contains("filesystem") || reason.contains("isolation")),
                "Error message should explain lack of filesystem/network isolation: {reason}"
            );
        }
        other => panic!(
            "Expected ClaudeCodeError::SandboxPolicyViolation, got {:?}",
            other
        ),
    }
}

#[test]
#[cfg(windows)]
fn test_sandbox_standard_creates_win32_job_object_with_kill_on_close() {
    let policy = SandboxPolicy::standard();
    assert_eq!(policy.level, SandboxLevel::Standard);
    assert!(policy.requires_os_containment());
    assert!(policy.enforce_job_object);
    assert!(policy.kill_on_close);
    assert_eq!(policy.max_memory_bytes, Some(4 * 1024 * 1024 * 1024)); // 4 GiB default
    assert_eq!(policy.max_processes, Some(64)); // 64 processes default

    let containment = ProcessContainment::new(policy)
        .expect("Standard sandbox initialization should succeed on Windows");
    assert!(containment.job_object().is_some());

    let job = containment.job_object().unwrap();
    let raw = job.raw_handle();
    assert_ne!(raw, 0, "Job handle must be non-null");
    assert_ne!(raw, -1, "Job handle must not be INVALID_HANDLE_VALUE");

    // Spawn a quick short-lived child process and verify containment assignment
    let mut child = std::process::Command::new("cmd.exe")
        .args(["/C", "ping 127.0.0.1 -n 3 > nul"])
        .spawn()
        .expect("Failed to spawn test child process");

    let pid = child.id();

    // Verify assigning process by PID
    let assign_res = containment.assign_child(pid);
    assert!(
        assign_res.is_ok(),
        "Assigning child PID {pid} to Job Object must succeed: {:?}",
        assign_res.err()
    );

    // Verify containment query
    let in_job = containment
        .is_child_in_job(pid)
        .expect("Failed to query IsProcessInJob");
    assert!(
        in_job,
        "Child process must be verified inside the Job Object"
    );

    // Terminate containment tree
    let term_res = containment.terminate();
    assert!(
        term_res.is_ok(),
        "Terminating containment tree must succeed"
    );

    let _ = child.wait();
}

#[test]
fn test_sandbox_none_runs_unconstrained() {
    let policy = SandboxPolicy::unconstrained();
    assert_eq!(policy.level, SandboxLevel::None);
    assert!(!policy.requires_os_containment());
    assert!(!policy.enforce_job_object);
    assert!(!policy.kill_on_close);
    assert!(policy.max_memory_bytes.is_none());
    assert!(policy.max_processes.is_none());

    let containment = ProcessContainment::new(policy).expect("Unconstrained sandbox must succeed");
    assert_eq!(containment.policy().level, SandboxLevel::None);

    #[cfg(windows)]
    assert!(
        containment.job_object().is_none(),
        "Unconstrained containment must not create a Job Object"
    );

    // assign_raw_handle on unconstrained succeeds as a no-op
    assert!(containment.assign_raw_handle(0).is_ok());

    // Also verify disabled() constructor alias
    let disabled_policy = SandboxPolicy::disabled();
    assert_eq!(disabled_policy.level, SandboxLevel::None);
    assert!(!disabled_policy.enforce_job_object);
}

#[test]
fn test_sandbox_memory_quota_and_process_count_configuration() {
    // 1. Using with_max_memory_mb and with_max_processes
    let policy = SandboxPolicy::standard()
        .with_max_memory_mb(512)
        .with_max_processes(32);

    assert_eq!(policy.max_memory_bytes, Some(512 * 1024 * 1024));
    assert_eq!(policy.max_processes, Some(32));
    assert!(policy.kill_on_close);

    #[cfg(windows)]
    {
        let containment =
            ProcessContainment::new(policy).expect("Should succeed with 512MB and 32 processes");
        assert!(containment.job_object().is_some());
    }

    // 2. Using with_memory_limit and with_process_limit
    let custom_policy = SandboxPolicy::standard()
        .with_memory_limit(1024 * 1024 * 1024)
        .with_process_limit(16);

    assert_eq!(custom_policy.max_memory_bytes, Some(1024 * 1024 * 1024));
    assert_eq!(custom_policy.max_processes, Some(16));

    #[cfg(windows)]
    {
        let custom_containment = ProcessContainment::new(custom_policy)
            .expect("Should succeed with 1GB and 16 processes");
        assert!(custom_containment.job_object().is_some());
    }
}
