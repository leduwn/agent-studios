/**
 * Agent Studios — ExecutionClient & Task Cancellation Unit Tests
 * Milestone M15 UI-01: Interactive Agent Execution Workbench
 */

import * as assert from 'assert';
import {
  ExecutionClient,
} from '../../bridge/ipc/ExecutionClient';
import { CapabilityMatrix } from '../../bridge/ipc/CapabilityMatrix';
import {
  RustIpcCommand,
  RustIpcResponse,
} from '../../bridge/ipc/RustIpcProtocolTypes';

class MockCommandTransport {
  public sentCommands: RustIpcCommand[] = [];
  public mockResponse: RustIpcResponse = {
    type: 'task_cancelled',
    summary: {
      cancelled_tasks: ['task-123'],
      cancelled_runs: ['run-456'],
      cancelled_approvals: [],
    },
  };

  public async sendCommand(cmd: RustIpcCommand): Promise<RustIpcResponse> {
    this.sentCommands.push(cmd);
    return this.mockResponse;
  }
}

describe('M15 ExecutionClient Unit Tests', () => {
  let transport: MockCommandTransport;

  beforeEach(() => {
    transport = new MockCommandTransport();
    CapabilityMatrix.resetCapabilities();
  });

  afterEach(() => {
    CapabilityMatrix.resetCapabilities();
  });

  describe('submitTask Capability Gate', () => {
    it('fails closed when task.submit capability is disabled', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', false);

      await assert.rejects(
        async () => {
          await ExecutionClient.submitTask(transport, {
            studioId: 'studio-1',
            prompt: 'Test prompt',
            confirmedByUser: true,
          });
        },
        /task.submit capability missing/
      );
    });
  });

  describe('cancelTask Protocol & Validation', () => {
    it('fails closed when task.cancel capability is disabled', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.cancel', false);

      await assert.rejects(
        async () => {
          await ExecutionClient.cancelTask(transport, {
            studioId: 'studio-1',
            taskId: 'task-1',
          });
        },
        (err: Error) => {
          assert.strictEqual(err.name, 'TaskCancellationError');
          assert.match(err.message, /task.cancel capability missing/);
          return true;
        }
      );
    });

    it('rejects empty or whitespace-only studioId', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.cancel', true);

      await assert.rejects(
        async () => {
          await ExecutionClient.cancelTask(transport, {
            studioId: '   ',
            taskId: 'task-1',
          });
        },
        /Studio ID is required/
      );
    });

    it('rejects empty or whitespace-only taskId', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.cancel', true);

      await assert.rejects(
        async () => {
          await ExecutionClient.cancelTask(transport, {
            studioId: 'studio-1',
            taskId: '',
          });
        },
        /Task ID is required/
      );
    });

    it('dispatches valid IpcCommand::CancelTask and parses RustCancellationSummary', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.cancel', true);

      const result = await ExecutionClient.cancelTask(transport, {
        studioId: 'studio-abc',
        taskId: 'task-xyz',
        reason: 'User cancelled via UI',
      });

      assert.strictEqual(result.success, true);
      assert.strictEqual(result.taskId, 'task-xyz');
      assert.strictEqual(result.studioId, 'studio-abc');
      assert.deepStrictEqual(result.summary.cancelled_tasks, ['task-123']);
      assert.deepStrictEqual(result.summary.cancelled_runs, ['run-456']);

      assert.strictEqual(transport.sentCommands.length, 1);
      const sent = transport.sentCommands[0];
      assert.strictEqual(sent.type, 'cancel_task');
      if (sent.type === 'cancel_task') {
        assert.strictEqual(sent.studio_id, 'studio-abc');
        assert.strictEqual(sent.task_id, 'task-xyz');
        assert.strictEqual(sent.reason, 'User cancelled via UI');
      }
    });

    it('handles daemon error response variant cleanly', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.cancel', true);
      transport.mockResponse = {
        type: 'error',
        message: 'Task is already in terminal succeeded state and cannot be cancelled',
      };

      await assert.rejects(
        async () => {
          await ExecutionClient.cancelTask(transport, {
            studioId: 'studio-abc',
            taskId: 'task-xyz',
          });
        },
        /Task is already in terminal succeeded state/
      );
    });

    it('handles unexpected response variant gracefully', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.cancel', true);
      transport.mockResponse = {
        type: 'shutdown_acknowledged',
      };

      await assert.rejects(
        async () => {
          await ExecutionClient.cancelTask(transport, {
            studioId: 'studio-abc',
            taskId: 'task-xyz',
          });
        },
        /Unexpected response from daemon on cancel_task/
      );
    });
  });
});
