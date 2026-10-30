/**
 * Agent Studios — Mission Control Read Models & Wire Types
 * Milestone M14 Mission Control Workbench
 */

import { ConnectionState } from '../../bridge/types';

export type MissionControlTabId =
  | 'overview'
  | 'agents'
  | 'tasks'
  | 'runs'
  | 'timeline'
  | 'review'
  | 'approvals'
  | 'providers'
  | 'diagnostics';

export type TaskLifecycleState =
  | 'pending'
  | 'ready'
  | 'running'
  | 'blocked'
  | 'review'
  | 'succeeded'
  | 'failed'
  | 'cancelled';

export interface StudioSummary {
  id: string;
  name: string;
  workspacePath: string;
  createdAt: string;
  runStatus: 'running' | 'idle' | 'completed' | 'failed';
}

export interface AttentionItem {
  id: string;
  severity: 'info' | 'warning' | 'error' | 'critical';
  title: string;
  description: string;
  targetTab: MissionControlTabId;
  targetId?: string;
  timestamp: number;
}

export interface OverviewProjection {
  activeAgentCount: number;
  totalAgentsCount: number;
  taskCounts: {
    pending: number;
    ready: number;
    running: number;
    blocked: number;
    review: number;
    succeeded: number;
    failed: number;
    cancelled: number;
  };
  attentionItems: AttentionItem[];
  recentEvents: TimelineEventRecord[];
  activeRunCount: number;
}

export interface AgentFleetMember {
  agentId: string;
  name: string;
  role: string;
  status: 'idle' | 'running' | 'blocked' | 'failed' | 'completed';
  assignedTaskId?: string;
  assignedTaskTitle?: string;
  activeRunId?: string;
  runtime: string;
  providerModel: string;
  lastActivityAt: number;
  toolCallCount: number;
  worktreePath?: string;
  error?: string;
}

export interface TaskLifecycleModel {
  id: string;
  studioId: string;
  title: string;
  description: string;
  state: TaskLifecycleState;
  dependencies: string[];
  assignedAgentId?: string;
  assignedAgentName?: string;
  createdAt: string;
  updatedAt: string;
  progress: number;
  worktreePath?: string;
  error?: string;
}

export interface ToolInvocationRecord {
  id: string;
  toolName: string;
  status: 'started' | 'succeeded' | 'failed';
  inputSummary: string;
  outputSummary?: string;
  timestamp: number;
  durationMs?: number;
}

export interface RunRecord {
  runId: string;
  taskId: string;
  agentId: string;
  runtime: string;
  status: 'queued' | 'running' | 'succeeded' | 'failed' | 'interrupted' | 'cancelled';
  startedAt: number;
  completedAt?: number;
  worktreePath?: string;
  toolCalls: ToolInvocationRecord[];
  terminalEvidence?: {
    exitCode?: number;
    command?: string;
    stdoutSnippet?: string;
    stderrSnippet?: string;
  };
  error?: string;
}

export interface TimelineEventRecord {
  sequence: number;
  timestamp: string;
  type: string;
  taskId?: string;
  agentId?: string;
  runId?: string;
  summary: string;
  details?: Record<string, unknown>;
}

export interface ArtifactRecord {
  id: string;
  sessionId: string;
  taskId?: string;
  path: string;
  changeType: 'added' | 'modified' | 'deleted';
  diffSummary?: string;
  originalUri?: string;
  modifiedUri?: string;
  sizeBytes?: number;
}

export interface ApprovalRecord {
  id: string;
  sessionId: string;
  taskId?: string;
  actionType: string;
  commandOrTarget: string;
  riskLevel: 'low' | 'medium' | 'high' | 'critical';
  status: 'pending' | 'approved' | 'denied' | 'expired';
  createdAt: number;
  requestedPermissions: string[];
}

export interface ProviderUsageSummary {
  id: string;
  displayName: string;
  modelId: string;
  status: 'available' | 'degraded' | 'unavailable';
  contextWindow?: number;
  maxOutputTokens?: number;
  latencyMs?: number;
  tokensUsed?: number;
}

export interface DiagnosticsRecord {
  endpoint: string;
  sessionId: string | null;
  protocolVersion: string;
  agreedCapabilities: string[];
  watermark: number;
  activeClients: number;
  uptimeSecs: number;
  lastError: string | null;
}

export interface MissionControlState {
  version: string;
  timestamp: number;
  connectionState: ConnectionState;
  isPreview: boolean;
  activeTab: MissionControlTabId;
  activeStudio: StudioSummary | null;
  overview: OverviewProjection;
  fleet: AgentFleetMember[];
  tasks: TaskLifecycleModel[];
  runs: RunRecord[];
  timeline: TimelineEventRecord[];
  artifacts: ArtifactRecord[];
  approvals: ApprovalRecord[];
  providers: ProviderUsageSummary[];
  diagnostics: DiagnosticsRecord;
  selectedAgentId?: string;
  selectedTaskId?: string;
  selectedRunId?: string;
}

export type WebviewToHostMessage =
  | { type: 'switchTab'; tab: MissionControlTabId }
  | { type: 'selectAgent'; agentId: string | null }
  | { type: 'selectTask'; taskId: string | null }
  | { type: 'selectRun'; runId: string | null }
  | { type: 'openDiff'; artifactId: string }
  | { type: 'openChat' }
  | { type: 'approveAction'; id: string }
  | { type: 'denyAction'; id: string }
  | { type: 'refresh' }
  | { type: 'filterTimeline'; filter: string };

export type HostToWebviewMessage =
  | { type: 'stateUpdate'; state: MissionControlState }
  | { type: 'tabChanged'; tab: MissionControlTabId };

export interface MissionControlWebviewMessage {
  type: string;
  payload?: {
    tab?: MissionControlTabId;
    agentId?: string;
    taskId?: string;
    runId?: string;
    artifactId?: string;
    sessionId?: string;
    actionId?: string;
    [key: string]: unknown;
  };
}
