/**
 * Agent Studios — Submit Task IPC Adapter
 * Milestone M14 Integration Freeze & M15 UI Execution Preparation
 *
 * Implements typed submission of tasks against the authoritative Rust daemon command:
 * `IpcCommand::SubmitTask` with payload `task_spec: BatchTaskSpec`.
 *
 * Strict Architectural Invariants:
 * - Active studio identity is explicit.
 * - Prompt text and task description are preserved verbatim.
 * - Operator confirmation is required before dispatching mutating coding tasks.
 * - Task correlation uses genuine Rust-assigned UUIDs from `IpcResponse::TasksSubmitted`.
 * - Backend submission errors remain visible and are never swallowed.
 * - No fake or synthetic task success.
 * - No secrets or credentials transmitted or leaked into view state.
 */

import { NativeIpcTransport } from './NativeIpcTransport';
import {
  RustBatchTaskSpec,
  RustIpcResponse,
} from './RustIpcProtocolTypes';
import {
  CapabilityMatrix,
  TaskSubmissionPreflightRequest,
  TaskSubmissionPreflightResult,
} from './CapabilityMatrix';

export interface SubmitTaskRequest {
  studioId: string;
  prompt: string;
  confirmedByUser: boolean;
  requestedModelId?: string;
  requestedProviderId?: string;
  workspacePath?: string;
  customKey?: string;
}

export interface SubmitTaskResult {
  success: boolean;
  taskId: string;
  taskKey: string;
  studioId: string;
  limitationWarning?: string;
}

export class TaskSubmissionError extends Error {
  constructor(message: string, public readonly studioId?: string) {
    super(message);
    this.name = 'TaskSubmissionError';
  }
}

export class SubmitTaskAdapter {
  /**
   * Evaluates preflight criteria and dispatches IpcCommand::SubmitTask to the real daemon.
   */
  public static async submitTask(
    transport: NativeIpcTransport,
    req: SubmitTaskRequest
  ): Promise<SubmitTaskResult> {
    // 1. Run security and capability preflight
    const preflightReq: TaskSubmissionPreflightRequest = {
      studioId: req.studioId,
      prompt: req.prompt,
      requestedModelId: req.requestedModelId,
      requestedProviderId: req.requestedProviderId,
      confirmedByUser: req.confirmedByUser,
      workspacePath: req.workspacePath,
    };

    const preflight: TaskSubmissionPreflightResult =
      CapabilityMatrix.evaluateTaskSubmissionPreflight(preflightReq);

    if (!preflight.allowed || !preflight.sanitizedSpec) {
      throw new TaskSubmissionError(
        preflight.error || 'Task submission preflight check failed.',
        req.studioId
      );
    }

    // 2. Construct authoritative Rust BatchTaskSpec
    /* eslint-disable @typescript-eslint/naming-convention */
    const taskSpec: RustBatchTaskSpec = {
      key: req.customKey || preflight.sanitizedSpec.key,
      title: preflight.sanitizedSpec.title,
      description: preflight.sanitizedSpec.description,
      assigned_agent_id: null,
      parent_task_key: null,
      parent_task_id: null,
      dependency_keys: [],
      dependency_task_ids: [],
      dependency_output_policy: 'task_success',
    };

    // 3. Dispatch authoritative IpcCommand::SubmitTask
    const resp: RustIpcResponse = await transport.sendCommand({
      type: 'submit_task',
      studio_id: req.studioId,
      task_spec: taskSpec,
    });
    /* eslint-enable @typescript-eslint/naming-convention */

    // 4. Handle response variants
    if (resp.type === 'error') {
      throw new TaskSubmissionError(
        `Daemon rejected task submission: ${resp.message}`,
        req.studioId
      );
    }

    if (resp.type !== 'tasks_submitted') {
      throw new TaskSubmissionError(
        `Unexpected response from daemon on submit_task: ${JSON.stringify(resp)}`,
        req.studioId
      );
    }

    if (!resp.task_ids || resp.task_ids.length === 0) {
      throw new TaskSubmissionError(
        'Daemon returned empty task_ids array upon successful submission.',
        req.studioId
      );
    }

    // Correlate with authoritative Rust-assigned task ID
    const assignedTaskId = resp.task_ids[0];

    return {
      success: true,
      taskId: assignedTaskId,
      taskKey: taskSpec.key,
      studioId: req.studioId,
      limitationWarning: preflight.limitationWarning,
    };
  }
}
