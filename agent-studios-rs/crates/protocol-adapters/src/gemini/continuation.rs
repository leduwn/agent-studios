use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::types::GeminiContent;

/// Reference to the origin Gemini response and part that generated a Codex item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeminiOriginRef {
    pub response_id: String,
    pub candidate_index: u32,
    pub part_index: u32,
}

/// Metadata stored for an observed Gemini tool call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeminiToolCallMetadata {
    pub name: String,
    pub provider_call_id: Option<String>,
    pub response_id: String,
    pub candidate_index: u32,
    pub part_index: u32,
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
    /// Association of Codex item ID -> origin reference.
    item_origins: HashMap<String, GeminiOriginRef>,
    /// Association of Codex function call call_id -> origin reference.
    call_origins: HashMap<String, GeminiOriginRef>,
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
        response_id: impl Into<String>,
        candidate_index: u32,
        part_index: u32,
    ) {
        let call_id = internal_call_id.into();
        let resp_id = response_id.into();
        self.tool_calls.insert(
            call_id.clone(),
            GeminiToolCallMetadata {
                name: name.into(),
                provider_call_id,
                response_id: resp_id.clone(),
                candidate_index,
                part_index,
            },
        );
        self.call_origins.insert(
            call_id,
            GeminiOriginRef {
                response_id: resp_id,
                candidate_index,
                part_index,
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

    /// Records reverse origin association for an emitted Codex item ID.
    pub fn record_item_origin(&mut self, item_id: impl Into<String>, origin: GeminiOriginRef) {
        self.item_origins.insert(item_id.into(), origin);
    }

    /// Retrieves origin reference for a Codex item ID.
    pub fn get_item_origin(&self, item_id: &str) -> Option<&GeminiOriginRef> {
        self.item_origins.get(item_id)
    }

    /// Records reverse origin association for a function call call_id.
    pub fn record_call_origin(&mut self, call_id: impl Into<String>, origin: GeminiOriginRef) {
        self.call_origins.insert(call_id.into(), origin);
    }

    /// Retrieves origin reference for a function call call_id.
    pub fn get_call_origin(&self, call_id: &str) -> Option<&GeminiOriginRef> {
        self.call_origins.get(call_id)
    }

    /// Checks if a response ID exists in the continuation state.
    pub fn has_response(&self, response_id: &str) -> bool {
        self.response_turns.contains_key(response_id)
    }

    /// Creates a deterministic internal call ID for a Gemini tool call based on response/candidate/part.
    pub fn generate_internal_call_id(
        response_id: &str,
        candidate_index: u32,
        part_index: u32,
    ) -> String {
        format!("gemini-call-{response_id}-{candidate_index}-{part_index}")
    }
}
