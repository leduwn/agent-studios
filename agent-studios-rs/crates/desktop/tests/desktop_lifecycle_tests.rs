//! Real Integration Tests for Standalone `AgentStudios.exe` Desktop Host & Daemon Supervisor (Wave 2–3)

use std::path::PathBuf;

use agent_studios_desktop::{
    CanonicalDesktopPaths, DesktopDaemonSupervisor, DesktopMode, DesktopSessionState,
    DesktopSupervisorConfig, build_workbench_launch_args,
};
use agent_studios_ipc_transport::IpcClient;
use tempfile::tempdir;

fn locate_compiled_agsd() -> PathBuf {
    let bin_name = if cfg!(windows) { "agsd.exe" } else { "agsd" };
    if let Ok(env_path) = std::env::var("AGSD_BIN_PATH") {
        let p = PathBuf::from(env_path);
        if p.is_file() {
            return p;
        }
    }
    let test_exe = std::env::current_exe().expect("current_exe");
    let candidate = test_exe
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join(bin_name))
        .expect("target debug dir");
    assert!(
        candidate.is_file(),
        "Compiled agsd binary missing at {} — run `cargo build --bin agsd` first",
        candidate.display()
    );
    candidate
}

#[test]
fn test_canonical_profile_provisioning_preserves_existing_user_config() {
    let app_data = tempdir().unwrap();
    let local_app_data = tempdir().unwrap();

    let paths = CanonicalDesktopPaths::from_roots(
        app_data.path().to_path_buf(),
        local_app_data.path().to_path_buf(),
    );

    // Pre-write custom user config.toml
    std::fs::create_dir_all(&paths.config_dir).unwrap();
    let custom_toml = "# Custom operator settings\n[ui]\ndisplay_language = \"vi-VN\"\n";
    std::fs::write(&paths.config_toml, custom_toml).unwrap();

    paths.ensure_provisioned("en-US", "auto").unwrap();

    let after_toml = std::fs::read_to_string(&paths.config_toml).unwrap();
    assert_eq!(
        after_toml, custom_toml,
        "ensure_provisioned must preserve existing user config.toml"
    );

    assert!(paths.artifacts_dir.is_dir());
    assert!(paths.extensions_dir.is_dir());

    let settings_raw = std::fs::read_to_string(
        paths
            .profile_user_data_dir
            .join("User")
            .join("settings.json"),
    )
    .unwrap();
    let settings: serde_json::Value = serde_json::from_str(&settings_raw).unwrap();
    assert_eq!(settings["agentStudios.displayLanguage"], "en-US");
    assert_eq!(settings["agentStudios.agentResponseLanguage"], "auto");
    assert_eq!(settings["workbench.startupEditor"], "none");
}

#[tokio::test]
async fn test_daemon_supervisor_autostart_crash_respawn_and_session_continuity() {
    let app_data = tempdir().unwrap();
    let local_app_data = tempdir().unwrap();
    let paths = CanonicalDesktopPaths::from_roots(
        app_data.path().to_path_buf(),
        local_app_data.path().to_path_buf(),
    );
    paths.ensure_provisioned("en-US", "auto").unwrap();

    let agsd_bin = locate_compiled_agsd();
    let cfg = DesktopSupervisorConfig::from_paths(paths.clone()).with_agsd_binary(agsd_bin);
    let mut supervisor = DesktopDaemonSupervisor::new(cfg);

    // 1. Auto-start daemon and verify healthy IPC
    let status1 = supervisor
        .ensure_running()
        .await
        .expect("daemon auto-start must succeed");
    assert!(status1.running);
    let pid1 = status1.pid.expect("pid1 must exist");

    // 2. Create durable Studio in SQLite WAL store
    let mut ipc1 = IpcClient::connect_from_bootstrap_file(&paths.runtime_bootstrap)
        .await
        .expect("connect to daemon 1");
    let created_studio = ipc1
        .create_studio("desktop-continuity-workspace")
        .await
        .expect("create_studio");
    let studio_id = created_studio.id;
    drop(ipc1);

    // 3. Simulate daemon crash (hard kill)
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/PID", &pid1.to_string()])
            .output();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("kill")
            .args(["-9", &pid1.to_string()])
            .output();
    }
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    // 4. Supervisor detects crash and respawns daemon onto the same state_db
    let status2 = supervisor
        .ensure_healthy_or_respawn()
        .await
        .expect("daemon respawn must succeed");
    assert!(status2.running);
    let pid2 = status2.pid.expect("pid2 must exist");
    assert_ne!(pid2, pid1, "Respawned daemon must have a new OS PID");

    // 5. Verify SQLite WAL state continuity across daemon crash
    let mut ipc2 = IpcClient::connect_from_bootstrap_file(&paths.runtime_bootstrap)
        .await
        .expect("connect to respawned daemon");
    let studios = ipc2.list_studios().await.expect("list_studios");
    assert!(
        studios.iter().any(|s| s.id == studio_id),
        "Persisted studio must survive daemon crash and respawn"
    );
    drop(ipc2);

    // 6. Mode transition Agent <-> IDE preserves daemon PID and studio session identity
    let mut session_state = DesktopSessionState {
        studio_id,
        daemon_pid: pid2,
        active_mode: DesktopMode::Agent,
        workspace_path: PathBuf::from("."),
    };
    session_state.transition_mode(DesktopMode::Ide);
    assert_eq!(session_state.active_mode, DesktopMode::Ide);
    assert_eq!(session_state.studio_id, studio_id);
    assert_eq!(session_state.daemon_pid, pid2);
    session_state.transition_mode(DesktopMode::Agent);
    assert_eq!(session_state.active_mode, DesktopMode::Agent);
    assert_eq!(session_state.daemon_pid, pid2);

    // 7. Graceful admin shutdown
    let stopped = supervisor
        .shutdown_daemon()
        .await
        .expect("admin shutdown must succeed");
    assert!(stopped);
    let after = supervisor.probe_status().await;
    assert!(!after.running);
}

#[test]
fn test_workbench_launch_args_enforce_isolated_profile_and_mode() {
    let app_data = tempdir().unwrap();
    let local_app_data = tempdir().unwrap();
    let paths = CanonicalDesktopPaths::from_roots(
        app_data.path().to_path_buf(),
        local_app_data.path().to_path_buf(),
    );

    let ws = PathBuf::from(r"D:\SampleRepo");
    let (args, env_vars) = build_workbench_launch_args(&paths, Some(&ws), DesktopMode::Agent);

    assert!(args.contains(&"--user-data-dir".to_string()));
    assert!(args.contains(&"--extensions-dir".to_string()));
    assert!(args.contains(&r"D:\SampleRepo".to_string()));
    assert!(
        env_vars
            .iter()
            .any(|(k, v)| k == "AGENT_STUDIOS_DEFAULT_MODE" && v == "agent")
    );
    assert!(env_vars.iter().any(|(k, _)| k == "AGSD_RUNTIME_INFO_PATH"));
}
