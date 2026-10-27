//! In-process Embedded Implementation of AgentStudiosClient
//!
//! Owns the deterministic in-process ControlPlane, EventStore, and local event broadcast channel.

use std::sync::Arc;
use async_trait::async_trait;
use tokio::sync::{broadcast, mpsc, Mutex, RwLock};

use agent_studios_control_plane::clock::SystemClock;
use agent_studios_control_plane::engine::ControlPlane;
use agent_studios_control_plane::store::{EventStore, InMemoryStore};
use agent_studios_orchestration::projection::{
    build_task_graph_snapshot, project_agent_summary, project_task_timeline,
};
use agent_studios_orchestration::read_models::{
    AgentSummary, TaskGraphSnapshot, TaskTimelineProjection,
};
use agent_studios_protocol::agent::AgentKind;
use agent_studios_protocol::approval::ApprovalState;
use agent_studios_protocol::cancellation::CancellationScope;
use agent_studios_protocol::event::EventEnvelope;
use agent_studios_protocol::id::{ApprovalId, ReconciliationId, StudioId, TaskId, WorktreeId};
use agent_studios_workspace::capture::CapturedChanges;
use agent_studios_workspace::reconciliation::ReconciliationOutcome;

use crate::client::{
    AgentStudiosClient, ClientApprovalDecision, ClientError, ClientSubscription, ExecutionMode,
    GoalSubmissionOptions, WorktreeRecordSummary,
};

/// In-process client running the deterministic control plane inside the CLI process.
pub struct EmbeddedClient {
    studio_id: StudioId,
    control_plane: Arc<Mutex<ControlPlane<SystemClock, InMemoryStore>>>,
    events_tx: broadcast::Sender<EventEnvelope>,
    events_history: Arc<RwLock<Vec<EventEnvelope>>>,
}

impl EmbeddedClient {
    /// Bootstraps a fresh in-process EmbeddedClient.
    pub async fn bootstrap(workspace_name: &str) -> Result<Self, ClientError> {
        let store = InMemoryStore::new();
        let clock = SystemClock;
        let mut cp = ControlPlane::new(clock, store);

        let (tx, _rx) = broadcast::channel(512);
        let events_history = Arc::new(RwLock::new(Vec::new()));

        let studio = cp
            .create_studio(workspace_name)
            .map_err(|e| ClientError::OperationFailed(format!("Failed to create studio: {e}")))?;
        let studio_id = studio.id;

        // Register default multi-agent team roles
        let default_roles = [
            ("Coordinator", AgentKind::Internal, Some("coordinator".to_string())),
            ("Coder", AgentKind::Internal, Some("coder".to_string())),
            ("Tester", AgentKind::Internal, Some("tester".to_string())),
            ("Reviewer", AgentKind::Internal, Some("reviewer".to_string())),
        ];

        for (name, kind, role) in default_roles {
            let _ = cp.register_agent(studio_id, name, kind, role);
        }

        // Collect all initial events into history
        if let Ok(events) = cp.store().events_for_studio(studio_id, 0) {
            let mut history = events_history.write().await;
            for ev in events {
                let _ = tx.send(ev.clone());
                history.push(ev);
            }
        }

        Ok(Self {
            studio_id,
            control_plane: Arc::new(Mutex::new(cp)),
            events_tx: tx,
            events_history,
        })
    }

    async fn sync_events_internal(
        cp: &ControlPlane<SystemClock, InMemoryStore>,
        studio_id: StudioId,
        history_lock: &RwLock<Vec<EventEnvelope>>,
        tx: &broadcast::Sender<EventEnvelope>,
    ) {
        if let Ok(events) = cp.store().events_for_studio(studio_id, 0) {
            let mut history = history_lock.write().await;
            for ev in events {
                if !history.iter().any(|h| h.event_id == ev.event_id) {
                    let _ = tx.send(ev.clone());
                    history.push(ev);
                }
            }
        }
    }
}

#[async_trait]
impl AgentStudiosClient for EmbeddedClient {
    fn studio_id(&self) -> StudioId {
        self.studio_id
    }

    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Embedded
    }

    async fn shutdown(&self) -> Result<(), ClientError> {
        tracing::info!(studio_id = %self.studio_id, "Embedded client shutting down");
        Ok(())
    }

    async fn submit_goal(
        &self,
        prompt: &str,
        _options: GoalSubmissionOptions,
    ) -> Result<TaskId, ClientError> {
        let mut cp = self.control_plane.lock().await;
        let task = cp
            .create_task(
                self.studio_id,
                prompt.chars().take(80).collect::<String>(),
                prompt,
                None,
                None,
                Vec::new(),
            )
            .map_err(|e| ClientError::OperationFailed(format!("Failed to submit task: {e}")))?;

        Self::sync_events_internal(
            &cp,
            self.studio_id,
            &self.events_history,
            &self.events_tx,
        )
        .await;

        Ok(task.id)
    }

    async fn cancel_active_run(&self, reason: Option<String>) -> Result<(), ClientError> {
        let mut cp = self.control_plane.lock().await;
        let _ = cp
            .request_cancellation(CancellationScope::Studio(self.studio_id), reason)
            .map_err(|e| ClientError::OperationFailed(format!("Failed to request cancellation: {e}")))?;

        Self::sync_events_internal(
            &cp,
            self.studio_id,
            &self.events_history,
            &self.events_tx,
        )
        .await;

        Ok(())
    }

    async fn subscribe_events(&self, from_sequence: u64) -> Result<ClientSubscription, ClientError> {
        let history = self.events_history.read().await;
        let backfill: Vec<EventEnvelope> = history
            .iter()
            .filter(|env| env.sequence >= from_sequence)
            .cloned()
            .collect();

        let (tx, rx) = mpsc::channel(256);
        let mut bcast_rx = self.events_tx.subscribe();

        tokio::spawn(async move {
            while let Ok(env) = bcast_rx.recv().await {
                if tx.send(env).await.is_err() {
                    break;
                }
            }
        });

        Ok(ClientSubscription {
            backfill,
            stream: rx,
        })
    }

    async fn get_task_graph(&self) -> Result<TaskGraphSnapshot, ClientError> {
        let cp = self.control_plane.lock().await;
        Ok(build_task_graph_snapshot(self.studio_id, cp.state()))
    }

    async fn get_agent_summaries(&self) -> Result<Vec<AgentSummary>, ClientError> {
        let cp = self.control_plane.lock().await;
        let history = self.events_history.read().await;

        let mut summaries = Vec::new();
        for agent in cp.state().agents.values() {
            if agent.studio_id == self.studio_id {
                if let Some(summary) = project_agent_summary(agent.id, &history, None) {
                    summaries.push(summary);
                }
            }
        }

        Ok(summaries)
    }

    async fn get_task_timeline(&self, task_id: TaskId) -> Result<TaskTimelineProjection, ClientError> {
        let history = self.events_history.read().await;
        project_task_timeline(task_id, &history)
            .ok_or_else(|| ClientError::NotFound(format!("Task {task_id} timeline not found")))
    }

    async fn resolve_approval(
        &self,
        approval_id: ApprovalId,
        decision: ClientApprovalDecision,
    ) -> Result<(), ClientError> {
        let mut cp = self.control_plane.lock().await;

        let target_state = match decision {
            ClientApprovalDecision::Allow | ClientApprovalDecision::AllowForSession => {
                ApprovalState::Approved
            }
            ClientApprovalDecision::Deny => ApprovalState::Denied,
        };

        cp.resolve_approval(approval_id, target_state)
            .map_err(|e| ClientError::OperationFailed(format!("Failed to resolve approval: {e}")))?;

        Self::sync_events_internal(
            &cp,
            self.studio_id,
            &self.events_history,
            &self.events_tx,
        )
        .await;

        Ok(())
    }

    async fn list_worktrees(&self) -> Result<Vec<WorktreeRecordSummary>, ClientError> {
        let cp = self.control_plane.lock().await;
        let mut summaries = Vec::new();

        for wt in cp.state().worktrees.values() {
            if wt.studio_id == self.studio_id {
                summaries.push(WorktreeRecordSummary {
                    worktree_id: wt.id,
                    rel_path: wt.name.clone(),
                    state: format!("{:?}", wt.state),
                });
            }
        }

        Ok(summaries)
    }

    async fn get_worktree_diff(&self, _worktree_id: WorktreeId) -> Result<CapturedChanges, ClientError> {
        Ok(CapturedChanges {
            patch_bytes: Vec::new(),
            files_changed: 0,
            changed_files: Vec::new(),
            stats_summary: "No changes".to_string(),
            base_commit: "HEAD".to_string(),
            head_commit: None,
        })
    }

    async fn reconcile_patch(
        &self,
        _reconciliation_id: ReconciliationId,
    ) -> Result<ReconciliationOutcome, ClientError> {
        Ok(ReconciliationOutcome::Applied {
            merge_commit: Some("clean".to_string()),
        })
    }
}
