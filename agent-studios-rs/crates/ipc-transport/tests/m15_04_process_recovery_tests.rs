//! Integration tests executing authentic compiled `agsd` subprocesses (M15-04.1).
//! Validates real OS process startup, single-writer locking, role-based redaction,
//! and crash recovery reconciliation.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tempfile::TempDir;
use tokio::process::{Child, Command};
use uuid::Uuid;

use agent_studios_ipc_transport::IpcClient;
use agent_studios_protocol::task::{BatchTaskSpec, DependencyOutputPolicy, TaskState};

struct AgsdSubprocess {
    child: Child,
    #[allow(dead_code)]
    db_path: PathBuf,
    runtime_info_path: PathBuf,
    admin_runtime_info_path: PathBuf,
    #[allow(dead_code)]
    #[cfg(windows)]
    pipe_name: String,
    #[allow(dead_code)]
    #[cfg(unix)]
    socket_path: PathBuf,
}

impl AgsdSubprocess {
    async fn spawn(state_dir: &Path, db_path: Option<PathBuf>) -> Self {
        let bin_path = env!("CARGO_BIN_EXE_agsd");
        let resolved_db = db_path.unwrap_or_else(|| state_dir.join("agsd.db"));
        let runtime_info = state_dir.join("agsd.json");
        let admin_runtime_info = state_dir.join("agsd-admin.json");

        let unique = Uuid::new_v4().simple().to_string();
        #[cfg(windows)]
        let pipe_name = format!(r"\\.\pipe\agent-studios-test-{}", unique);
        #[cfg(unix)]
        let socket_path = state_dir.join(format!("agsd-{}.sock", unique));

        let mut cmd = Command::new(bin_path);
        cmd.arg("--state_dir")
            .arg(state_dir)
            .arg("--db_path")
            .arg(&resolved_db)
            .arg("--runtime_info_path")
            .arg(&runtime_info)
            .arg("--admin_runtime_info_path")
            .arg(&admin_runtime_info)
            .arg("--key_id")
            .arg("test-client")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        #[cfg(windows)]
        cmd.arg("--pipe_name").arg(&pipe_name);
        #[cfg(unix)]
        cmd.arg("--socket_path").arg(&socket_path);

        let child = cmd.spawn().expect("Failed to spawn agsd binary");

        let proc = Self {
            child,
            db_path: resolved_db,
            runtime_info_path: runtime_info,
            admin_runtime_info_path: admin_runtime_info,
            #[cfg(windows)]
            pipe_name,
            #[cfg(unix)]
            socket_path,
        };

        proc.wait_ready(Duration::from_secs(10)).await;
        proc
    }

    async fn wait_ready(&self, timeout: Duration) {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            if self.runtime_info_path.exists() && self.admin_runtime_info_path.exists() {
                // Short settling delay for pipe/socket bind
                tokio::time::sleep(Duration::from_millis(150)).await;
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!(
            "agsd subprocess did not write bootstrap files within {:?}",
            timeout
        );
    }

    async fn connect_client(&self) -> IpcClient {
        IpcClient::connect_from_bootstrap_file(&self.runtime_info_path)
            .await
            .expect("Failed to connect client from bootstrap file")
    }

    async fn connect_admin_client(&self) -> IpcClient {
        IpcClient::connect_from_admin_bootstrap_file(&self.admin_runtime_info_path)
            .await
            .expect("Failed to connect admin client from bootstrap file")
    }

    async fn shutdown(mut self) {
        let mut admin = self.connect_admin_client().await;
        let _ = admin.shutdown().await;

        let status = tokio::time::timeout(Duration::from_secs(5), self.child.wait())
            .await
            .expect("Timed out waiting for agsd to exit cleanly")
            .expect("Failed to wait on child process");
        assert!(status.success(), "agsd did not exit cleanly: {:?}", status);
    }

    #[allow(dead_code)]
    async fn kill(mut self) {
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
    }
}

#[tokio::test]
async fn test_subprocess_clean_restart_and_identity_preservation() {
    let state_dir = TempDir::new().expect("TempDir failed");
    let state_dir_path = state_dir.path().to_path_buf();
    let db_path = state_dir_path.join("agsd.db");

    let studio_id_saved;
    let task_id_saved;

    // Phase 1: Start subprocess 1, create studio and submit task
    {
        let daemon = AgsdSubprocess::spawn(state_dir.path(), Some(db_path.clone())).await;
        let mut client = daemon.connect_client().await;

        let studio = client
            .create_studio("Subprocess Studio")
            .await
            .expect("Create studio failed");
        studio_id_saved = studio.id;

        let task_spec = BatchTaskSpec {
            key: "subproc-task-1".to_string(),
            title: "Subprocess Task 1".to_string(),
            description: "Survives subprocess exit".to_string(),
            assigned_agent_id: None,
            parent_task_key: None,
            parent_task_id: None,
            dependency_keys: vec![],
            dependency_task_ids: vec![],
            dependency_output_policy: DependencyOutputPolicy::TaskSuccess,
            execution: None,
        };

        let task_ids = client
            .submit_task(studio_id_saved, task_spec)
            .await
            .expect("Submit task failed");
        assert_eq!(task_ids.len(), 1);
        task_id_saved = task_ids[0];

        daemon.shutdown().await;
    }

    assert!(db_path.exists(), "Database file must persist on disk");

    // Phase 2: Start subprocess 2 with the exact same state dir and db
    {
        let daemon2 = AgsdSubprocess::spawn(state_dir.path(), Some(db_path.clone())).await;
        let mut client2 = daemon2.connect_client().await;

        let task = client2
            .get_task(studio_id_saved, task_id_saved)
            .await
            .expect("Get task failed on restart")
            .expect("Task must exist");
        assert_eq!(task.id, task_id_saved);
        assert_eq!(task.studio_id, studio_id_saved);
        assert_eq!(task.title, "Subprocess Task 1");

        let status = client2
            .get_recovery_status()
            .await
            .expect("Get recovery status failed");
        assert!(status.database_healthy);
        assert!(status.total_persisted_events >= 2);
        assert_eq!(status.interrupted_tasks_count, 0);

        daemon2.shutdown().await;
    }
}

#[tokio::test]
async fn test_subprocess_single_writer_lock_rejection() {
    let state_dir = TempDir::new().expect("TempDir failed");
    let db_path = state_dir.path().join("shared.db");

    let daemon1 = AgsdSubprocess::spawn(state_dir.path(), Some(db_path.clone())).await;

    // Attempt to start second daemon pointing to identical db_path
    let state_dir2 = TempDir::new().expect("TempDir 2 failed");
    let bin_path = env!("CARGO_BIN_EXE_agsd");
    let mut cmd2 = Command::new(bin_path);
    cmd2.arg("--state_dir")
        .arg(state_dir2.path())
        .arg("--db_path")
        .arg(&db_path)
        .arg("--key_id")
        .arg("test-client-2")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(windows)]
    cmd2.arg("--pipe_name").arg(format!(
        r"\\.\pipe\agent-studios-test-{}",
        Uuid::new_v4().simple()
    ));
    #[cfg(unix)]
    cmd2.arg("--socket_path")
        .arg(state_dir2.path().join("daemon2.sock"));

    let mut child2 = cmd2.spawn().expect("Failed to spawn competing agsd");
    let exit_status = tokio::time::timeout(Duration::from_secs(5), child2.wait())
        .await
        .expect("Timed out waiting for competing agsd to exit")
        .expect("Failed to wait on competing agsd");

    assert!(
        !exit_status.success(),
        "Second agsd instance must fail lock acquisition and exit non-zero"
    );

    // Verify first daemon is still alive and responsive
    let mut client1 = daemon1.connect_client().await;
    let pong = client1
        .ping("ping")
        .await
        .expect("Daemon 1 should still respond to ping");
    assert_eq!(pong.payload, "ping");

    daemon1.shutdown().await;
}

#[tokio::test]
async fn test_subprocess_role_based_recovery_status_redaction() {
    let state_dir = TempDir::new().expect("TempDir failed");
    let daemon = AgsdSubprocess::spawn(state_dir.path(), None).await;

    // 1. Standard client: database_path must be redacted (None)
    let mut client = daemon.connect_client().await;
    let status_standard = client
        .get_recovery_status()
        .await
        .expect("Standard get_recovery_status failed");
    assert!(
        status_standard.database_path.is_none(),
        "Standard role must NOT receive host database path"
    );
    assert!(status_standard.database_healthy);

    // 2. Admin client: database_path must be disclosed (Some)
    let mut admin_client = daemon.connect_admin_client().await;
    let status_admin = admin_client
        .get_recovery_status()
        .await
        .expect("Admin get_recovery_status failed");
    assert!(
        status_admin.database_path.is_some(),
        "Admin role MUST receive host database path"
    );
    assert!(
        status_admin.database_path.unwrap().contains("agsd.db"),
        "Admin database_path must contain db file name"
    );

    daemon.shutdown().await;
}

#[tokio::test]
async fn test_subprocess_startup_failure_cleans_bootstrap_files() {
    let state_dir = TempDir::new().expect("TempDir failed");
    let corrupt_db = state_dir.path().join("corrupt.db");
    // Write invalid/corrupt data to simulate corrupted SQLite database
    std::fs::write(&corrupt_db, b"NOT_A_VALID_SQLITE_DATABASE_HEADER_DATA").unwrap();

    let runtime_info = state_dir.path().join("agsd.json");
    let admin_runtime_info = state_dir.path().join("agsd-admin.json");

    let bin_path = env!("CARGO_BIN_EXE_agsd");
    let mut cmd = Command::new(bin_path);
    cmd.arg("--state_dir")
        .arg(state_dir.path())
        .arg("--db_path")
        .arg(&corrupt_db)
        .arg("--runtime_info_path")
        .arg(&runtime_info)
        .arg("--admin_runtime_info_path")
        .arg(&admin_runtime_info)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(windows)]
    cmd.arg("--pipe_name").arg(format!(
        r"\\.\pipe\agent-studios-test-{}",
        Uuid::new_v4().simple()
    ));
    #[cfg(unix)]
    cmd.arg("--socket_path")
        .arg(state_dir.path().join("corrupt.sock"));

    let mut child = cmd.spawn().expect("Failed to spawn agsd");
    let exit_status = tokio::time::timeout(Duration::from_secs(5), child.wait())
        .await
        .expect("Timed out waiting for agsd to fail")
        .expect("Failed to wait on agsd");

    assert!(
        !exit_status.success(),
        "agsd must exit non-zero when database fails preflight integrity check"
    );

    // Bootstrap files must NOT be present
    assert!(
        !runtime_info.exists(),
        "Bootstrap runtime_info must not remain on startup failure"
    );
    assert!(
        !admin_runtime_info.exists(),
        "Bootstrap admin_runtime_info must not remain on startup failure"
    );
}

#[tokio::test]
async fn test_subprocess_kill_and_crash_recovery_reconciliation() {
    let state_dir = TempDir::new().expect("TempDir failed");
    let db_path = state_dir.path().join("crash_recovery.db");

    let studio_id_saved;
    let task_id_saved;

    // Pre-populate database with an in-flight (Running) task using SqliteEventStore
    {
        use agent_studios_control_plane::sqlite_store::SqliteEventStore;
        use agent_studios_control_plane::store::EventStore;
        use agent_studios_protocol::event::{ControlPlaneEvent, EventEnvelope};
        use agent_studios_protocol::id::{AgentId, EventId, StudioId, TaskId};
        use agent_studios_protocol::task::TaskRecord;

        let mut store = SqliteEventStore::open(&db_path).expect("Failed to open SQLite store");
        let studio_id = StudioId::new();
        let task_id = TaskId::new();
        let agent_id = AgentId::new();
        studio_id_saved = studio_id;
        task_id_saved = task_id;

        let studio = agent_studios_protocol::studio::Studio::with_id(
            studio_id,
            "Crash Test Studio".to_string(),
            chrono::Utc::now(),
        );

        let agent = agent_studios_protocol::agent::AgentDescriptor {
            id: agent_id,
            studio_id,
            display_name: "Worker Agent".to_string(),
            kind: agent_studios_protocol::agent::AgentKind::Internal,
            state: agent_studios_protocol::agent::AgentState::Registered,
            role: None,
        };

        let task = TaskRecord {
            id: task_id,
            studio_id,
            parent_task_id: None,
            assigned_agent_id: Some(agent_id),
            title: "Interrupted Task".to_string(),
            description: "In-flight during simulated kill".to_string(),
            state: TaskState::Running,
            dependencies: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            retry_count: 0,
            dependency_output_policy: DependencyOutputPolicy::TaskSuccess,
            execution: None,
        };

        let events = vec![
            EventEnvelope {
                event_id: EventId::new(),
                schema_version: 1,
                studio_id,
                sequence: 1,
                timestamp: chrono::Utc::now(),
                event: ControlPlaneEvent::StudioCreated { studio },
            },
            EventEnvelope {
                event_id: EventId::new(),
                schema_version: 1,
                studio_id,
                sequence: 2,
                timestamp: chrono::Utc::now(),
                event: ControlPlaneEvent::AgentRegistered { agent },
            },
            EventEnvelope {
                event_id: EventId::new(),
                schema_version: 1,
                studio_id,
                sequence: 3,
                timestamp: chrono::Utc::now(),
                event: ControlPlaneEvent::TaskCreated { task },
            },
        ];

        store
            .append_batch(&events)
            .expect("Failed to append seed events");
    }

    // Now launch agsd subprocess pointing to this database
    let daemon = AgsdSubprocess::spawn(state_dir.path(), Some(db_path.clone())).await;
    let mut client = daemon.connect_client().await;

    // Check recovery status: should have detected 1 interrupted task and transitioned it to Failed
    let status = client
        .get_recovery_status()
        .await
        .expect("Get recovery status failed");
    assert_eq!(
        status.interrupted_tasks_count, 1,
        "Startup recovery must detect the 1 in-flight running task"
    );

    // Verify task state via IPC is now Failed
    let task = client
        .get_task(studio_id_saved, task_id_saved)
        .await
        .expect("Get task failed")
        .expect("Task must exist");
    assert_eq!(task.state, TaskState::Failed);

    daemon.shutdown().await;
}
