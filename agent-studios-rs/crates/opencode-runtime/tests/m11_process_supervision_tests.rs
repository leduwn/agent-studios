use std::path::PathBuf;
use std::time::Duration;
use url::Url;

use agent_studios_opencode_runtime::{
    OpenCodeError, OpenCodeServerConfig, OpenCodeServerHandle, ProcessContainment, SandboxLevel,
    SandboxPolicy,
};

#[test]
fn test_server_config_builder() {
    let policy = SandboxPolicy::standard();
    let config = OpenCodeServerConfig::new("opencode", PathBuf::from("."))
        .with_port(4096)
        .with_auth_credentials("custom_user", "custom_pass")
        .with_startup_timeout(Duration::from_secs(15))
        .with_sandbox_policy(policy.clone())
        .with_env_var("TEST_VAR", "test_val");

    assert_eq!(config.port, Some(4096));
    assert_eq!(config.username, "custom_user");
    assert_eq!(config.password, Some("custom_pass".to_string()));
    assert_eq!(config.startup_timeout, Duration::from_secs(15));
    assert_eq!(config.sandbox_policy.level, SandboxLevel::Standard);
    assert_eq!(
        config.env_vars,
        vec![("TEST_VAR".to_string(), "test_val".to_string())]
    );
}

#[tokio::test]
async fn test_server_handle_from_existing_credentials() {
    let containment = ProcessContainment::new(SandboxPolicy::unconstrained())
        .expect("Failed to create unconstrained containment");

    let url = Url::parse("http://127.0.0.1:4096").unwrap();
    let handle = OpenCodeServerHandle::from_existing_credentials(
        url.clone(),
        "custom_user",
        "custom_secret",
        containment,
    )
    .expect("from_existing_credentials failed for loopback URL");

    assert_eq!(handle.base_url(), &url);
    assert_eq!(handle.username(), "custom_user");
    assert_eq!(handle.password(), "custom_secret");
    assert_eq!(handle.auth_token(), "custom_secret");

    // Idempotent stop
    let stop_res1 = handle.stop().await;
    assert!(stop_res1.is_ok(), "First stop() must succeed");

    let stop_res2 = handle.stop().await;
    assert!(stop_res2.is_ok(), "Second stop() must succeed idempotently");
}

#[tokio::test]
async fn test_server_handle_rejects_non_loopback() {
    let containment = ProcessContainment::new(SandboxPolicy::unconstrained())
        .expect("Failed to create unconstrained containment");

    let remote_url = Url::parse("http://10.0.0.1:4096").unwrap();
    let handle_res =
        OpenCodeServerHandle::from_existing_credentials(remote_url, "user", "secret", containment);

    assert!(
        handle_res.is_err(),
        "from_existing_credentials must reject non-loopback URL"
    );
    match handle_res.err().unwrap() {
        OpenCodeError::SandboxPolicyViolation(msg) => {
            assert!(msg.contains("strictly bind to loopback"));
        }
        other => panic!("Expected SandboxPolicyViolation, got: {other:?}"),
    }
}

#[cfg(windows)]
#[test]
fn test_assign_child_invalid_pid_fails_closed() {
    let policy = SandboxPolicy::standard();
    let containment = ProcessContainment::new(policy).expect("ProcessContainment creation failed");

    // PID 0xFFFFFFFE is reserved and invalid for normal processes
    let res = containment.assign_child(0xFFFFFFFE);
    assert!(res.is_err(), "Assigning invalid PID must fail closed");
}

#[cfg(windows)]
#[test]
fn test_assign_and_resume_invalid_pid_fails_closed() {
    let policy = SandboxPolicy::standard();
    let containment = ProcessContainment::new(policy).expect("ProcessContainment creation failed");

    let res = containment.assign_and_resume_child(0xFFFFFFFE);
    assert!(
        res.is_err(),
        "Assigning and resuming invalid PID must fail closed"
    );
}
