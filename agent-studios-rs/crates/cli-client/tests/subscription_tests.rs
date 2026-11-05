use chrono::Utc;
use std::fs;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;
use tokio::time::timeout;

use agent_studios_cli_client::{AgentStudiosClient, EmbeddedClient, GoalSubmissionOptions};
use agent_studios_protocol::event::{ControlPlaneEvent, EventEnvelope};
use agent_studios_protocol::id::{AgentId, TaskId};
use agent_studios_protocol::task::TaskState;
use agent_studios_workspace::WorkspaceOrchestrator;

fn init_git_repo(path: &std::path::Path) -> String {
    let run = |args: &[&str]| {
        let output = Command::new("git")
            .current_dir(path)
            .args(args)
            .output()
            .unwrap_or_else(|e| panic!("Failed to run git {:?}: {e}", args));
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    };

    run(&["init"]);
    run(&["config", "user.name", "Agent Studios Test"]);
    run(&["config", "user.email", "test@agent-studios.internal"]);
    run(&["config", "commit.gpgsign", "false"]);

    fs::write(path.join("file_in_main.txt"), "Initial main content\n").unwrap();
    run(&["add", "file_in_main.txt"]);
    run(&["commit", "-m", "Initial commit"]);

    run(&["rev-parse", "HEAD"])
}

struct TestFixture {
    _source_dir: tempfile::TempDir,
    _orchestrator_dir: tempfile::TempDir,
    client: EmbeddedClient,
}

impl TestFixture {
    async fn new(workspace_name: &str, capacity: usize) -> Self {
        let source_dir = tempdir().expect("source tempdir");
        init_git_repo(source_dir.path());

        let orchestrator_dir = tempdir().expect("orchestrator tempdir");
        let wt_root = orchestrator_dir.path().join("worktrees");
        let art_root = orchestrator_dir.path().join("artifacts");

        let orchestrator = Arc::new(
            WorkspaceOrchestrator::new(wt_root, art_root).expect("orchestrator initialization"),
        );

        let client = EmbeddedClient::bootstrap_with_capacity(workspace_name, capacity)
            .await
            .expect("bootstrap failed")
            .with_orchestrator(orchestrator)
            .with_working_dir(source_dir.path().to_path_buf());

        Self {
            _source_dir: source_dir,
            _orchestrator_dir: orchestrator_dir,
            client,
        }
    }

    fn into_parts(self) -> (tempfile::TempDir, tempfile::TempDir, EmbeddedClient) {
        (self._source_dir, self._orchestrator_dir, self.client)
    }
}

#[tokio::test]
async fn test_subscription_backfill_and_stream_deduplication() {
    let fixture = TestFixture::new("sub-dedup-test", 512).await;
    let client = &fixture.client;

    // Initial events: StudioCreated (1) + 4x AgentRegistered (2..=5) = 5 events
    let mut sub = client.subscribe_events(0).await.expect("subscribe failed");

    assert_eq!(sub.backfill.len(), 5);
    let backfill_sequences: Vec<u64> = sub.backfill.iter().map(|e| e.sequence).collect();
    assert_eq!(backfill_sequences, vec![1, 2, 3, 4, 5]);

    // Submit a goal -> triggers new event (sequence 6)
    let _task_id = client
        .submit_goal("Goal 1", GoalSubmissionOptions::default())
        .await
        .expect("submit_goal failed");

    let live_ev = timeout(Duration::from_secs(3), sub.stream.recv())
        .await
        .expect("timeout waiting for live event")
        .expect("stream ended unexpectedly");

    assert_eq!(live_ev.sequence, 6);

    // Subscribe again from sequence 1: backfill should contain all events emitted so far (>= 6)
    let mut sub2 = client.subscribe_events(1).await.expect("subscribe2 failed");

    assert!(sub2.backfill.len() >= 6);
    let last_backfill_seq = sub2.backfill.last().map(|e| e.sequence).unwrap();

    // Submit second goal -> triggers next events
    let _task_id2 = client
        .submit_goal("Goal 2", GoalSubmissionOptions::default())
        .await
        .expect("submit_goal 2 failed");

    let live_ev2 = timeout(Duration::from_secs(3), sub2.stream.recv())
        .await
        .expect("timeout waiting for live event 2")
        .expect("stream ended unexpectedly");

    // Must be sequence right after last_backfill_seq, deduplicating any prior events <= last_backfill_seq
    assert_eq!(live_ev2.sequence, last_backfill_seq + 1);
}

#[tokio::test]
async fn test_subscription_future_sequence_filtering() {
    let fixture = TestFixture::new("future-seq-test", 512).await;
    let client = &fixture.client;

    // Client starts at sequence 5. Subscribe from sequence 7 (future sequence)
    let mut sub = client.subscribe_events(7).await.expect("subscribe failed");

    assert!(
        sub.backfill.is_empty(),
        "Backfill must be empty for future sequence"
    );

    // Goal 1 generates sequence 6: should be filtered out by stream
    let _task1 = client
        .submit_goal("Goal 1 (seq 6)", GoalSubmissionOptions::default())
        .await
        .expect("submit_goal 1 failed");

    // Goal 2 generates sequence 7: should be delivered
    let _task2 = client
        .submit_goal("Goal 2 (seq 7)", GoalSubmissionOptions::default())
        .await
        .expect("submit_goal 2 failed");

    let ev = timeout(Duration::from_secs(3), sub.stream.recv())
        .await
        .expect("timeout waiting for event")
        .expect("stream ended");

    assert_eq!(
        ev.sequence, 7,
        "First received stream event must be sequence 7"
    );
}

#[tokio::test]
async fn test_subscription_broadcast_lag_recovery_zero_event_loss() {
    // Bootstrap client with tiny broadcast capacity (4) to intentionally force broadcast lag
    let fixture = TestFixture::new("lag-recovery-test", 4).await;
    let client = &fixture.client;

    // Initial events: 5. Subscribe from 0.
    let mut sub = client.subscribe_events(0).await.expect("subscribe failed");

    assert_eq!(sub.backfill.len(), 5);

    // Flood with 20 tasks without reading from stream immediately
    // This forces broadcast ring buffer (capacity 4) to overflow and lag
    let total_tasks = 20;
    for i in 1..=total_tasks {
        client
            .submit_goal(&format!("Batch goal {i}"), GoalSubmissionOptions::default())
            .await
            .expect("submit_goal failed");
    }

    // Read all 20 events from the stream
    // The lag recovery mechanism must fetch missed events from store, guaranteeing zero loss
    let mut received_sequences = Vec::new();
    for _ in 1..=total_tasks {
        let ev = timeout(Duration::from_secs(5), sub.stream.recv())
            .await
            .expect("timeout waiting for stream event during lag recovery")
            .expect("stream ended unexpectedly");
        received_sequences.push(ev.sequence);
    }

    let expected_sequences: Vec<u64> = (6..=25).collect();
    assert_eq!(
        received_sequences, expected_sequences,
        "All events must be received in sequential order without loss or duplication despite broadcast lag"
    );
}

#[tokio::test]
async fn test_subscription_sequence_gap_recovery_from_history() {
    let client = EmbeddedClient::bootstrap("gap-recovery-test")
        .await
        .expect("bootstrap failed");

    let mut sub = client.subscribe_events(0).await.expect("subscribe failed");
    assert_eq!(sub.backfill.len(), 5);

    let studio_id = client.studio_id();

    // Event 6: broadcasted and added to history
    let ev6 = EventEnvelope::new(
        studio_id,
        6,
        Utc::now(),
        ControlPlaneEvent::AssistantMessageChunk {
            agent_id: AgentId::new(),
            task_id: None,
            run_id: None,
            content: "Chunk 6".to_string(),
            is_thought: false,
        },
    );
    client
        .inject_raw_event_for_testing(ev6.clone(), true, true)
        .await;

    let r6 = timeout(Duration::from_secs(3), sub.stream.recv())
        .await
        .expect("timeout waiting for ev6")
        .expect("stream ended");
    assert_eq!(r6.sequence, 6);

    // Event 7: Dropped from broadcast (history ONLY)
    let ev7 = EventEnvelope::new(
        studio_id,
        7,
        Utc::now(),
        ControlPlaneEvent::AssistantMessageChunk {
            agent_id: AgentId::new(),
            task_id: None,
            run_id: None,
            content: "Chunk 7 (missed broadcast)".to_string(),
            is_thought: false,
        },
    );
    client
        .inject_raw_event_for_testing(ev7.clone(), true, false)
        .await;

    // Event 8: Broadcasted and added to history
    let ev8 = EventEnvelope::new(
        studio_id,
        8,
        Utc::now(),
        ControlPlaneEvent::AssistantMessageChunk {
            agent_id: AgentId::new(),
            task_id: None,
            run_id: None,
            content: "Chunk 8".to_string(),
            is_thought: false,
        },
    );
    client
        .inject_raw_event_for_testing(ev8.clone(), true, true)
        .await;

    // Stream receives live event 8, detects gap for sequence 7, recovers 7 from history,
    // and emits sequence 7 followed by 8
    let r7 = timeout(Duration::from_secs(3), sub.stream.recv())
        .await
        .expect("timeout waiting for recovered ev7")
        .expect("stream ended");
    assert_eq!(r7.sequence, 7);
    assert_eq!(r7.event, ev7.event);

    let r8 = timeout(Duration::from_secs(3), sub.stream.recv())
        .await
        .expect("timeout waiting for ev8")
        .expect("stream ended");
    assert_eq!(r8.sequence, 8);
    assert_eq!(r8.event, ev8.event);
}

#[tokio::test]
async fn test_subscription_unrecoverable_gap_preserves_terminal_events() {
    let client = EmbeddedClient::bootstrap("unrecoverable-gap-test")
        .await
        .expect("bootstrap failed");

    let mut sub = client.subscribe_events(0).await.expect("subscribe failed");
    assert_eq!(sub.backfill.len(), 5);

    let studio_id = client.studio_id();

    // Event 6 delivered normally
    let ev6 = EventEnvelope::new(
        studio_id,
        6,
        Utc::now(),
        ControlPlaneEvent::AssistantMessageChunk {
            agent_id: AgentId::new(),
            task_id: None,
            run_id: None,
            content: "Chunk 6".to_string(),
            is_thought: false,
        },
    );
    client
        .inject_raw_event_for_testing(ev6.clone(), true, true)
        .await;
    let r6 = timeout(Duration::from_secs(3), sub.stream.recv())
        .await
        .expect("timeout waiting for ev6")
        .expect("stream ended");
    assert_eq!(r6.sequence, 6);

    // Sequence 7 is permanently lost/pruned: neither in broadcast nor in history.
    // Event 8 is non-terminal, broadcasted and in history.
    let ev8 = EventEnvelope::new(
        studio_id,
        8,
        Utc::now(),
        ControlPlaneEvent::AssistantMessageChunk {
            agent_id: AgentId::new(),
            task_id: None,
            run_id: None,
            content: "Chunk 8 (after unrecoverable gap)".to_string(),
            is_thought: false,
        },
    );
    client
        .inject_raw_event_for_testing(ev8.clone(), true, true)
        .await;

    // Event 9 is TERMINAL (TaskStateChanged -> Succeeded)
    let ev9 = EventEnvelope::new(
        studio_id,
        9,
        Utc::now(),
        ControlPlaneEvent::TaskStateChanged {
            task_id: TaskId::new(),
            previous_state: TaskState::Running,
            new_state: TaskState::Succeeded,
        },
    );
    client
        .inject_raw_event_for_testing(ev9.clone(), true, true)
        .await;

    // Terminal event 9 must be delivered despite unrecoverable gap; non-terminal event 8 dropped.
    let r_term = timeout(Duration::from_secs(3), sub.stream.recv())
        .await
        .expect("timeout waiting for terminal event")
        .expect("stream ended");
    assert_eq!(r_term.sequence, 9);
    assert_eq!(r_term.event, ev9.event);
}

#[tokio::test]
async fn test_simultaneous_multiple_subscribers() {
    let fixture = TestFixture::new("multi-sub-test", 512).await;
    let client = Arc::new(fixture.client);

    // Initial 5 events
    let sub1 = client.subscribe_events(0).await.expect("sub1 failed");
    let sub2 = client.subscribe_events(3).await.expect("sub2 failed");
    let sub3 = client.subscribe_events(5).await.expect("sub3 failed");
    let sub4 = client.subscribe_events(6).await.expect("sub4 failed");

    assert_eq!(sub1.backfill.len(), 5);
    assert_eq!(sub2.backfill.len(), 3); // 3, 4, 5
    assert_eq!(sub3.backfill.len(), 1); // 5
    assert_eq!(sub4.backfill.len(), 0);

    let client_clone = client.clone();
    let producer = tokio::spawn(async move {
        for i in 1..=10 {
            client_clone
                .submit_goal(
                    &format!("Multi-sub goal {i}"),
                    GoalSubmissionOptions::default(),
                )
                .await
                .expect("submit failed");
        }
    });

    let read_stream = |mut stream: tokio::sync::mpsc::Receiver<EventEnvelope>, count: usize| {
        tokio::spawn(async move {
            let mut seqs = Vec::new();
            for _ in 0..count {
                if let Ok(Some(ev)) = timeout(Duration::from_secs(20), stream.recv()).await {
                    seqs.push(ev.sequence);
                } else {
                    break;
                }
            }
            seqs
        })
    };

    let h1 = read_stream(sub1.stream, 10);
    let h2 = read_stream(sub2.stream, 10);
    let h3 = read_stream(sub3.stream, 10);
    let h4 = read_stream(sub4.stream, 10);

    producer.await.expect("producer task failed");

    let s1 = h1.await.expect("h1 failed");
    let s2 = h2.await.expect("h2 failed");
    let s3 = h3.await.expect("h3 failed");
    let s4 = h4.await.expect("h4 failed");

    let expected: Vec<u64> = (6..=15).collect();
    assert_eq!(s1, expected);
    assert_eq!(s2, expected);
    assert_eq!(s3, expected);
    assert_eq!(s4, expected);
}

#[tokio::test]
async fn test_channel_shutdown_and_lag_recovery() {
    let (_src, _orch, client) = TestFixture::new("lag-shutdown-test", 2).await.into_parts();

    let mut sub = client.subscribe_events(0).await.expect("sub failed");
    assert_eq!(sub.backfill.len(), 5);

    // Flood tasks to force broadcast buffer overflow and lag
    for i in 1..=10 {
        client
            .submit_goal(&format!("Lag task {i}"), GoalSubmissionOptions::default())
            .await
            .expect("submit failed");
    }

    // Read first 5 stream events
    let mut received = Vec::new();
    for _ in 0..5 {
        let ev = timeout(Duration::from_secs(3), sub.stream.recv())
            .await
            .expect("timeout")
            .expect("stream ended prematurely");
        received.push(ev.sequence);
    }
    assert_eq!(received, vec![6, 7, 8, 9, 10]);

    // Channel shutdown: drop client while remaining events are unread
    drop(client);
    let _ = (&_src, &_orch);

    // Subscriber drains history on Closed event in strict monotonic order until stream EOF (None)
    let mut last_seq = 10;
    let mut drained_count = 0;
    while let Some(ev) = timeout(Duration::from_secs(3), sub.stream.recv())
        .await
        .expect("timeout waiting for stream drain")
    {
        assert_eq!(
            ev.sequence,
            last_seq + 1,
            "Sequences must be strictly contiguous during shutdown drain"
        );
        last_seq = ev.sequence;
        drained_count += 1;
    }

    assert!(
        drained_count >= 5,
        "Expected at least 5 remaining events to be drained on shutdown, got {drained_count}"
    );
}
