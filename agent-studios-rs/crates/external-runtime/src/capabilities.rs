use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::RuntimeError;

/// Enumeration of distinct feature capabilities supported by an external agent runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeCapability {
    StreamingEvents,
    Interrupt,
    Resume,
    Stop,
    PersistentSession,
    WorkspaceBinding,
    ModelSelection,
    ProviderSelection,
    Tools,
    Approvals,
    Mcp,
    Skills,
    ImageInput,
    StructuredOutput,
    Subagents,
    UsageReporting,
}

impl fmt::Display for RuntimeCapability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StreamingEvents => write!(f, "streaming_events"),
            Self::Interrupt => write!(f, "interrupt"),
            Self::Resume => write!(f, "resume"),
            Self::Stop => write!(f, "stop"),
            Self::PersistentSession => write!(f, "persistent_session"),
            Self::WorkspaceBinding => write!(f, "workspace_binding"),
            Self::ModelSelection => write!(f, "model_selection"),
            Self::ProviderSelection => write!(f, "provider_selection"),
            Self::Tools => write!(f, "tools"),
            Self::Approvals => write!(f, "approvals"),
            Self::Mcp => write!(f, "mcp"),
            Self::Skills => write!(f, "skills"),
            Self::ImageInput => write!(f, "image_input"),
            Self::StructuredOutput => write!(f, "structured_output"),
            Self::Subagents => write!(f, "subagents"),
            Self::UsageReporting => write!(f, "usage_reporting"),
        }
    }
}

/// Tristate capability support indicating whether a runtime supports a feature.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeCapabilitySupport {
    #[default]
    Unknown,
    Unsupported,
    Supported,
}

impl RuntimeCapabilitySupport {
    pub const fn is_supported(&self) -> bool {
        matches!(self, Self::Supported)
    }

    pub const fn is_unsupported(&self) -> bool {
        matches!(self, Self::Unsupported)
    }

    pub const fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown)
    }
}

impl From<bool> for RuntimeCapabilitySupport {
    fn from(supported: bool) -> Self {
        if supported {
            Self::Supported
        } else {
            Self::Unsupported
        }
    }
}

impl fmt::Display for RuntimeCapabilitySupport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown => write!(f, "unknown"),
            Self::Unsupported => write!(f, "unsupported"),
            Self::Supported => write!(f, "supported"),
        }
    }
}

/// Typed capability profile declaring the features supported by a runtime implementation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RuntimeCapabilities {
    pub streaming_events: RuntimeCapabilitySupport,
    pub interrupt: RuntimeCapabilitySupport,
    pub resume: RuntimeCapabilitySupport,
    pub stop: RuntimeCapabilitySupport,
    pub persistent_session: RuntimeCapabilitySupport,
    pub workspace_binding: RuntimeCapabilitySupport,
    pub model_selection: RuntimeCapabilitySupport,
    pub provider_selection: RuntimeCapabilitySupport,
    pub tools: RuntimeCapabilitySupport,
    pub approvals: RuntimeCapabilitySupport,
    pub mcp: RuntimeCapabilitySupport,
    pub skills: RuntimeCapabilitySupport,
    pub image_input: RuntimeCapabilitySupport,
    pub structured_output: RuntimeCapabilitySupport,
    pub subagents: RuntimeCapabilitySupport,
    pub usage_reporting: RuntimeCapabilitySupport,
}

impl RuntimeCapabilities {
    /// Returns a capabilities descriptor with all capabilities marked as Unknown.
    pub fn unknown() -> Self {
        Self::default()
    }

    /// Queries the support level of a specific capability.
    pub fn check_support(&self, cap: RuntimeCapability) -> RuntimeCapabilitySupport {
        match cap {
            RuntimeCapability::StreamingEvents => self.streaming_events,
            RuntimeCapability::Interrupt => self.interrupt,
            RuntimeCapability::Resume => self.resume,
            RuntimeCapability::Stop => self.stop,
            RuntimeCapability::PersistentSession => self.persistent_session,
            RuntimeCapability::WorkspaceBinding => self.workspace_binding,
            RuntimeCapability::ModelSelection => self.model_selection,
            RuntimeCapability::ProviderSelection => self.provider_selection,
            RuntimeCapability::Tools => self.tools,
            RuntimeCapability::Approvals => self.approvals,
            RuntimeCapability::Mcp => self.mcp,
            RuntimeCapability::Skills => self.skills,
            RuntimeCapability::ImageInput => self.image_input,
            RuntimeCapability::StructuredOutput => self.structured_output,
            RuntimeCapability::Subagents => self.subagents,
            RuntimeCapability::UsageReporting => self.usage_reporting,
        }
    }

    /// Returns true if the specified capability is explicitly confirmed as supported.
    pub fn supports(&self, cap: RuntimeCapability) -> bool {
        self.check_support(cap).is_supported()
    }

    /// Validates that a required capability is supported, returning a typed `RuntimeError::UnsupportedCapability` if not.
    pub fn ensure_supported(&self, cap: RuntimeCapability) -> Result<(), RuntimeError> {
        let support = self.check_support(cap);
        if support.is_supported() {
            Ok(())
        } else {
            Err(RuntimeError::UnsupportedCapability {
                capability: cap,
                reason: format!("Capability '{}' is not supported (state: {})", cap, support),
            })
        }
    }

    pub fn with_streaming_events(mut self, support: impl Into<RuntimeCapabilitySupport>) -> Self {
        self.streaming_events = support.into();
        self
    }

    pub fn with_interrupt(mut self, support: impl Into<RuntimeCapabilitySupport>) -> Self {
        self.interrupt = support.into();
        self
    }

    pub fn with_resume(mut self, support: impl Into<RuntimeCapabilitySupport>) -> Self {
        self.resume = support.into();
        self
    }

    pub fn with_stop(mut self, support: impl Into<RuntimeCapabilitySupport>) -> Self {
        self.stop = support.into();
        self
    }

    pub fn with_persistent_session(mut self, support: impl Into<RuntimeCapabilitySupport>) -> Self {
        self.persistent_session = support.into();
        self
    }

    pub fn with_workspace_binding(mut self, support: impl Into<RuntimeCapabilitySupport>) -> Self {
        self.workspace_binding = support.into();
        self
    }

    pub fn with_model_selection(mut self, support: impl Into<RuntimeCapabilitySupport>) -> Self {
        self.model_selection = support.into();
        self
    }

    pub fn with_provider_selection(mut self, support: impl Into<RuntimeCapabilitySupport>) -> Self {
        self.provider_selection = support.into();
        self
    }

    pub fn with_tools(mut self, support: impl Into<RuntimeCapabilitySupport>) -> Self {
        self.tools = support.into();
        self
    }

    pub fn with_approvals(mut self, support: impl Into<RuntimeCapabilitySupport>) -> Self {
        self.approvals = support.into();
        self
    }

    pub fn with_mcp(mut self, support: impl Into<RuntimeCapabilitySupport>) -> Self {
        self.mcp = support.into();
        self
    }

    pub fn with_skills(mut self, support: impl Into<RuntimeCapabilitySupport>) -> Self {
        self.skills = support.into();
        self
    }

    pub fn with_image_input(mut self, support: impl Into<RuntimeCapabilitySupport>) -> Self {
        self.image_input = support.into();
        self
    }

    pub fn with_structured_output(mut self, support: impl Into<RuntimeCapabilitySupport>) -> Self {
        self.structured_output = support.into();
        self
    }

    pub fn with_subagents(mut self, support: impl Into<RuntimeCapabilitySupport>) -> Self {
        self.subagents = support.into();
        self
    }

    pub fn with_usage_reporting(mut self, support: impl Into<RuntimeCapabilitySupport>) -> Self {
        self.usage_reporting = support.into();
        self
    }
}
