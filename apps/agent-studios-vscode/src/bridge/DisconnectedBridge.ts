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
  ConnectionState,
  EditorActionRequest,
  EditorActionResult,
  ProviderSummary,
  SessionInfo,
  TaskGraph,
  TaskNode,
  TimelineEvent,
} from './types';

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

  public dispose(): void {
    this._onConnectionStateChanged.dispose();
    this._onEvent.dispose();
  }
}
