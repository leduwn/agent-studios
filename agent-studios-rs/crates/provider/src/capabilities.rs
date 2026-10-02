use std::fmt;

use serde::{Deserialize, Serialize};

/// Tristate capability support indicating whether a model or provider supports a feature.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum CapabilitySupport {
    /// Capability status has not been probed or declared.
    #[default]
    Unknown,
    /// Feature is explicitly known to be unsupported.
    Unsupported,
    /// Feature is confirmed supported.
    Supported,
}

impl CapabilitySupport {
    pub fn is_supported(&self) -> bool {
        matches!(self, Self::Supported)
    }

    pub fn is_unsupported(&self) -> bool {
        matches!(self, Self::Unsupported)
    }

    pub fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown)
    }
}

impl fmt::Display for CapabilitySupport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown => write!(f, "unknown"),
            Self::Unsupported => write!(f, "unsupported"),
            Self::Supported => write!(f, "supported"),
        }
    }
}

/// Feature capabilities supported by a model.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ModelCapabilities {
    pub tool_calling: CapabilitySupport,
    pub parallel_tool_calls: CapabilitySupport,
    pub vision_input: CapabilitySupport,
    pub reasoning: CapabilitySupport,
    pub streaming: CapabilitySupport,
    pub structured_output: CapabilitySupport,
    pub prompt_caching: CapabilitySupport,
    pub web_search: CapabilitySupport,
    pub audio_input: CapabilitySupport,
    pub audio_output: CapabilitySupport,
}

impl ModelCapabilities {
    /// Returns a capabilities profile with all features marked as Unknown.
    pub fn unknown() -> Self {
        Self::default()
    }

    pub fn with_tool_calling(mut self, support: CapabilitySupport) -> Self {
        self.tool_calling = support;
        self
    }

    pub fn with_parallel_tool_calls(mut self, support: CapabilitySupport) -> Self {
        self.parallel_tool_calls = support;
        self
    }

    pub fn with_vision_input(mut self, support: CapabilitySupport) -> Self {
        self.vision_input = support;
        self
    }

    pub fn with_reasoning(mut self, support: CapabilitySupport) -> Self {
        self.reasoning = support;
        self
    }

    pub fn with_streaming(mut self, support: CapabilitySupport) -> Self {
        self.streaming = support;
        self
    }

    pub fn with_structured_output(mut self, support: CapabilitySupport) -> Self {
        self.structured_output = support;
        self
    }

    pub fn with_prompt_caching(mut self, support: CapabilitySupport) -> Self {
        self.prompt_caching = support;
        self
    }

    pub fn with_web_search(mut self, support: CapabilitySupport) -> Self {
        self.web_search = support;
        self
    }

    pub fn with_audio_input(mut self, support: CapabilitySupport) -> Self {
        self.audio_input = support;
        self
    }

    pub fn with_audio_output(mut self, support: CapabilitySupport) -> Self {
        self.audio_output = support;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_support_semantics() {
        let unk = CapabilitySupport::Unknown;
        assert!(unk.is_unknown());
        assert!(!unk.is_supported());
        assert!(!unk.is_unsupported());

        let supp = CapabilitySupport::Supported;
        assert!(supp.is_supported());
        assert!(!supp.is_unknown());

        let unsupp = CapabilitySupport::Unsupported;
        assert!(unsupp.is_unsupported());
        assert!(!unsupp.is_supported());
    }

    #[test]
    fn test_capabilities_builder_and_serde() {
        let caps = ModelCapabilities::unknown()
            .with_tool_calling(CapabilitySupport::Supported)
            .with_reasoning(CapabilitySupport::Unsupported);

        assert_eq!(caps.tool_calling, CapabilitySupport::Supported);
        assert_eq!(caps.reasoning, CapabilitySupport::Unsupported);
        assert_eq!(caps.vision_input, CapabilitySupport::Unknown);

        let json = serde_json::to_string(&caps).unwrap();
        let de: ModelCapabilities = serde_json::from_str(&json).unwrap();
        assert_eq!(caps, de);
    }
}
