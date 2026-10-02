use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::types::GeminiContent;

/// Metadata stored for an observed Gemini tool call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeminiToolCallMetadata {
    pub name: String,
    pub provider_call_id: Option<String>,
}

/// Sidecar continuation state tracking native Gemini model turns, thought signatures, and tool associations.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GeminiContinuationState {
    /// Ordered native model turns keyed by response ID.
    response_turns: HashMap<String, GeminiContent>,
    /// Association of internal Codex call ID -> provider tool metadata.
    tool_calls: HashMap<String, GeminiToolCallMetadata>,
    /// Thought signatures keyed by item ID or call ID.
    signatures: HashMap<String, String>,
}

impl GeminiContinuationState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stores a complete native model Content turn for a response ID.
    pub fn insert_model_turn(&mut self, response_id: impl Into<String>, turn: GeminiContent) {
        self.response_turns.insert(response_id.into(), turn);
    }

    /// Retrieves a native model Content turn by response ID.
    pub fn get_model_turn(&self, response_id: &str) -> Option<&GeminiContent> {
        self.response_turns.get(response_id)
    }

    /// Records a tool call association.
    pub fn record_tool_call(
        &mut self,
        internal_call_id: impl Into<String>,
        name: impl Into<String>,
        provider_call_id: Option<String>,
    ) {
        self.tool_calls.insert(
            internal_call_id.into(),
            GeminiToolCallMetadata {
                name: name.into(),
                provider_call_id,
            },
        );
    }

    /// Looks up metadata for a tool call by its internal or provider call ID.
    pub fn get_tool_call(&self, call_id: &str) -> Option<&GeminiToolCallMetadata> {
        self.tool_calls.get(call_id)
    }

    /// Records an opaque thought signature.
    pub fn record_signature(&mut self, key: impl Into<String>, signature: impl Into<String>) {
        self.signatures.insert(key.into(), signature.into());
    }

    /// Retrieves a recorded thought signature.
    pub fn get_signature(&self, key: &str) -> Option<&str> {
        self.signatures.get(key).map(|s| s.as_str())
    }

    /// Checks if a response ID exists in the continuation state.
    pub fn has_response(&self, response_id: &str) -> bool {
        self.response_turns.contains_key(response_id)
    }

    /// Creates a deterministic internal call ID for a Gemini tool call when provider omitted ID.
    pub fn generate_internal_call_id(
        response_id: &str,
        candidate_index: i32,
        part_index: usize,
    ) -> String {
        format!("gemini-call-{response_id}-{candidate_index}-{part_index}")
    }
}
