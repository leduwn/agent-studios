use std::collections::BTreeMap;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::protocol::{ClaudeTaskUsage, TaskNotificationMessage, TaskStartedMessage};

/// In-memory representation of an active or completed Claude Code subagent node.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SubagentNode {
    pub task_id: String,
    pub tool_use_id: String,
    pub parent_tool_use_id: Option<String>,
    pub description: String,
    pub task_type: String,
    pub spawn_depth: u32,
    pub is_backgrounded: bool,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub status: Option<String>,
    pub summary: Option<String>,
    pub output_file: Option<String>,
    pub usage: Option<ClaudeTaskUsage>,
    pub success: bool,
}

impl SubagentNode {
    pub fn is_active(&self) -> bool {
        self.completed_at.is_none()
    }
}

/// Correlation tracker maintaining the subagent hierarchy DAG.
/// Strictly maps native `task_id` and `tool_use_id` from Claude Code without inventing synthetic IDs.
#[derive(Clone, Debug, Default)]
pub struct SubagentCorrelationTracker {
    /// Maps tool_use_id -> SubagentNode
    nodes_by_tool_call: BTreeMap<String, SubagentNode>,
    /// Maps task_id -> tool_use_id
    tool_calls_by_task_id: BTreeMap<String, String>,
    /// Known tool names indexed by tool_use_id
    tool_names_by_call_id: BTreeMap<String, String>,
}

impl SubagentCorrelationTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a tool invocation call ID to its tool name.
    pub fn register_tool_use(&mut self, tool_use_id: impl Into<String>, tool_name: impl Into<String>) {
        let id = tool_use_id.into();
        let name = tool_name.into();
        self.tool_names_by_call_id.insert(id, name);
    }

    /// Retrieves the tool name associated with a tool call ID.
    pub fn get_tool_name(&self, tool_use_id: &str) -> Option<&str> {
        self.tool_names_by_call_id.get(tool_use_id).map(|s| s.as_str())
    }

    /// Registers a newly spawned subagent from a `system/task_started` NDJSON event.
    pub fn register_task_started(&mut self, event: &TaskStartedMessage) -> &SubagentNode {
        let node = SubagentNode {
            task_id: event.task_id.clone(),
            tool_use_id: event.tool_use_id.clone(),
            parent_tool_use_id: None, // Resolved or updated if parent link is provided
            description: event.description.clone(),
            task_type: event.task_type.clone().unwrap_or_else(|| "subagent".to_string()),
            spawn_depth: event.spawn_depth.unwrap_or(1),
            is_backgrounded: event.is_backgrounded.unwrap_or(false),
            started_at: Utc::now(),
            completed_at: None,
            status: Some("running".to_string()),
            summary: None,
            output_file: None,
            usage: None,
            success: false,
        };

        debug!(
            task_id = %node.task_id,
            tool_use_id = %node.tool_use_id,
            spawn_depth = node.spawn_depth,
            "Registered subagent task started"
        );

        self.tool_calls_by_task_id
            .insert(event.task_id.clone(), event.tool_use_id.clone());
        self.nodes_by_tool_call.insert(event.tool_use_id.clone(), node);

        self.nodes_by_tool_call
            .get(&event.tool_use_id)
            .expect("node just inserted")
    }

    /// Associates an existing subagent node with its parent tool call ID.
    pub fn set_parent_tool_use_id(&mut self, tool_use_id: &str, parent_tool_use_id: impl Into<String>) {
        if let Some(node) = self.nodes_by_tool_call.get_mut(tool_use_id) {
            node.parent_tool_use_id = Some(parent_tool_use_id.into());
        }
    }

    /// Completes a subagent lifecycle from a `system/task_notification` NDJSON event.
    pub fn register_task_notification(
        &mut self,
        event: &TaskNotificationMessage,
    ) -> Option<&SubagentNode> {
        let tool_use_id = self.tool_calls_by_task_id.get(&event.task_id)?.clone();
        let node = self.nodes_by_tool_call.get_mut(&tool_use_id)?;

        node.completed_at = Some(Utc::now());
        node.status = Some(event.status.clone());
        node.summary = event.summary.clone();
        node.output_file = event.output_file.clone();
        node.usage = event.usage.clone();
        node.success = event.status == "completed";

        debug!(
            task_id = %node.task_id,
            tool_use_id = %node.tool_use_id,
            status = %event.status,
            success = node.success,
            "Completed subagent task notification"
        );

        self.nodes_by_tool_call.get(&tool_use_id)
    }

    /// Resolves a subagent node by its spawning `tool_use_id`.
    pub fn resolve_by_tool_call(&self, tool_use_id: &str) -> Option<&SubagentNode> {
        self.nodes_by_tool_call.get(tool_use_id)
    }

    /// Resolves a subagent node by its native `task_id`.
    pub fn resolve_by_task_id(&self, task_id: &str) -> Option<&SubagentNode> {
        let tool_use_id = self.tool_calls_by_task_id.get(task_id)?;
        self.nodes_by_tool_call.get(tool_use_id)
    }

    /// Resolves parent subagent for an incoming message with `parent_tool_use_id`.
    pub fn resolve_parent(&self, parent_tool_use_id: &str) -> Option<&SubagentNode> {
        self.nodes_by_tool_call.get(parent_tool_use_id)
    }

    /// Traces the full parent ancestry chain from child subagent up to the root.
    pub fn resolve_ancestry(&self, tool_use_id: &str) -> Vec<&SubagentNode> {
        let mut ancestry = Vec::new();
        let mut current_id = Some(tool_use_id);

        while let Some(id) = current_id {
            if let Some(node) = self.nodes_by_tool_call.get(id) {
                ancestry.push(node);
                current_id = node.parent_tool_use_id.as_deref();
            } else {
                break;
            }
        }

        ancestry
    }

    /// Checks if a task is currently active.
    pub fn is_active(&self, task_id: &str) -> bool {
        self.resolve_by_task_id(task_id)
            .map(|n| n.is_active())
            .unwrap_or(false)
    }

    /// Returns all currently active subagents.
    pub fn active_subagents(&self) -> Vec<&SubagentNode> {
        self.nodes_by_tool_call
            .values()
            .filter(|n| n.is_active())
            .collect()
    }

    /// Returns all registered subagents (active and completed).
    pub fn all_subagents(&self) -> Vec<&SubagentNode> {
        self.nodes_by_tool_call.values().collect()
    }

    pub fn count(&self) -> usize {
        self.nodes_by_tool_call.len()
    }

    pub fn active_count(&self) -> usize {
        self.active_subagents().len()
    }
}
