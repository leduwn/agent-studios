/**
 * Agent Studios — Chat Presentation Model & Unified Readiness Resolver
 * Maps low-level bridge/session/provider state into calm, plain-language UI state,
 * ensures runtime/model compatibility, and prevents silent disabled send buttons.
 */

import { AgentMode, ConnectionState, ProviderSummary } from '../bridge/types';
import { WorkspaceAuthorizationStatus } from '../security/WorkspaceVerification';

export type UiConnectionStatus =
  | 'not_configured'
  | 'starting'
  | 'connecting'
  | 'ready'
  | 'running'
  | 'waiting_for_approval'
  | 'reconnecting'
  | 'error'
  | 'disconnected';

export type UiStatusTone = 'neutral' | 'ok' | 'active' | 'warn' | 'error';

export interface UiConnectionAction {
  id: 'configure_provider' | 'connect' | 'retry' | 'review_approval' | 'open_folder';
  label: string;
}

export interface UiConnectionPresentation {
  status: UiConnectionStatus;
  label: string;
  detail: string;
  tone: UiStatusTone;
  action?: UiConnectionAction;
}

export interface RuntimeDescriptor {
  id: string;
  name: string;
  enabled: boolean;
  tooltip?: string;
  instanceId?: string;
  implementation?: string;
}

export interface SendReadinessInput {
  uiStatus: UiConnectionStatus;
  mode: AgentMode;
  hasProviders: boolean;
  hasWorkspace: boolean;
  isWorkspaceTrusted: boolean;
  workspaceStatus?: WorkspaceAuthorizationStatus;
  isRunning?: boolean;
}

export interface SendReadinessResult {
  canSendImmediately: boolean;
  blockingReason?:
    | 'not_configured'
    | 'disconnected'
    | 'starting'
    | 'error'
    | 'no_workspace'
    | 'untrusted_workspace'
    | 'running';
  message?: string;
  action?: UiConnectionAction;
}

export function resolveUiConnectionStatus(input: {
  connectionState: ConnectionState;
  providers: readonly ProviderSummary[];
  isRunning?: boolean;
  pendingApprovalCount?: number;
  isStartingEngine?: boolean;
  lastError?: string | null;
}): UiConnectionPresentation {
  const {
    connectionState,
    providers,
    isRunning = false,
    pendingApprovalCount = 0,
    isStartingEngine = false,
    lastError,
  } = input;

  if (connectionState === 'authentication_failed' || connectionState === 'auth_required') {
    return {
      status: 'error',
      label: 'Auth error',
      detail: 'Authentication with the local engine failed.',
      tone: 'error',
      action: { id: 'retry', label: 'Retry' },
    };
  }

  if (connectionState === 'incompatible' || connectionState === 'unsupported_protocol') {
    return {
      status: 'error',
      label: 'Version mismatch',
      detail: 'Extension and background engine versions do not match.',
      tone: 'error',
      action: { id: 'retry', label: 'Retry' },
    };
  }

  if (lastError && connectionState === 'disconnected') {
    return {
      status: 'error',
      label: 'Error',
      detail: lastError,
      tone: 'error',
      action: { id: 'retry', label: 'Retry' },
    };
  }

  if (isStartingEngine) {
    return {
      status: 'starting',
      label: 'Starting…',
      detail: 'Starting background engine…',
      tone: 'active',
    };
  }

  if (connectionState === 'connecting' || connectionState === 'authenticating') {
    return {
      status: 'connecting',
      label: 'Connecting…',
      detail: 'Connecting to workspace engine…',
      tone: 'active',
    };
  }

  if (connectionState === 'reconnecting' || connectionState === 'degraded') {
    return {
      status: 'reconnecting',
      label: 'Reconnecting…',
      detail: 'Connection interrupted, reconnecting automatically.',
      tone: 'warn',
      action: { id: 'retry', label: 'Retry' },
    };
  }

  if (connectionState === 'disconnected') {
    return {
      status: 'disconnected',
      label: 'Offline',
      detail: 'Background engine is offline.',
      tone: 'neutral',
      action: { id: 'connect', label: 'Connect' },
    };
  }

  if (pendingApprovalCount > 0) {
    return {
      status: 'waiting_for_approval',
      label: `${pendingApprovalCount} approval needed`,
      detail: 'Review requested action to continue.',
      tone: 'warn',
      action: { id: 'review_approval', label: 'Review' },
    };
  }

  if (isRunning) {
    return {
      status: 'running',
      label: 'Working…',
      detail: 'Running response.',
      tone: 'active',
    };
  }

  const configuredProviders = providers.filter((p) => p.status !== 'unconfigured');
  if (configuredProviders.length === 0) {
    return {
      status: 'not_configured',
      label: 'Add a provider',
      detail: 'Configure an API provider or local model to start chatting.',
      tone: 'warn',
      action: { id: 'configure_provider', label: 'Configure' },
    };
  }

  return {
    status: 'ready',
    label: 'Ready',
    detail: 'Connected and ready.',
    tone: 'ok',
  };
}

/**
 * Ensures runtime adapters match the active provider/model family.
 * For example, Claude Code CLI runtime only pairs with Anthropic / Claude models,
 * while Internal Codex handles OpenAI-compatible, DeepSeek, Gemini, Ollama, LM Studio, and Custom endpoints.
 */
export function isRuntimeCompatibleWithProvider(
  runtimeId: string,
  providerType?: string,
  modelId?: string
): boolean {
  const normRuntime = (runtimeId || '').toLowerCase();
  const normProvider = (providerType || '').toLowerCase();
  const normModel = (modelId || '').toLowerCase();

  if (normRuntime === 'claude-code' || normRuntime === 'claude') {
    if (!normProvider && !normModel) {
      return true;
    }
    return normProvider === 'anthropic' || normModel.includes('claude');
  }

  return true;
}

export function filterCompatibleRuntimes(
  runtimes: readonly RuntimeDescriptor[],
  providerType?: string,
  modelId?: string
): RuntimeDescriptor[] {
  const source: RuntimeDescriptor[] =
    runtimes.length > 0
      ? [...runtimes]
      : [
          {
            id: 'internal-codex',
            name: 'Agent Studios Engine',
            enabled: true,
          },
          {
            id: 'codex',
            name: 'Codex CLI',
            enabled: true,
          },
          {
            id: 'claude-code',
            name: 'Claude Code CLI',
            enabled: true,
          },
        ];

  const compatible = source.filter((rt) =>
    isRuntimeCompatibleWithProvider(rt.id, providerType, modelId)
  );
  if (compatible.length > 0) {
    return compatible;
  }
  return [
    {
      id: 'internal-codex',
      name: 'Agent Studios Engine',
      enabled: true,
    },
  ];
}

export function pickDefaultRuntime(
  runtimes: readonly RuntimeDescriptor[],
  currentRuntimeId?: string,
  providerType?: string,
  modelId?: string
): string {
  const compatible = filterCompatibleRuntimes(runtimes, providerType, modelId);
  if (
    currentRuntimeId &&
    compatible.some((r) => r.id === currentRuntimeId && r.enabled)
  ) {
    return currentRuntimeId;
  }
  const preferred =
    compatible.find((r) => r.id === 'internal-codex' && r.enabled) ??
    compatible.find((r) => r.enabled) ??
    compatible[0];
  return preferred?.id || 'internal-codex';
}

export function resolveSendReadiness(input: SendReadinessInput): SendReadinessResult {
  if (input.isRunning) {
    return {
      canSendImmediately: false,
      blockingReason: 'running',
      message: 'A run is already in progress. Stop the current run or wait for it to finish.',
    };
  }

  if (!input.isWorkspaceTrusted && input.mode === 'agent') {
    return {
      canSendImmediately: false,
      blockingReason: 'untrusted_workspace',
      message: 'Trust this workspace folder before running editing or terminal tasks.',
    };
  }

  if (input.uiStatus === 'starting') {
    return {
      canSendImmediately: false,
      blockingReason: 'starting',
      message: 'Starting background engine…',
    };
  }

  if (input.uiStatus === 'error') {
    return {
      canSendImmediately: false,
      blockingReason: 'error',
      message: 'Engine connection error. Retry connecting before sending.',
      action: { id: 'retry', label: 'Retry Connection' },
    };
  }

  if (input.uiStatus === 'disconnected') {
    return {
      canSendImmediately: false,
      blockingReason: 'disconnected',
      message: 'Connect the background engine to send messages.',
      action: { id: 'connect', label: 'Connect' },
    };
  }

  if (!input.hasProviders) {
    return {
      canSendImmediately: false,
      blockingReason: 'not_configured',
      message: 'Add an AI provider and model to start chatting.',
      action: { id: 'configure_provider', label: 'Configure Provider' },
    };
  }

  if (input.mode === 'agent' && !input.hasWorkspace) {
    return {
      canSendImmediately: false,
      blockingReason: 'no_workspace',
      message: 'Open a workspace folder to edit files or run commands in Agent mode.',
      action: { id: 'open_folder', label: 'Open Folder' },
    };
  }

  return { canSendImmediately: true };
}
