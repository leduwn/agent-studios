/**
 * Agent Studios — Provider & Model Selector (Read-Only)
 * Milestone M14-UI-01: Professional AI Coding Assistant Experience
 *
 * Strictly READ-ONLY in webview: zero API keys in webview, zero secrets passed across IPC boundary!
 */

import * as vscode from 'vscode';
import { AgentStudiosBridge } from '../bridge/AgentStudiosBridge';
import { ProviderSummary } from '../bridge/types';

export type ProviderType =
  | 'anthropic'
  | 'openai'
  | 'gemini'
  | 'google'
  | 'openrouter'
  | 'ollama'
  | 'lmstudio'
  | 'custom'
  | string;

export type ProviderAvailability = 'available' | 'unconfigured' | 'offline';

export interface SafeModelSummary {
  readonly id: string;
  readonly name: string;
  readonly contextWindow: number;
  readonly formattedContextWindow: string;
  readonly isDefault?: boolean;
}

export interface SafeProviderSummary {
  readonly instanceId: string;
  readonly providerType: ProviderType;
  readonly displayName: string;
  readonly status: ProviderAvailability;
  readonly models: readonly SafeModelSummary[];
}

export interface SelectedProviderModel {
  readonly instanceId: string;
  readonly providerType: ProviderType;
  readonly providerName: string;
  readonly modelId: string;
  readonly modelName: string;
  readonly contextWindow: number;
  readonly formattedContextWindow: string;
  readonly status: ProviderAvailability;
}

export interface PersistedSelection {
  readonly instanceId: string;
  readonly modelId: string;
}

export const AGENT_STUDIOS_PROVIDER_MODEL_KEY = 'agentStudios.selectedProviderModel';

/**
 * Formats token count into human-readable context window badge (e.g. 200k tokens, 1M tokens).
 */
export function formatContextWindow(tokens: number): string {
  if (typeof tokens !== 'number' || isNaN(tokens) || tokens <= 0) {
    return '0 tokens';
  }
  if (tokens >= 1_000_000) {
    const millions = tokens / 1_000_000;
    const formatted = tokens % 1_000_000 === 0 ? millions.toString() : millions.toFixed(1);
    return `${formatted}M tokens`;
  }
  if (tokens >= 1_000) {
    const k = Math.round(tokens / 1_000);
    return `${k}k tokens`;
  }
  return `${tokens} tokens`;
}

/**
 * Strictly sanitizes raw provider summaries. Strips all API keys, secrets, tokens, or auth headers.
 * Guaranteed safe for transmission across Webview IPC boundaries.
 */
export function sanitizeProviderSummary(raw: unknown): SafeProviderSummary | null {
  if (!raw || typeof raw !== 'object') {
    return null;
  }

  const candidate = raw as Record<string, unknown>;

  const instanceId = typeof candidate.instanceId === 'string' ? candidate.instanceId : '';
  if (!instanceId) {
    return null;
  }

  const providerType = (
    typeof candidate.providerType === 'string' ? candidate.providerType.toLowerCase() : 'custom'
  ) as ProviderType;

  const displayName = typeof candidate.displayName === 'string'
    ? candidate.displayName
    : instanceId;

  let status: ProviderAvailability = 'offline';
  if (candidate.status === 'available') {
    status = 'available';
  } else if (candidate.status === 'unconfigured') {
    status = 'unconfigured';
  }

  const rawModels = Array.isArray(candidate.models) ? candidate.models : [];
  const safeModels: SafeModelSummary[] = [];

  for (const m of rawModels) {
    if (m && typeof m === 'object') {
      const modelObj = m as Record<string, unknown>;
      const modelId = typeof modelObj.id === 'string' ? modelObj.id : '';
      if (!modelId) {
        continue;
      }
      const modelName = typeof modelObj.name === 'string' ? modelObj.name : modelId;
      const contextWindow = typeof modelObj.contextWindow === 'number' && modelObj.contextWindow > 0
        ? modelObj.contextWindow
        : 8000;

      safeModels.push({
        id: modelId,
        name: modelName,
        contextWindow,
        formattedContextWindow: formatContextWindow(contextWindow),
        isDefault: Boolean(modelObj.isDefault),
      });
    }
  }

  return {
    instanceId,
    providerType,
    displayName,
    status,
    models: Object.freeze(safeModels),
  };
}

/**
 * Normalizes and categorizes provider types for UI icons and grouping.
 */
export function getProviderCategoryLabel(providerType: ProviderType): string {
  switch (providerType.toLowerCase()) {
    case 'anthropic':
      return 'Anthropic';
    case 'openai':
      return 'OpenAI';
    case 'gemini':
    case 'google':
      return 'Google Gemini';
    case 'openrouter':
      return 'OpenRouter';
    case 'ollama':
      return 'Local Ollama';
    case 'lmstudio':
    case 'lm-studio':
      return 'LM Studio';
    default:
      return 'Custom Endpoint';
  }
}

/**
 * Read-Only Provider & Model Selector controller managing available runtime
 * models, fallback resolution, persistence, and safe webview HTML generation.
 */
export class ProviderModelSelector implements vscode.Disposable {
  private _bridge: AgentStudiosBridge;
  private _memento?: vscode.Memento;
  private _providers: SafeProviderSummary[] = [];
  private _selected: SelectedProviderModel | null = null;

  private readonly _onDidChangeSelection = new vscode.EventEmitter<SelectedProviderModel | null>();
  public readonly onDidChangeSelection: vscode.Event<SelectedProviderModel | null> =
    this._onDidChangeSelection.event;

  private readonly _onDidRefreshProviders = new vscode.EventEmitter<SafeProviderSummary[]>();
  public readonly onDidRefreshProviders: vscode.Event<SafeProviderSummary[]> =
    this._onDidRefreshProviders.event;

  constructor(bridge: AgentStudiosBridge, memento?: vscode.Memento) {
    this._bridge = bridge;
    this._memento = memento;
  }

  public get providers(): SafeProviderSummary[] {
    return [...this._providers];
  }

  public get selected(): SelectedProviderModel | null {
    return this._selected ? { ...this._selected } : null;
  }

  /**
   * Refreshes provider list from bridge, sanitizes records, and computes fallback selection.
   */
  public async refresh(): Promise<SafeProviderSummary[]> {
    let rawSummaries: ProviderSummary[] = [];
    try {
      rawSummaries = await this._bridge.listProviderSummaries();
    } catch {
      rawSummaries = [];
    }

    const safeList: SafeProviderSummary[] = [];
    for (const raw of rawSummaries) {
      const sanitized = sanitizeProviderSummary(raw);
      if (sanitized && sanitized.models.length > 0) {
        safeList.push(sanitized);
      }
    }

    this._providers = safeList;
    this._resolveSelection();
    this._onDidRefreshProviders.fire(this.providers);
    return this.providers;
  }

  /**
   * Manually sets providers (useful for initialization or testing).
   */
  public setProviders(summaries: unknown[]): void {
    const safeList: SafeProviderSummary[] = [];
    for (const raw of summaries) {
      const sanitized = sanitizeProviderSummary(raw);
      if (sanitized && sanitized.models.length > 0) {
        safeList.push(sanitized);
      }
    }
    this._providers = safeList;
    this._resolveSelection();
    this._onDidRefreshProviders.fire(this.providers);
  }

  /**
   * Sets the active bridge instance.
   */
  public setBridge(bridge: AgentStudiosBridge): void {
    this._bridge = bridge;
  }

  /**
   * Selects a specific provider instance and model.
   * Returns true if selection was valid and applied; false otherwise.
   */
  public select(instanceId: string, modelId: string): boolean {
    const provider = this._providers.find((p) => p.instanceId === instanceId);
    if (!provider) {
      return false;
    }

    const model = provider.models.find((m) => m.id === modelId);
    if (!model) {
      return false;
    }

    const newSelection: SelectedProviderModel = {
      instanceId: provider.instanceId,
      providerType: provider.providerType,
      providerName: provider.displayName,
      modelId: model.id,
      modelName: model.name,
      contextWindow: model.contextWindow,
      formattedContextWindow: model.formattedContextWindow,
      status: provider.status,
    };

    this._selected = newSelection;

    // Persist user selection
    if (this._memento) {
      const persisted: PersistedSelection = {
        instanceId: provider.instanceId,
        modelId: model.id,
      };
      void this._memento.update(AGENT_STUDIOS_PROVIDER_MODEL_KEY, persisted);
    }

    this._onDidChangeSelection.fire(this.selected);
    return true;
  }

  /**
   * Resolves current selection considering persisted choice and fallback rules.
   *
   * Fallback rules:
   * 1. If stored selection exists and is valid -> use it.
   * 2. If stored provider exists but model does not -> use provider's default or first model.
   * 3. Fall back to first available provider with default or first model.
   * 4. Fall back to first provider's default or first model regardless of availability status.
   * 5. Fall back to null if no providers exist.
   */
  private _resolveSelection(): void {
    const persisted = this._memento?.get<PersistedSelection>(AGENT_STUDIOS_PROVIDER_MODEL_KEY);

    if (this._providers.length === 0) {
      this._selected = null;
      this._onDidChangeSelection.fire(null);
      return;
    }

    // 1. Try restoring persisted provider and model
    if (persisted && persisted.instanceId && persisted.modelId) {
      const provider = this._providers.find((p) => p.instanceId === persisted.instanceId);
      if (provider) {
        const exactModel = provider.models.find((m) => m.id === persisted.modelId);
        if (exactModel) {
          this._applySelection(provider, exactModel);
          return;
        }

        // Provider found, model missing -> fall back to provider default
        const defaultModel = provider.models.find((m) => m.isDefault) ?? provider.models[0];
        if (defaultModel) {
          this._applySelection(provider, defaultModel);
          return;
        }
      }
    }

    // 2. Try current selection if still valid
    if (this._selected) {
      const currentProvider = this._providers.find((p) => p.instanceId === this._selected?.instanceId);
      if (currentProvider) {
        const currentModel = currentProvider.models.find((m) => m.id === this._selected?.modelId);
        if (currentModel) {
          this._applySelection(currentProvider, currentModel);
          return;
        }
      }
    }

    // 3. Fall back to first available provider
    const availableProvider = this._providers.find((p) => p.status === 'available');
    if (availableProvider) {
      const model = availableProvider.models.find((m) => m.isDefault) ?? availableProvider.models[0];
      if (model) {
        this._applySelection(availableProvider, model);
        return;
      }
    }

    // 4. Fall back to first provider (even if unconfigured or offline)
    const firstProvider = this._providers[0];
    const fallbackModel = firstProvider.models.find((m) => m.isDefault) ?? firstProvider.models[0];
    if (fallbackModel) {
      this._applySelection(firstProvider, fallbackModel);
      return;
    }

    this._selected = null;
    this._onDidChangeSelection.fire(null);
  }

  private _applySelection(provider: SafeProviderSummary, model: SafeModelSummary): void {
    const next: SelectedProviderModel = {
      instanceId: provider.instanceId,
      providerType: provider.providerType,
      providerName: provider.displayName,
      modelId: model.id,
      modelName: model.name,
      contextWindow: model.contextWindow,
      formattedContextWindow: model.formattedContextWindow,
      status: provider.status,
    };

    const hasChanged =
      !this._selected ||
      this._selected.instanceId !== next.instanceId ||
      this._selected.modelId !== next.modelId ||
      this._selected.status !== next.status;

    this._selected = next;

    if (hasChanged) {
      this._onDidChangeSelection.fire(this.selected);
    }
  }

  /**
   * Generates strictly READ-ONLY accessible HTML for the Chat Webview.
   * Guaranteed ZERO secrets / API keys in markup.
   */
  public renderHtml(): string {
    if (this._providers.length === 0) {
      return `
      <div class="provider-model-selector empty" aria-label="Provider and Model Selection">
        <span class="status-indicator status-offline" aria-hidden="true"></span>
        <span class="provider-empty-label">No AI providers connected</span>
      </div>`.trim();
    }

    const currentKey = this._selected
      ? `${this._selected.instanceId}::${this._selected.modelId}`
      : '';

    const optionsHtml = this._providers
      .map((provider) => {
        const category = getProviderCategoryLabel(provider.providerType);
        const statusBadge = `[${provider.status.toUpperCase()}]`;

        const modelOptions = provider.models
          .map((m) => {
            const key = `${provider.instanceId}::${m.id}`;
            const isSelected = key === currentKey;
            const text = `${m.name} (${m.formattedContextWindow}) - ${provider.displayName} ${statusBadge}`;
            return `<option value="${key}" ${isSelected ? 'selected' : ''}>${escapeHtml(text)}</option>`;
          })
          .join('\n');

        return `
        <optgroup label="${escapeHtml(category + ' — ' + provider.displayName + ' (' + provider.status + ')')}" data-status="${provider.status}">
          ${modelOptions}
        </optgroup>`.trim();
      })
      .join('\n');

    const statusClass = this._selected ? `status-${this._selected.status}` : 'status-offline';
    const statusTooltip = this._selected
      ? `${this._selected.providerName} (${this._selected.status})`
      : 'No model selected';

    const contextBadge = this._selected
      ? `<span class="context-window-badge" title="Context Window Limit: ${this._selected.formattedContextWindow}">${this._selected.formattedContextWindow}</span>`
      : '';

    return `
    <div class="provider-model-selector" aria-label="AI Provider and Model Selector">
      <span class="status-dot ${statusClass}" title="${escapeHtml(statusTooltip)}" aria-hidden="true"></span>
      <select
        id="provider-model-select"
        class="provider-model-dropdown"
        role="combobox"
        aria-label="Select AI Model and Provider"
        data-current="${escapeHtml(currentKey)}"
      >
        ${optionsHtml}
      </select>
      ${contextBadge}
    </div>`.trim();
  }

  public dispose(): void {
    this._onDidChangeSelection.dispose();
    this._onDidRefreshProviders.dispose();
  }
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}
