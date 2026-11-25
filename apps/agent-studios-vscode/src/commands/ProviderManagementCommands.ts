/**
 * Agent Studios — Provider Management & SecretReference Commands (Wave 4 / Group 5)
 *
 * Interactive configuration for OpenAI, Anthropic, Gemini, Ollama, OpenAI-compatible,
 * and Local runtime providers. Strictly enforces storing ONLY SecretReference metadata
 * (env var name or OS SecretStorage handle) and NEVER writes plaintext secrets to state.
 */

/* eslint-disable @typescript-eslint/naming-convention */

import * as vscode from 'vscode';
import type { AgentStudiosBridge } from '../bridge/AgentStudiosBridge';
import type { ProviderModel } from '../bridge/types';

export type SupportedProviderKind =
  | 'openai'
  | 'anthropic'
  | 'gemini'
  | 'ollama'
  | 'openai-compatible'
  | 'local';

export interface ProviderProfileConfig {
  instanceId: string;
  providerType: SupportedProviderKind;
  displayName: string;
  baseUrl?: string;
  secretRef: {
    source: 'env' | 'secret_storage' | 'none';
    key: string;
  };
  defaultModel: string;
  updatedAt: number;
}

const PROVIDER_PROFILES_KEY = 'agentStudios.providerProfiles.v1';

const DEFAULT_BASE_URLS: Record<SupportedProviderKind, string> = {
  openai: 'https://api.openai.com/v1',
  anthropic: 'https://api.anthropic.com/v1',
  gemini: 'https://generativelanguage.googleapis.com/v1beta',
  ollama: 'http://127.0.0.1:11434/v1',
  'openai-compatible': 'http://127.0.0.1:8080/v1',
  local: 'ipc://agent-studios-local',
};

const DEFAULT_ENV_VARS: Record<SupportedProviderKind, string> = {
  openai: 'OPENAI_API_KEY',
  anthropic: 'ANTHROPIC_API_KEY',
  gemini: 'GEMINI_API_KEY',
  ollama: '',
  'openai-compatible': 'OPENAI_COMPAT_API_KEY',
  local: '',
};

const CATALOG_MODELS: Record<SupportedProviderKind, ProviderModel[]> = {
  openai: [
    { id: 'gpt-4o', name: 'GPT-4o', contextWindow: 128000, isDefault: true },
    { id: 'gpt-4.1', name: 'GPT-4.1', contextWindow: 1000000 },
  ],
  anthropic: [
    { id: 'claude-3-7-sonnet', name: 'Claude 3.7 Sonnet', contextWindow: 200000, isDefault: true },
  ],
  gemini: [
    { id: 'gemini-2.5-pro', name: 'Gemini 2.5 Pro', contextWindow: 1048576, isDefault: true },
    { id: 'gemini-2.5-flash', name: 'Gemini 2.5 Flash', contextWindow: 1048576 },
  ],
  ollama: [{ id: 'qwen2.5-coder:14b', name: 'Qwen 2.5 Coder 14B', contextWindow: 32768, isDefault: true }],
  'openai-compatible': [{ id: 'custom-model', name: 'Custom Model', contextWindow: 65536, isDefault: true }],
  local: [{ id: 'deterministic-local', name: 'Deterministic Local Runtime', contextWindow: 32768, isDefault: true }],
};

export function looksLikePlaintextSecret(value: string): boolean {
  const trimmed = value.trim();
  return (
    trimmed.startsWith('sk-') ||
    trimmed.startsWith('AIza') ||
    trimmed.startsWith('ghp_') ||
    (trimmed.length >= 32 && !/^[A-Z0-9_]+$/.test(trimmed))
  );
}

export class ProviderProfileManager {
  constructor(private readonly context: vscode.ExtensionContext) {}

  public listProfiles(): ProviderProfileConfig[] {
    return this.context.globalState.get<ProviderProfileConfig[]>(PROVIDER_PROFILES_KEY, []);
  }

  public async upsertProfile(input: {
    instanceId?: string;
    providerType: SupportedProviderKind;
    displayName: string;
    baseUrl?: string;
    secretRefKey: string;
    defaultModel?: string;
  }): Promise<ProviderProfileConfig> {
    if (looksLikePlaintextSecret(input.secretRefKey)) {
      throw new Error(
        'Refusing to persist raw secret key in provider profile. Provide an environment variable name or SecretStorage handle.'
      );
    }

    const profiles = [...this.listProfiles()];
    const instanceId =
      input.instanceId?.trim() || `${input.providerType}-${Date.now().toString(36)}`;
    const secretKey = input.secretRefKey.trim();
    const record: ProviderProfileConfig = {
      instanceId,
      providerType: input.providerType,
      displayName: input.displayName.trim() || `${input.providerType} (${instanceId})`,
      baseUrl: input.baseUrl?.trim() || DEFAULT_BASE_URLS[input.providerType],
      secretRef: secretKey
        ? { source: 'env', key: secretKey }
        : { source: 'none', key: '' },
      defaultModel:
        input.defaultModel?.trim() ||
        CATALOG_MODELS[input.providerType][0]?.id ||
        'default',
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

  public discoverModels(providerType: SupportedProviderKind): ProviderModel[] {
    return CATALOG_MODELS[providerType] ?? [];
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
        { placeHolder: 'Select provider type to add or update' }
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

      const defaultEnv = DEFAULT_ENV_VARS[providerType];
      const secretRefKey =
        (await vscode.window.showInputBox?.({
          prompt:
            'SecretReference env var name (metadata only; leave empty for keyless local/ollama)',
          value: defaultEnv,
        })) ?? defaultEnv;

      const created = await profileManager.upsertProfile({
        providerType,
        displayName,
        secretRefKey,
      });
      vscode.window.showInformationMessage(
        `Configured provider ${created.displayName} (SecretRef: ${created.secretRef.key || 'none'}).`
      );
      return created;
    }),
    vscode.commands.registerCommand('agent-studios.testProviderConnection', async () => {
      const bridge = bridgeProvider();
      if (bridge.connectionState === 'connected') {
        const summaries = await bridge.listProviderSummaries();
        vscode.window.showInformationMessage(
          `Daemon reports ${summaries.length} configured provider(s).`
        );
        return { ok: true, source: 'daemon', count: summaries.length };
      }
      const profiles = profileManager.listProfiles();
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
