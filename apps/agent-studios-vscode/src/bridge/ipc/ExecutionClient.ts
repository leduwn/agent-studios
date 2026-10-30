/**
 * Agent Studios — Typed IPC Execution Client
 * Milestone M15 UI-01: Interactive Agent Execution Workbench
 *
 * Implements typed submission and cancellation against the authoritative Rust daemon commands:
 * - `IpcCommand::SubmitTask { studio_id, task_spec }`
 * - `IpcCommand::CancelTask { studio_id, task_id, reason }`
 *
 * Strict Architectural Invariants:
 * - Fail-closed capability checking before wire dispatch (task.submit, task.cancel).
 * - Exact wire framing parity with backend (connection.rs:1758-1769).
 * - Multi-line Markdown preserved verbatim in title, description empty, frame size < 4 MiB.
 * - Single-use cryptographic operator confirmation token required for submission.
 * - Zero simulated task success, zero synthetic cancellation state.
 */

import { NativeIpcTransport } from './NativeIpcTransport';
import {
  RustCancellationSummary,
  RustIpcCommand,
  RustIpcResponse,
} from './RustIpcProtocolTypes';
import { CapabilityMatrix } from './CapabilityMatrix';
import { RustIpcCodec } from './RustIpcCodec';
import {
  SubmitTaskAdapter,
  SubmitTaskRequest,
  SubmitTaskResult,
  TaskSubmissionError,
  VerifiedDaemonConfig,
} from './SubmitTaskAdapter';

export { SubmitTaskRequest, SubmitTaskResult, TaskSubmissionError, VerifiedDaemonConfig };

export interface CancelTaskRequest {
  studioId: string;
  taskId: string;
  reason?: string;
}

export interface CancelTaskResult {
  success: boolean;
  taskId: string;
  studioId: string;
  summary: RustCancellationSummary;
}

export class TaskCancellationError extends Error {
  constructor(message: string, public readonly studioId?: string, public readonly taskId?: string) {
    super(message);
    this.name = 'TaskCancellationError';
  }
}

export type IpcCommandTransport =
  | { sendCommand: (cmd: RustIpcCommand, timeoutMs?: number) => Promise<RustIpcResponse> }
  | NativeIpcTransport;

export class ExecutionClient {
  /**
   * Verifies effective daemon configuration over IPC.
   */
  public static async verifyDaemonConfiguration(
    transport: IpcCommandTransport,
    options?: {
      explicitRepoPath?: string;
      defaultModelEnv?: string;
    }
  ): Promise<VerifiedDaemonConfig> {
    return SubmitTaskAdapter.verifyDaemonConfiguration(transport, options);
  }

  /**
   * Submits a task to the daemon with preflight capability gate validation.
   */
  public static async submitTask(
    transport: IpcCommandTransport,
    req: SubmitTaskRequest
  ): Promise<SubmitTaskResult> {
    // 1. Fail-closed capability check
    if (!CapabilityMatrix.isCapabilityEnabled('task.submit')) {
      throw new TaskSubmissionError(
        'Task submission is disabled by capability matrix gate (task.submit capability missing).',
        req.studioId
      );
    }

    // 2. Delegate to authoritative SubmitTaskAdapter
    return SubmitTaskAdapter.submitTask(transport, req);
  }

  /**
   * Cancels an active task on the daemon with preflight capability gate validation.
   */
  public static async cancelTask(
    transport: IpcCommandTransport,
    req: CancelTaskRequest
  ): Promise<CancelTaskResult> {
    // 1. Fail-closed capability check
    if (!CapabilityMatrix.isCapabilityEnabled('task.cancel')) {
      throw new TaskCancellationError(
        'Task cancellation is disabled by capability matrix gate (task.cancel capability missing).',
        req.studioId,
        req.taskId
      );
    }

    // 2. Parameter presence validation
    if (!req.studioId || typeof req.studioId !== 'string' || req.studioId.trim().length === 0) {
      throw new TaskCancellationError('Studio ID is required for task cancellation.');
    }
    if (!req.taskId || typeof req.taskId !== 'string' || req.taskId.trim().length === 0) {
      throw new TaskCancellationError('Task ID is required for task cancellation.', req.studioId);
    }

    const trimmedStudioId = req.studioId.trim();
    const trimmedTaskId = req.taskId.trim();
    const reason = req.reason?.trim() || null;

    // 3. Construct authoritative Rust IpcCommand::CancelTask
    /* eslint-disable @typescript-eslint/naming-convention */
    const command: RustIpcCommand = {
      type: 'cancel_task',
      studio_id: trimmedStudioId,
      task_id: trimmedTaskId,
      reason,
    };
    /* eslint-enable @typescript-eslint/naming-convention */

    // 4. Validate wire framing serialization & frame size boundary (< 4 MiB)
    try {
      RustIpcCodec.encodeCommand(command);
    } catch (err: unknown) {
      throw new TaskCancellationError(
        `Wire frame encoding failed for cancel_task: ${(err as Error).message}`,
        trimmedStudioId,
        trimmedTaskId
      );
    }

    // 5. Dispatch command over transport
    const resp: RustIpcResponse = await transport.sendCommand(command);

    // 6. Handle response variants
    if (resp.type === 'error') {
      throw new TaskCancellationError(
        `Daemon rejected task cancellation: ${resp.message}`,
        trimmedStudioId,
        trimmedTaskId
      );
    }

    if (resp.type !== 'task_cancelled') {
      throw new TaskCancellationError(
        `Unexpected response from daemon on cancel_task: ${JSON.stringify(resp)}`,
        trimmedStudioId,
        trimmedTaskId
      );
    }

    return {
      success: true,
      taskId: trimmedTaskId,
      studioId: trimmedStudioId,
      summary: resp.summary,
    };
  }
}
