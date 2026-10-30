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
  ProviderSummary,
  SessionInfo,
  SessionSummary,
  TaskGraph,
  TaskNode,
  TimelineEvent,
} from './types';
import { SubmitTaskRequest, SubmitTaskResult } from './ipc/SubmitTaskAdapter';
import { CancelTaskResult } from './ipc/ExecutionClient';

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
  listProviderSummaries(): Promise<ProviderSummary[]>;
  submitEditorAction(action: EditorActionRequest): Promise<EditorActionResult>;

  sendMessage(
    request: ChatSendRequest,
    onChunk: (chunk: ChatStreamChunk) => void
  ): Promise<ChatMessage>;
  submitTask?(req: SubmitTaskRequest): Promise<SubmitTaskResult>;
  cancelTask?(taskId: string, reason?: string): Promise<CancelTaskResult>;
  cancelCurrentRun(): Promise<void>;
  listSessions(): Promise<SessionSummary[]>;
  createSession(goal?: string): Promise<SessionInfo>;
  switchSession(sessionId: string): Promise<SessionInfo>;

  dispose(): void;
}
