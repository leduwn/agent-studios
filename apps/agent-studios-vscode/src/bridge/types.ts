/**
 * Agent Studios — Canonical Bridge and Domain Types
 * Milestone M14-UI-00: Code-OSS Native Agents Workbench
 */

export type SessionId = string;
export type StudioId = string;
export type RunId = string;
export type TaskId = string;
export type AgentId = string;
export type ApprovalId = string;
export type ArtifactId = string;

export type ConnectionState =
  | 'disconnected'
  | 'connecting'
  | 'authenticating'
  | 'connected'
  | 'reconnecting'
  | 'degraded'
  | 'auth_required'
  | 'authentication_failed'
  | 'incompatible'
  | 'unsupported_protocol';

export type SessionStatus =
  | 'idle'
  | 'planning'
  | 'running'
  | 'paused'
  | 'completed'
  | 'failed';

export type TaskStatus =
  | 'pending'
  | 'blocked'
  | 'ready'
  | 'running'
  | 'succeeded'
  | 'failed'
  | 'cancelled';

export type ApprovalStatus =
  | 'pending'
  | 'approved'
  | 'denied'
  | 'expired';

export type ApprovalRiskLevel =
  | 'low'
  | 'medium'
  | 'high'
  | 'critical';

export type ApprovalActionType =
  | 'exec_command'
  | 'apply_patch'
  | 'file_write'
  | 'network_access'
  | 'custom';

export interface SessionInfo {
  id: SessionId;
  workspacePath: string;
  goalSummary: string;
  status: SessionStatus;
  activeRunId?: RunId;
  activeAgentCount: number;
  totalAgentCount: number;
  completedTasksCount: number;
  totalTasksCount: number;
  providerSummary?: string;
  startedAt: number;
  elapsedMs: number;
  isPreview?: boolean;
}

export interface TaskNode {
  id: TaskId;
  runId: RunId;
  title: string;
  description: string;
  status: TaskStatus;
  assignedAgentId?: AgentId;
  assignedAgentName?: string;
  dependencies: TaskId[];
  subtasks?: TaskId[];
  progress?: number; // 0 - 100, undefined if indeterminate/unmeasured
  error?: string;
  createdAt: number;
  updatedAt: number;
}

export interface TaskGraph {
  runId: RunId;
  nodes: TaskNode[];
  edges: Array<{ from: TaskId; to: TaskId }>;
}

export interface ApprovalRequest {
  id: ApprovalId;
  sessionId: SessionId;
  taskId?: TaskId;
  taskTitle?: string;
  actionType: ApprovalActionType;
  commandOrTarget: string;
  riskLevel: ApprovalRiskLevel;
  requestedPermissions: string[];
  status: ApprovalStatus;
  createdAt: number;
  expiresAt?: number;
}

export interface Artifact {
  id: ArtifactId;
  sessionId: SessionId;
  taskId?: TaskId;
  taskTitle?: string;
  name: string;
  path: string;
  diffContent: string;
  fileCount: number;
  sha256: string;
  version: number;
  createdAt: number;
}

export type TimelineEventType =
  | 'session_started'
  | 'session_ended'
  | 'run_started'
  | 'run_completed'
  | 'run_failed'
  | 'task_started'
  | 'task_completed'
  | 'task_failed'
  | 'task_blocked'
  | 'agent_assigned'
  | 'tool_executing'
  | 'tool_completed'
  | 'approval_requested'
  | 'approval_decided'
  | 'artifact_created';

export interface TimelineEvent {
  sequenceNumber: number;
  id: string;
  timestamp: number;
  type: TimelineEventType;
  summary: string;
  agentName?: string;
  taskId?: TaskId;
  details?: Record<string, unknown>;
}

export interface ProviderModel {
  id: string;
  name: string;
  contextWindow: number;
  isDefault?: boolean;
}

export interface ProviderSummary {
  instanceId: string;
  providerType: string;
  displayName: string;
  models: ProviderModel[];
  status: 'available' | 'unconfigured' | 'offline';
}

export interface ApprovalDecisionResult {
  id: ApprovalId;
  decision: 'approved' | 'denied';
  applied: boolean;
  message?: string;
}

export interface EditorActionRequest {
  action: 'explain' | 'refactor' | 'generateTests';
  documentUri: string;
  languageId: string;
  selectedText: string;
  selectionRange: {
    startLine: number;
    startCol: number;
    endLine: number;
    endCol: number;
  };
}

export interface EditorActionResult {
  accepted: boolean;
  taskId?: TaskId;
  message: string;
}

export type AgentMode = 'ask' | 'plan' | 'agent';

export interface ContextAttachment {
  type: 'file' | 'selection' | 'diagnostic';
  uri: string;
  relativePath: string;
  selectionRange?: {
    startLine: number;
    endLine: number;
  };
  contentSnippet?: string;
  languageId?: string;
}

export type ChatMessageRole = 'user' | 'assistant' | 'system';

export interface ToolCallInfo {
  id: string;
  name: string;
  status: 'running' | 'completed' | 'failed';
  target?: string;
  inputSummary?: string;
  outputSummary?: string;
  durationMs?: number;
  error?: string;
}

export interface ChatMessage {
  id: string;
  role: ChatMessageRole;
  content: string;
  timestamp: number;
  mode?: AgentMode;
  attachments?: ContextAttachment[];
  toolCalls?: ToolCallInfo[];
  approvalId?: string;
  isStreaming?: boolean;
}

export interface ChatSendRequest {
  message: string;
  mode: AgentMode;
  providerInstanceId?: string;
  modelId?: string;
  attachments?: ContextAttachment[];
}

export interface SessionSummary {
  id: string;
  title: string;
  status: SessionStatus;
  updatedAt: number;
  isPreview?: boolean;
}

import type { AgentStudiosBridge } from './AgentStudiosBridge';

export interface ChatStreamChunk {
  type: 'text_delta' | 'tool_call' | 'approval_required' | 'task_progress' | 'done' | 'error';
  text?: string;
  toolCall?: ToolCallInfo;
  approval?: ApprovalRequest;
  taskSummary?: string;
  error?: string;
}

export interface SlashCommandContext {
  bridge: AgentStudiosBridge;
  clearHistory?: () => Promise<void> | void;
  setMode?: (mode: AgentMode) => void;
  activeEditorContext?: ContextAttachment | null;
  captureActiveEditorContext?: () => Promise<ContextAttachment | null>;
  switchSession?: (sessionId: string) => Promise<void>;
  currentSessionId?: string;
  workspaceRoot?: string;
}

export interface SlashCommandResult {
  success: boolean;
  message?: string;
  markdown?: string;
  mode?: AgentMode;
  formattedPrompt?: string;
  unsupported?: boolean;
  actionTaken?: string;
  data?: unknown;
}

export interface SlashCommandDescriptor {
  name: string;
  description: string;
  execute: (args: string, context: SlashCommandContext) => Promise<SlashCommandResult>;
}

export type SlashCommand = SlashCommandDescriptor;

