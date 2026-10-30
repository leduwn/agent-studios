/**
 * Agent Studios — Disconnected Bridge Implementation
 * Milestone M14-UI-00: Code-OSS Native Agents Workbench
 */

import * as vscode from 'vscode';
import { AgentStudiosBridge, BridgeDisconnectedError } from './AgentStudiosBridge';
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
 * Production-safe default bridge used when the Agent Studios daemon is not connected.
 * Fails closed on any mutating actions and yields null / empty collections for reads.
 */
export class DisconnectedBridge implements AgentStudiosBridge {
  public readonly connectionState: ConnectionState = 'disconnected';

  private readonly _onConnectionStateChanged = new vscode.EventEmitter<ConnectionState>();
  public readonly onConnectionStateChanged: vscode.Event<ConnectionState> = this._onConnectionStateChanged.event;

  private readonly _onEvent = new vscode.EventEmitter<TimelineEvent>();
  public readonly onEvent: vscode.Event<TimelineEvent> = this._onEvent.event;

  public async connect(): Promise<void> {
    // DisconnectedBridge remains disconnected until replaced by an active daemon connection
    return Promise.resolve();
  }

  public async disconnect(): Promise<void> {
    return Promise.resolve();
  }

  public async getCurrentSession(): Promise<SessionInfo | null> {
    return null;
  }

  public async getTaskGraph(_runId?: string): Promise<TaskGraph | null> {
    return null;
  }

  public async listTasks(): Promise<TaskNode[]> {
    return [];
  }

  public async listPendingApprovals(): Promise<ApprovalRequest[]> {
    return [];
  }

  public async requestApprovalDecision(
    _id: string,
    _decision: 'approved' | 'denied'
  ): Promise<ApprovalDecisionResult> {
    throw new BridgeDisconnectedError(
      'Cannot request approval decision: Agent Studios daemon bridge is disconnected.'
    );
  }

  public async listArtifacts(): Promise<Artifact[]> {
    return [];
  }

  public async getArtifactDiff(_artifactId: string): Promise<string | null> {
    return null;
  }

  public async listProviderSummaries(): Promise<ProviderSummary[]> {
    return [];
  }

  public async submitEditorAction(_action: EditorActionRequest): Promise<EditorActionResult> {
    throw new BridgeDisconnectedError(
      'Cannot submit editor action: Agent Studios daemon bridge is disconnected.'
    );
  }

  public async sendMessage(
    _request: ChatSendRequest,
    _onChunk: (chunk: ChatStreamChunk) => void
  ): Promise<ChatMessage> {
    throw new BridgeDisconnectedError(
      'Cannot send chat message: Agent Studios daemon bridge is disconnected.'
    );
  }

  public async submitTask(_req: SubmitTaskRequest): Promise<SubmitTaskResult> {
    throw new BridgeDisconnectedError(
      'Cannot submit task: Agent Studios daemon bridge is disconnected.'
    );
  }

  public async cancelTask(_taskId: string, _reason?: string): Promise<CancelTaskResult> {
    throw new BridgeDisconnectedError(
      'Cannot cancel task: Agent Studios daemon bridge is disconnected.'
    );
  }

  public async cancelCurrentRun(): Promise<void> {
    return Promise.resolve();
  }

  public async listSessions(): Promise<SessionSummary[]> {
    return [];
  }

  public async createSession(_goal?: string): Promise<SessionInfo> {
    throw new BridgeDisconnectedError(
      'Cannot create session: Agent Studios daemon bridge is disconnected.'
    );
  }

  public async switchSession(_sessionId: string): Promise<SessionInfo> {
    throw new BridgeDisconnectedError(
      'Cannot switch session: Agent Studios daemon bridge is disconnected.'
    );
  }

  public dispose(): void {
    this._onConnectionStateChanged.dispose();
    this._onEvent.dispose();
  }
}
