#[cfg(windows)]
use agent_studios_opencode_runtime::{JobObjectGuard, ProcessContainment};
use agent_studios_opencode_runtime::{SandboxLevel, SandboxPolicy};

#[test]
fn test_sandbox_policy_defaults() {
    let default_policy = SandboxPolicy::default();
    assert_eq!(default_policy.level, SandboxLevel::Standard);
    assert!(default_policy.enforce_job_object);
    assert_eq!(
        default_policy.max_memory_bytes,
        Some(4 * 1024 * 1024 * 1024)
    );
    assert_eq!(default_policy.max_processes, Some(64));

    let strict_policy = SandboxPolicy::strict();
    assert_eq!(strict_policy.level, SandboxLevel::Strict);
    assert!(strict_policy.enforce_job_object);
    assert_eq!(strict_policy.max_memory_bytes, Some(2 * 1024 * 1024 * 1024));
    assert_eq!(strict_policy.max_processes, Some(16));

    let unconstrained = SandboxPolicy::unconstrained();
    assert_eq!(unconstrained.level, SandboxLevel::None);
    assert!(!unconstrained.enforce_job_object);
    assert_eq!(unconstrained.max_memory_bytes, None);
    assert_eq!(unconstrained.max_processes, None);
}

#[test]
fn test_sandbox_policy_custom_builder() {
    let policy = SandboxPolicy::standard()
        .with_max_memory_mb(1024)
        .with_max_processes(32);

    assert_eq!(policy.max_memory_bytes, Some(1024 * 1024 * 1024));
    assert_eq!(policy.max_processes, Some(32));
    assert!(policy.enforce_job_object);
}

#[cfg(windows)]
#[test]
fn test_windows_job_object_guard_lifecycle() {
    let policy = SandboxPolicy::strict();
    let guard = JobObjectGuard::create(&policy);
    assert!(
        guard.is_ok(),
        "Failed to create JobObjectGuard on Windows: {:?}",
        guard.err()
    );

    let guard = guard.unwrap();
    assert!(guard.raw_handle() != 0);
}

#[cfg(windows)]
#[tokio::test]
async fn test_windows_process_containment_assignment() {
    let policy = SandboxPolicy::standard();
    let containment = ProcessContainment::from_policy(&policy);
    assert!(
        containment.is_ok(),
        "Failed to initialize ProcessContainment: {:?}",
        containment.err()
    );

    let containment = containment.unwrap();

    // Spawn a short-lived native process to assign
    let child = std::process::Command::new("cmd.exe")
        .args(["/c", "echo hello"])
        .spawn();

    assert!(child.is_ok(), "Failed to spawn test cmd.exe process");
    let mut child = child.unwrap();
    let pid = child.id();

    let assign_res = containment.assign_process(pid);
    assert!(
        assign_res.is_ok(),
        "Failed to assign process to JobObject: {:?}",
        assign_res.err()
    );

    let _ = child.wait();
}
