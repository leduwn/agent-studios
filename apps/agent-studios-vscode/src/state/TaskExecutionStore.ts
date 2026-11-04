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
  runtimePreference?: string;
  runtimeInstanceId?: string;
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
  private readonly _taskStates = new Map<string, TaskExecutionState>();
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

  public beginSubmission(
    studioId: string,
    prompt: string,
    displayTitle?: string,
    runtimePreference?: string,
    runtimeInstanceId?: string
  ): void {
    this.stopTimer();
    const now = Date.now();
    this._state = {
      ...this.buildInitialState(),
      status: 'submitting',
      activeStudioId: studioId,
      prompt,
      displayTitle: displayTitle || (prompt.length > 60 ? `${prompt.substring(0, 60)}...` : prompt),
      startedAt: now,
      runtimePreference,
      runtimeInstanceId,
    };
    this.startTimer();
    this.notifyStateChanged();
  }

  public taskSubmitted(taskId: string, studioId: string): void {
    this._state = {
      ...this._state,
      status: 'queued',
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
    if (this._state.activeTaskId) {
      this._taskStates.set(this._state.activeTaskId, { ...this._state });
    }
    this.notifyStateChanged();
  }

  public switchTask(taskId: string, studioId?: string): void {
    if (this._state.activeTaskId === taskId) {
      return;
    }
    if (this._state.activeTaskId) {
      this._taskStates.set(this._state.activeTaskId, { ...this._state });
    }

    const cached = this._taskStates.get(taskId);
    if (cached) {
      this._state = { ...cached };
    } else {
      this._state = {
        ...this.buildInitialState(),
        activeTaskId: taskId,
        activeStudioId: studioId || this._state.activeStudioId,
      };
      this._taskStates.set(taskId, { ...this._state });
    }
    this.notifyStateChanged();
  }

  public switchStudio(studioId: string | null): void {
    this.stopTimer();
    this._taskStates.clear();
    this._state = {
      ...this.buildInitialState(),
      activeStudioId: studioId ?? undefined,
    };
    this.notifyStateChanged();
  }

  public getTaskState(taskId: string): TaskExecutionState | undefined {
    if (this._state.activeTaskId === taskId) {
      return { ...this._state };
    }
    const cached = this._taskStates.get(taskId);
    return cached ? { ...cached } : undefined;
  }

  public reconcileDaemonRestart(reason: string = 'daemon_interrupted'): void {
    const error: ExecutionError = {
      code: 'daemon_interrupted',
      message: `Task interrupted: daemon_interrupted (${reason})`,
    };

    if (
      this._state.status === 'submitting' ||
      this._state.status === 'queued' ||
      this._state.status === 'running'
    ) {
      this.completeTask('failed', error);
    }

    for (const [taskId, tState] of this._taskStates.entries()) {
      if (
        tState.status === 'submitting' ||
        tState.status === 'queued' ||
        tState.status === 'running'
      ) {
        this._taskStates.set(taskId, {
          ...tState,
          status: 'failed',
          finishedAt: Date.now(),
          lastError: error,
          activeTool: undefined,
        });
      }
    }
  }

  public reset(): void {
    this.stopTimer();
    this._taskStates.clear();
    this._state = this.buildInitialState();
    this.notifyStateChanged();
  }

  public consumeDaemonTimelineEvent(event: TimelineEvent): void {
    if (!event) return;

    // Filter by active task if set; background tasks are updated in isolation without leaking into active task
    if (this._state.activeTaskId && event.taskId && event.taskId !== this._state.activeTaskId) {
      let bgState = this._taskStates.get(event.taskId);
      if (!bgState) {
        bgState = {
          ...this.buildInitialState(),
          activeTaskId: event.taskId,
          activeStudioId: this._state.activeStudioId,
        };
        this._taskStates.set(event.taskId, bgState);
      }
      this.applyEventToTaskState(bgState, event, false);
      return;
    }

    this.applyEventToTaskState(this._state, event, true);
  }

  private applyEventToTaskState(
    target: TaskExecutionState,
    event: TimelineEvent,
    isActive: boolean
  ): void {
    const payload = (event.details && typeof event.details === 'object')
      ? (event.details as Record<string, unknown>)
      : {};

    switch (event.type) {
      case 'task_started':
      case 'run_started':
        if (target.status === 'submitting' || target.status === 'queued') {
          const exec = payload.execution as Record<string, unknown> | undefined;
          target.status = 'running';
          target.activeTaskId = event.taskId ?? target.activeTaskId;
          target.activeRunId = ((payload.run_id || payload.runId) as string | undefined) ?? target.activeRunId;
          target.runtimePreference = ((payload.runtime_preference || exec?.runtime_preference) as string | undefined) ?? target.runtimePreference;
          target.runtimeInstanceId = ((payload.runtime_instance_id || exec?.runtime_instance_id) as string | undefined) ?? target.runtimeInstanceId;
          if (isActive) {
            this.notifyStateChanged();
          }
        }
        break;

      case 'task_state_transitioned':
      case 'run_state_transitioned': {
        const toState = (payload.to_state || payload.new_state || payload.state) as string | undefined;
        if (toState === 'running' || toState === 'executing') {
          if (target.status === 'submitting' || target.status === 'queued') {
            target.status = 'running';
            target.activeTaskId = event.taskId ?? target.activeTaskId;
            target.activeRunId = ((payload.run_id || payload.runId) as string | undefined) ?? target.activeRunId;
            if (isActive) {
              this.notifyStateChanged();
            }
          }
        } else if (toState === 'succeeded' || toState === 'completed') {
          if (isActive) {
            this.completeTask('succeeded');
          } else {
            target.status = 'succeeded';
            target.finishedAt = Date.now();
            target.activeTool = undefined;
          }
        } else if (toState === 'failed' || toState === 'daemon_interrupted' || toState === 'interrupted' || toState === 'error') {
          const isInterrupted = toState === 'daemon_interrupted' || toState === 'interrupted' || payload.reason === 'daemon_interrupted';
          const err: ExecutionError = {
            code: isInterrupted ? 'daemon_interrupted' : (typeof payload.code === 'string' ? payload.code : undefined),
            message: event.summary || (isInterrupted ? 'Task interrupted by daemon restart' : 'Task transitioned to failed state'),
            details: payload,
          };
          if (isActive) {
            this.completeTask('failed', err);
          } else {
            target.status = 'failed';
            target.finishedAt = Date.now();
            target.lastError = err;
            target.activeTool = undefined;
          }
        } else if (toState === 'cancelled') {
          const err: ExecutionError = {
            message: event.summary || 'Task transitioned to cancelled state',
            details: payload,
          };
          if (isActive) {
            this.completeTask('cancelled', err);
          } else {
            target.status = 'cancelled';
            target.finishedAt = Date.now();
            target.lastError = err;
            target.activeTool = undefined;
          }
        }
        break;
      }

      case 'tool_started':
      case 'tool_executing': {
        let toolName = 'unknown';
        if (typeof payload.tool_name === 'string') {
          toolName = payload.tool_name;
        } else if (typeof payload.name === 'string') {
          toolName = payload.name;
        } else if (event.summary && event.summary.startsWith('Tool ')) {
          const match = event.summary.match(/^Tool (\S+)/);
          toolName = match ? match[1] : 'unknown';
        }
        if (isActive) {
          this.setToolExecution(toolName, 'executing');
        } else {
          target.activeTool = { name: toolName, status: 'executing', timestamp: Date.now() };
        }
        break;
      }

      case 'tool_completed': {
        const toolName = typeof payload.tool_name === 'string'
          ? payload.tool_name
          : (typeof payload.name === 'string' ? payload.name : (target.activeTool?.name || 'unknown'));
        if (isActive) {
          this.setToolExecution(toolName, 'completed');
        } else {
          target.activeTool = { name: toolName, status: 'completed', timestamp: Date.now() };
        }
        break;
      }

      case 'tool_failed': {
        const toolName = typeof payload.tool_name === 'string'
          ? payload.tool_name
          : typeof payload.name === 'string'
            ? payload.name
            : (target.activeTool?.name || 'unknown');
        const err = typeof payload.error === 'string'
          ? payload.error
          : (typeof payload.message === 'string' ? payload.message : undefined);
        if (isActive) {
          this.setToolExecution(toolName, 'failed', err);
        } else {
          target.activeTool = { name: toolName, status: 'failed', error: err, timestamp: Date.now() };
        }
        break;
      }

      case 'runtime_output_delta':
      case 'output_delta': {
        const text = typeof payload.text === 'string'
          ? payload.text
          : (typeof payload.output === 'string' ? payload.output : (typeof payload.chunk === 'string' ? payload.chunk : undefined));
        if (text) {
          if (isActive) {
            this.appendStreamChunk(text);
          } else {
            target.streamOutput += text;
          }
        }
        break;
      }

      case 'task_completed':
      case 'run_completed':
        if (isActive) {
          this.completeTask('succeeded');
        } else {
          target.status = 'succeeded';
          target.finishedAt = Date.now();
          target.activeTool = undefined;
        }
        break;

      case 'task_failed':
      case 'run_failed': {
        const isInterrupted = payload.reason === 'daemon_interrupted' ||
          payload.error === 'daemon_interrupted' ||
          payload.to_state === 'daemon_interrupted' ||
          (typeof event.summary === 'string' && event.summary.includes('daemon_interrupted'));
        const err: ExecutionError = {
          code: isInterrupted ? 'daemon_interrupted' : (typeof payload.code === 'string' ? payload.code : undefined),
          message: event.summary || (isInterrupted ? 'Task interrupted by daemon restart' : 'Task failed on daemon'),
          details: payload,
        };
        if (isActive) {
          this.completeTask('failed', err);
        } else {
          target.status = 'failed';
          target.finishedAt = Date.now();
          target.lastError = err;
          target.activeTool = undefined;
        }
        break;
      }

      case 'task_cancelled': {
        const err: ExecutionError = {
          message: event.summary || 'Task was cancelled',
          details: payload,
        };
        if (isActive) {
          this.completeTask('cancelled', err);
        } else {
          target.status = 'cancelled';
          target.finishedAt = Date.now();
          target.lastError = err;
          target.activeTool = undefined;
        }
        break;
      }
    }

    // Capture stdout or stderr text delta if payload carries stream content outside delta events
    if (event.type !== 'runtime_output_delta' && event.type !== 'output_delta') {
      const extraOutput = typeof payload.output === 'string'
        ? payload.output
        : (typeof payload.chunk === 'string' ? payload.chunk : undefined);
      if (extraOutput) {
        if (isActive) {
          this.appendStreamChunk(extraOutput);
        } else {
          target.streamOutput += extraOutput;
        }
      }
    }

    // Capture metrics authoritatively if reported in event payload
    if (payload.metrics && typeof payload.metrics === 'object') {
      const m = payload.metrics as Record<string, unknown>;
      const metricsData = {
        promptTokens: typeof m.prompt_tokens === 'number' ? m.prompt_tokens : (typeof m.promptTokens === 'number' ? m.promptTokens : undefined),
        completionTokens: typeof m.completion_tokens === 'number' ? m.completion_tokens : (typeof m.completionTokens === 'number' ? m.completionTokens : undefined),
        totalTokens: typeof m.total_tokens === 'number' ? m.total_tokens : (typeof m.totalTokens === 'number' ? m.totalTokens : undefined),
        costUsd: typeof m.cost_usd === 'number' ? m.cost_usd : (typeof m.costUsd === 'number' ? m.costUsd : undefined),
        latencyMs: typeof m.latency_ms === 'number' ? m.latency_ms : (typeof m.latencyMs === 'number' ? m.latencyMs : undefined),
      };
      if (isActive) {
        this.updateMetrics(metricsData);
      } else {
        target.metrics = { ...metricsData, isReported: true };
      }
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
