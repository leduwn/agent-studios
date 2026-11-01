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
import * as fs from 'fs';
import {
  PromptSanitizer,
  PromptSanitizationError,
} from './PromptSanitizer';
import { RustExecutionReadinessReport } from './RustIpcProtocolTypes';

export interface RuntimeValidationResult {
  valid: boolean;
  runtimePreference?: string;
  runtimeInstanceId?: string;
  reason?: string;
}

export interface RuntimeValidationOptions {
  action?: 'task_execution' | 'task_cancellation' | 'read_only';
}

export interface OperatorConfirmationChallenge {
  token: string;
  studioId: string;
  promptHash: string;
  workspacePath?: string;
  modelId?: string;
  providerId?: string;
  runtimePreference?: string;
  runtimeInstanceId?: string;
  intent?: string;
  expiresAt: number;
  signature: string;
}

interface TrackedConfirmation {
  token: string;
  studioId: string;
  promptHash: string;
  workspacePath?: string;
  modelId?: string;
  providerId?: string;
  runtimePreference?: string;
  runtimeInstanceId?: string;
  intent: string;
  issuedAt: number;
  expiresAt: number;
  confirmed: boolean;
  consumed: boolean;
  consumedAt?: number;
}

export class OperatorConfirmationManager {
  private static secret: Buffer = crypto.randomBytes(32);
  private static readonly activeChallenges: Map<string, TrackedConfirmation> = new Map();

  /**
   * Resets secret and clears active challenges for unit test hermeticity.
   */
  public static resetForTesting(): void {
    this.secret = crypto.randomBytes(32);
    this.activeChallenges.clear();
  }

  /**
   * Returns number of active pending challenges in Extension Host memory.
   */
  public static getActiveChallengesCountForTesting(): number {
    return this.activeChallenges.size;
  }

  /**
   * Generates a signed challenge using an explicit foreign secret to test signature verification failure.
   */
  public static createChallengeWithForeignSecretForTesting(
    studioId: string,
    prompt: string,
    foreignSecret: Buffer,
    options?: {
      workspacePath?: string;
      modelId?: string;
      providerId?: string;
      runtimePreference?: string;
      runtimeInstanceId?: string;
      intent?: string;
      ttlMs?: number;
    }
  ): OperatorConfirmationChallenge {
    const normalizedPrompt = prompt.replace(/\r\n/g, '\n');
    const promptHash = crypto.createHash('sha256').update(normalizedPrompt, 'utf8').digest('hex');
    const expiresAt = Date.now() + (options?.ttlMs ?? 5 * 60 * 1000);
    const token = crypto.randomBytes(16).toString('hex');
    const intent = options?.intent ?? 'task_execution';
    const normalizedWs = options?.workspacePath ? normalizeWorkspacePath(options.workspacePath) : '';
    const model = options?.modelId || '';
    const provider = options?.providerId || '';
    const runtimePref = options?.runtimePreference || '';
    const runtimeInst = options?.runtimeInstanceId || '';
    const payload = `${token}:${studioId}:${promptHash}:${normalizedWs}:${model}:${provider}:${runtimePref}:${runtimeInst}:${intent}:${expiresAt}`;
    const signature = crypto.createHmac('sha256', foreignSecret).update(payload, 'utf8').digest('hex');

    const challenge: OperatorConfirmationChallenge = {
      token,
      studioId,
      promptHash,
      workspacePath: options?.workspacePath,
      modelId: options?.modelId,
      providerId: options?.providerId,
      runtimePreference: options?.runtimePreference,
      runtimeInstanceId: options?.runtimeInstanceId,
      intent,
      expiresAt,
      signature,
    };

    // Register in host active challenges so signature check is reached
    this.activeChallenges.set(token, {
      token,
      studioId,
      promptHash,
      workspacePath: options?.workspacePath,
      modelId: options?.modelId,
      providerId: options?.providerId,
      runtimePreference: options?.runtimePreference,
      runtimeInstanceId: options?.runtimeInstanceId,
      intent,
      issuedAt: Date.now(),
      expiresAt,
      confirmed: true,
      consumed: false,
    });

    return challenge;
  }

  /**
   * Issues an authoritative operator confirmation challenge registered in Extension Host memory.
   */
  public static createChallenge(
    studioId: string,
    prompt: string,
    options?: {
      workspacePath?: string;
      modelId?: string;
      providerId?: string;
      runtimePreference?: string;
      runtimeInstanceId?: string;
      intent?: string;
      ttlMs?: number;
    }
  ): OperatorConfirmationChallenge {
    const normalizedPrompt = prompt.replace(/\r\n/g, '\n');
    const promptHash = crypto.createHash('sha256').update(normalizedPrompt, 'utf8').digest('hex');
    const expiresAt = Date.now() + (options?.ttlMs ?? 5 * 60 * 1000);
    const token = crypto.randomBytes(16).toString('hex');
    const intent = options?.intent ?? 'task_execution';
    const normalizedWs = options?.workspacePath ? normalizeWorkspacePath(options.workspacePath) : '';
    const model = options?.modelId || '';
    const provider = options?.providerId || '';
    const runtimePref = options?.runtimePreference || '';
    const runtimeInst = options?.runtimeInstanceId || '';
    const payload = `${token}:${studioId}:${promptHash}:${normalizedWs}:${model}:${provider}:${runtimePref}:${runtimeInst}:${intent}:${expiresAt}`;
    const signature = crypto.createHmac('sha256', this.secret).update(payload, 'utf8').digest('hex');

    const challenge: OperatorConfirmationChallenge = {
      token,
      studioId,
      promptHash,
      workspacePath: options?.workspacePath,
      modelId: options?.modelId,
      providerId: options?.providerId,
      runtimePreference: options?.runtimePreference,
      runtimeInstanceId: options?.runtimeInstanceId,
      intent,
      expiresAt,
      signature,
    };

    this.activeChallenges.set(token, {
      token,
      studioId,
      promptHash,
      workspacePath: options?.workspacePath,
      modelId: options?.modelId,
      providerId: options?.providerId,
      runtimePreference: options?.runtimePreference,
      runtimeInstanceId: options?.runtimeInstanceId,
      intent,
      issuedAt: Date.now(),
      expiresAt,
      confirmed: true,
      consumed: false,
    });

    return challenge;
  }

  /**
   * Clears all pending confirmations (e.g. on session switch, bridge mode change, or daemon reconnect).
   */
  public static clearPendingChallenges(): void {
    this.activeChallenges.clear();
  }

  /**
   * Verifies and atomically consumes an operator confirmation challenge.
   * Enforces:
   * 1. Cryptographic HMAC signature integrity.
   * 2. Host registry tracking (rejects forged/unregistered tokens).
   * 3. Single-use atomic consumption (anti-replay).
   * 4. Expiration check (TTL).
   * 5. Studio ID exact match.
   * 6. Prompt hash exact match (SHA-256 over normalized LF prompt).
   * 7. Exact presence/absence and path match for workspacePath.
   * 8. Exact presence/absence and string match for modelId.
   * 9. Exact presence/absence and string match for providerId.
   * 10. Intent match ('submit_task').
   */
  public static verifyAndConsumeChallenge(
    challenge: OperatorConfirmationChallenge | string,
    expected: {
      studioId: string;
      prompt: string;
      workspacePath?: string;
      modelId?: string;
      providerId?: string;
      runtimePreference?: string;
      runtimeInstanceId?: string;
      intent?: string;
    },
    options?: {
      consume?: boolean;
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

    // Verify HMAC signature
    let normWsForSig = '';
    try {
      normWsForSig = parsed.workspacePath ? normalizeWorkspacePath(parsed.workspacePath) : '';
    } catch {
      return { valid: false, reason: 'Invalid or uninterpretable workspace path in challenge' };
    }

    const model = parsed.modelId || '';
    const provider = parsed.providerId || '';
    const runtimePref = parsed.runtimePreference || '';
    const runtimeInst = parsed.runtimeInstanceId || '';
    const intent = parsed.intent || 'task_execution';
    const payload = `${parsed.token}:${parsed.studioId}:${parsed.promptHash}:${normWsForSig}:${model}:${provider}:${runtimePref}:${runtimeInst}:${intent}:${parsed.expiresAt}`;
    const expectedSig = crypto.createHmac('sha256', this.secret).update(payload, 'utf8').digest('hex');

    const sigBuf = Buffer.from(parsed.signature, 'hex');
    const expBuf = Buffer.from(expectedSig, 'hex');
    if (sigBuf.length !== expBuf.length || !crypto.timingSafeEqual(sigBuf, expBuf)) {
      return { valid: false, reason: 'Forged confirmation challenge: invalid cryptographic signature' };
    }

    // Extension Host registry lookup
    const tracked = this.activeChallenges.get(parsed.token);
    if (!tracked) {
      return {
        valid: false,
        reason: 'Challenge token not found in Extension Host registry (unrecognized, expired, or cleared)',
      };
    }

    if (tracked.consumed) {
      return {
        valid: false,
        reason: 'Challenge token has already been consumed (replay prevented)',
      };
    }

    if (!tracked.confirmed) {
      return {
        valid: false,
        reason: 'Challenge has not been confirmed by human operator',
      };
    }

    // Studio match
    if (parsed.studioId !== expected.studioId) {
      return {
        valid: false,
        reason: `Studio ID mismatch in confirmation challenge (expected '${expected.studioId}', got '${parsed.studioId}')`,
      };
    }

    // Prompt hash match
    const normalizedExpectedPrompt = expected.prompt.replace(/\r\n/g, '\n');
    const expectedPromptHash = crypto.createHash('sha256').update(normalizedExpectedPrompt, 'utf8').digest('hex');
    if (parsed.promptHash !== expectedPromptHash) {
      return {
        valid: false,
        reason: 'Prompt hash mismatch: prompt was altered after confirmation challenge issued',
      };
    }

    // Exact presence/absence equality for workspacePath:
    const hasParsedWs = Boolean(parsed.workspacePath && parsed.workspacePath.trim().length > 0);
    const hasExpectedWs = Boolean(expected.workspacePath && expected.workspacePath.trim().length > 0);

    if (hasParsedWs !== hasExpectedWs) {
      return {
        valid: false,
        reason: `Workspace directory presence mismatch in confirmation challenge (challenge had: ${hasParsedWs}, request had: ${hasExpectedWs})`,
      };
    }

    if (hasParsedWs && hasExpectedWs) {
      let normParsedWs = '';
      let normExpectedWs = '';
      try {
        normParsedWs = normalizeWorkspacePath(parsed.workspacePath!);
        normExpectedWs = normalizeWorkspacePath(expected.workspacePath!);
      } catch (err: unknown) {
        return {
          valid: false,
          reason: `Workspace directory normalization error: ${(err as Error).message}`,
        };
      }
      if (normParsedWs !== normExpectedWs) {
        return {
          valid: false,
          reason: `Workspace directory mismatch in confirmation challenge (expected '${normExpectedWs}', challenge had '${normParsedWs}')`,
        };
      }
    }

    // Exact presence/absence equality for modelId:
    const hasParsedModel = Boolean(parsed.modelId && parsed.modelId.trim().length > 0);
    const hasExpectedModel = Boolean(expected.modelId && expected.modelId.trim().length > 0);

    if (hasParsedModel !== hasExpectedModel) {
      return {
        valid: false,
        reason: `Model ID presence mismatch in confirmation challenge (challenge had: ${hasParsedModel}, request had: ${hasExpectedModel})`,
      };
    }

    if (hasParsedModel && hasExpectedModel && parsed.modelId !== expected.modelId) {
      return {
        valid: false,
        reason: `Model ID mismatch in confirmation challenge (expected '${expected.modelId}', got '${parsed.modelId}')`,
      };
    }

    // Exact presence/absence equality for providerId:
    const hasParsedProvider = Boolean(parsed.providerId && parsed.providerId.trim().length > 0);
    const hasExpectedProvider = Boolean(expected.providerId && expected.providerId.trim().length > 0);

    if (hasParsedProvider !== hasExpectedProvider) {
      return {
        valid: false,
        reason: `Provider ID presence mismatch in confirmation challenge (challenge had: ${hasParsedProvider}, request had: ${hasExpectedProvider})`,
      };
    }

    if (hasParsedProvider && hasExpectedProvider && parsed.providerId !== expected.providerId) {
      return {
        valid: false,
        reason: `Provider ID mismatch in confirmation challenge (expected '${expected.providerId}', got '${parsed.providerId}')`,
      };
    }

    // Exact presence/absence equality for runtimePreference:
    const hasParsedRuntime = Boolean(parsed.runtimePreference && parsed.runtimePreference.trim().length > 0);
    const hasExpectedRuntime = Boolean(expected.runtimePreference && expected.runtimePreference.trim().length > 0);

    if (hasParsedRuntime !== hasExpectedRuntime) {
      return {
        valid: false,
        reason: `Runtime preference presence mismatch in confirmation challenge (challenge had: ${hasParsedRuntime}, request had: ${hasExpectedRuntime})`,
      };
    }

    if (hasParsedRuntime && hasExpectedRuntime && parsed.runtimePreference !== expected.runtimePreference) {
      return {
        valid: false,
        reason: `Runtime preference mismatch in confirmation challenge (expected '${expected.runtimePreference}', got '${parsed.runtimePreference}')`,
      };
    }

    // Exact presence/absence equality for runtimeInstanceId:
    const hasParsedInstance = Boolean(parsed.runtimeInstanceId && parsed.runtimeInstanceId.trim().length > 0);
    const hasExpectedInstance = Boolean(expected.runtimeInstanceId && expected.runtimeInstanceId.trim().length > 0);

    if (hasParsedInstance !== hasExpectedInstance) {
      return {
        valid: false,
        reason: `Runtime instance ID presence mismatch in confirmation challenge (challenge had: ${hasParsedInstance}, request had: ${hasExpectedInstance})`,
      };
    }

    if (hasParsedInstance && hasExpectedInstance && parsed.runtimeInstanceId !== expected.runtimeInstanceId) {
      return {
        valid: false,
        reason: `Runtime instance ID mismatch in confirmation challenge (expected '${expected.runtimeInstanceId}', got '${parsed.runtimeInstanceId}')`,
      };
    }

    // Intent match
    const expectedIntent = expected.intent ?? 'task_execution';
    const parsedIntent = parsed.intent ?? 'task_execution';
    if (parsedIntent !== expectedIntent) {
      return {
        valid: false,
        reason: `Execution intent mismatch in confirmation challenge (expected '${expectedIntent}', got '${parsedIntent}')`,
      };
    }

    // Single-use atomic consumption
    const shouldConsume = options?.consume ?? true;
    if (shouldConsume) {
      tracked.consumed = true;
      tracked.consumedAt = Date.now();
    }

    return { valid: true };
  }

  public static verifyChallenge(
    challenge: OperatorConfirmationChallenge | string,
    expected: {
      studioId: string;
      prompt: string;
      workspacePath?: string;
      modelId?: string;
      providerId?: string;
      runtimePreference?: string;
      runtimeInstanceId?: string;
      intent?: string;
    },
    options?: {
      consume?: boolean;
    }
  ): { valid: boolean; reason?: string } {
    return this.verifyAndConsumeChallenge(challenge, expected, options);
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
  runtimePreference?: string;
  runtimeInstanceId?: string;
  confirmedByUser: boolean;
  confirmationChallenge?: OperatorConfirmationChallenge | string;
  consumeChallenge?: boolean;
  workspacePath?: string;
  daemonRepoRoot?: string;
  daemonEffectiveModel?: string;
  daemonEffectiveProvider?: string;
  customKey?: string;
  requireAuthoritativeCheck?: boolean;
  requireChallenge?: boolean;
  requireWorkspace?: boolean;
  readiness?: RustExecutionReadinessReport | null;
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
 * - Handles null/empty safely.
 * - Rejects foreign-platform Windows drive paths or UNC paths on POSIX.
 * - Resolves symlinks canonically via realpathSync if path exists on disk.
 * - Strips trailing slashes.
 * - Applies case-insensitive lowercasing on Windows (win32).
 */
export function normalizeWorkspacePath(p: string): string {
  if (!p || typeof p !== 'string') {
    return '';
  }
  const trimmed = p.trim();
  if (trimmed.length === 0) {
    return '';
  }

  // Reject foreign-platform Windows drive letters or UNC paths on POSIX
  if (process.platform !== 'win32') {
    if (/^[a-zA-Z]:[/\\]/.test(trimmed) || trimmed.startsWith('\\\\')) {
      throw new Error(
        `Foreign-platform Windows path '${trimmed}' cannot be safely interpreted on POSIX platform.`
      );
    }
  }

  // Canonicalize via path.resolve
  let resolved = path.resolve(trimmed);

  // If path exists on disk, resolve symlinks canonically
  try {
    if (fs.existsSync(resolved)) {
      resolved = fs.realpathSync(resolved);
    }
  } catch {
    // If not accessible or does not exist on disk, keep path.resolve result
  }

  // Strip trailing slashes
  resolved = resolved.replace(/[/\\]+$/, '');

  // Case-insensitive comparison on Windows
  return process.platform === 'win32' ? resolved.toLowerCase() : resolved;
}

export interface TaskSubmissionPreflightResult {
  allowed: boolean;
  tier: CapabilityTier;
  error?: string;
  limitationWarning?: string;
  resolvedRuntimePreference?: string;
  resolvedRuntimeInstanceId?: string;
  resolvedModelId?: string;
  resolvedProviderId?: string;
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
   * Prohibited in production mode and strictly isolated to automated test environments.
   */
  public static setCapabilityEnabled(capabilityId: string, enabled: boolean): void {
    if (process.env.NODE_ENV === 'production') {
      throw new Error('Dynamic capability overrides are prohibited in production mode.');
    }
    const isTestEnv =
      typeof (global as any).describe === 'function' ||
      typeof (global as any).it === 'function' ||
      process.env.NODE_ENV === 'test';
    if (!isTestEnv) {
      throw new Error('Dynamic capability overrides are strictly prohibited outside automated test environments.');
    }
    this.dynamicOverrides.set(capabilityId, enabled);
  }

  /**
   * Resets all dynamic capability overrides to static defaults.
   */
  public static resetCapabilities(): void {
    this.dynamicOverrides.clear();
  }

  /**
   * Validates runtime instance selection against authoritative daemon readiness.
   * Enforces:
   * 1. Readiness presence (fails closed if readiness report is missing/null).
   * 2. Duplicate instance identity detection (rejects if duplicate instance_ids exist).
   * 3. Distinct instance resolution (never silently picks first if ambiguous).
   * 4. Health verification (rejects unhealthy/degraded instances).
   * 5. Unimplemented/unverified adapter rejection (rejects native Claude Code / Codex unless backend explicitly verified).
   * 6. Capability verification (rejects if required capabilities are unsupported).
   * 7. Configured default resolution when no explicit selection provided.
   */
  public static validateRuntimeInstanceSelection(
    readiness: RustExecutionReadinessReport | null | undefined,
    selection?: {
      runtime?: string;
      runtimePreference?: string;
      runtimeInstanceId?: string;
    },
    options?: RuntimeValidationOptions
  ): RuntimeValidationResult {
    const action = options?.action ?? 'task_execution';
    if (!readiness) {
      return {
        valid: false,
        reason: 'Execution readiness report is unavailable from daemon.',
      };
    }

    const instances = readiness.runtime_instances || [];
    if (instances.length === 0) {
      return {
        valid: false,
        reason: 'Daemon reports zero runtime instances available.',
      };
    }

    // Check for duplicate instance IDs
    const seenIds = new Set<string>();
    const duplicateIds = new Set<string>();
    for (const inst of instances) {
      if (seenIds.has(inst.instance_id)) {
        duplicateIds.add(inst.instance_id);
      }
      seenIds.add(inst.instance_id);
    }

    let reqInstId: string | undefined = selection?.runtimeInstanceId;
    let reqPref: string | undefined = selection?.runtimePreference;

    if (selection?.runtime && selection.runtime.trim().length > 0) {
      const trimmed = selection.runtime.trim();
      if (trimmed.includes('|')) {
        const [partId, partPref] = trimmed.split('|');
        if (!reqInstId && partId.trim().length > 0) {
          reqInstId = partId.trim();
        }
        if (!reqPref && partPref.trim().length > 0) {
          reqPref = partPref.trim();
        }
      } else if (!reqInstId && !reqPref) {
        // Could be instance_id or implementation
        const byId = instances.find((i) => i.instance_id === trimmed);
        if (byId) {
          reqInstId = byId.instance_id;
          reqPref = byId.implementation;
        } else {
          reqPref = trimmed;
        }
      }
    }

    let resolvedInstance: (typeof instances)[0] | undefined;

    if (reqInstId) {
      if (duplicateIds.has(reqInstId)) {
        return {
          valid: false,
          reason: `Duplicate runtime instance identity detected in daemon readiness: '${reqInstId}'. Execution rejected (fail-closed).`,
        };
      }
      resolvedInstance = instances.find((i) => i.instance_id === reqInstId);
      if (!resolvedInstance) {
        return {
          valid: false,
          reason: `Unknown runtime instance '${reqInstId}'. Not found in daemon discovered instances.`,
        };
      }
      if (reqPref && reqPref !== resolvedInstance.implementation) {
        return {
          valid: false,
          reason: `Runtime preference mismatch: requested implementation '${reqPref}', but instance '${reqInstId}' implements '${resolvedInstance.implementation}'.`,
        };
      }
    } else if (reqPref) {
      const matching = instances.filter((i) => i.implementation === reqPref);
      if (matching.length === 0) {
        return {
          valid: false,
          reason: `Unknown runtime implementation '${reqPref}'. Not found in daemon discovered instances.`,
        };
      }
      if (matching.length > 1) {
        return {
          valid: false,
          reason: `Ambiguous runtime selection: multiple instances discovered for implementation '${reqPref}'. Explicit instance selection required.`,
        };
      }
      resolvedInstance = matching[0];
      if (duplicateIds.has(resolvedInstance.instance_id)) {
        return {
          valid: false,
          reason: `Duplicate runtime instance identity detected in daemon readiness: '${resolvedInstance.instance_id}'. Execution rejected (fail-closed).`,
        };
      }
    } else {
      // Neither instance ID nor preference specified — check configured default
      const defaultInstId = readiness.defaults?.configured_instance_id;
      if (defaultInstId && defaultInstId.trim().length > 0) {
        if (duplicateIds.has(defaultInstId)) {
          return {
            valid: false,
            reason: `Duplicate runtime instance identity detected in daemon readiness for default: '${defaultInstId}'. Execution rejected (fail-closed).`,
          };
        }
        resolvedInstance = instances.find((i) => i.instance_id === defaultInstId);
        if (!resolvedInstance) {
          return {
            valid: false,
            reason: `Configured default runtime instance '${defaultInstId}' not found in daemon discovered instances.`,
          };
        }
      } else {
        // Missing default and no selection provided
        const healthy = instances.filter((i) => i.healthy);
        if (healthy.length === 0) {
          return {
            valid: false,
            reason: 'No healthy runtime instances available on daemon. Configuration required.',
          };
        }
        if (healthy.length > 1) {
          return {
            valid: false,
            reason: 'Multiple runtime instances discovered but no configured default or explicit selection provided. Explicit instance selection required.',
          };
        }
        resolvedInstance = healthy[0];
        if (duplicateIds.has(resolvedInstance.instance_id)) {
          return {
            valid: false,
            reason: `Duplicate runtime instance identity detected in daemon readiness: '${resolvedInstance.instance_id}'. Execution rejected (fail-closed).`,
          };
        }
      }
    }

    // Health check
    if (!resolvedInstance.healthy) {
      return {
        valid: false,
        reason: `Runtime instance '${resolvedInstance.instance_id}' (${resolvedInstance.implementation}) is degraded or unhealthy. Task execution disabled.`,
      };
    }

    // Native Claude Code / Codex verification guard
    const impl = resolvedInstance.implementation;
    if (impl === 'claude_code' || impl === 'claude-code' || impl === 'codex') {
      const isAvailable = readiness.available_runtimes && readiness.available_runtimes.includes(impl);
      if (!isAvailable) {
        return {
          valid: false,
          reason: `Native '${impl}' runtime adapter is not implemented or verified in backend. Task execution disabled.`,
        };
      }
    }

    // Tri-state runtime capability validation
    // If readiness.runtime_capabilities is omitted or null (legacy M15-01 response),
    // preserve backward-compatible operation without error.
    if (readiness.runtime_capabilities !== undefined && readiness.runtime_capabilities !== null) {
      const instanceCaps = readiness.runtime_capabilities[resolvedInstance.instance_id];

      let requiredCaps: string[] = [];
      if (action === 'task_execution') {
        requiredCaps = ['streaming_events', 'stop'];
      } else if (action === 'task_cancellation') {
        requiredCaps = ['stop'];
      } else if (action === 'read_only') {
        requiredCaps = [];
      }

      if (requiredCaps.length > 0) {
        if (!instanceCaps) {
          return {
            valid: false,
            reason: `Runtime instance '${resolvedInstance.instance_id}' has missing runtime capabilities in daemon readiness. Required: ${requiredCaps.join(', ')}.`,
          };
        }

        for (const cap of requiredCaps) {
          const status = instanceCaps[cap];
          if (!status) {
            return {
              valid: false,
              reason: `Runtime instance '${resolvedInstance.instance_id}' is missing required '${cap}' capability for action '${action}'.`,
            };
          }
          if (status === 'unsupported') {
            return {
              valid: false,
              reason: `Runtime instance '${resolvedInstance.instance_id}' does not support required '${cap}' capability (status: unsupported) for action '${action}'.`,
            };
          }
          if (status === 'unknown') {
            return {
              valid: false,
              reason: `Runtime instance '${resolvedInstance.instance_id}' has unknown support for required '${cap}' capability (status: unknown) for action '${action}'.`,
            };
          }
          if (status !== 'supported') {
            return {
              valid: false,
              reason: `Runtime instance '${resolvedInstance.instance_id}' capability '${cap}' has invalid status '${status}' (must be 'supported') for action '${action}'.`,
            };
          }
        }
      }
    }

    return {
      valid: true,
      runtimePreference: resolvedInstance.implementation,
      runtimeInstanceId: resolvedInstance.instance_id,
    };
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

    // 4.5. Early workspace path attack sequence validation (fail closed before challenge evaluation)
    if (req.workspacePath) {
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

      if (process.platform !== 'win32') {
        if (/^[a-zA-Z]:[/\\]/.test(req.workspacePath.trim())) {
          return {
            allowed: false,
            tier: 'TASK_SUBMISSION_READY',
            error: `Workspace normalization error: Foreign-platform Windows path '${req.workspacePath.trim()}' cannot be safely interpreted on POSIX platform.`,
          };
        }
      }
    }

    // 4.6. Runtime instance selection validation against authoritative daemon readiness
    if (req.readiness !== undefined && req.readiness !== null) {
      const runtimeVal = this.validateRuntimeInstanceSelection(
        req.readiness,
        {
          runtimePreference: req.runtimePreference,
          runtimeInstanceId: req.runtimeInstanceId,
        },
        {
          action: 'task_execution',
        }
      );
      if (!runtimeVal.valid) {
        return {
          allowed: false,
          tier: 'TASK_SUBMISSION_READY',
          error: runtimeVal.reason || 'Runtime selection validation failed against daemon readiness.',
        };
      }
      req.runtimePreference = runtimeVal.runtimePreference;
      req.runtimeInstanceId = runtimeVal.runtimeInstanceId;
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
      const verification = OperatorConfirmationManager.verifyAndConsumeChallenge(
        req.confirmationChallenge,
        {
          studioId: req.studioId!,
          prompt: req.prompt,
          workspacePath: req.workspacePath,
          modelId: req.requestedModelId,
          providerId: req.requestedProviderId,
          runtimePreference: req.runtimePreference,
          runtimeInstanceId: req.runtimeInstanceId,
          intent: 'task_execution',
        },
        {
          consume: req.consumeChallenge ?? true,
        }
      );

      if (!verification.valid) {
        return {
          allowed: false,
          tier: 'TASK_SUBMISSION_READY',
          error: `Operator confirmation challenge verification failed: ${verification.reason}`,
        };
      }
    }

    // 5. Authoritative Model & Provider validation (Parity with connection.rs:1771-1827 / M15 multi-model catalog)
    let resolvedModelId: string | undefined;
    let resolvedProviderId: string | undefined;

    if (req.readiness !== undefined && req.readiness !== null) {
      const availableModels = req.readiness.available_models || [];

      if (req.requestedModelId && req.requestedModelId.trim().length > 0 && req.requestedModelId !== 'default') {
        const trimmedModelId = req.requestedModelId.trim();

        if (availableModels.length > 0) {
          const matchingModels = availableModels.filter((m) => m.model_id === trimmedModelId);
          if (matchingModels.length === 0) {
            return {
              allowed: false,
              tier: 'TASK_SUBMISSION_READY',
              error: `Model routing rejected: Requested model '${trimmedModelId}' not found in daemon available models.`,
            };
          }

          if (req.requestedProviderId && req.requestedProviderId.trim().length > 0 && req.requestedProviderId !== 'default') {
            const trimmedProvId = req.requestedProviderId.trim();
            const providerMatch = matchingModels.find((m) => m.provider_instance_id === trimmedProvId);
            if (!providerMatch) {
              return {
                allowed: false,
                tier: 'TASK_SUBMISSION_READY',
                error: `Provider routing mismatch: Requested model '${trimmedModelId}' with provider '${trimmedProvId}' not found in daemon available models.`,
              };
            }
            resolvedModelId = trimmedModelId;
            resolvedProviderId = trimmedProvId;
          } else if (matchingModels.length > 1) {
            // Parity: connection.rs:1980 - ambiguous provider instance
            const matchedProvIds = matchingModels.map((m) => m.provider_instance_id).join(', ');
            return {
              allowed: false,
              tier: 'TASK_SUBMISSION_READY',
              error: `Model routing ambiguous: Requested model '${trimmedModelId}' matches multiple provider instances (${matchedProvIds}). Explicit provider ID required.`,
            };
          } else {
            resolvedModelId = trimmedModelId;
            resolvedProviderId = matchingModels[0].provider_instance_id;
          }
        } else if (req.daemonEffectiveModel && trimmedModelId !== req.daemonEffectiveModel) {
          return {
            allowed: false,
            tier: 'TASK_SUBMISSION_READY',
            error: `Model routing mismatch: Requested model '${trimmedModelId}' differs from daemon effective configuration ('${req.daemonEffectiveModel}'). Per-task model override is rejected.`,
          };
        } else {
          resolvedModelId = trimmedModelId;
          resolvedProviderId =
            req.requestedProviderId && req.requestedProviderId !== 'default'
              ? req.requestedProviderId
              : req.daemonEffectiveProvider;
        }
      } else {
        // No explicit model requested (or 'default') — resolve daemon default model
        const defaultModelId =
          req.readiness.defaults?.default_model_id ||
          (availableModels.length === 1 ? availableModels[0].model_id : req.daemonEffectiveModel);
        const defaultProviderId =
          req.readiness.defaults?.default_provider_instance_id ||
          (availableModels.length === 1 ? availableModels[0].provider_instance_id : req.daemonEffectiveProvider);

        if (!defaultModelId && availableModels.length > 1) {
          return {
            allowed: false,
            tier: 'TASK_SUBMISSION_READY',
            error: `Multiple models available (${availableModels.length}) in daemon catalog, but no default model configured. Explicit model selection required.`,
          };
        }

        resolvedModelId = defaultModelId;
        resolvedProviderId = defaultProviderId;
      }
    } else {
      // Legacy path without readiness report: validate against daemonEffectiveModel / daemonEffectiveProvider
      const parsedDefaultModel = parseDefaultModelEnv();
      const effectiveModel = req.daemonEffectiveModel || parsedDefaultModel?.modelId;
      const effectiveProvider = req.daemonEffectiveProvider || parsedDefaultModel?.providerId;

      if (req.requestedModelId && req.requestedModelId.trim().length > 0 && req.requestedModelId !== 'default') {
        if (!effectiveModel) {
          return {
            allowed: false,
            tier: 'TASK_SUBMISSION_READY',
            error: `Model routing rejected (fail-closed): requested model '${req.requestedModelId}', but daemon default model could not be verified.`,
          };
        }
        if (req.requestedModelId !== effectiveModel) {
          return {
            allowed: false,
            tier: 'TASK_SUBMISSION_READY',
            error: `Model routing mismatch: Requested model '${req.requestedModelId}' differs from daemon effective configuration ('${effectiveModel}'). Per-task model override is rejected.`,
          };
        }
        resolvedModelId = req.requestedModelId;
      } else {
        resolvedModelId = effectiveModel;
      }

      if (req.requestedProviderId && req.requestedProviderId.trim().length > 0 && req.requestedProviderId !== 'default') {
        if (effectiveProvider && req.requestedProviderId !== effectiveProvider) {
          return {
            allowed: false,
            tier: 'TASK_SUBMISSION_READY',
            error: `Provider routing mismatch: Requested provider '${req.requestedProviderId}' differs from daemon verified default provider ('${effectiveProvider}'). Per-task provider override is rejected.`,
          };
        }
        resolvedProviderId = req.requestedProviderId;
      } else {
        resolvedProviderId = effectiveProvider;
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

      let normalizedWs: string;
      let normalizedRepo: string;
      try {
        normalizedWs = normalizeWorkspacePath(req.workspacePath);
        normalizedRepo = normalizeWorkspacePath(effectiveDaemonRepo);
      } catch (err: unknown) {
        return {
          allowed: false,
          tier: 'TASK_SUBMISSION_READY',
          error: `Workspace normalization error: ${(err as Error).message}`,
        };
      }

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
      resolvedRuntimePreference: req.runtimePreference,
      resolvedRuntimeInstanceId: req.runtimeInstanceId,
      resolvedModelId,
      resolvedProviderId,
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
