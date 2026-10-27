pub mod approval;
pub mod discovery;
pub mod error;
pub mod mapping;
pub mod process;
pub mod protocol;
pub mod runtime;
pub mod sandbox;
pub mod stream;
pub mod subagent;

pub use approval::{
    ApprovalAuthority, ApprovalDecision, PendingApproval, parse_max_budget_usd,
    sanitize_diagnostic, validate_max_budget_usd,
};
pub use discovery::{
    CLAUDE_CODE_IMPLEMENTATION_ID, CLAUDE_CODE_RUNTIME_ID, ClaudeCodeDiscoveryConfig,
    DEFAULT_INSTANCE_ID, default_claude_capabilities, discover_claude_binary, resolve_binary_path,
};
pub use error::ClaudeCodeError;
pub use mapping::map_claude_message;
pub use process::{ClaudeCodeProcess, ClaudeCodeProcessConfig, spawn_claude_process};
pub use protocol::{
    AssistantMessage, CanUseToolRequest, ClaudeInterruptInput, ClaudeStreamMessage,
    ClaudeTokenUsage, ClaudeUserPromptInput, ContentBlock, ControlDecision, ControlRequest,
    ControlRequestPayload, ControlResponse, ControlResponseBody, ResultSummary, StreamDelta,
    StreamEventPayload, StreamEventWrapper, SystemInitMessage, SystemMessage,
    TaskNotificationMessage, TaskStartedMessage, ToolProgressMessage, ToolResultBlock, UserMessage,
};
pub use runtime::{
    ClaudeCodeActiveSession, ClaudeCodeRuntime, DEFAULT_APPROVAL_TIMEOUT, classify_exit_code,
    classify_exit_status,
};
pub use sandbox::{
    JobObjectGuard, ProcessContainment, SandboxLevel, SandboxPolicy, validate_session_workspace,
    validate_workspace_path,
};
pub use stream::{MAX_BUFFER_CAPACITY, MAX_LINE_LENGTH, NdjsonStreamReader, parse_ndjson_line};
pub use subagent::{SubagentCorrelationTracker, SubagentNode};
