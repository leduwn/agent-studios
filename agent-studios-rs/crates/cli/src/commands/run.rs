//! Headless & Direct Goal Execution Command (`ags run ...`)
//!
//! Supports `--headless` NDJSON streaming, `--stdin` piped prompts,
//! deterministic exit codes (0=succeeded, 1=failed/timeout, 2=approval blocked, 3=config/IPC error),
//! and secret scrubbing on emitted telemetry.

use std::io::Read;
use std::sync::Arc;
use std::time::Duration;

use clap::{Args, ValueEnum};
use tokio::time::{Instant, timeout};

use agent_studios_cli_client::client::{
    ClientApprovalDecision, ClientError, DynAgentStudiosClient, GoalSubmissionOptions, RuntimeMode,
};
use agent_studios_cli_client::connected::ConnectedClient;
use agent_studios_cli_client::embedded::EmbeddedClient;
use agent_studios_ipc_protocol::SecretRedactor;
use agent_studios_ipc_transport::daemon::DaemonBootstrapInfo;
use agent_studios_protocol::event::ControlPlaneEvent;
use agent_studios_protocol::task::TaskState;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum AutoApproveTier {
    None,
    Low,
    Medium,
    High,
    All,
}

#[derive(Args, Debug)]
pub struct RunArgs {
    /// Natural language engineering goal to execute
    #[arg(value_name = "GOAL")]
    pub goal: Option<String>,

    /// Read goal prompt from standard input (STDIN)
    #[arg(long)]
    pub stdin: bool,

    /// Run in non-interactive headless automation mode (streams NDJSON to stdout)
    #[arg(long)]
    pub headless: bool,

    /// Specify model target (e.g. "<instance_id>/<model_id>" or "<provider>/<model>")
    #[arg(short, long)]
    pub model: Option<String>,

    /// Specify provider family (e.g. "openai", "anthropic", "ollama")
    #[arg(long)]
    pub provider: Option<String>,

    /// Specify preferred runtime implementation ("claude-code", "opencode", "deterministic-local")
    #[arg(long)]
    pub runtime: Option<String>,

    /// Automatically approve actions up to the requested risk tier
    #[arg(long, default_value = "none", value_enum)]
    pub auto_approve: AutoApproveTier,

    /// Hard execution timeout in seconds
    #[arg(long, default_value_t = 1800)]
    pub timeout: u64,

    /// Execute using deterministic local demo mode ($0.00 spend, no remote API keys required)
    #[arg(long)]
    pub demo: bool,

    /// Workspace name for studio isolation
    #[arg(long, default_value = "agent-studios-workspace")]
    pub workspace: String,
}

pub fn resolve_goal_prompt(args: &RunArgs) -> Result<String, String> {
    if args.stdin {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| format!("Failed to read prompt from STDIN: {e}"))?;
        let trimmed = buf.trim().to_string();
        if trimmed.is_empty() {
            return Err("STDIN prompt was empty".to_string());
        }
        return Ok(trimmed);
    }

    if let Some(ref g) = args.goal {
        let trimmed = g.trim().to_string();
        if !trimmed.is_empty() {
            return Ok(trimmed);
        }
    }

    Err("Missing goal prompt: pass \"<GOAL>\" or --stdin".to_string())
}

fn auto_approve_allows_command(tier: AutoApproveTier) -> bool {
    matches!(
        tier,
        AutoApproveTier::Medium | AutoApproveTier::High | AutoApproveTier::All
    )
}

fn scrub_json_line(raw: &str) -> String {
    SecretRedactor::new().redact_text(raw)
}

async fn resolve_client_for_run(
    args: &RunArgs,
    runtime_mode: RuntimeMode,
) -> Result<DynAgentStudiosClient, ClientError> {
    // If an explicit local/demo mode or explicit runtime preference is requested,
    // use EmbeddedClient with that preference unless daemon is explicitly online and no override is forced.
    if runtime_mode == RuntimeMode::Demo || args.runtime.is_some() {
        let embedded = EmbeddedClient::bootstrap_with_preference(
            &args.workspace,
            runtime_mode,
            args.runtime.as_deref(),
        )
        .await?;
        return Ok(Arc::new(embedded));
    }

    // Production default: try live agsd daemon over IPC first, fallback gracefully to EmbeddedClient.
    let bootstrap_path = DaemonBootstrapInfo::default_runtime_path();
    if bootstrap_path.exists()
        && let Ok(connected) =
            ConnectedClient::connect_from_bootstrap(&bootstrap_path, &args.workspace).await
    {
        return Ok(Arc::new(connected));
    }

    let embedded = EmbeddedClient::bootstrap_with_mode(&args.workspace, runtime_mode).await?;
    Ok(Arc::new(embedded))
}

/// Executes `ags run` and returns the deterministic contract exit code:
/// - `0`: Goal completed (`Succeeded`)
/// - `1`: Task execution failed, cancelled, or timed out
/// - `2`: Blocked on unattended approval without sufficient `--auto-approve`
/// - `3`: Configuration, input, or IPC/daemon connection failure
pub async fn execute_run_command(args: RunArgs, global_demo: bool) -> i32 {
    let prompt = match resolve_goal_prompt(&args) {
        Ok(p) => p,
        Err(msg) => {
            eprintln!("Error: {msg}");
            return 3;
        }
    };

    let runtime_mode = if args.demo || global_demo {
        RuntimeMode::Demo
    } else {
        RuntimeMode::Production
    };

    let client = match resolve_client_for_run(&args, runtime_mode).await {
        Ok(c) => c,
        Err(err) => {
            eprintln!("Error initializing Agent Studios runtime/client: {err}");
            return 3;
        }
    };

    execute_run_with_client(client, &prompt, &args).await
}

/// Executes a run against a provided `DynAgentStudiosClient` (used directly by CLI & integration tests).
pub async fn execute_run_with_client(
    client: DynAgentStudiosClient,
    prompt: &str,
    args: &RunArgs,
) -> i32 {
    let mut sub = match client.subscribe_events(0).await {
        Ok(s) => s,
        Err(err) => {
            eprintln!("Error subscribing to control plane event stream: {err}");
            return 3;
        }
    };

    let options = GoalSubmissionOptions {
        provider: args.provider.clone(),
        model: args.model.clone(),
        runtime: args.runtime.clone(),
        auto_approve_policy: Some(format!("{:?}", args.auto_approve).to_lowercase()),
        workspace_access_mode: None,
    };

    let task_id = match client.submit_goal(prompt, options).await {
        Ok(tid) => tid,
        Err(err) => {
            eprintln!("Error submitting goal to control plane: {err}");
            return 3;
        }
    };

    let deadline = Instant::now() + Duration::from_secs(args.timeout.max(1));

    loop {
        let now = Instant::now();
        if now >= deadline {
            eprintln!("Execution timed out after {}s", args.timeout);
            let _ = client
                .cancel_active_run(Some("Execution timeout reached".to_string()))
                .await;
            return 1;
        }

        let remaining = deadline.saturating_duration_since(now);
        let wait_slice = remaining.min(Duration::from_millis(250));

        match timeout(wait_slice, sub.stream.recv()).await {
            Ok(Some(envelope)) => {
                if args.headless {
                    if let Ok(json) = serde_json::to_string(&envelope) {
                        println!("{}", scrub_json_line(&json));
                    }
                } else {
                    print_human_event(&envelope.event);
                }

                match &envelope.event {
                    ControlPlaneEvent::ApprovalRequested { approval }
                        if approval.task_id == task_id =>
                    {
                        if auto_approve_allows_command(args.auto_approve) {
                            if let Err(err) = client
                                .resolve_approval(approval.id, ClientApprovalDecision::Allow)
                                .await
                            {
                                eprintln!("Failed to auto-approve {}: {err}", approval.id);
                                return 1;
                            }
                        } else {
                            eprintln!(
                                "Approval blocked in non-interactive run ({}): {}",
                                approval.id,
                                scrub_json_line(&approval.summary)
                            );
                            let _ = client
                                .resolve_approval(approval.id, ClientApprovalDecision::Deny)
                                .await;
                            let _ = client
                                .cancel_active_run(Some(
                                    "Unattended run blocked on approval gate".to_string(),
                                ))
                                .await;
                            return 2;
                        }
                    }
                    ControlPlaneEvent::TaskStateChanged {
                        task_id: tid,
                        new_state,
                        ..
                    } if *tid == task_id && new_state.is_terminal() => {
                        return match new_state {
                            TaskState::Succeeded => 0,
                            TaskState::Failed | TaskState::Cancelled => 1,
                            _ => 1,
                        };
                    }
                    _ => {}
                }
            }
            Ok(None) => {
                eprintln!("Control plane event stream closed unexpectedly before task completion");
                return 3;
            }
            Err(_) => {
                // Poll snapshot as a safety guard in case a terminal event was missed
                if let Ok(graph) = client.get_task_graph().await
                    && let Some(task) = graph.tasks.iter().find(|t| t.id == task_id)
                    && task.state.is_terminal()
                {
                    return if task.state == TaskState::Succeeded {
                        0
                    } else {
                        1
                    };
                }
            }
        }
    }
}

fn print_human_event(event: &ControlPlaneEvent) {
    match event {
        ControlPlaneEvent::AssistantMessageChunk { content, .. } => {
            print!("{}", scrub_json_line(content));
        }
        ControlPlaneEvent::ToolStarted { tool_name, .. } => {
            eprintln!("[tool:started] {tool_name}");
        }
        ControlPlaneEvent::ToolCompleted {
            tool_name,
            duration_ms,
            ..
        } => {
            eprintln!("[tool:completed] {tool_name} ({duration_ms}ms)");
        }
        ControlPlaneEvent::TaskStateChanged {
            task_id, new_state, ..
        } => {
            eprintln!("[task:{task_id}] -> {new_state:?}");
        }
        _ => {}
    }
}
