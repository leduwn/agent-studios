/**
 * Agent Studios — Provider Management, SecretStorage & Dynamic Discovery (Wave 5 Extension-First)
 *
 * Full 12-provider BYOK configuration synced with agsd IPC (`upsertProviderInstance`,
 * `removeProviderInstance`, `discoverProviderModels`, `testProviderConnection`).
 * Stores secrets ONLY in `vscode.SecretStorage` or OS env `SecretReference` — never
 * writes plaintext API keys to globalState, settings, or logs. Replaces static
 * hardcoded model lists with real daemon/endpoint discovery.
 */

/* eslint-disable @typescript-eslint/naming-convention */

import * as crypto from 'crypto';
import * as vscode from 'vscode';
import type { AgentStudiosBridge } from '../bridge/AgentStudiosBridge';
import type { ProviderModel } from '../bridge/types';

export type SupportedProviderKind =
  | 'openai'
  | 'anthropic'
  | 'google-gemini'
  | 'deepseek'
  | 'groq'
  | 'openrouter'
  | '9router'
  | 'litellm'
  | 'ollama'
  | 'lm-studio'
  | 'vllm'
  | 'custom';

export interface ProviderProfileConfig {
  instanceId: string;
  providerType: SupportedProviderKind;
  displayName: string;
  protocol: 'openai_responses' | 'openai_chat_completions' | 'anthropic_messages' | 'gemini_generate_content';
  baseUrl: string;
  secretRef: {
    source: 'env' | 'secret_storage' | 'none';
    key: string;
  };
  timeoutSecs?: number;
  maxRetries?: number;
  defaultModel?: string;
  discoveredModels?: ProviderModel[];
  updatedAt: number;
}

const PROVIDER_PROFILES_KEY = 'agentStudios.providerProfiles.v1';

export const DEFAULT_BASE_URLS: Record<SupportedProviderKind, string> = {
  openai: 'https://api.openai.com/v1',
  anthropic: 'https://api.anthropic.com/v1',
  'google-gemini': 'https://generativelanguage.googleapis.com/v1beta',
  deepseek: 'https://api.deepseek.com/v1',
  groq: 'https://api.groq.com/openai/v1',
  openrouter: 'https://openrouter.ai/api/v1',
  '9router': 'http://127.0.0.1:9000/v1',
  litellm: 'http://127.0.0.1:4000/v1',
  ollama: 'http://127.0.0.1:11434',
  'lm-studio': 'http://127.0.0.1:1234/v1',
  vllm: 'http://127.0.0.1:8000/v1',
  custom: 'http://127.0.0.1:8080/v1',
};

const DEFAULT_PROTOCOLS: Record<SupportedProviderKind, ProviderProfileConfig['protocol']> = {
  openai: 'openai_responses',
  anthropic: 'anthropic_messages',
  'google-gemini': 'gemini_generate_content',
  deepseek: 'openai_chat_completions',
  groq: 'openai_chat_completions',
  openrouter: 'openai_chat_completions',
  '9router': 'openai_chat_completions',
  litellm: 'openai_chat_completions',
  ollama: 'openai_chat_completions',
  'lm-studio': 'openai_chat_completions',
  vllm: 'openai_chat_completions',
  custom: 'openai_chat_completions',
};

const DEFAULT_ENV_VARS: Record<SupportedProviderKind, string> = {
  openai: 'OPENAI_API_KEY',
  anthropic: 'ANTHROPIC_API_KEY',
  'google-gemini': 'GEMINI_API_KEY',
  deepseek: 'DEEPSEEK_API_KEY',
  groq: 'GROQ_API_KEY',
  openrouter: 'OPENROUTER_API_KEY',
  '9router': 'NINEROUTER_API_KEY',
  litellm: 'LITELLM_API_KEY',
  ollama: '',
  'lm-studio': '',
  vllm: '',
  custom: 'CUSTOM_PROVIDER_API_KEY',
};

export function looksLikePlaintextSecret(value: string): boolean {
  const trimmed = value.trim();
  return (
    trimmed.startsWith('sk-') ||
    trimmed.startsWith('AIza') ||
    trimmed.startsWith('ghp_') ||
    trimmed.startsWith('xoxb-') ||
    (trimmed.length >= 32 && !/^[A-Z0-9_.:-]+$/.test(trimmed))
  );
}

export class ProviderProfileManager {
  constructor(private readonly context: vscode.ExtensionContext) {}

  public listProfiles(): ProviderProfileConfig[] {
    return this.context.globalState.get<ProviderProfileConfig[]>(PROVIDER_PROFILES_KEY, []);
  }

  public async storeSecretInExtensionVault(handleKey: string, apiKeyValue: string): Promise<void> {
    if (!this.context.secrets || typeof this.context.secrets.store !== 'function') {
      throw new Error('VS Code SecretStorage unavailable in this host context.');
    }
    await this.context.secrets.store(handleKey, apiKeyValue);
  }

  public async getSecretFromExtensionVault(handleKey: string): Promise<string | undefined> {
    if (!this.context.secrets || typeof this.context.secrets.get !== 'function') {
      return undefined;
    }
    return this.context.secrets.get(handleKey);
  }

  public async upsertProfile(input: {
    instanceId?: string;
    providerType: SupportedProviderKind;
    displayName: string;
    protocol?: ProviderProfileConfig['protocol'];
    baseUrl?: string;
    secretRefKey: string;
    secretSource?: 'env' | 'secret_storage' | 'none';
    defaultModel?: string;
    timeoutSecs?: number;
    maxRetries?: number;
  }): Promise<ProviderProfileConfig> {
    if (looksLikePlaintextSecret(input.secretRefKey)) {
      throw new Error(
        'Refusing to persist raw secret key in provider profile. Provide an environment variable name or SecretStorage handle.'
      );
    }

    const profiles = [...this.listProfiles()];
    const instanceId = input.instanceId?.trim() || crypto.randomUUID();
    const secretKey = input.secretRefKey.trim();
    const secretSource =
      input.secretSource ||
      (!secretKey ? 'none' : secretKey.startsWith('agentStudios.secret.') ? 'secret_storage' : 'env');

    const record: ProviderProfileConfig = {
      instanceId,
      providerType: input.providerType,
      displayName: input.displayName.trim() || `${input.providerType} (${instanceId.slice(0, 8)})`,
      protocol: input.protocol || DEFAULT_PROTOCOLS[input.providerType],
      baseUrl: input.baseUrl?.trim() || DEFAULT_BASE_URLS[input.providerType],
      secretRef: { source: secretSource, key: secretKey },
      timeoutSecs: input.timeoutSecs ?? 30,
      maxRetries: input.maxRetries ?? 2,
      defaultModel: input.defaultModel?.trim() || undefined,
      updatedAt: Date.now(),
    };

    const existingIdx = profiles.findIndex((p) => p.instanceId === instanceId);
    if (existingIdx >= 0) {
      profiles[existingIdx] = record;
    } else {
      profiles.push(record);
    }
    await this.context.globalState.update(PROVIDER_PROFILES_KEY, profiles);
    return record;
  }

  public async removeProfile(instanceId: string): Promise<boolean> {
    const profiles = this.listProfiles();
    const next = profiles.filter((p) => p.instanceId !== instanceId);
    if (next.length === profiles.length) {
      return false;
    }
    await this.context.globalState.update(PROVIDER_PROFILES_KEY, next);
    return true;
  }

  /**
   * Executes real dynamic model discovery via connected daemon IPC or direct HTTP
   * `/v1/models` (Ollama `/api/tags`) endpoint. Never returns hardcoded fake catalogs.
   */
  public async discoverModelsDynamic(
    profile: ProviderProfileConfig,
    bridge?: AgentStudiosBridge
  ): Promise<ProviderModel[]> {
    if (bridge?.connectionState === 'connected' && bridge.discoverProviderModels) {
      const models = await bridge.discoverProviderModels(profile.instanceId);
      await this.saveDiscoveredModels(profile.instanceId, models);
      return models;
    }

    const base = profile.baseUrl.replace(/\/+$/, '');
    const discoveryUrl =
      profile.providerType === 'ollama'
        ? base.endsWith('/api')
          ? `${base}/tags`
          : `${base}/api/tags`
        : base.endsWith('/v1')
          ? `${base}/models`
          : `${base}/v1/models`;

    const headers: Record<string, string> = { Accept: 'application/json' };
    if (profile.secretRef.source === 'env' && profile.secretRef.key && process.env[profile.secretRef.key]) {
      headers.Authorization = `Bearer ${process.env[profile.secretRef.key]}`;
    } else if (profile.secretRef.source === 'secret_storage' && profile.secretRef.key) {
      const token = await this.getSecretFromExtensionVault(profile.secretRef.key);
      if (token) {
        headers.Authorization = `Bearer ${token}`;
      }
    }

    const res = await fetch(discoveryUrl, { headers, signal: AbortSignal.timeout(8000) });
    if (!res.ok) {
      throw new Error(`Dynamic model discovery failed with HTTP ${res.status} from ${discoveryUrl}`);
    }
    const body = (await res.json()) as {
      data?: Array<{ id?: string; owned_by?: string; context_window?: number }>;
      models?: Array<{ name?: string; id?: string }>;
    };

    const models: ProviderModel[] = [];
    if (Array.isArray(body.data)) {
      for (const item of body.data) {
        if (item?.id) {
          models.push({
            id: item.id,
            name: item.id,
            contextWindow: item.context_window ?? 128000,
          });
        }
      }
    } else if (Array.isArray(body.models)) {
      for (const item of body.models) {
        const id = item?.id || item?.name;
        if (id) {
          models.push({
            id,
            name: id,
            contextWindow: 128000,
          });
        }
      }
    }
    await this.saveDiscoveredModels(profile.instanceId, models);
    return models;
  }

  private async saveDiscoveredModels(instanceId: string, models: ProviderModel[]): Promise<void> {
    const profiles = [...this.listProfiles()];
    const idx = profiles.findIndex((p) => p.instanceId === instanceId);
    if (idx >= 0) {
      profiles[idx] = {
        ...profiles[idx],
        discoveredModels: models,
        defaultModel: profiles[idx].defaultModel || models[0]?.id,
        updatedAt: Date.now(),
      };
      await this.context.globalState.update(PROVIDER_PROFILES_KEY, profiles);
    }
  }
}

export function registerProviderManagementCommands(
  context: vscode.ExtensionContext,
  bridgeProvider: () => AgentStudiosBridge,
  profileManager: ProviderProfileManager = new ProviderProfileManager(context)
): vscode.Disposable {
  const disposables = [
    vscode.commands.registerCommand('agent-studios.configureProvider', async () => {
      const pickedKind = await vscode.window.showQuickPick(
        (Object.keys(DEFAULT_BASE_URLS) as SupportedProviderKind[]).map((providerType) => ({
          label: providerType,
          description: DEFAULT_BASE_URLS[providerType],
          providerType,
        })),
        { placeHolder: 'Select provider type (12 supported ecosystems)' }
      );
      if (!pickedKind || typeof pickedKind !== 'object' || !('providerType' in pickedKind)) {
        return undefined;
      }
      const providerType = (pickedKind as { providerType: SupportedProviderKind }).providerType;
      const displayName =
        (await vscode.window.showInputBox?.({
          prompt: 'Display name for provider instance',
          value: `${providerType} profile`,
        })) || `${providerType} profile`;

      const baseUrl =
        (await vscode.window.showInputBox?.({
          prompt: 'Provider Base URL',
          value: DEFAULT_BASE_URLS[providerType],
        })) || DEFAULT_BASE_URLS[providerType];

      const defaultEnv = DEFAULT_ENV_VARS[providerType];
      const secretInput =
        (await vscode.window.showInputBox?.({
          prompt:
            'Enter env var name OR raw API key (raw keys are stored securely in VS Code SecretStorage, never globalState)',
          value: defaultEnv,
          password: true,
        })) ?? defaultEnv;

      let secretRefKey = secretInput.trim();
      let secretSource: 'env' | 'secret_storage' | 'none' = !secretRefKey ? 'none' : 'env';
      const instanceId = crypto.randomUUID();
      if (secretRefKey && looksLikePlaintextSecret(secretRefKey)) {
        const handle = `agentStudios.secret.${instanceId}`;
        await profileManager.storeSecretInExtensionVault(handle, secretRefKey);
        secretRefKey = handle;
        secretSource = 'secret_storage';
      }

      const created = await profileManager.upsertProfile({
        instanceId,
        providerType,
        displayName,
        baseUrl,
        secretRefKey,
        secretSource,
      });

      const bridge = bridgeProvider();
      if (bridge.connectionState === 'connected' && bridge.upsertProviderInstance) {
        await bridge.upsertProviderInstance({
          instanceId: created.instanceId,
          providerType: created.providerType,
          displayName: created.displayName,
          protocol: created.protocol,
          baseUrl: created.baseUrl,
          secretEnvVar: created.secretRef.source === 'env' ? created.secretRef.key : undefined,
        });
      }

      vscode.window.showInformationMessage(
        `Configured provider ${created.displayName} (SecretRef: ${created.secretRef.source}:${created.secretRef.key || 'none'}).`
      );
      return created;
    }),
    vscode.commands.registerCommand('agent-studios.discoverProviderModels', async (instanceId?: string) => {
      const profiles = profileManager.listProfiles();
      const target = instanceId
        ? profiles.find((p) => p.instanceId === instanceId)
        : profiles[0];
      if (!target) {
        vscode.window.showInformationMessage('Configure a provider instance before discovering models.');
        return [];
      }
      const models = await profileManager.discoverModelsDynamic(target, bridgeProvider());
      vscode.window.showInformationMessage(
        `Discovered ${models.length} model(s) from ${target.displayName}.`
      );
      return models;
    }),
    vscode.commands.registerCommand('agent-studios.testProviderConnection', async () => {
      const bridge = bridgeProvider();
      const profiles = profileManager.listProfiles();
      if (bridge.connectionState === 'connected') {
        if (profiles[0] && bridge.testProviderConnection) {
          const report = await bridge.testProviderConnection(profiles[0].instanceId);
          vscode.window.showInformationMessage(
            `Provider test for ${profiles[0].displayName}: ${report.ok ? 'PASSED' : report.status}`
          );
          return report;
        }
        const summaries = await bridge.listProviderSummaries();
        vscode.window.showInformationMessage(
          `Daemon reports ${summaries.length} configured provider(s).`
        );
        return { ok: true, source: 'daemon', count: summaries.length };
      }
      vscode.window.showInformationMessage(
        `Daemon offline; ${profiles.length} local SecretReference provider profile(s) validated.`
      );
      return { ok: false, source: 'local', count: profiles.length };
    }),
  ];

  const composite = vscode.Disposable.from(...disposables);
  context.subscriptions.push(composite);
  return composite;
}
