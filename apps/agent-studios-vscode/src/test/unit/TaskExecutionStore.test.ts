/**
 * Agent Studios — TaskExecutionStore Unit Tests
 * Milestone M15 UI-01: Interactive Agent Execution Workbench
 */

import * as assert from 'assert';
import {
  TaskExecutionStore,
  TaskExecutionState,
} from '../../state/TaskExecutionStore';
import { TimelineEvent } from '../../bridge/types';

describe('M15 TaskExecutionStore Unit Tests', () => {
  let store: TaskExecutionStore;

  beforeEach(() => {
    store = new TaskExecutionStore();
  });

  afterEach(() => {
    store.dispose();
  });

  describe('Initial State & Reset', () => {
    it('initializes with clean idle state and no fabricated metrics', () => {
      const state = store.getState();
      assert.strictEqual(state.status, 'idle');
      assert.strictEqual(state.elapsedSeconds, 0);
      assert.strictEqual(state.streamOutput, '');
      assert.strictEqual(state.activeTaskId, undefined);
      assert.strictEqual(state.metrics, undefined);
      assert.strictEqual(state.activeTool, undefined);
      assert.strictEqual(state.lastError, undefined);
    });

    it('resets state cleanly to idle', () => {
      store.beginSubmission('studio-1', 'Prompt');
      store.taskSubmitted('task-100', 'studio-1');
      store.appendStreamChunk('Chunk 1\n');
      assert.strictEqual(store.getState().status, 'running');

      store.reset();
      const state = store.getState();
      assert.strictEqual(state.status, 'idle');
      assert.strictEqual(state.streamOutput, '');
      assert.strictEqual(state.activeTaskId, undefined);
    });
  });

  describe('Lifecycle State Transitions', () => {
    it('progresses from submitting to running to succeeded', () => {
      const states: TaskExecutionState[] = [];
      store.onDidChangeExecutionState((s) => states.push(s));

      store.beginSubmission('studio-abc', 'Run test suite', 'Run test suite');
      assert.strictEqual(store.getState().status, 'submitting');
      assert.strictEqual(store.getState().activeStudioId, 'studio-abc');
      assert.strictEqual(store.getState().prompt, 'Run test suite');

      store.taskSubmitted('task-1', 'studio-abc');
      assert.strictEqual(store.getState().status, 'running');
      assert.strictEqual(store.getState().activeTaskId, 'task-1');

      store.completeTask('succeeded');
      assert.strictEqual(store.getState().status, 'succeeded');
      assert.ok(store.getState().finishedAt);
      assert.strictEqual(store.getState().activeTool, undefined);
    });

    it('transitions to failed with structured error on completeTask(failed)', () => {
      store.beginSubmission('studio-abc', 'Failing task');
      store.taskSubmitted('task-2', 'studio-abc');

      store.completeTask('failed', {
        code: 'COMPILATION_ERROR',
        message: 'Rust build failed with exit code 1',
        details: { stderr: 'error[E0425]: cannot find value' },
      });

      const state = store.getState();
      assert.strictEqual(state.status, 'failed');
      assert.ok(state.lastError);
      assert.strictEqual(state.lastError.code, 'COMPILATION_ERROR');
      assert.strictEqual(state.lastError.message, 'Rust build failed with exit code 1');
    });

    it('transitions to cancelled on completeTask(cancelled)', () => {
      store.beginSubmission('studio-abc', 'Task to cancel');
      store.taskSubmitted('task-3', 'studio-abc');

      store.completeTask('cancelled', {
        message: 'Cancelled by operator via UI',
      });

      const state = store.getState();
      assert.strictEqual(state.status, 'cancelled');
      assert.strictEqual(state.lastError?.message, 'Cancelled by operator via UI');
    });
  });

  describe('Stream Output & Tool Activity', () => {
    it('aggregates stream chunks and fires onDidStreamChunk', () => {
      const chunks: string[] = [];
      store.onDidStreamChunk((c) => chunks.push(c));

      store.appendStreamChunk('Compiling crates...\n');
      store.appendStreamChunk('Finished dev [unoptimized + debuginfo] target(s)\n');

      assert.strictEqual(
        store.getState().streamOutput,
        'Compiling crates...\nFinished dev [unoptimized + debuginfo] target(s)\n'
      );
      assert.deepStrictEqual(chunks, [
        'Compiling crates...\n',
        'Finished dev [unoptimized + debuginfo] target(s)\n',
      ]);
    });

    it('tracks active tool execution state', () => {
      store.setToolExecution('bash', 'executing');
      assert.deepStrictEqual(store.getState().activeTool?.name, 'bash');
      assert.deepStrictEqual(store.getState().activeTool?.status, 'executing');

      store.setToolExecution('bash', 'completed');
      assert.deepStrictEqual(store.getState().activeTool?.status, 'completed');

      store.setToolExecution('file_patch', 'failed', 'Permission denied');
      assert.deepStrictEqual(store.getState().activeTool?.name, 'file_patch');
      assert.deepStrictEqual(store.getState().activeTool?.status, 'failed');
      assert.deepStrictEqual(store.getState().activeTool?.error, 'Permission denied');
    });

    it('updates metrics authoritatively with isReported: true', () => {
      store.updateMetrics({
        promptTokens: 1200,
        completionTokens: 350,
        totalTokens: 1550,
        costUsd: 0.025,
        latencyMs: 1450,
      });

      const metrics = store.getState().metrics;
      assert.ok(metrics);
      assert.strictEqual(metrics.isReported, true);
      assert.strictEqual(metrics.totalTokens, 1550);
      assert.strictEqual(metrics.costUsd, 0.025);
    });
  });

  describe('Daemon Timeline Event Consumption', () => {
    it('maps tool_executing, tool_completed, tool_failed from daemon events', () => {
      store.beginSubmission('studio-1', 'Task');
      store.taskSubmitted('task-10', 'studio-1');

      const toolStartEvt: TimelineEvent = {
        id: 'evt-1',
        sequenceNumber: 1,
        timestamp: Date.now(),
        type: 'tool_executing',
        summary: 'Tool bash started',
        taskId: 'task-10',
        details: { tool_name: 'bash' },
      };
      store.consumeDaemonTimelineEvent(toolStartEvt);
      assert.strictEqual(store.getState().activeTool?.name, 'bash');
      assert.strictEqual(store.getState().activeTool?.status, 'executing');

      const toolDoneEvt: TimelineEvent = {
        id: 'evt-2',
        sequenceNumber: 2,
        timestamp: Date.now(),
        type: 'tool_completed',
        summary: 'Tool bash completed',
        taskId: 'task-10',
        details: { tool_name: 'bash' },
      };
      store.consumeDaemonTimelineEvent(toolDoneEvt);
      assert.strictEqual(store.getState().activeTool?.status, 'completed');
    });

    it('maps task completion and failure events', () => {
      store.beginSubmission('studio-1', 'Task');
      store.taskSubmitted('task-10', 'studio-1');

      const taskDoneEvt: TimelineEvent = {
        id: 'evt-3',
        sequenceNumber: 3,
        timestamp: Date.now(),
        type: 'task_completed',
        summary: 'Task completed successfully',
        taskId: 'task-10',
      };
      store.consumeDaemonTimelineEvent(taskDoneEvt);
      assert.strictEqual(store.getState().status, 'succeeded');
    });

    it('captures streaming output deltas from timeline event payload', () => {
      store.beginSubmission('studio-1', 'Task');
      store.taskSubmitted('task-10', 'studio-1');

      const streamEvt: TimelineEvent = {
        id: 'evt-4',
        sequenceNumber: 4,
        timestamp: Date.now(),
        type: 'tool_executing',
        summary: 'Stream output',
        taskId: 'task-10',
        details: { output: 'cargo check passed\n' },
      };
      store.consumeDaemonTimelineEvent(streamEvt);
      assert.strictEqual(store.getState().streamOutput, 'cargo check passed\n');
    });
  });
});
