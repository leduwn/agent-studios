/**
 * Agent Studios — Unified Task Execution Store & Streaming Projection
 * Milestone M15 UI-01: Interactive Agent Execution Workbench
 *
 * Maintains live execution state for the active task run, including:
 * - Status progression (idle -> submitting -> queued -> running -> succeeded/failed/cancelled)
 * - Raw stream chunks (stdout/stderr) for console inspection
 * - Active tool execution tracking (e.g. bash, file_write)
 * - Truthful metrics (only if authoritatively reported by daemon — never fabricated!)
 * - Lifecycle synchronization across Chat and Mission Control views
 */

import * as vscode from 'vscode';
import { TimelineEvent } from '../bridge/types';

export type ExecutionStatus =
  | 'idle'
  | 'submitting'
  | 'queued'
  | 'running'
  | 'succeeded'
  | 'failed'
  | 'cancelled';

export interface ActiveToolExecution {
  name: string;
  status: 'executing' | 'completed' | 'failed';
  error?: string;
  timestamp: number;
}

export interface ExecutionMetrics {
  promptTokens?: number;
  completionTokens?: number;
  totalTokens?: number;
  costUsd?: number;
  latencyMs?: number;
  isReported: boolean;
}

export interface ExecutionError {
  code?: string;
  message: string;
  details?: unknown;
}

export interface TaskExecutionState {
  status: ExecutionStatus;
  activeTaskId?: string;
  activeRunId?: string;
  activeStudioId?: string;
  prompt?: string;
  displayTitle?: string;
  startedAt?: number;
  finishedAt?: number;
  elapsedSeconds: number;
  activeTool?: ActiveToolExecution;
  streamOutput: string;
  metrics?: ExecutionMetrics;
  lastError?: ExecutionError;
}

export class TaskExecutionStore implements vscode.Disposable {
  private static _instance?: TaskExecutionStore;

  public static get instance(): TaskExecutionStore {
    if (!this._instance) {
      this._instance = new TaskExecutionStore();
    }
    return this._instance;
  }

  private _state: TaskExecutionState;
  private readonly _onDidChangeExecutionState = new vscode.EventEmitter<TaskExecutionState>();
  public readonly onDidChangeExecutionState: vscode.Event<TaskExecutionState> =
    this._onDidChangeExecutionState.event;

  private readonly _onDidStreamChunk = new vscode.EventEmitter<string>();
  public readonly onDidStreamChunk: vscode.Event<string> = this._onDidStreamChunk.event;

  private _timer?: NodeJS.Timeout;
  private readonly _disposables: vscode.Disposable[] = [];
  private _isDisposed = false;

  constructor() {
    this._state = this.buildInitialState();
    this._disposables.push(this._onDidChangeExecutionState, this._onDidStreamChunk);
  }

  public getState(): TaskExecutionState {
    return { ...this._state };
  }

  public get state(): TaskExecutionState {
    return { ...this._state };
  }

  public beginSubmission(studioId: string, prompt: string, displayTitle?: string): void {
    this.stopTimer();
    const now = Date.now();
    this._state = {
      ...this.buildInitialState(),
      status: 'submitting',
      activeStudioId: studioId,
      prompt,
      displayTitle: displayTitle || (prompt.length > 60 ? `${prompt.substring(0, 60)}...` : prompt),
      startedAt: now,
    };
    this.startTimer();
    this.notifyStateChanged();
  }

  public taskSubmitted(taskId: string, studioId: string): void {
    this._state = {
      ...this._state,
      status: 'running',
      activeTaskId: taskId,
      activeStudioId: studioId,
    };
    this.notifyStateChanged();
  }

  public appendStreamChunk(chunk: string): void {
    if (!chunk || typeof chunk !== 'string') {
      return;
    }
    this._state = {
      ...this._state,
      streamOutput: this._state.streamOutput + chunk,
    };
    this._onDidStreamChunk.fire(chunk);
    this.notifyStateChanged();
  }

  public setToolExecution(
    toolName: string,
    status: 'executing' | 'completed' | 'failed',
    error?: string
  ): void {
    this._state = {
      ...this._state,
      activeTool: {
        name: toolName,
        status,
        error,
        timestamp: Date.now(),
      },
    };
    this.notifyStateChanged();
  }

  public updateMetrics(metrics: {
    promptTokens?: number;
    completionTokens?: number;
    totalTokens?: number;
    costUsd?: number;
    latencyMs?: number;
  }): void {
    this._state = {
      ...this._state,
      metrics: {
        ...metrics,
        isReported: true,
      },
    };
    this.notifyStateChanged();
  }

  public completeTask(
    status: 'succeeded' | 'failed' | 'cancelled',
    error?: ExecutionError
  ): void {
    this.stopTimer();
    const now = Date.now();
    const started = this._state.startedAt ?? now;
    const elapsedSeconds = Math.max(0, Math.floor((now - started) / 1000));

    this._state = {
      ...this._state,
      status,
      finishedAt: now,
      elapsedSeconds,
      lastError: error,
      activeTool: undefined,
    };
    this.notifyStateChanged();
  }

  public reset(): void {
    this.stopTimer();
    this._state = this.buildInitialState();
    this.notifyStateChanged();
  }

  public consumeDaemonTimelineEvent(event: TimelineEvent): void {
    if (!event) return;

    // Filter by active task if set
    if (this._state.activeTaskId && event.taskId && event.taskId !== this._state.activeTaskId) {
      return;
    }

    const payload = (event.details && typeof event.details === 'object')
      ? (event.details as Record<string, unknown>)
      : {};

    switch (event.type) {
      case 'task_submitted':
      case 'task_started':
      case 'run_started':
        if (this._state.status === 'submitting' || this._state.status === 'queued') {
          this._state = {
            ...this._state,
            status: 'running',
            activeTaskId: event.taskId ?? this._state.activeTaskId,
          };
          this.notifyStateChanged();
        }
        break;

      case 'tool_executing':
        if (typeof payload.tool_name === 'string') {
          this.setToolExecution(payload.tool_name, 'executing');
        } else if (event.summary && event.summary.startsWith('Tool ')) {
          const match = event.summary.match(/^Tool (\S+)/);
          this.setToolExecution(match ? match[1] : 'unknown', 'executing');
        }
        break;

      case 'tool_completed':
        if (typeof payload.tool_name === 'string') {
          this.setToolExecution(payload.tool_name, 'completed');
        } else if (this._state.activeTool) {
          this.setToolExecution(this._state.activeTool.name, 'completed');
        }
        break;

      case 'tool_failed':
        if (typeof payload.tool_name === 'string') {
          const err = typeof payload.error === 'string' ? payload.error : undefined;
          this.setToolExecution(payload.tool_name, 'failed', err);
        } else if (this._state.activeTool) {
          this.setToolExecution(this._state.activeTool.name, 'failed');
        }
        break;

      case 'task_completed':
      case 'run_completed':
        this.completeTask('succeeded');
        break;

      case 'task_failed':
      case 'run_failed':
        this.completeTask('failed', {
          message: event.summary || 'Task failed on daemon',
          details: payload,
        });
        break;

      case 'task_cancelled':
        this.completeTask('cancelled', {
          message: event.summary || 'Task was cancelled',
          details: payload,
        });
        break;
    }

    // Capture stdout or stderr text delta if payload carries stream content
    if (typeof payload.output === 'string') {
      this.appendStreamChunk(payload.output);
    } else if (typeof payload.chunk === 'string') {
      this.appendStreamChunk(payload.chunk);
    }
  }

  public dispose(): void {
    if (this._isDisposed) return;
    this._isDisposed = true;
    this.stopTimer();
    while (this._disposables.length > 0) {
      const d = this._disposables.pop();
      d?.dispose();
    }
  }

  // --- Private Helpers ---

  private buildInitialState(): TaskExecutionState {
    return {
      status: 'idle',
      elapsedSeconds: 0,
      streamOutput: '',
    };
  }

  private startTimer(): void {
    this.stopTimer();
    this._timer = setInterval(() => {
      if (this._state.startedAt && (this._state.status === 'submitting' || this._state.status === 'running' || this._state.status === 'queued')) {
        const elapsed = Math.max(0, Math.floor((Date.now() - this._state.startedAt) / 1000));
        this._state = {
          ...this._state,
          elapsedSeconds: elapsed,
        };
        this.notifyStateChanged();
      }
    }, 1000);
  }

  private stopTimer(): void {
    if (this._timer) {
      clearInterval(this._timer);
      this._timer = undefined;
    }
  }

  private notifyStateChanged(): void {
    this._onDidChangeExecutionState.fire(this.getState());
  }
}
