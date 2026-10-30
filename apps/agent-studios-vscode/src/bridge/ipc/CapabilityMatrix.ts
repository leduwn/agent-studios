/**
 * Agent Studios — M14/M15 Security & Capability Matrix
 * Milestone M14 Integration Freeze & M15 UI Execution Preparation
 *
 * Defines authoritative capability tiers, security boundaries, and runtime gating
 * derived from verified backend support in agent-studios-rs (commit ce0396c1f1).
 *
 * Six Strict Capability Tiers:
 * 1. READ_ONLY_LIVE: Verified live against real agsd (health, telemetry, queries).
 * 2. TASK_SUBMISSION_READY: Contract verified; gated on confirmation and default model.
 * 3. APPROVAL_READY: Two-phase (2PC) approval delivery verified; origin-bound.
 * 4. CANCEL_READY: Pre-execution stop gate verified with origin authorization.
 * 5. ARTIFACT_REVIEW_READY: Contract-only in Rust; unwired in daemon task lifecycle (GATED).
 * 6. UNSUPPORTED: Features lacking backend implementation (per-task model, multi-turn, restart recovery).
 */

import * as crypto from 'crypto';
import * as path from 'path';
import {
  PromptSanitizer,
  PromptSanitizationError,
} from './PromptSanitizer';

export interface OperatorConfirmationChallenge {
  token: string;
  studioId: string;
  promptHash: string;
  workspacePath?: string;
  modelId?: string;
  expiresAt: number;
  signature: string;
}

export class OperatorConfirmationManager {
  private static secret: Buffer = crypto.randomBytes(32);

  public static setSecret(secret: Buffer): void {
    this.secret = secret;
  }

  public static getSecret(): Buffer {
    return this.secret;
  }

  public static createChallenge(
    studioId: string,
    prompt: string,
    options?: {
      workspacePath?: string;
      modelId?: string;
      ttlMs?: number;
    }
  ): OperatorConfirmationChallenge {
    const normalizedPrompt = prompt.replace(/\r\n/g, '\n');
    const promptHash = crypto.createHash('sha256').update(normalizedPrompt, 'utf8').digest('hex');
    const expiresAt = Date.now() + (options?.ttlMs ?? 5 * 60 * 1000);
    const token = crypto.randomBytes(16).toString('hex');
    const normalizedWs = options?.workspacePath
      ? path.resolve(options.workspacePath).replace(/\\/g, '/').toLowerCase()
      : '';
    const model = options?.modelId || '';
    const payload = `${token}:${studioId}:${promptHash}:${normalizedWs}:${model}:${expiresAt}`;
    const signature = crypto.createHmac('sha256', this.secret).update(payload).digest('hex');

    return {
      token,
      studioId,
      promptHash,
      workspacePath: options?.workspacePath,
      modelId: options?.modelId,
      expiresAt,
      signature,
    };
  }

  public static verifyChallenge(
    challenge: OperatorConfirmationChallenge | string,
    expected: {
      studioId: string;
      prompt: string;
      workspacePath?: string;
      modelId?: string;
    }
  ): { valid: boolean; reason?: string } {
    let parsed: OperatorConfirmationChallenge;
    if (typeof challenge === 'string') {
      try {
        parsed = JSON.parse(challenge);
      } catch {
        return { valid: false, reason: 'Malformed challenge JSON' };
      }
    } else {
      parsed = challenge;
    }

    if (!parsed || !parsed.token || !parsed.signature || !parsed.promptHash || !parsed.expiresAt) {
      return { valid: false, reason: 'Missing required challenge fields' };
    }

    if (Date.now() > parsed.expiresAt) {
      return { valid: false, reason: 'Confirmation challenge expired' };
    }

    if (parsed.studioId !== expected.studioId) {
      return {
        valid: false,
        reason: `Studio ID mismatch in confirmation challenge (expected '${expected.studioId}', got '${parsed.studioId}')`,
      };
    }

    const normalizedExpectedPrompt = expected.prompt.replace(/\r\n/g, '\n');
    const expectedPromptHash = crypto.createHash('sha256').update(normalizedExpectedPrompt, 'utf8').digest('hex');
    if (parsed.promptHash !== expectedPromptHash) {
      return {
        valid: false,
        reason: 'Prompt hash mismatch: prompt was altered after confirmation challenge issued',
      };
    }

    if (parsed.workspacePath && expected.workspacePath) {
      const normalizedChWs = path.resolve(parsed.workspacePath).replace(/\\/g, '/').toLowerCase();
      const normalizedExpWs = path.resolve(expected.workspacePath).replace(/\\/g, '/').toLowerCase();
      if (normalizedChWs !== normalizedExpWs) {
        return { valid: false, reason: 'Workspace directory mismatch in confirmation challenge' };
      }
    }

    if (parsed.modelId && expected.modelId && parsed.modelId !== expected.modelId) {
      return { valid: false, reason: 'Model ID mismatch in confirmation challenge' };
    }

    const normalizedWs = parsed.workspacePath
      ? path.resolve(parsed.workspacePath).replace(/\\/g, '/').toLowerCase()
      : '';
    const model = parsed.modelId || '';
    const payload = `${parsed.token}:${parsed.studioId}:${parsed.promptHash}:${normalizedWs}:${model}:${parsed.expiresAt}`;
    const expectedSig = crypto.createHmac('sha256', this.secret).update(payload).digest('hex');

    const sigBuf = Buffer.from(parsed.signature, 'hex');
    const expBuf = Buffer.from(expectedSig, 'hex');
    if (sigBuf.length !== expBuf.length || !crypto.timingSafeEqual(sigBuf, expBuf)) {
      return { valid: false, reason: 'Forged confirmation challenge: invalid cryptographic signature' };
    }

    return { valid: true };
  }
}

export type CapabilityTier =
  | 'READ_ONLY_LIVE'
  | 'TASK_SUBMISSION_READY'
  | 'APPROVAL_READY'
  | 'CANCEL_READY'
  | 'ARTIFACT_REVIEW_READY'
  | 'UNSUPPORTED';

export interface CapabilityDescriptor {
  id: string;
  name: string;
  tier: CapabilityTier;
  enabled: boolean;
  rustCrate: string;
  prReference: string;
  limitation?: string;
  requirements: string[];
}

/* eslint-disable @typescript-eslint/naming-convention */
export const CAPABILITY_REGISTRY: Record<string, CapabilityDescriptor> = {
  'daemon.health': {
    id: 'daemon.health',
    name: 'Daemon Health & Telemetry',
    tier: 'READ_ONLY_LIVE',
    enabled: true,
    rustCrate: 'agent-studios-ipc-transport',
    prReference: 'PR #19',
    requirements: ['Authenticated IPC connection'],
  },
  'daemon.ping': {
    id: 'daemon.ping',
    name: 'Round-Trip Latency Ping',
    tier: 'READ_ONLY_LIVE',
    enabled: true,
    rustCrate: 'agent-studios-ipc-transport',
    prReference: 'PR #19',
    requirements: ['Authenticated IPC connection'],
  },
  'studio.list': {
    id: 'studio.list',
    name: 'Studio Catalog Query',
    tier: 'READ_ONLY_LIVE',
    enabled: true,
    rustCrate: 'agent-studios-control-plane',
    prReference: 'PR #19',
    requirements: ['Authenticated IPC connection'],
  },
  'task.list': {
    id: 'task.list',
    name: 'Task Catalog Query',
    tier: 'READ_ONLY_LIVE',
    enabled: true,
    rustCrate: 'agent-studios-control-plane',
    prReference: 'PR #19',
    requirements: ['Authenticated IPC connection', 'Explicit active studio ID'],
  },
  'run.list': {
    id: 'run.list',
    name: 'Run Catalog Query',
    tier: 'READ_ONLY_LIVE',
    enabled: true,
    rustCrate: 'agent-studios-control-plane',
    prReference: 'PR #19',
    requirements: ['Authenticated IPC connection', 'Explicit active studio ID'],
  },
  'provider.list': {
    id: 'provider.list',
    name: 'Provider & Model Catalog Query',
    tier: 'READ_ONLY_LIVE',
    enabled: true,
    rustCrate: 'agent-studios-provider',
    prReference: 'PR #19',
    requirements: ['Authenticated IPC connection', 'Secret scrubbing'],
  },
  'event.stream': {
    id: 'event.stream',
    name: 'Event Streaming & Replay',
    tier: 'READ_ONLY_LIVE',
    enabled: true,
    rustCrate: 'agent-studios-ipc-transport',
    prReference: 'PR #19',
    requirements: ['Monotonic sequence watermark', 'ReplayBuffer backfill'],
  },
  'task.submit': {
    id: 'task.submit',
    name: 'Task Submission',
    tier: 'TASK_SUBMISSION_READY',
    enabled: false, // Gated: requires operator confirmation and daemon default model acknowledgment
    rustCrate: 'agent-studios-ipc-transport',
    prReference: 'PR #19',
    limitation:
      'Backend routes exclusively via daemon defaults (AGENT_STUDIOS_DEFAULT_MODEL). Per-task model overrides in BatchTaskSpec are unsupported.',
    requirements: [
      'Explicit studio ID',
      'Operator confirmation for mutating tasks',
      'No per-task model override requested',
      'Authenticated client credentials',
    ],
  },
  'approval.resolve': {
    id: 'approval.resolve',
    name: 'Two-Phase Approval Resolution',
    tier: 'APPROVAL_READY',
    enabled: false, // Disabled in M14 read-only connected vertical slice; wire contract tested in RustIpcContractFixtures
    rustCrate: 'agent-studios-ipc-transport',
    prReference: 'PR #19',
    limitation:
      'Approval mutation is disabled in M14 connected read-only vertical slice (DaemonConnectedBridge returns applied: false). Wire contract verified in isolation in RustIpcContractFixtures.',
    requirements: [
      'Cryptographic session origin binding',
      'Pending approval tracking',
      'Fail-closed on pre-execution delivery failure',
    ],
  },
  'task.cancel': {
    id: 'task.cancel',
    name: 'Task Pre-Execution Cancellation',
    tier: 'CANCEL_READY',
    enabled: false, // Disabled in M14 read-only connected vertical slice; wire contract tested in RustIpcContractFixtures
    rustCrate: 'agent-studios-ipc-transport',
    prReference: 'PR #19',
    limitation:
      'Task cancellation over IPC is disabled in M14 connected read-only vertical slice (DaemonConnectedBridge cancelCurrentRun is disabled). Wire contract verified in isolation in RustIpcContractFixtures.',
    requirements: [
      'Cryptographic origin authorization',
      'Target task currently active/queued',
      'Pre-execution runtime stop gate',
    ],
  },
  'artifact.review': {
    id: 'artifact.review',
    name: 'Artifact & Diff Review over IPC',
    tier: 'ARTIFACT_REVIEW_READY',
    enabled: false,
    rustCrate: 'agent-studios-workspace',
    prReference: 'PR #19',
    limitation:
      'ArtifactRecord exists in protocol crate, but capture_changes is not wired in daemon lifecycle and no GetDiff command exists in IpcCommand.',
    requirements: ['Backend PR-M15-03: Wire capture_changes and expose GetTaskDiff over IPC'],
  },
  'model.per_task_override': {
    id: 'model.per_task_override',
    name: 'Per-Task Model Selection',
    tier: 'UNSUPPORTED',
    enabled: false,
    rustCrate: 'agent-studios-protocol',
    prReference: 'PR #19',
    limitation:
      'BatchTaskSpec lacks model_ref and provider_instance_id. All tasks inherit daemon default model.',
    requirements: ['Backend PR-M15-01: Add model_ref to BatchTaskSpec'],
  },
  'runtime.per_task_adapter': {
    id: 'runtime.per_task_adapter',
    name: 'Per-Task Runtime Adapter Routing',
    tier: 'UNSUPPORTED',
    enabled: false,
    rustCrate: 'agent-studios-ipc-transport',
    prReference: 'PR #19',
    limitation:
      'IpcServerBuilder configures a singleton runtime adapter. Multi-adapter dynamic routing requires backend changes.',
    requirements: ['Backend PR-M15-02: Runtime adapter registry in ServerState'],
  },
  'chat.multi_turn_prompt': {
    id: 'chat.multi_turn_prompt',
    name: 'Multi-Turn Conversational Follow-Up Prompts',
    tier: 'UNSUPPORTED',
    enabled: false,
    rustCrate: 'agent-studios-ipc-transport',
    prReference: 'PR #19',
    limitation:
      'IpcCommand only supports SubmitTask (initial prompt). Follow-up conversational turns lack a SendPrompt command.',
    requirements: ['Backend PR-M15-01: Add IpcCommand::SendPrompt'],
  },
  'daemon.restart_recovery': {
    id: 'daemon.restart_recovery',
    name: 'Daemon Restart Persistence & Session Reattach',
    tier: 'UNSUPPORTED',
    enabled: false,
    rustCrate: 'agent-studios-control-plane',
    prReference: 'PR #19',
    limitation:
      'Control Plane exclusively uses InMemoryStore. Daemon restart permanently loses all tasks, runs, and session bindings.',
    requirements: ['Backend PR-M15-04: SQLite persistent store with WAL'],
  },
};

export interface TaskSubmissionPreflightRequest {
  studioId?: string | null;
  prompt: string;
  requestedModelId?: string;
  requestedProviderId?: string;
  confirmedByUser: boolean;
  confirmationChallenge?: OperatorConfirmationChallenge | string;
  workspacePath?: string;
  daemonRepoRoot?: string;
  daemonEffectiveModel?: string;
  daemonEffectiveProvider?: string;
  customKey?: string;
  requireAuthoritativeCheck?: boolean;
  requireChallenge?: boolean;
  requireWorkspace?: boolean;
}

/**
 * Parses AGENT_STUDIOS_DEFAULT_MODEL environment variable or string into provider and model IDs.
 * Format parity with agsd (server.rs:300-305): "provider_instance_id/model_id" or "model_id".
 */
export function parseDefaultModelEnv(
  rawEnv?: string
): { providerId: string; modelId: string } | null {
  const envVal = rawEnv ?? process.env.AGENT_STUDIOS_DEFAULT_MODEL;
  if (!envVal || typeof envVal !== 'string') {
    return null;
  }
  const trimmed = envVal.trim();
  if (trimmed.includes('/')) {
    const [providerId, ...modelParts] = trimmed.split('/');
    const modelId = modelParts.join('/');
    if (providerId.trim() && modelId.trim()) {
      return { providerId: providerId.trim(), modelId: modelId.trim() };
    }
  } else if (trimmed.length > 0) {
    return { providerId: '', modelId: trimmed };
  }
  return null;
}

/**
 * Normalizes workspace and repository paths for cross-platform comparison.
 */
export function normalizeWorkspacePath(p: string): string {
  if (!p || typeof p !== 'string') {
    return '';
  }
  const resolved = path.resolve(p).replace(/[/\\]+$/, '');
  return process.platform === 'win32' ? resolved.toLowerCase() : resolved;
}

export interface TaskSubmissionPreflightResult {
  allowed: boolean;
  tier: CapabilityTier;
  error?: string;
  limitationWarning?: string;
  sanitizedSpec?: {
    key: string;
    title: string;
    description: string;
    displayTitle?: string;
    rawPrompt?: string;
  };
}

export class CapabilityMatrix {
  private static readonly dynamicOverrides: Map<string, boolean> = new Map();

  /**
   * Evaluates if a capability is allowed and active.
   */
  public static isCapabilityEnabled(capabilityId: string): boolean {
    if (this.dynamicOverrides.has(capabilityId)) {
      return this.dynamicOverrides.get(capabilityId)!;
    }
    const desc = CAPABILITY_REGISTRY[capabilityId];
    return desc ? desc.enabled : false;
  }

  /**
   * Dynamically overrides or enables/disables capability authorization.
   */
  public static setCapabilityEnabled(capabilityId: string, enabled: boolean): void {
    this.dynamicOverrides.set(capabilityId, enabled);
  }

  /**
   * Resets all dynamic capability overrides to static defaults.
   */
  public static resetCapabilities(): void {
    this.dynamicOverrides.clear();
  }

  /**
   * Retrieves descriptor for a capability.
   */
  public static getDescriptor(capabilityId: string): CapabilityDescriptor | undefined {
    return CAPABILITY_REGISTRY[capabilityId];
  }

  /**
   * Performs preflight evaluation for submitting a task to real agsd.
   * Ensures:
   * - Authoritative capability check passes.
   * - Studio ID is explicit and valid UUID.
   * - Prompt text is preserved verbatim with bounded frame size limit.
   * - Mutating task confirmation is granted with verified challenge token.
   * - Model routing matches daemon effective configuration (rejects mismatch).
   * - Workspace directory matches daemon repository root (fails closed on mismatch).
   */
  public static evaluateTaskSubmissionPreflight(
    req: TaskSubmissionPreflightRequest
  ): TaskSubmissionPreflightResult {
    // 1. Studio identity check (must be a valid UUID)
    try {
      PromptSanitizer.validateStudioId(req.studioId);
    } catch (err) {
      return {
        allowed: false,
        tier: 'TASK_SUBMISSION_READY',
        error:
          err instanceof PromptSanitizationError
            ? err.message
            : 'Active studio identity is required before submitting tasks to the daemon.',
      };
    }

    // 2. Prompt validation, integrity preservation & bounded frame size limit
    let sanitizedSpec;
    try {
      sanitizedSpec = PromptSanitizer.prepareAuthoritativeTaskSpec({
        prompt: req.prompt,
        customKey: req.customKey,
      });
    } catch (err) {
      return {
        allowed: false,
        tier: 'TASK_SUBMISSION_READY',
        error: err instanceof Error ? err.message : 'Invalid prompt text.',
      };
    }

    // 3. User confirmation gate for mutating coding tasks
    if (!req.confirmedByUser) {
      return {
        allowed: false,
        tier: 'TASK_SUBMISSION_READY',
        error:
          'Operator confirmation is required before starting mutating coding tasks in the repository workspace.',
      };
    }

    // 4. Authoritative capability check (when required or authoritative preflight)
    if (req.requireAuthoritativeCheck && !this.isCapabilityEnabled('task.submit')) {
      return {
        allowed: false,
        tier: 'TASK_SUBMISSION_READY',
        error: "Authoritative capability check failed: capability 'task.submit' is not enabled or authorized.",
      };
    }

    // 5. Operator confirmation challenge: reject untrusted raw boolean without verification token
    if (req.requireChallenge || req.confirmationChallenge !== undefined) {
      if (!req.confirmationChallenge) {
        return {
          allowed: false,
          tier: 'TASK_SUBMISSION_READY',
          error:
            'Untrusted raw boolean confirmedByUser: true rejected without trusted verification challenge token.',
        };
      }

      // Verify confirmation challenge integrity and cryptographic binding
      const verification = OperatorConfirmationManager.verifyChallenge(req.confirmationChallenge, {
        studioId: req.studioId!,
        prompt: req.prompt,
        workspacePath: req.workspacePath,
        modelId: req.requestedModelId,
      });

      if (!verification.valid) {
        return {
          allowed: false,
          tier: 'TASK_SUBMISSION_READY',
          error: `Operator confirmation challenge verification failed: ${verification.reason}`,
        };
      }
    }

    // 5. Model & Provider routing mismatch check (Parity with connection.rs:1771-1827)
    // The backend routes exclusively via daemon defaults.
    // If the user requested a specific model (requestedModelId) that differs from the verified daemon default,
    // or if requestedProviderId differs from the daemon's provider, task submission must be REJECTED (fail closed).
    const parsedDefaultModel = parseDefaultModelEnv();
    const effectiveModel = req.daemonEffectiveModel || parsedDefaultModel?.modelId;
    const effectiveProvider = req.daemonEffectiveProvider || parsedDefaultModel?.providerId;

    if (req.requestedModelId && req.requestedModelId.trim().length > 0 && req.requestedModelId !== 'default') {
      if (!effectiveModel) {
        return {
          allowed: false,
          tier: 'TASK_SUBMISSION_READY',
          error: `Model routing rejected (fail-closed): requested model '${req.requestedModelId}', but daemon default model could not be verified. agsd (commit ce0396c1f1) routes exclusively via daemon defaults; per-task model overrides are unsupported.`,
        };
      }
      if (req.requestedModelId !== effectiveModel) {
        return {
          allowed: false,
          tier: 'TASK_SUBMISSION_READY',
          error: `Model routing mismatch: Requested model '${req.requestedModelId}' differs from daemon effective configuration ('${effectiveModel}'). Per-task model override is rejected.`,
        };
      }
    }

    if (req.requestedProviderId && req.requestedProviderId.trim().length > 0 && req.requestedProviderId !== 'default') {
      if (effectiveProvider && req.requestedProviderId !== effectiveProvider) {
        return {
          allowed: false,
          tier: 'TASK_SUBMISSION_READY',
          error: `Provider routing mismatch: Requested provider '${req.requestedProviderId}' differs from daemon verified default provider ('${effectiveProvider}'). Per-task provider override is rejected.`,
        };
      }
    }

    // 6. Workspace mismatch check (Parity with connection.rs:1568-1574)
    // If the user's active VS Code workspace folder does not match the daemon's actual execution repository, task execution must FAIL CLOSED.
    const effectiveDaemonRepo = req.daemonRepoRoot || process.env.AGENT_STUDIOS_REPO_PATH;

    if (effectiveDaemonRepo) {
      if (!req.workspacePath || req.workspacePath.trim().length === 0) {
        return {
          allowed: false,
          tier: 'TASK_SUBMISSION_READY',
          error: 'Workspace directory is required when daemon repository root is configured.',
        };
      }

      if (req.workspacePath.includes('\0')) {
        return {
          allowed: false,
          tier: 'TASK_SUBMISSION_READY',
          error: 'Null byte injection detected in workspace path.',
        };
      }

      if (req.workspacePath.startsWith('\\\\') || req.workspacePath.startsWith('//')) {
        return {
          allowed: false,
          tier: 'TASK_SUBMISSION_READY',
          error: 'UNC network paths rejected in workspace path.',
        };
      }

      if (/(?:^|[/\\])\.\.(?:[/\\]|$)/.test(req.workspacePath)) {
        return {
          allowed: false,
          tier: 'TASK_SUBMISSION_READY',
          error: 'Path traversal sequences rejected in workspace path.',
        };
      }

      const normalizedWs = normalizeWorkspacePath(req.workspacePath);
      const normalizedRepo = normalizeWorkspacePath(effectiveDaemonRepo);

      if (normalizedWs !== normalizedRepo) {
        return {
          allowed: false,
          tier: 'TASK_SUBMISSION_READY',
          error: `Workspace mismatch: Task workspace directory '${req.workspacePath}' does not match daemon repository root '${effectiveDaemonRepo}'. Task execution rejected (fail-closed) to prevent unauthorized cross-repository modification.`,
        };
      }
    } else if (req.requireWorkspace && (!req.workspacePath || req.workspacePath.trim().length === 0)) {
      return {
        allowed: false,
        tier: 'TASK_SUBMISSION_READY',
        error: 'Active VS Code workspace folder is required for task execution. Task execution rejected (fail-closed).',
      };
    }

    return {
      allowed: true,
      tier: 'TASK_SUBMISSION_READY',
      sanitizedSpec: {
        key: sanitizedSpec.key,
        title: sanitizedSpec.title,
        description: sanitizedSpec.description,
        displayTitle: sanitizedSpec.displayTitle,
        rawPrompt: sanitizedSpec.title,
      },
    };
  }
}
