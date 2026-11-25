/**
 * Agent Studios — Bridge Interface and Disconnected Error
 * Milestone M14-UI-00: Code-OSS Native Agents Workbench
 */

import * as vscode from 'vscode';
import {
  ApprovalDecisionResult,
  ApprovalRequest,
  Artifact,
  ChatMessage,
  ChatSendRequest,
  ChatStreamChunk,
  ConnectionState,
  EditorActionRequest,
  EditorActionResult,
  ProviderModel,
  ProviderSummary,
  SessionInfo,
  SessionSummary,
  TaskGraph,
  TaskNode,
  TimelineEvent,
} from './types';
import { SubmitTaskRequest, SubmitTaskResult } from './ipc/SubmitTaskAdapter';
import { CancelTaskResult } from './ipc/ExecutionClient';
import {
  RustArtifactRecord,
  RustExecutionReadinessReport,
  RustRecoveryStatusReport,
} from './ipc/RustIpcProtocolTypes';
import { ReassembledArtifactContent, ReassembledTaskDiff } from './ipc/ArtifactClient';

/**
 * Thrown when an operation requiring daemon connectivity is attempted while disconnected.
 * Fails closed with applied: false.
 */
export class BridgeDisconnectedError extends Error {
  public readonly applied: boolean = false;

  constructor(message: string = 'Agent Studios daemon bridge is disconnected') {
    super(message);
    this.name = 'BridgeDisconnectedError';
    Object.setPrototypeOf(this, BridgeDisconnectedError.prototype);
  }
}

/**
 * Canonical bridge interface connecting the Code-OSS workbench extension
 * to either the authoritative local/remote daemon or the developer fixture.
 */
export interface AgentStudiosBridge extends vscode.Disposable {
  readonly connectionState: ConnectionState;
  readonly onConnectionStateChanged: vscode.Event<ConnectionState>;
  readonly onEvent: vscode.Event<TimelineEvent>;

  connect(): Promise<void>;
  disconnect(): Promise<void>;

  getCurrentSession(): Promise<SessionInfo | null>;
  getTaskGraph(runId?: string): Promise<TaskGraph | null>;
  listTasks(): Promise<TaskNode[]>;
  listPendingApprovals(): Promise<ApprovalRequest[]>;
  requestApprovalDecision(id: string, decision: 'approved' | 'denied'): Promise<ApprovalDecisionResult>;
  listArtifacts(): Promise<Artifact[]>;
  getArtifactDiff(artifactId: string): Promise<string | null>;
  getTaskDiff?(studioId: string, taskId: string, signal?: AbortSignal): Promise<ReassembledTaskDiff>;
  listTaskArtifacts?(studioId: string, taskId: string): Promise<RustArtifactRecord[]>;
  fetchArtifactContent?(studioId: string, artifactId: string, signal?: AbortSignal): Promise<ReassembledArtifactContent>;
  listProviderSummaries(): Promise<ProviderSummary[]>;
  submitEditorAction(action: EditorActionRequest): Promise<EditorActionResult>;

  sendMessage(
    request: ChatSendRequest,
    onChunk: (chunk: ChatStreamChunk) => void
  ): Promise<ChatMessage>;
  submitTask?(req: SubmitTaskRequest): Promise<SubmitTaskResult>;
  cancelTask?(taskId: string, reason?: string): Promise<CancelTaskResult>;
  getExecutionReadiness?(): Promise<RustExecutionReadinessReport>;
  getRecoveryStatus?(): Promise<RustRecoveryStatusReport>;
  cancelCurrentRun(): Promise<void>;
  listSessions(): Promise<SessionSummary[]>;
  createSession(goal?: string): Promise<SessionInfo>;
  switchSession(sessionId: string): Promise<SessionInfo>;
  createStudioAuthoritative?(name: string): Promise<SessionInfo>;
  forkSession?(sourceStudioId: string, name?: string): Promise<SessionInfo>;
  resolveApprovalAuthoritative?(
    approvalId: string,
    decision: 'approved' | 'denied'
  ): Promise<ApprovalDecisionResult>;
  upsertProviderInstance?(payload: {
    instanceId?: string;
    providerType: string;
    displayName: string;
    protocol?: string;
    baseUrl: string;
    secretEnvVar?: string;
    secretLocator?: string;
    secretBackend?: 'environment_variable' | 'os_credential_store' | 'external';
    secretValue?: string;
    models?: ProviderModel[];
  }): Promise<ProviderSummary>;
  setProviderSecret?(
    locator: string,
    secretValue: string,
    backend?: 'environment_variable' | 'os_credential_store' | 'external'
  ): Promise<boolean>;
  deleteProviderSecret?(
    locator: string,
    backend?: 'environment_variable' | 'os_credential_store' | 'external'
  ): Promise<boolean>;
  getSessionTranscript?(studioId: string): Promise<ChatMessage[]>;
  executeMultiAgentGoal?(request: {
    studioId?: string;
    goal: string;
    providerInstanceId?: string;
    modelId?: string;
    repositoryPath?: string;
    permissionMode?: 'read-only' | 'auto-edit' | 'full-access';
  }): Promise<Record<string, unknown>>;
  removeProviderInstance?(providerId: string): Promise<boolean>;
  discoverProviderModels?(providerId: string): Promise<ProviderModel[]>;
  testProviderConnection?(providerId: string): Promise<{
    ok: boolean;
    status: string;
    details?: unknown;
  }>;
  listExtensibility?(workspaceRoot?: string): Promise<{
    skills: unknown[];
    mcpServers: unknown[];
    instructions: unknown[];
  }>;

  dispose(): void;
}
