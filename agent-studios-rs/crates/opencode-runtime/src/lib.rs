//! OpenCode CLI and Server runtime adapter for Agent Studios.
//! Implements vendor-neutral `AgentRuntime` lifecycle, Windows Job Object containment,
//! SSE event translation, and Control Plane worktree enforcement.

pub mod client;
pub mod control_plane;
pub mod discovery;
pub mod error;
pub mod provider_mapping;
pub mod runtime;
pub mod sandbox;
pub mod server;
pub mod sse;

pub use client::{
    CreateSessionRequest, OpenCodeHttpClient, OpenCodeMessageRequest, OpenCodePromptResponse,
    OpenCodeSessionResponse, OpenCodeStatusResponse,
};
pub use control_plane::OpenCodeSessionContext;
pub use discovery::{DEFAULT_INSTANCE_ID, OPENCODE_IMPLEMENTATION_ID, OpenCodeDiscoveryConfig};
pub use error::OpenCodeError;
pub use provider_mapping::{OpenCodeModelBinding, map_model_ref_to_opencode};
pub use runtime::{OpenCodeActiveSession, OpenCodeRuntime};
pub use sandbox::{JobObjectGuard, ProcessContainment, SandboxLevel, SandboxPolicy};
pub use server::{OpenCodeServerConfig, OpenCodeServerHandle, verify_loopback_url};
pub use sse::{SseEvent, SseParser, run_sse_ingestion_loop, translate_opencode_event};
