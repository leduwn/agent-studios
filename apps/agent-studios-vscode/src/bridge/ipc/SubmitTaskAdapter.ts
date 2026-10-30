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
  RustIpcCommand,
  RustIpcResponse,
  RustProviderInstance,
  RustModelDescriptor,
} from './RustIpcProtocolTypes';
import {
  CapabilityMatrix,
  OperatorConfirmationChallenge,
  TaskSubmissionPreflightRequest,
  TaskSubmissionPreflightResult,
  parseDefaultModelEnv,
} from './CapabilityMatrix';
import { RustIpcCodec } from './RustIpcCodec';

export interface SubmitTaskRequest {
  studioId: string;
  prompt: string;
  confirmedByUser: boolean;
  confirmationChallenge?: OperatorConfirmationChallenge | string;
  requestedModelId?: string;
  requestedProviderId?: string;
  runtimeInstanceId?: string;
  runtimePreference?: string;
  workspacePath?: string;
  daemonRepoRoot?: string;
  daemonEffectiveModel?: string;
  daemonEffectiveProvider?: string;
  customKey?: string;
  verifyDaemon?: boolean;
  requireWorkspace?: boolean;
}

export interface VerifiedDaemonConfig {
  healthy: boolean;
  status: string;
  defaultProviderId?: string;
  defaultModelId?: string;
  availableProviders: RustProviderInstance[];
  availableModels: RustModelDescriptor[];
  daemonRepoRoot?: string;
}

export interface SubmitTaskResult {
  success: boolean;
  taskId: string;
  taskKey: string;
  studioId: string;
  limitationWarning?: string;
  displayTitle?: string;
}

export class TaskSubmissionError extends Error {
  constructor(message: string, public readonly studioId?: string) {
    super(message);
    this.name = 'TaskSubmissionError';
  }
}

export class SubmitTaskAdapter {
  /**
   * Verifies effective daemon configuration over IPC:
   * - Liveness & health via IpcCommand::GetHealth
   * - Available providers via IpcCommand::ListProviders
   * - Available models via IpcCommand::ListModels
   * - Authoritative model selection parity with connection.rs:1771-1827
   * - Authoritative repository root parity with connection.rs:1568-1574
   */
  public static async verifyDaemonConfiguration(
    transport: { sendCommand: (cmd: RustIpcCommand, timeoutMs?: number) => Promise<RustIpcResponse> } | NativeIpcTransport,
    options?: {
      explicitRepoPath?: string;
      defaultModelEnv?: string;
    }
  ): Promise<VerifiedDaemonConfig> {
    // 1. Verify health & liveness via IpcCommand::GetHealth
    const healthResp = await transport.sendCommand({ type: 'get_health' });
    if (healthResp.type === 'error') {
      throw new TaskSubmissionError(`Daemon health check failed: ${healthResp.message}`);
    }
    if (healthResp.type !== 'health' || healthResp.status !== 'healthy') {
      throw new TaskSubmissionError(
        `Daemon reported unhealthy status: ${healthResp.type === 'health' ? healthResp.status : JSON.stringify(healthResp)}`
      );
    }

    // 2. Query available providers via IpcCommand::ListProviders
    const provResp = await transport.sendCommand({ type: 'list_providers' });
    if (provResp.type === 'error') {
      throw new TaskSubmissionError(`Daemon list_providers failed: ${provResp.message}`);
    }
    if (provResp.type !== 'providers') {
      throw new TaskSubmissionError(`Unexpected response from list_providers: ${JSON.stringify(provResp)}`);
    }
    const availableProviders = provResp.providers;

    // 3. Query catalog models via IpcCommand::ListModels
    const modelResp = await transport.sendCommand({ type: 'list_models' });
    if (modelResp.type === 'error') {
      throw new TaskSubmissionError(`Daemon list_models failed: ${modelResp.message}`);
    }
    if (modelResp.type !== 'models') {
      throw new TaskSubmissionError(`Unexpected response from list_models: ${JSON.stringify(modelResp)}`);
    }
    const availableModels = modelResp.models;

    // 4. Authoritative daemon model resolution parity (agent-studios-rs/connection.rs lines 1771-1827)
    let defaultModelId: string | undefined;
    let defaultProviderId: string | undefined;

    const parsedEnv = parseDefaultModelEnv(options?.defaultModelEnv);
    if (parsedEnv) {
      // Find model in catalog
      const matched = availableModels.find((m) => {
        if (parsedEnv.providerId) {
          return m.provider_instance_id === parsedEnv.providerId && m.id === parsedEnv.modelId;
        }
        return m.id === parsedEnv.modelId;
      });

      if (!matched) {
        throw new TaskSubmissionError(
          `Configured default model '${parsedEnv.providerId ? `${parsedEnv.providerId}/${parsedEnv.modelId}` : parsedEnv.modelId}' not found in daemon provider catalog (parity: connection.rs:1788).`
        );
      }
      defaultModelId = matched.id;
      defaultProviderId = matched.provider_instance_id;
    } else {
      if (availableModels.length === 0) {
        // Catalog empty (test fixture backward compatibility)
        defaultModelId = undefined;
        defaultProviderId = undefined;
      } else if (availableModels.length === 1) {
        // Unambiguous single-model catalog (parity: connection.rs:1806)
        defaultModelId = availableModels[0].id;
        defaultProviderId = availableModels[0].provider_instance_id;
      } else {
        // Multiple models in catalog without explicit AGENT_STUDIOS_DEFAULT_MODEL
        // Backend agsd fails closed at connection.rs line 1809
        throw new TaskSubmissionError(
          `Multiple models available (${availableModels.length}) in daemon catalog, but no explicit model route configured. Set AGENT_STUDIOS_DEFAULT_MODEL (parity: connection.rs:1816).`
        );
      }
    }

    const daemonRepoRoot = options?.explicitRepoPath || process.env.AGENT_STUDIOS_REPO_PATH;

    return {
      healthy: true,
      status: healthResp.status,
      defaultProviderId,
      defaultModelId,
      availableProviders,
      availableModels,
      daemonRepoRoot,
    };
  }

  /**
   * Evaluates preflight criteria and dispatches IpcCommand::SubmitTask to the real daemon.
   */
  public static async submitTask(
    transport: { sendCommand: (cmd: RustIpcCommand, timeoutMs?: number) => Promise<RustIpcResponse> } | NativeIpcTransport,
    req: SubmitTaskRequest
  ): Promise<SubmitTaskResult> {
    // 0. Dynamic daemon configuration verification if requested
    let effectiveRepo = req.daemonRepoRoot;
    let effectiveModel = req.daemonEffectiveModel;
    let effectiveProvider = req.daemonEffectiveProvider;

    if (req.verifyDaemon) {
      const verified = await this.verifyDaemonConfiguration(transport, {
        explicitRepoPath: req.daemonRepoRoot,
        defaultModelEnv: req.daemonEffectiveModel,
      });
      effectiveRepo = verified.daemonRepoRoot ?? effectiveRepo;
      effectiveModel = verified.defaultModelId ?? effectiveModel;
      effectiveProvider = verified.defaultProviderId ?? effectiveProvider;
    }

    // 1. Run security and capability preflight
    const preflightReq: TaskSubmissionPreflightRequest = {
      studioId: req.studioId,
      prompt: req.prompt,
      requestedModelId: req.requestedModelId,
      requestedProviderId: req.requestedProviderId,
      confirmedByUser: req.confirmedByUser,
      confirmationChallenge: req.confirmationChallenge,
      consumeChallenge: true, // Atomically consume challenge at dispatch boundary
      workspacePath: req.workspacePath,
      daemonRepoRoot: effectiveRepo,
      daemonEffectiveModel: effectiveModel,
      daemonEffectiveProvider: effectiveProvider,
      customKey: req.customKey,
      requireWorkspace: req.requireWorkspace ?? (effectiveRepo !== undefined),
      requireAuthoritativeCheck: true,
      requireChallenge: true,
    };

    const preflight: TaskSubmissionPreflightResult =
      CapabilityMatrix.evaluateTaskSubmissionPreflight(preflightReq);

    if (!preflight.allowed || !preflight.sanitizedSpec) {
      throw new TaskSubmissionError(
        preflight.error || 'Task submission preflight check failed.',
        req.studioId
      );
    }

    // 2. Construct authoritative Rust BatchTaskSpec with execution spec (M15)
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
      execution: {
        initial_prompt: req.prompt,
        repository_path: req.workspacePath ?? null,
        runtime_instance_id: req.runtimeInstanceId ?? null,
        runtime_preference: req.runtimePreference ?? null,
        provider_instance_id: req.requestedProviderId ?? effectiveProvider ?? null,
        model_id: req.requestedModelId ?? effectiveModel ?? null,
      },
    };

    const command: RustIpcCommand = {
      type: 'submit_task',
      studio_id: req.studioId,
      task_spec: taskSpec,
    };
    /* eslint-enable @typescript-eslint/naming-convention */

    // 3. Validate wire frame serialization & bounded frame size limit before dispatch
    try {
      RustIpcCodec.encodeCommand(command);
    } catch (err: unknown) {
      throw new TaskSubmissionError(
        `Wire frame encoding failed: ${(err as Error).message}`,
        req.studioId
      );
    }

    // 4. Dispatch authoritative IpcCommand::SubmitTask
    const resp: RustIpcResponse = await transport.sendCommand(command);
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
      displayTitle: preflight.sanitizedSpec.displayTitle,
    };
  }
}
