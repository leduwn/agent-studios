use std::time::Duration;
use url::Url;

#[cfg(windows)]
use agent_studios_opencode_runtime::JobObjectGuard;
use agent_studios_opencode_runtime::{
    OpenCodeError, OpenCodeServerConfig, ProcessContainment, SandboxLevel, SandboxPolicy,
    generate_secure_password, spawn_opencode_server, verify_loopback_url,
};

#[test]
fn test_strict_fails_closed_without_filesystem_isolation() {
    let strict_policy = SandboxPolicy::strict();
    assert_eq!(strict_policy.level, SandboxLevel::Strict);

    // Win32 Job Objects do not provide OS-level filesystem or network namespace isolation.
    // SandboxLevel::Strict must fail closed with SandboxPolicyViolation rather than silently downgrading.
    let containment_res = ProcessContainment::new(strict_policy);
    assert!(
        containment_res.is_err(),
        "SandboxLevel::Strict must fail closed"
    );

    match containment_res.err().unwrap() {
        #[cfg(windows)]
        OpenCodeError::SandboxPolicyViolation(msg) => {
            assert!(
                msg.contains("filesystem isolation"),
                "Expected violation message mentioning filesystem isolation, got: {msg}"
            );
        }
        #[cfg(not(windows))]
        OpenCodeError::UnsupportedPlatform(msg) => {
            assert!(
                msg.contains("requires OS-level containment"),
                "Expected unsupported platform message, got: {msg}"
            );
        }
        other => panic!("Expected platform-appropriate sandbox rejection, got: {other:?}"),
    }
}

#[test]
fn test_unconstrained_sandbox_policy() {
    let unconstrained = SandboxPolicy::unconstrained();
    assert_eq!(unconstrained.level, SandboxLevel::None);
    assert!(!unconstrained.enforce_job_object);

    let containment_res = ProcessContainment::new(unconstrained);
    assert!(
        containment_res.is_ok(),
        "SandboxLevel::None must initialize successfully"
    );
}

#[test]
fn test_loopback_url_strict_enforcement() {
    // Valid loopback URLs
    assert!(verify_loopback_url(&Url::parse("http://127.0.0.1:4096").unwrap()).is_ok());
    assert!(verify_loopback_url(&Url::parse("http://localhost:8080").unwrap()).is_ok());
    assert!(verify_loopback_url(&Url::parse("http://[::1]:3000").unwrap()).is_ok());

    // Rejected non-loopback / remote URLs
    let remote = Url::parse("http://192.168.1.50:4096").unwrap();
    assert!(
        verify_loopback_url(&remote).is_err(),
        "Non-loopback RFC1918 IP must be rejected"
    );

    let public_domain = Url::parse("http://api.opencode.example.com").unwrap();
    assert!(
        verify_loopback_url(&public_domain).is_err(),
        "Public hostname must be rejected"
    );

    let all_interfaces = Url::parse("http://0.0.0.0:4096").unwrap();
    assert!(
        verify_loopback_url(&all_interfaces).is_err(),
        "0.0.0.0 wildcard binding must be rejected"
    );

    let non_http = Url::parse("ftp://127.0.0.1:4096").unwrap();
    assert!(
        verify_loopback_url(&non_http).is_err(),
        "Non-HTTP/HTTPS scheme must be rejected"
    );
}

#[test]
fn test_secure_password_generation_entropy() {
    let p1 = generate_secure_password();
    let p2 = generate_secure_password();

    assert_eq!(p1.len(), 64, "Expected 64 hex characters (32 bytes)");
    assert_eq!(p2.len(), 64, "Expected 64 hex characters (32 bytes)");
    assert_ne!(p1, p2, "CSPRNG must generate unique passwords");
}

#[cfg(windows)]
#[test]
fn test_standard_job_object_containment_and_tree_kill() {
    let policy = SandboxPolicy::standard();
    let guard = JobObjectGuard::create(&policy).expect("Failed to create JobObjectGuard");
    assert!(guard.raw_handle() != 0);

    // Spawn a harmless native child process
    let mut child = std::process::Command::new("ping.exe")
        .args(["127.0.0.1", "-n", "10"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("Failed to spawn test ping.exe");

    let pid = child.id();

    // Assign process to Job Object
    guard
        .assign_pid(pid)
        .expect("Failed to assign process to Job Object");

    // Verify containment membership via IsProcessInJob
    let in_job = guard
        .is_process_in_job(pid)
        .expect("Failed to verify process in job");
    assert!(in_job, "Process must be in job");

    // Terminate job object tree
    guard.terminate(1).expect("Failed to terminate Job Object");

    let status = child.wait().expect("Failed to wait on child");
    assert!(
        !status.success(),
        "Child process must have terminated via Job Object kill"
    );
}

#[cfg(windows)]
#[test]
fn test_zero_window_suspended_containment_and_resume() {
    use std::os::windows::process::CommandExt;

    let policy = SandboxPolicy::standard();
    let guard = JobObjectGuard::create(&policy).expect("Failed to create JobObjectGuard");

    // CREATE_SUSPENDED = 0x00000004
    const CREATE_SUSPENDED: u32 = 0x00000004;

    let mut child = std::process::Command::new("ping.exe")
        .args(["127.0.0.1", "-n", "5"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .creation_flags(CREATE_SUSPENDED)
        .spawn()
        .expect("Failed to spawn suspended test process");

    let pid = child.id();

    // Assign process while suspended, verify IsProcessInJob, and resume via NtResumeProcess
    guard
        .assign_and_resume_pid(pid)
        .expect("Failed to assign and resume suspended process");

    // Verify containment membership
    let in_job = guard
        .is_process_in_job(pid)
        .expect("Failed to query job membership");
    assert!(in_job, "Suspended process must be confirmed in Job Object");

    // Cleanup child
    guard.terminate(1).expect("Failed to terminate job");
    let _ = child.wait();
}

#[tokio::test]
async fn test_spawn_invalid_binary_fails_immediately() {
    let policy = if cfg!(windows) {
        SandboxPolicy::default()
    } else {
        SandboxPolicy::unconstrained()
    };
    let config = OpenCodeServerConfig::new(
        "non_existent_opencode_binary_xyz_123.exe",
        std::env::temp_dir(),
    )
    .with_startup_timeout(Duration::from_millis(500))
    .with_sandbox_policy(policy);

    let start = std::time::Instant::now();
    let res = spawn_opencode_server(config).await;

    assert!(
        res.is_err(),
        "Spawning non-existent binary must return error"
    );
    match res.err().unwrap() {
        OpenCodeError::ProcessSpawn(_) => {}
        other => panic!("Expected ProcessSpawn error, got: {other:?}"),
    }

    assert!(
        start.elapsed() < Duration::from_secs(2),
        "Spawn failure must fail immediately without waiting for startup timeout"
    );
}

#[tokio::test]
async fn test_early_child_crash_detected_without_hanging() {
    // On Windows, running `cmd.exe` with invalid CLI arguments causes it to exit immediately
    #[cfg(windows)]
    let binary = "cmd.exe";
    #[cfg(not(windows))]
    let binary = "false";

    let policy = if cfg!(windows) {
        SandboxPolicy::default()
    } else {
        SandboxPolicy::unconstrained()
    };
    let config = OpenCodeServerConfig::new(binary, std::env::temp_dir())
        .with_startup_timeout(Duration::from_secs(5))
        .with_sandbox_policy(policy);

    let start = std::time::Instant::now();
    let res = spawn_opencode_server(config).await;

    assert!(
        res.is_err(),
        "Early exiting binary must fail server startup"
    );
    match res.err().unwrap() {
        OpenCodeError::ServerExitUnexpected { exit_code, .. } => {
            assert!(
                exit_code.is_some(),
                "Expected child exit code from crashed/exited process"
            );
        }
        OpenCodeError::ProcessSpawn(_) => {}
        other => panic!("Expected ServerExitUnexpected or ProcessSpawn, got: {other:?}"),
    }

    // Must detect child exit promptly without waiting for the full 5 second timeout
    assert!(
        start.elapsed() < Duration::from_secs(3),
        "Premature child exit must be detected promptly (elapsed: {:?})",
        start.elapsed()
    );
}

#[tokio::test]
async fn test_non_existent_working_dir_fails_closed() {
    let non_existent_dir = std::env::temp_dir().join("definitely_non_existent_dir_m11_999");
    #[cfg(windows)]
    let binary = "cmd.exe";
    #[cfg(not(windows))]
    let binary = "sh";
    let config = OpenCodeServerConfig::new(binary, non_existent_dir);

    let res = spawn_opencode_server(config).await;
    assert!(res.is_err(), "Non-existent working_dir must fail closed");
    match res.err().unwrap() {
        OpenCodeError::InvalidConfiguration(msg) => {
            assert!(
                msg.contains("Working directory does not exist"),
                "Expected working directory error, got: {msg}"
            );
        }
        other => panic!("Expected InvalidConfiguration, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_opencode_discovery_smoke_without_network() {
    use agent_studios_opencode_runtime::OpenCodeDiscoveryConfig;
    use agent_studios_opencode_runtime::discovery::discover_opencode_binary;

    let config = OpenCodeDiscoveryConfig::default();
    let res = discover_opencode_binary(&config).await;

    assert!(
        res.is_ok(),
        "Discovery probe must not panic or error without network: {:?}",
        res.err()
    );
}
