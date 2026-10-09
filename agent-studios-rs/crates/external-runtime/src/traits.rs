use async_trait::async_trait;
use tokio::sync::broadcast;

use crate::capabilities::RuntimeCapabilities;
use crate::discovery::DiscoveredRuntime;
use crate::error::RuntimeError;
use crate::event::RuntimeEvent;
use crate::handle::{RuntimeSessionHandle, RuntimeStartRequest};
use crate::id::{RuntimeImplementationId, RuntimeSessionId};
use crate::input::RuntimeInput;
use crate::lifecycle::RuntimeLifecycleState;

/// Vendor-neutral contract that external agent runtimes must implement to integrate with Agent Studios.
#[async_trait]
pub trait AgentRuntime: Send + Sync {
    /// Returns the unique implementation identifier for this runtime.
    fn implementation_id(&self) -> &RuntimeImplementationId;

    /// Discovers runtime availability, local binary path, and metadata in the environment.
    async fn discover(&self) -> Result<DiscoveredRuntime, RuntimeError>;

    /// Advertises the static or dynamic capability profile supported by this runtime.
    async fn capabilities(&self) -> RuntimeCapabilities;

    /// Starts a new external runtime process or session according to the start request.
    async fn start(
        &self,
        request: RuntimeStartRequest,
    ) -> Result<RuntimeSessionHandle, RuntimeError>;

    /// Sends typed input (user text, continuation, or approval response) to an active session.
    async fn send(
        &self,
        session_id: &RuntimeSessionId,
        input: RuntimeInput,
    ) -> Result<(), RuntimeError>;

    /// Interrupts the active execution turn while keeping the session process and context intact.
    async fn interrupt(&self, session_id: &RuntimeSessionId) -> Result<(), RuntimeError>;

    /// Resumes execution after interruption if the runtime advertises `RuntimeCapability::Resume`.
    async fn resume(
        &self,
        session_id: &RuntimeSessionId,
        input: Option<RuntimeInput>,
    ) -> Result<(), RuntimeError>;

    /// Terminates the runtime session permanently.
    async fn stop(&self, session_id: &RuntimeSessionId) -> Result<(), RuntimeError>;

    /// Queries the current lifecycle state of a session.
    async fn status(
        &self,
        session_id: &RuntimeSessionId,
    ) -> Result<RuntimeLifecycleState, RuntimeError>;

    /// Subscribes to the broadcast stream of normalized events emitted by the session.
    async fn events(
        &self,
        session_id: &RuntimeSessionId,
    ) -> Result<broadcast::Receiver<RuntimeEvent>, RuntimeError>;
}
