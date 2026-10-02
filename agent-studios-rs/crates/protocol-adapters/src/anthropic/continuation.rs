use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Provider-native thinking block preserved for exact continuation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NativeThinkingBlock {
    Thinking { thinking: String, signature: String },
    RedactedThinking { data: String },
}

/// Opaque continuation state holding Anthropic-native thinking and signature blocks
/// required for subsequent model turn requests.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnthropicContinuationState {
    /// Native thinking blocks keyed by deterministic item ID (e.g. `anthropic-reasoning-{message_id}-{block_index}`).
    pub reasoning_blocks: HashMap<String, NativeThinkingBlock>,
    /// Optional response-level metadata passthrough.
    pub message_id: Option<String>,
}

impl AnthropicContinuationState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Generates a deterministic item ID for an Anthropic reasoning block.
    pub fn deterministic_reasoning_id(message_id: &str, block_index: u64) -> String {
        format!("anthropic-reasoning-{message_id}-{block_index}")
    }

    /// Stores a native thinking block.
    pub fn insert_reasoning_block(
        &mut self,
        item_id: impl Into<String>,
        block: NativeThinkingBlock,
    ) {
        self.reasoning_blocks.insert(item_id.into(), block);
    }

    /// Retrieves a native thinking block by item ID.
    pub fn get_reasoning_block(&self, item_id: &str) -> Option<&NativeThinkingBlock> {
        self.reasoning_blocks.get(item_id)
    }

    /// True if there are any recorded reasoning continuation blocks.
    pub fn is_empty(&self) -> bool {
        self.reasoning_blocks.is_empty()
    }
}
