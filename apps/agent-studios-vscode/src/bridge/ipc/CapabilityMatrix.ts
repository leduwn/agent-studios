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
    enabled: true,
    rustCrate: 'agent-studios-ipc-transport',
    prReference: 'PR #19',
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
    enabled: true,
    rustCrate: 'agent-studios-ipc-transport',
    prReference: 'PR #19',
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
  workspacePath?: string;
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
  };
}

export class CapabilityMatrix {
  /**
   * Evaluates if a capability is allowed and active.
   */
  public static isCapabilityEnabled(capabilityId: string): boolean {
    const desc = CAPABILITY_REGISTRY[capabilityId];
    return desc ? desc.enabled : false;
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
   * - Studio ID is explicit.
   * - Mutating task confirmation is granted.
   * - Prompt text is preserved.
   * - Explicit configuration limitation warnings when per-task model routing is requested.
   */
  public static evaluateTaskSubmissionPreflight(
    req: TaskSubmissionPreflightRequest
  ): TaskSubmissionPreflightResult {
    // 1. Studio identity check
    if (!req.studioId || req.studioId.trim().length === 0) {
      return {
        allowed: false,
        tier: 'TASK_SUBMISSION_READY',
        error: 'Active studio identity is required before submitting tasks to the daemon.',
      };
    }

    // 2. Prompt validation
    const trimmed = req.prompt.trim();
    if (trimmed.length === 0) {
      return {
        allowed: false,
        tier: 'TASK_SUBMISSION_READY',
        error: 'Prompt text cannot be empty.',
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

    // 4. Per-task model routing limitation check
    // The backend only routes via daemon startup default (AGENT_STUDIOS_DEFAULT_MODEL).
    let limitationWarning: string | undefined;
    if (req.requestedModelId && req.requestedModelId !== 'default') {
      limitationWarning =
        `The Rust backend daemon (agsd) currently routes tasks using daemon-level defaults (AGENT_STUDIOS_DEFAULT_MODEL). ` +
        `Per-task model override ('${req.requestedModelId}') is not yet supported in BatchTaskSpec (PR-M15-01). ` +
        `Task will execute under the daemon configured default model.`;
    }

    // 5. Construct sanitized specification
    const lines = trimmed.split('\n').filter((l) => l.trim().length > 0);
    const title = lines[0].slice(0, 100);
    const description = lines.length > 1 ? lines.slice(1).join('\n') : trimmed;
    const taskKey = `task-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 7)}`;

    return {
      allowed: true,
      tier: 'TASK_SUBMISSION_READY',
      limitationWarning,
      sanitizedSpec: {
        key: taskKey,
        title,
        description,
      },
    };
  }
}
