use agent_studios_cli_client::{AgentStudiosClient, EmbeddedClient, GoalSubmissionOptions};
use std::time::Duration;
use tokio::time::timeout;

#[tokio::test]
async fn test_subscription_backfill_and_stream_deduplication() {
    let client = EmbeddedClient::bootstrap("sub-dedup-test")
        .await
        .expect("bootstrap failed");

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
    let client = EmbeddedClient::bootstrap("future-seq-test")
        .await
        .expect("bootstrap failed");

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
    let client = EmbeddedClient::bootstrap_with_capacity("lag-recovery-test", 4)
        .await
        .expect("bootstrap failed");

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
