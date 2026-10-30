/**
 * Agent Studios — Agent Mode Selector
 * Milestone M14-UI-01: Professional AI Coding Assistant Experience
 */

import * as vscode from 'vscode';

export type AgentMode = 'ask' | 'plan' | 'agent';

export interface AgentModeCapabilities {
  readonly canEditFiles: boolean;
  readonly canRunCommands: boolean;
  readonly canProposeDag: boolean;
  readonly isReadOnly: boolean;
}

export interface AgentModeDefinition {
  readonly id: AgentMode;
  readonly name: string;
  readonly label: string;
  readonly description: string;
  readonly icon: string;
  readonly isReadOnly: boolean;
  readonly capabilities: AgentModeCapabilities;
}

export interface ModeValidationResult {
  readonly mode: AgentMode;
  readonly supported: boolean;
  readonly reason?: string;
}

export const AGENT_MODES: Record<AgentMode, AgentModeDefinition> = {
  ask: {
    id: 'ask',
    name: 'Ask',
    label: 'Ask',
    description: 'Read-only explanation, code search, non-mutating queries.',
    icon: 'comment-discussion',
    isReadOnly: true,
    capabilities: {
      canEditFiles: false,
      canRunCommands: false,
      canProposeDag: false,
      isReadOnly: true,
    },
  },
  plan: {
    id: 'plan',
    name: 'Plan',
    label: 'Plan',
    description: 'Architecture breakdown, dependency planning, DAG proposal without writing files.',
    icon: 'list-tree',
    isReadOnly: true,
    capabilities: {
      canEditFiles: false,
      canRunCommands: false,
      canProposeDag: true,
      isReadOnly: true,
    },
  },
  agent: {
    id: 'agent',
    name: 'Agent',
    label: 'Agent',
    description: 'Full execution mode (file edits, running tests, invoking tools via daemon).',
    icon: 'hubot',
    isReadOnly: false,
    capabilities: {
      canEditFiles: true,
      canRunCommands: true,
      canProposeDag: true,
      isReadOnly: false,
    },
  },
};

export const AGENT_STUDIOS_LAST_MODE_KEY = 'agentStudios.lastUsedMode';

/**
 * Normalizes input string to canonical AgentMode, or null if invalid.
 */
export function normalizeAgentMode(input: unknown): AgentMode | null {
  if (typeof input !== 'string') {
    return null;
  }
  const clean = input.trim().toLowerCase();
  if (clean === 'ask') {
    return 'ask';
  }
  if (clean === 'plan') {
    return 'plan';
  }
  if (clean === 'agent') {
    return 'agent';
  }
  return null;
}

/**
 * ModeSelector coordinates selection, validation, backend capability matching,
 * and persistence for the 3 Agent Modes (Ask, Plan, Agent).
 */
export class ModeSelector implements vscode.Disposable {
  private _currentMode: AgentMode;
  private _supportedModes: Set<AgentMode>;
  private _memento?: vscode.Memento;

  private readonly _onDidChangeMode = new vscode.EventEmitter<AgentMode>();
  public readonly onDidChangeMode: vscode.Event<AgentMode> = this._onDidChangeMode.event;

  private readonly _onDidValidateMode = new vscode.EventEmitter<ModeValidationResult>();
  public readonly onDidValidateMode: vscode.Event<ModeValidationResult> = this._onDidValidateMode.event;

  constructor(
    memento?: vscode.Memento,
    defaultMode: AgentMode = 'agent',
    initialSupportedModes: AgentMode[] = ['ask', 'plan', 'agent']
  ) {
    this._memento = memento;
    this._supportedModes = new Set(initialSupportedModes);

    // 1. Restore persisted mode if present and valid
    let restoredMode: AgentMode | null = null;
    if (this._memento) {
      const persisted = this._memento.get<string>(AGENT_STUDIOS_LAST_MODE_KEY);
      restoredMode = normalizeAgentMode(persisted);
    }

    // 2. Fall back to defaultMode if restored mode is invalid or unsupported
    if (restoredMode && this._supportedModes.has(restoredMode)) {
      this._currentMode = restoredMode;
    } else if (this._supportedModes.has(defaultMode)) {
      this._currentMode = defaultMode;
    } else {
      // Pick first available supported mode, or fallback to 'ask'
      const first = Array.from(this._supportedModes)[0];
      this._currentMode = first ?? 'ask';
    }
  }

  public get currentMode(): AgentMode {
    return this._currentMode;
  }

  public get currentDefinition(): AgentModeDefinition {
    return AGENT_MODES[this._currentMode];
  }

  public get isCurrentModeSupported(): boolean {
    return this._supportedModes.has(this._currentMode);
  }

  public get supportedModes(): AgentMode[] {
    return Array.from(this._supportedModes);
  }

  /**
   * Sets backend-supported modes. If current mode becomes unsupported,
   * fires validation event.
   */
  public setSupportedModes(modes: AgentMode[]): void {
    this._supportedModes = new Set(modes);
    const validation = this.validateMode(this._currentMode);
    this._onDidValidateMode.fire(validation);
  }

  /**
   * Checks whether a specific mode is supported by backend.
   */
  public isModeSupported(mode: AgentMode | string): boolean {
    const normalized = normalizeAgentMode(mode);
    if (!normalized) {
      return false;
    }
    return this._supportedModes.has(normalized);
  }

  /**
   * Validates support status for a mode.
   */
  public validateMode(mode: AgentMode | string): ModeValidationResult {
    const normalized = normalizeAgentMode(mode);
    if (!normalized) {
      return {
        mode: this._currentMode,
        supported: false,
        reason: `Unknown agent mode: ${String(mode)}. Supported modes are Ask, Plan, Agent.`,
      };
    }

    const supported = this._supportedModes.has(normalized);
    return {
      mode: normalized,
      supported,
      reason: supported
        ? undefined
        : `Mode "${AGENT_MODES[normalized].name}" is not supported by the current backend daemon.`,
    };
  }

  /**
   * Sets the active mode. Validates backend support and persists selection.
   */
  public setMode(mode: AgentMode | string): ModeValidationResult {
    const normalized = normalizeAgentMode(mode);
    if (!normalized) {
      const result: ModeValidationResult = {
        mode: this._currentMode,
        supported: false,
        reason: `Invalid mode "${String(mode)}". Valid modes are: Ask, Plan, Agent.`,
      };
      this._onDidValidateMode.fire(result);
      return result;
    }

    const supported = this._supportedModes.has(normalized);
    const validation: ModeValidationResult = {
      mode: normalized,
      supported,
      reason: supported
        ? undefined
        : `Mode "${AGENT_MODES[normalized].name}" is not supported by the current backend daemon.`,
    };

    const previousMode = this._currentMode;
    this._currentMode = normalized;

    // Persist to Memento if supported
    if (this._memento && supported) {
      void this._memento.update(AGENT_STUDIOS_LAST_MODE_KEY, normalized);
    }

    if (previousMode !== normalized) {
      this._onDidChangeMode.fire(normalized);
    }

    this._onDidValidateMode.fire(validation);
    return validation;
  }

  /**
   * Returns list of all defined agent modes.
   */
  public getAllModes(): AgentModeDefinition[] {
    return [AGENT_MODES.ask, AGENT_MODES.plan, AGENT_MODES.agent];
  }

  /**
   * Returns metadata definition for a specific mode.
   */
  public getModeDefinition(mode: AgentMode): AgentModeDefinition {
    return AGENT_MODES[mode];
  }

  /**
   * Generates accessible HTML string for rendering within the Chat Webview.
   */
  public renderHtml(): string {
    const modes = this.getAllModes();
    const pills = modes
      .map((def) => {
        const isSelected = def.id === this._currentMode;
        const isSupported = this._supportedModes.has(def.id);
        const ariaChecked = isSelected ? 'true' : 'false';
        const classes = [
          'mode-pill',
          isSelected ? 'active' : '',
          !isSupported ? 'unsupported' : '',
        ]
          .filter(Boolean)
          .join(' ');

        const title = isSupported
          ? `${def.name}: ${def.description}`
          : `${def.name}: Unsupported by current backend runtime.`;

        const badgeHtml = !isSupported
          ? '<span class="mode-unsupported-badge" aria-label="unsupported">(unsupported)</span>'
          : '';

        return `
        <button
          type="button"
          role="radio"
          aria-checked="${ariaChecked}"
          aria-label="${def.name} mode - ${def.description}"
          class="${classes}"
          data-mode="${def.id}"
          title="${title}"
          ${!isSupported ? 'data-unsupported="true"' : ''}
        >
          <span class="codicon codicon-${def.icon}" aria-hidden="true"></span>
          <span class="mode-label">${def.name}</span>
          ${badgeHtml}
        </button>`.trim();
      })
      .join('\n');

    return `
    <div
      class="mode-selector-container"
      role="radiogroup"
      aria-label="Agent execution mode selection"
    >
      ${pills}
    </div>`.trim();
  }

  public dispose(): void {
    this._onDidChangeMode.dispose();
    this._onDidValidateMode.dispose();
  }
}
