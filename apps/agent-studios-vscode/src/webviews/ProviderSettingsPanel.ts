/**
 * Agent Studios — Providers & Models Settings WebviewPanel
 * Allows adding, editing, testing, discovering models, and setting defaults
 * for all 12 supported provider ecosystems + custom OpenAI-compatible endpoints.
 * API keys are strictly write-only and persisted via vscode.SecretStorage -> DaemonSecretVault.
 */

import * as crypto from 'crypto';
import * as vscode from 'vscode';
import { AgentStudiosBridge } from '../bridge/AgentStudiosBridge';
import { ProviderModel } from '../bridge/types';
import {
  DEFAULT_BASE_URLS,
  ProviderProfileConfig,
  ProviderProfileManager,
  SupportedProviderKind,
} from '../commands/ProviderManagementCommands';
import { AGENT_STUDIOS_PROVIDER_MODEL_KEY } from '../controls/ProviderModelSelector';
import { escapeHtml } from './markdownRenderer';

export interface ProviderPresetMeta {
  kind: SupportedProviderKind;
  label: string;
  defaultProtocol: ProviderProfileConfig['protocol'];
  defaultBaseUrl: string;
  requiresKey: boolean;
}

export const PROVIDER_PRESETS: readonly ProviderPresetMeta[] = [
  { kind: 'openai', label: 'OpenAI', defaultProtocol: 'openai_responses', defaultBaseUrl: DEFAULT_BASE_URLS.openai, requiresKey: true },
  { kind: 'anthropic', label: 'Anthropic', defaultProtocol: 'anthropic_messages', defaultBaseUrl: DEFAULT_BASE_URLS.anthropic, requiresKey: true },
  { kind: 'google-gemini', label: 'Google Gemini', defaultProtocol: 'gemini_generate_content', defaultBaseUrl: DEFAULT_BASE_URLS['google-gemini'], requiresKey: true },
  { kind: 'deepseek', label: 'DeepSeek', defaultProtocol: 'openai_chat_completions', defaultBaseUrl: DEFAULT_BASE_URLS.deepseek, requiresKey: true },
  { kind: 'openrouter', label: 'OpenRouter', defaultProtocol: 'openai_chat_completions', defaultBaseUrl: DEFAULT_BASE_URLS.openrouter, requiresKey: true },
  { kind: 'groq', label: 'Groq', defaultProtocol: 'openai_chat_completions', defaultBaseUrl: DEFAULT_BASE_URLS.groq, requiresKey: true },
  { kind: 'ollama', label: 'Ollama (Local)', defaultProtocol: 'openai_chat_completions', defaultBaseUrl: DEFAULT_BASE_URLS.ollama, requiresKey: false },
  { kind: 'lm-studio', label: 'LM Studio (Local)', defaultProtocol: 'openai_chat_completions', defaultBaseUrl: DEFAULT_BASE_URLS['lm-studio'], requiresKey: false },
  { kind: 'vllm', label: 'vLLM (Local)', defaultProtocol: 'openai_chat_completions', defaultBaseUrl: DEFAULT_BASE_URLS.vllm, requiresKey: false },
  { kind: 'litellm', label: 'LiteLLM Proxy', defaultProtocol: 'openai_chat_completions', defaultBaseUrl: DEFAULT_BASE_URLS.litellm, requiresKey: true },
  { kind: '9router', label: '9Router', defaultProtocol: 'openai_chat_completions', defaultBaseUrl: DEFAULT_BASE_URLS['9router'], requiresKey: true },
  { kind: 'custom', label: 'Custom OpenAI-Compatible', defaultProtocol: 'openai_chat_completions', defaultBaseUrl: DEFAULT_BASE_URLS.custom, requiresKey: false },
];

export interface SafeProviderProfileView {
  instanceId: string;
  providerType: SupportedProviderKind;
  displayName: string;
  protocol: ProviderProfileConfig['protocol'];
  baseUrl: string;
  secretSource: 'env' | 'secret_storage' | 'none';
  hasStoredSecret: boolean;
  envVarName?: string;
  defaultModel?: string;
  discoveredModels: ProviderModel[];
  isDefault: boolean;
}

export interface ProviderSettingsTestStatus {
  instanceId?: string;
  ok: boolean;
  message: string;
}

export interface ProviderSettingsViewState {
  profiles: SafeProviderProfileView[];
  editingInstanceId: string | null;
  defaultInstanceId?: string;
  defaultModelId?: string;
  daemonConnected: boolean;
  statusBanner?: ProviderSettingsTestStatus | null;
}

export class ProviderSettingsPanel implements vscode.Disposable {
  public static readonly viewType = 'agentStudios.providerSettings';
  public static currentPanel: ProviderSettingsPanel | undefined;

  private readonly _panel: vscode.WebviewPanel;
  private readonly _context: vscode.ExtensionContext;
  private readonly _bridgeProvider: () => AgentStudiosBridge;
  private readonly _profileManager: ProviderProfileManager;
  private readonly _onProvidersChanged?: () => void | Promise<void>;
  private readonly _disposables: vscode.Disposable[] = [];

  private _editingInstanceId: string | null = null;
  private _statusBanner: ProviderSettingsTestStatus | null = null;

  public static createOrShow(
    context: vscode.ExtensionContext,
    bridgeProvider: () => AgentStudiosBridge,
    onProvidersChanged?: () => void | Promise<void>
  ): ProviderSettingsPanel {
    const column = vscode.window.activeTextEditor?.viewColumn ?? vscode.ViewColumn.One;
    if (ProviderSettingsPanel.currentPanel) {
      ProviderSettingsPanel.currentPanel._panel.reveal(column);
      void ProviderSettingsPanel.currentPanel.refresh();
      return ProviderSettingsPanel.currentPanel;
    }

    const panel = vscode.window.createWebviewPanel(
      ProviderSettingsPanel.viewType,
      'Providers & Models',
      column,
      {
        enableScripts: true,
        retainContextWhenHidden: true,
      }
    );

    ProviderSettingsPanel.currentPanel = new ProviderSettingsPanel(
      panel,
      context,
      bridgeProvider,
      onProvidersChanged
    );
    return ProviderSettingsPanel.currentPanel;
  }

  private constructor(
    panel: vscode.WebviewPanel,
    context: vscode.ExtensionContext,
    bridgeProvider: () => AgentStudiosBridge,
    onProvidersChanged?: () => void | Promise<void>
  ) {
    this._panel = panel;
    this._context = context;
    this._bridgeProvider = bridgeProvider;
    this._profileManager = new ProviderProfileManager(context);
    this._onProvidersChanged = onProvidersChanged;

    this._panel.onDidDispose(() => this.dispose(), null, this._disposables);
    this._panel.webview.onDidReceiveMessage(
      (msg) => void this.handleMessage(msg),
      null,
      this._disposables
    );

    void this.refresh();
  }

  public async refresh(): Promise<void> {
    const state = await this.buildViewState();
    this._panel.webview.html = ProviderSettingsPanel.buildHtml(this._panel.webview, state);
  }

  public async buildViewState(): Promise<ProviderSettingsViewState> {
    const profiles = this._profileManager.listProfiles();
    const persistedDefault = this._context.globalState.get<{
      instanceId?: string;
      modelId?: string;
    }>(AGENT_STUDIOS_PROVIDER_MODEL_KEY);

    const safeProfiles: SafeProviderProfileView[] = [];
    for (const p of profiles) {
      let hasStoredSecret = false;
      if (p.secretRef.source === 'secret_storage' && p.secretRef.key) {
        const stored = await this._profileManager.getSecretFromExtensionVault(p.secretRef.key);
        hasStoredSecret = Boolean(stored && stored.trim().length > 0);
      } else if (p.secretRef.source === 'env' && p.secretRef.key) {
        hasStoredSecret = Boolean(process.env[p.secretRef.key]);
      }

      safeProfiles.push({
        instanceId: p.instanceId,
        providerType: p.providerType,
        displayName: p.displayName,
        protocol: p.protocol,
        baseUrl: p.baseUrl,
        secretSource: p.secretRef.source,
        hasStoredSecret,
        envVarName: p.secretRef.source === 'env' ? p.secretRef.key : undefined,
        defaultModel: p.defaultModel,
        discoveredModels: Array.isArray(p.discoveredModels) ? [...p.discoveredModels] : [],
        isDefault: persistedDefault?.instanceId === p.instanceId,
      });
    }

    if (!this._editingInstanceId && safeProfiles.length > 0) {
      this._editingInstanceId =
        safeProfiles.find((p) => p.isDefault)?.instanceId ?? safeProfiles[0].instanceId;
    }

    return {
      profiles: safeProfiles,
      editingInstanceId: this._editingInstanceId,
      defaultInstanceId: persistedDefault?.instanceId ?? safeProfiles[0]?.instanceId,
      defaultModelId: persistedDefault?.modelId,
      daemonConnected: this._bridgeProvider().connectionState === 'connected',
      statusBanner: this._statusBanner,
    };
  }

  public async handleMessage(message: unknown): Promise<void> {
    if (!message || typeof message !== 'object') {
      return;
    }
    const msg = message as Record<string, unknown>;
    const command = typeof msg.command === 'string' ? msg.command : '';

    switch (command) {
      case 'selectProfile': {
        this._editingInstanceId =
          typeof msg.instanceId === 'string' && msg.instanceId ? msg.instanceId : null;
        this._statusBanner = null;
        await this.refresh();
        break;
      }
      case 'newProfile': {
        this._editingInstanceId = '__new__';
        this._statusBanner = null;
        await this.refresh();
        break;
      }
      case 'saveProvider': {
        await this.handleSaveProvider(msg);
        break;
      }
      case 'deleteProvider': {
        await this.handleDeleteProvider(
          typeof msg.instanceId === 'string' ? msg.instanceId : ''
        );
        break;
      }
      case 'setDefaultProvider': {
        await this.handleSetDefault(
          typeof msg.instanceId === 'string' ? msg.instanceId : '',
          typeof msg.modelId === 'string' ? msg.modelId : undefined
        );
        break;
      }
      case 'discoverModels': {
        await this.handleDiscoverModels(msg);
        break;
      }
      case 'testConnection': {
        await this.handleTestConnection(msg);
        break;
      }
      default:
        break;
    }
  }

  private async handleSaveProvider(
    msg: Record<string, unknown>
  ): Promise<ProviderProfileConfig | null> {
    const providerType = (
      typeof msg.providerType === 'string' ? msg.providerType : 'openai'
    ) as SupportedProviderKind;
    const displayName =
      typeof msg.displayName === 'string' && msg.displayName.trim()
        ? msg.displayName.trim()
        : PROVIDER_PRESETS.find((p) => p.kind === providerType)?.label || providerType;
    const protocol = (
      typeof msg.protocol === 'string'
        ? msg.protocol
        : PROVIDER_PRESETS.find((p) => p.kind === providerType)?.defaultProtocol ??
          'openai_chat_completions'
    ) as ProviderProfileConfig['protocol'];
    const baseUrl =
      typeof msg.baseUrl === 'string' && msg.baseUrl.trim()
        ? msg.baseUrl.trim()
        : DEFAULT_BASE_URLS[providerType] ?? DEFAULT_BASE_URLS.custom;

    if (!/^https?:\/\//i.test(baseUrl)) {
      this._statusBanner = {
        ok: false,
        message: 'Base URL must begin with http:// or https://.',
      };
      await this.refresh();
      return null;
    }

    const rawId =
      typeof msg.instanceId === 'string' &&
      msg.instanceId.trim() &&
      msg.instanceId !== '__new__'
        ? msg.instanceId.trim()
        : crypto.randomUUID();

    const existing = this._profileManager
      .listProfiles()
      .find((p) => p.instanceId === rawId);
    const apiKeyInput = typeof msg.apiKey === 'string' ? msg.apiKey.trim() : '';
    const envVarInput = typeof msg.envVarName === 'string' ? msg.envVarName.trim() : '';
    const secretSourceMode =
      msg.secretSource === 'env' ? 'env' : msg.secretSource === 'none' ? 'none' : 'secret_storage';
    const defaultModel =
      typeof msg.defaultModel === 'string' && msg.defaultModel.trim()
        ? msg.defaultModel.trim()
        : existing?.defaultModel;

    let secretSource: 'env' | 'secret_storage' | 'none' = existing?.secretRef.source ?? 'none';
    let secretRefKey = existing?.secretRef.key ?? '';
    let rawSecretToSync: string | undefined;

    if (secretSourceMode === 'env' && envVarInput) {
      secretSource = 'env';
      secretRefKey = envVarInput;
    } else if (secretSourceMode === 'none' && !apiKeyInput) {
      secretSource = 'none';
      secretRefKey = '';
    } else if (apiKeyInput) {
      const handle = `agentStudios.secret.${rawId}`;
      await this._profileManager.storeSecretInExtensionVault(handle, apiKeyInput);
      secretSource = 'secret_storage';
      secretRefKey = handle;
      rawSecretToSync = apiKeyInput;
    }

    const saved = await this._profileManager.upsertProfile({
      instanceId: rawId,
      providerType,
      displayName,
      protocol,
      baseUrl,
      secretRefKey,
      secretSource,
      defaultModel,
    });

    if (existing?.discoveredModels && existing.discoveredModels.length > 0) {
      // Preserve previously discovered models unless overwritten
      const profiles = this._profileManager.listProfiles().map((p) =>
        p.instanceId === saved.instanceId
          ? { ...p, discoveredModels: existing.discoveredModels }
          : p
      );
      await this._context.globalState.update('agentStudios.providerProfiles.v1', profiles);
    }

    const bridge = this._bridgeProvider();
    if (bridge.connectionState === 'connected' && bridge.upsertProviderInstance) {
      const syncModels = saved.defaultModel
        ? [
            {
              id: saved.defaultModel,
              name: saved.defaultModel,
              contextWindow: 128000,
              isDefault: true,
            },
          ]
        : existing?.discoveredModels ?? [];
      await bridge.upsertProviderInstance({
        instanceId: saved.instanceId,
        providerType: saved.providerType,
        displayName: saved.displayName,
        protocol: saved.protocol,
        baseUrl: saved.baseUrl,
        secretEnvVar: saved.secretRef.source === 'env' ? saved.secretRef.key : undefined,
        secretLocator:
          saved.secretRef.source === 'secret_storage' ? saved.secretRef.key : undefined,
        secretBackend:
          saved.secretRef.source === 'secret_storage'
            ? 'os_credential_store'
            : 'environment_variable',
        secretValue: rawSecretToSync,
        models: syncModels,
      });
    }

    if (msg.makeDefault === true || this._profileManager.listProfiles().length === 1) {
      await this._context.globalState.update(AGENT_STUDIOS_PROVIDER_MODEL_KEY, {
        instanceId: saved.instanceId,
        modelId: saved.defaultModel || '',
      });
    }

    this._editingInstanceId = saved.instanceId;
    this._statusBanner = {
      instanceId: saved.instanceId,
      ok: true,
      message: `Saved ${saved.displayName}${secretSource === 'secret_storage' ? ' (API key stored securely)' : ''}.`,
    };
    await this._onProvidersChanged?.();
    await this.refresh();
    return saved;
  }

  private async handleDeleteProvider(instanceId: string): Promise<void> {
    if (!instanceId) {
      return;
    }
    const profiles = this._profileManager.listProfiles();
    const target = profiles.find((p) => p.instanceId === instanceId);
    await this._profileManager.removeProfile(instanceId);

    if (target?.secretRef.source === 'secret_storage' && target.secretRef.key) {
      await this._context.secrets?.delete?.(target.secretRef.key);
    }

    const bridge = this._bridgeProvider();
    if (bridge.connectionState === 'connected' && bridge.removeProviderInstance) {
      await bridge.removeProviderInstance(instanceId).catch(() => false);
    }

    this._editingInstanceId = null;
    this._statusBanner = {
      ok: true,
      message: `Removed provider${target ? ` "${target.displayName}"` : ''}.`,
    };
    await this._onProvidersChanged?.();
    await this.refresh();
  }

  private async handleSetDefault(instanceId: string, modelId?: string): Promise<void> {
    const profile = this._profileManager
      .listProfiles()
      .find((p) => p.instanceId === instanceId);
    if (!profile) {
      return;
    }
    const chosenModel =
      (modelId && modelId.trim()) ||
      profile.defaultModel ||
      profile.discoveredModels?.[0]?.id ||
      '';
    await this._profileManager.upsertProfile({
      instanceId: profile.instanceId,
      providerType: profile.providerType,
      displayName: profile.displayName,
      protocol: profile.protocol,
      baseUrl: profile.baseUrl,
      secretRefKey: profile.secretRef.key,
      secretSource: profile.secretRef.source,
      defaultModel: chosenModel || undefined,
    });
    await this._context.globalState.update(AGENT_STUDIOS_PROVIDER_MODEL_KEY, {
      instanceId: profile.instanceId,
      modelId: chosenModel,
    });
    this._statusBanner = {
      instanceId,
      ok: true,
      message: `Default model set to ${chosenModel || profile.displayName}.`,
    };
    await this._onProvidersChanged?.();
    await this.refresh();
  }

  private async handleDiscoverModels(msg: Record<string, unknown>): Promise<void> {
    const saved = await this.handleSaveProvider(msg);
    if (!saved) {
      return;
    }
    try {
      const models = await this._profileManager.discoverModelsDynamic(
        saved,
        this._bridgeProvider()
      );
      this._statusBanner = {
        instanceId: saved.instanceId,
        ok: true,
        message:
          models.length > 0
            ? `Discovered ${models.length} model(s) from ${saved.displayName}.`
            : `Connected to ${saved.displayName}, but no models were returned. Enter a custom model ID below.`,
      };
      await this._onProvidersChanged?.();
    } catch (err: unknown) {
      this._statusBanner = {
        instanceId: saved.instanceId,
        ok: false,
        message: `Model discovery failed: ${err instanceof Error ? err.message : String(err)}`,
      };
    }
    await this.refresh();
  }

  private async handleTestConnection(msg: Record<string, unknown>): Promise<void> {
    const saved = await this.handleSaveProvider(msg);
    if (!saved) {
      return;
    }
    const started = Date.now();
    try {
      const bridge = this._bridgeProvider();
      if (bridge.connectionState === 'connected' && bridge.testProviderConnection) {
        const res = await bridge.testProviderConnection(saved.instanceId);
        this._statusBanner = {
          instanceId: saved.instanceId,
          ok: res.ok,
          message: res.ok
            ? `Connection verified (${Date.now() - started}ms).`
            : `Connection check failed: ${res.status}`,
        };
      } else {
        const models = await this._profileManager.discoverModelsDynamic(saved);
        this._statusBanner = {
          instanceId: saved.instanceId,
          ok: true,
          message: `Endpoint reachable (${Date.now() - started}ms, ${models.length} model(s) found).`,
        };
      }
      await this._onProvidersChanged?.();
    } catch (err: unknown) {
      this._statusBanner = {
        instanceId: saved.instanceId,
        ok: false,
        message: `Connection failed: ${err instanceof Error ? err.message : String(err)}`,
      };
    }
    await this.refresh();
  }

  public static buildHtml(
    webview: vscode.Webview,
    state: ProviderSettingsViewState
  ): string {
    const nonce = crypto.randomBytes(16).toString('base64');
    const isCreatingNew =
      state.editingInstanceId === '__new__' || state.profiles.length === 0;
    const activeProfile = !isCreatingNew
      ? state.profiles.find((p) => p.instanceId === state.editingInstanceId) ??
        state.profiles[0]
      : undefined;

    const selectedKind: SupportedProviderKind = activeProfile?.providerType ?? 'openai';
    const selectedPreset =
      PROVIDER_PRESETS.find((p) => p.kind === selectedKind) ?? PROVIDER_PRESETS[0];

    const profilesListHtml =
      state.profiles.length > 0
        ? state.profiles
            .map((p) => {
              const isSelected = !isCreatingNew && p.instanceId === activeProfile?.instanceId;
              const keyLabel = p.hasStoredSecret
                ? p.secretSource === 'env'
                  ? `Env: ${p.envVarName}`
                  : 'Key saved'
                : p.secretSource === 'none'
                  ? 'Local / No key'
                  : 'No key';
              return `
                <button class="provider-list-item ${isSelected ? 'active' : ''}" data-select-id="${escapeHtml(p.instanceId)}">
                  <div class="provider-item-top">
                    <span class="provider-item-name">${escapeHtml(p.displayName)}</span>
                    ${p.isDefault ? '<span class="pill-default">Default</span>' : ''}
                  </div>
                  <div class="provider-item-meta">
                    <span>${escapeHtml(p.providerType)}</span>
                    <span>·</span>
                    <span>${escapeHtml(p.defaultModel || 'No model')}</span>
                    <span>·</span>
                    <span>${escapeHtml(keyLabel)}</span>
                  </div>
                </button>
              `;
            })
            .join('')
        : '<div class="empty-list">No providers configured yet. Add one to get started.</div>';

    const presetOptionsHtml = PROVIDER_PRESETS.map(
      (p) =>
        `<option value="${escapeHtml(p.kind)}" ${p.kind === selectedKind ? 'selected' : ''}>${escapeHtml(p.label)}</option>`
    ).join('');

    const protocolOptions: Array<{ id: ProviderProfileConfig['protocol']; label: string }> = [
      { id: 'openai_responses', label: 'OpenAI Responses API' },
      { id: 'openai_chat_completions', label: 'OpenAI Chat Completions (/v1/chat/completions)' },
      { id: 'anthropic_messages', label: 'Anthropic Messages (/v1/messages)' },
      { id: 'gemini_generate_content', label: 'Google Gemini GenerateContent' },
    ];
    const currentProtocol = activeProfile?.protocol ?? selectedPreset.defaultProtocol;
    const protocolOptionsHtml = protocolOptions
      .map(
        (pr) =>
          `<option value="${pr.id}" ${pr.id === currentProtocol ? 'selected' : ''}>${escapeHtml(pr.label)}</option>`
      )
      .join('');

    const discoveredChipsHtml =
      activeProfile && activeProfile.discoveredModels.length > 0
        ? activeProfile.discoveredModels
            .map((m) => {
              const active = m.id === activeProfile.defaultModel;
              return `<button type="button" class="model-chip ${active ? 'selected' : ''}" data-pick-model="${escapeHtml(m.id)}">${escapeHtml(m.id)}</button>`;
            })
            .join('')
        : '<span class="hint-text">Click “Discover Models” or enter any model ID manually.</span>';

    const bannerHtml = state.statusBanner
      ? `<div class="status-banner ${state.statusBanner.ok ? 'status-ok' : 'status-error'}" role="status">${escapeHtml(state.statusBanner.message)}</div>`
      : '';

    const keyHint = activeProfile?.hasStoredSecret
      ? activeProfile.secretSource === 'secret_storage'
        ? 'API key is saved in VS Code SecretStorage. Leave blank to keep existing key.'
        : `Using environment variable ${activeProfile.envVarName}.`
      : selectedPreset.requiresKey
        ? 'Stored encrypted in VS Code SecretStorage (never exposed in plain JSON).'
        : 'Optional for local runtimes like Ollama or LM Studio.';

    const presetsJson = JSON.stringify(PROVIDER_PRESETS).replace(/</g, '\\u003c');

    return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src ${webview.cspSource} 'unsafe-inline'; script-src 'nonce-${nonce}';">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Agent Studios — Providers & Models</title>
  <style>
    *, *::before, *::after { box-sizing: border-box; }
    body {
      margin: 0;
      padding: 20px;
      font-family: var(--vscode-font-family, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif);
      font-size: var(--vscode-font-size, 13px);
      color: var(--vscode-foreground);
      background: var(--vscode-editor-background);
      line-height: 1.45;
    }
    .settings-shell {
      max-width: 960px;
      margin: 0 auto;
      display: flex;
      flex-direction: column;
      gap: 16px;
    }
    .settings-header {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      padding-bottom: 12px;
      border-bottom: 1px solid var(--vscode-panel-border, var(--vscode-widget-border));
    }
    .settings-title h1 {
      margin: 0;
      font-size: 16px;
      font-weight: 600;
    }
    .settings-title p {
      margin: 2px 0 0;
      font-size: 12px;
      color: var(--vscode-descriptionForeground);
    }
    .settings-grid {
      display: grid;
      grid-template-columns: 280px 1fr;
      gap: 20px;
      align-items: start;
    }
    @media (max-width: 680px) {
      .settings-grid { grid-template-columns: 1fr; }
    }
    .panel-card {
      background: var(--vscode-sideBar-background, var(--vscode-editorWidget-background));
      border: 1px solid var(--vscode-widget-border, var(--vscode-panel-border));
      border-radius: 6px;
      padding: 14px;
    }
    .sidebar-head {
      display: flex;
      justify-content: space-between;
      align-items: center;
      margin-bottom: 10px;
    }
    .sidebar-head span {
      font-size: 11px;
      font-weight: 600;
      text-transform: uppercase;
      letter-spacing: 0.04em;
      color: var(--vscode-descriptionForeground);
    }
    .provider-list {
      display: flex;
      flex-direction: column;
      gap: 6px;
    }
    .provider-list-item {
      text-align: left;
      width: 100%;
      padding: 8px 10px;
      border-radius: 4px;
      border: 1px solid transparent;
      background: transparent;
      color: var(--vscode-foreground);
      cursor: pointer;
    }
    .provider-list-item:hover {
      background: var(--vscode-list-hoverBackground);
    }
    .provider-list-item.active {
      background: var(--vscode-list-hoverBackground);
      border-color: var(--vscode-focusBorder);
    }
    .provider-item-top {
      display: flex;
      justify-content: space-between;
      align-items: center;
      gap: 6px;
    }
    .provider-item-name { font-weight: 600; }
    .provider-item-meta {
      margin-top: 2px;
      font-size: 11px;
      color: var(--vscode-descriptionForeground);
      display: flex;
      gap: 5px;
      flex-wrap: wrap;
    }
    .pill-default {
      font-size: 10px;
      padding: 1px 6px;
      border-radius: 999px;
      background: var(--vscode-badge-background);
      color: var(--vscode-badge-foreground);
    }
    .form-grid {
      display: grid;
      grid-template-columns: 1fr 1fr;
      gap: 12px;
    }
    @media (max-width: 560px) {
      .form-grid { grid-template-columns: 1fr; }
    }
    .field {
      display: flex;
      flex-direction: column;
      gap: 4px;
    }
    .field-full { grid-column: 1 / -1; }
    label {
      font-size: 12px;
      font-weight: 500;
      color: var(--vscode-foreground);
    }
    input, select {
      width: 100%;
      padding: 6px 8px;
      font-size: 13px;
      font-family: inherit;
      color: var(--vscode-input-foreground);
      background: var(--vscode-input-background);
      border: 1px solid var(--vscode-input-border, var(--vscode-widget-border));
      border-radius: 4px;
    }
    input:focus, select:focus {
      outline: 1px solid var(--vscode-focusBorder);
      border-color: var(--vscode-focusBorder);
    }
    .hint-text {
      font-size: 11px;
      color: var(--vscode-descriptionForeground);
    }
    .model-chips {
      display: flex;
      flex-wrap: wrap;
      gap: 6px;
      margin-top: 6px;
    }
    .model-chip {
      font-size: 11px;
      padding: 3px 8px;
      border-radius: 4px;
      border: 1px solid var(--vscode-widget-border, var(--vscode-input-border));
      background: var(--vscode-editor-background);
      color: var(--vscode-foreground);
      cursor: pointer;
    }
    .model-chip.selected {
      border-color: var(--vscode-focusBorder);
      background: var(--vscode-button-background);
      color: var(--vscode-button-foreground);
    }
    .action-row {
      display: flex;
      flex-wrap: wrap;
      gap: 8px;
      justify-content: space-between;
      align-items: center;
      margin-top: 14px;
      padding-top: 12px;
      border-top: 1px solid var(--vscode-panel-border, var(--vscode-widget-border));
    }
    .action-group { display: flex; flex-wrap: wrap; gap: 8px; }
    button {
      font-family: inherit;
      font-size: 12px;
      border-radius: 4px;
      padding: 6px 12px;
      cursor: pointer;
      border: 1px solid transparent;
    }
    .btn-primary {
      background: var(--vscode-button-background);
      color: var(--vscode-button-foreground);
    }
    .btn-primary:hover { background: var(--vscode-button-hoverBackground); }
    .btn-secondary {
      background: var(--vscode-button-secondaryBackground);
      color: var(--vscode-button-secondaryForeground);
    }
    .btn-secondary:hover { background: var(--vscode-button-secondaryHoverBackground); }
    .btn-danger {
      background: transparent;
      color: var(--vscode-errorForeground);
      border-color: var(--vscode-widget-border);
    }
    .status-banner {
      padding: 8px 12px;
      border-radius: 4px;
      font-size: 12px;
      border: 1px solid var(--vscode-widget-border);
    }
    .status-ok {
      border-color: var(--vscode-charts-green);
      color: var(--vscode-foreground);
    }
    .status-error {
      border-color: var(--vscode-errorForeground);
      color: var(--vscode-errorForeground);
    }
    .empty-list {
      font-size: 12px;
      color: var(--vscode-descriptionForeground);
      padding: 8px 2px;
    }
  </style>
</head>
<body>
  <div class="settings-shell">
    <header class="settings-header">
      <div class="settings-title">
        <h1>Providers &amp; Models</h1>
        <p>Configure cloud or local model endpoints. API keys are saved in VS Code SecretStorage.</p>
      </div>
      <button id="addProviderTopBtn" class="btn-primary" type="button">+ Add Provider</button>
    </header>

    ${bannerHtml}

    <div class="settings-grid">
      <aside class="panel-card" aria-label="Configured Providers">
        <div class="sidebar-head">
          <span>Configured (${state.profiles.length})</span>
        </div>
        <div class="provider-list">
          ${profilesListHtml}
        </div>
      </aside>

      <main class="panel-card" aria-label="Provider Configuration Form">
        <form id="providerForm">
          <input type="hidden" id="instanceIdInput" value="${escapeHtml(isCreatingNew ? '__new__' : activeProfile?.instanceId ?? '__new__')}" />
          <div class="form-grid">
            <div class="field">
              <label for="providerTypeSelect">Provider</label>
              <select id="providerTypeSelect">${presetOptionsHtml}</select>
            </div>
            <div class="field">
              <label for="displayNameInput">Display Name</label>
              <input id="displayNameInput" type="text" value="${escapeHtml(activeProfile?.displayName ?? selectedPreset.label)}" placeholder="e.g. OpenAI Work" />
            </div>
            <div class="field field-full">
              <label for="baseUrlInput">Base URL</label>
              <input id="baseUrlInput" type="url" value="${escapeHtml(activeProfile?.baseUrl ?? selectedPreset.defaultBaseUrl)}" placeholder="https://api.openai.com/v1" />
            </div>
            <div class="field">
              <label for="protocolSelect">API Protocol</label>
              <select id="protocolSelect">${protocolOptionsHtml}</select>
            </div>
            <div class="field">
              <label for="apiKeyInput">API Key (Write-Only)</label>
              <input id="apiKeyInput" type="password" value="" placeholder="${activeProfile?.hasStoredSecret ? '•••••••• (saved)' : 'sk-...'}" autocomplete="off" />
              <span class="hint-text">${escapeHtml(keyHint)}</span>
            </div>
            <div class="field field-full">
              <label for="defaultModelInput">Default / Custom Model ID</label>
              <div style="display:flex; gap:8px;">
                <input id="defaultModelInput" type="text" value="${escapeHtml(activeProfile?.defaultModel ?? '')}" placeholder="e.g. gpt-4.1, claude-sonnet-4-5, deepseek-chat, qwen2.5-coder" />
                <button id="discoverModelsBtn" type="button" class="btn-secondary">Discover Models</button>
              </div>
              <div class="model-chips" id="discoveredModelsContainer">${discoveredChipsHtml}</div>
            </div>
          </div>

          <div class="action-row">
            <div class="action-group">
              <button id="saveProviderBtn" type="submit" class="btn-primary">Save Provider</button>
              <button id="testConnectionBtn" type="button" class="btn-secondary">Test Connection</button>
              ${activeProfile && !activeProfile.isDefault ? `<button id="setDefaultBtn" type="button" class="btn-secondary" data-default-id="${escapeHtml(activeProfile.instanceId)}">Set Default</button>` : ''}
            </div>
            ${activeProfile ? `<button id="deleteProviderBtn" type="button" class="btn-danger" data-delete-id="${escapeHtml(activeProfile.instanceId)}">Remove</button>` : ''}
          </div>
        </form>
      </main>
    </div>
  </div>

  <script nonce="${nonce}">
    (function() {
      const vscodeApi = typeof acquireVsCodeApi === 'function' ? acquireVsCodeApi() : null;
      const presets = ${presetsJson};
      function post(payload) {
        if (vscodeApi) { vscodeApi.postMessage(payload); }
      }
      function readForm() {
        return {
          instanceId: document.getElementById('instanceIdInput').value,
          providerType: document.getElementById('providerTypeSelect').value,
          displayName: document.getElementById('displayNameInput').value,
          baseUrl: document.getElementById('baseUrlInput').value,
          protocol: document.getElementById('protocolSelect').value,
          apiKey: document.getElementById('apiKeyInput').value,
          defaultModel: document.getElementById('defaultModelInput').value,
        };
      }
      document.getElementById('addProviderTopBtn')?.addEventListener('click', () => {
        post({ command: 'newProfile' });
      });
      document.querySelectorAll('[data-select-id]').forEach((btn) => {
        btn.addEventListener('click', () => {
          post({ command: 'selectProfile', instanceId: btn.getAttribute('data-select-id') });
        });
      });
      document.getElementById('providerTypeSelect')?.addEventListener('change', (e) => {
        const kind = e.target.value;
        const preset = presets.find((p) => p.kind === kind);
        if (!preset) return;
        const instanceId = document.getElementById('instanceIdInput').value;
        if (instanceId === '__new__') {
          document.getElementById('baseUrlInput').value = preset.defaultBaseUrl;
          document.getElementById('protocolSelect').value = preset.defaultProtocol;
          document.getElementById('displayNameInput').value = preset.label;
        }
      });
      document.querySelectorAll('[data-pick-model]').forEach((chip) => {
        chip.addEventListener('click', () => {
          const modelId = chip.getAttribute('data-pick-model');
          document.getElementById('defaultModelInput').value = modelId;
          document.querySelectorAll('[data-pick-model]').forEach((c) => c.classList.remove('selected'));
          chip.classList.add('selected');
        });
      });
      document.getElementById('providerForm')?.addEventListener('submit', (e) => {
        e.preventDefault();
        post({ command: 'saveProvider', ...readForm() });
      });
      document.getElementById('discoverModelsBtn')?.addEventListener('click', () => {
        post({ command: 'discoverModels', ...readForm() });
      });
      document.getElementById('testConnectionBtn')?.addEventListener('click', () => {
        post({ command: 'testConnection', ...readForm() });
      });
      document.getElementById('setDefaultBtn')?.addEventListener('click', (e) => {
        const id = e.target.getAttribute('data-default-id');
        const modelId = document.getElementById('defaultModelInput').value;
        post({ command: 'setDefaultProvider', instanceId: id, modelId });
      });
      document.getElementById('deleteProviderBtn')?.addEventListener('click', (e) => {
        const id = e.target.getAttribute('data-delete-id');
        post({ command: 'deleteProvider', instanceId: id });
      });
    })();
  </script>
</body>
</html>`;
  }

  public dispose(): void {
    ProviderSettingsPanel.currentPanel = undefined;
    while (this._disposables.length > 0) {
      this._disposables.pop()?.dispose();
    }
    this._panel.dispose();
  }
}
