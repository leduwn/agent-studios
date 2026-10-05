use std::sync::Arc;

use agent_studios_control_plane::SystemClock;
use agent_studios_control_plane::engine::ControlPlane;
use agent_studios_control_plane::store::InMemoryStore;
use agent_studios_internal_agent::{
    AgentBudgetTracker, AgentExecutionBudget, AgentRuntimeExtensionContext,
    AgentStudiosToolLifecycleContributor, BudgetScopeId, ControlPlaneActor, ControlPlaneHandle,
};
use agent_studios_protocol::agent::AgentKind;
use agent_studios_protocol::id::{AgentId, RunId, TaskId};
use codex_extension_api::{
    ConversationHistorySnapshot, ExtensionData, ExtensionDataInit, ResponseItem, ToolCallOutcome,
    ToolCallSource, ToolFinishInput, ToolLifecycleContributor, ToolName, ToolPayload,
    ToolStartInput,
};

fn create_test_actor() -> (ControlPlaneHandle, tokio::task::JoinHandle<()>) {
    let clock = SystemClock;
    let store = InMemoryStore::new();
    let cp = ControlPlane::new(clock, store);
    ControlPlaneActor::spawn(cp)
}

struct EmptyHistory;
impl ConversationHistorySnapshot for EmptyHistory {
    fn history_version(&self) -> u64 {
        0
    }
    fn user_message_revision(&self) -> u64 {
        0
    }
    fn items(&self) -> Box<dyn Iterator<Item = &ResponseItem> + Send + '_> {
        Box::new(std::iter::empty())
    }
}

#[tokio::test]
async fn test_tool_lifecycle_contributor_budget_blocking_and_commit() {
    let (cp_handle, _task) = create_test_actor();
    let agent_id = AgentId::new();

    // Register studio and agent in CP
    let studio = cp_handle.create_studio("Test Studio").await.unwrap();
    let studio_id = studio.id;
    let _agent = cp_handle
        .register_agent(studio_id, "Worker", AgentKind::Internal, None)
        .await
        .unwrap();

    // Budget: max 1 tool call
    let budget = AgentExecutionBudget::default().with_tool_calls(1);
    let tracker = Arc::new(AgentBudgetTracker::new(agent_id, budget));

    let context = AgentRuntimeExtensionContext::new(
        studio_id,
        agent_id,
        Arc::clone(&tracker),
        cp_handle.clone(),
    );

    let mut init = ExtensionDataInit::new();
    init.insert(context);

    let thread_store = ExtensionData::new_with_init("test_thread", init);
    let session_store = ExtensionData::new("test_session");
    let turn_store = ExtensionData::new("test_turn");

    let contributor = AgentStudiosToolLifecycleContributor::new();

    let tool_name = ToolName::plain("read_file");
    let payload = ToolPayload::Custom {
        input: "{}".to_string(),
    };
    let history: Arc<dyn ConversationHistorySnapshot> = Arc::new(EmptyHistory);

    // Call 1: Authorized (within budget)
    let start_input1 = ToolStartInput {
        session_store: &session_store,
        thread_store: &thread_store,
        turn_store: &turn_store,
        turn_id: "turn_1",
        root_turn_id: None,
        call_id: "call_1",
        originating_item_id: None,
        tool_name: &tool_name,
        mcp_tool: None,
        permissions: Box::pin(std::future::ready(None)),
        payload: &payload,
        conversation_history: Arc::clone(&history),
        source: ToolCallSource::Direct,
    };

    assert!(contributor.authorize_tool_call(&start_input1).is_ok());
    assert_eq!(tracker.tool_calls_reserved(), 1);
    assert_eq!(tracker.tool_calls_used(), 0);

    // Call 2: Blocked pre-execution (N+1 exceeds limit 1)
    let start_input2 = ToolStartInput {
        session_store: &session_store,
        thread_store: &thread_store,
        turn_store: &turn_store,
        turn_id: "turn_1",
        root_turn_id: None,
        call_id: "call_2",
        originating_item_id: None,
        tool_name: &tool_name,
        mcp_tool: None,
        permissions: Box::pin(std::future::ready(None)),
        payload: &payload,
        conversation_history: Arc::clone(&history),
        source: ToolCallSource::Direct,
    };

    let err = contributor.authorize_tool_call(&start_input2);
    assert!(err.is_err());
    let err_msg = err.unwrap_err();
    assert!(err_msg.contains("Tool call budget exceeded"));

    // Finish call 1 successfully -> commits tool call
    let finish_input1 = ToolFinishInput {
        session_store: &session_store,
        thread_store: &thread_store,
        turn_store: &turn_store,
        turn_id: "turn_1",
        call_id: "call_1",
        tool_name: &tool_name,
        source: ToolCallSource::Direct,
        outcome: ToolCallOutcome::Completed { success: true },
    };

    contributor.on_tool_finish(finish_input1).await;

    assert_eq!(tracker.tool_calls_reserved(), 0);
    assert_eq!(tracker.tool_calls_used(), 1);

    // Finish call 2 as Blocked -> does not touch reservations or increments
    let finish_input2 = ToolFinishInput {
        session_store: &session_store,
        thread_store: &thread_store,
        turn_store: &turn_store,
        turn_id: "turn_1",
        call_id: "call_2",
        tool_name: &tool_name,
        source: ToolCallSource::Direct,
        outcome: ToolCallOutcome::Blocked,
    };

    contributor.on_tool_finish(finish_input2).await;

    assert_eq!(tracker.tool_calls_reserved(), 0);
    assert_eq!(tracker.tool_calls_used(), 1);
}

#[tokio::test]
async fn test_tool_lifecycle_contributor_worker_reuse_context_update() {
    let (cp_handle, _task) = create_test_actor();
    let agent_id = AgentId::new();

    let studio = cp_handle.create_studio("Test Studio").await.unwrap();
    let studio_id = studio.id;
    let _agent = cp_handle
        .register_agent(studio_id, "Worker", AgentKind::Internal, None)
        .await
        .unwrap();

    // Run 1: Budget 1 tool call
    let run1_id = RunId::new();
    let task1_id = TaskId::new();
    let tracker1 = Arc::new(AgentBudgetTracker::new_with_scope(
        BudgetScopeId::Run(run1_id),
        agent_id,
        AgentExecutionBudget::default().with_tool_calls(1),
    ));

    let context = AgentRuntimeExtensionContext::new(
        studio_id,
        agent_id,
        Arc::clone(&tracker1),
        cp_handle.clone(),
    );
    context.set_task_and_run(Some(task1_id), Some(run1_id), Arc::clone(&tracker1));

    let mut init = ExtensionDataInit::new();
    init.insert(context.clone());

    let thread_store = ExtensionData::new_with_init("reused_thread", init);
    let session_store = ExtensionData::new("test_session");
    let turn_store = ExtensionData::new("test_turn");
    let contributor = AgentStudiosToolLifecycleContributor::new();

    let tool_name = ToolName::plain("bash");
    let payload = ToolPayload::Custom {
        input: "{}".to_string(),
    };
    let history: Arc<dyn ConversationHistorySnapshot> = Arc::new(EmptyHistory);

    // Call under Run 1 succeeds
    let start_1 = ToolStartInput {
        session_store: &session_store,
        thread_store: &thread_store,
        turn_store: &turn_store,
        turn_id: "turn_1",
        root_turn_id: None,
        call_id: "call_r1",
        originating_item_id: None,
        tool_name: &tool_name,
        mcp_tool: None,
        permissions: Box::pin(std::future::ready(None)),
        payload: &payload,
        conversation_history: Arc::clone(&history),
        source: ToolCallSource::Direct,
    };
    assert!(contributor.authorize_tool_call(&start_1).is_ok());

    let finish_1 = ToolFinishInput {
        session_store: &session_store,
        thread_store: &thread_store,
        turn_store: &turn_store,
        turn_id: "turn_1",
        call_id: "call_r1",
        tool_name: &tool_name,
        source: ToolCallSource::Direct,
        outcome: ToolCallOutcome::Completed { success: true },
    };
    contributor.on_tool_finish(finish_1).await;
    assert_eq!(tracker1.tool_calls_used(), 1);

    // Now worker is REUSED for Run 2 on same thread!
    let run2_id = RunId::new();
    let task2_id = TaskId::new();
    let tracker2 = Arc::new(AgentBudgetTracker::new_with_scope(
        BudgetScopeId::Run(run2_id),
        agent_id,
        AgentExecutionBudget::default().with_tool_calls(2),
    ));

    context.set_task_and_run(Some(task2_id), Some(run2_id), Arc::clone(&tracker2));

    // Call under Run 2 uses tracker2 fresh budget
    let start_2 = ToolStartInput {
        session_store: &session_store,
        thread_store: &thread_store,
        turn_store: &turn_store,
        turn_id: "turn_2",
        root_turn_id: None,
        call_id: "call_r2_1",
        originating_item_id: None,
        tool_name: &tool_name,
        mcp_tool: None,
        permissions: Box::pin(std::future::ready(None)),
        payload: &payload,
        conversation_history: Arc::clone(&history),
        source: ToolCallSource::Direct,
    };
    assert!(contributor.authorize_tool_call(&start_2).is_ok());
    assert_eq!(tracker2.tool_calls_reserved(), 1);
    assert_eq!(tracker1.tool_calls_used(), 1); // unchanged
}
