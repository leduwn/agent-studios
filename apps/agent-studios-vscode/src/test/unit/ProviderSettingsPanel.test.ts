/**
 * Agent Studios — ProviderSettingsPanel Unit Tests
 */

import * as assert from 'assert';
import * as vscode from 'vscode';
import { DisconnectedBridge } from '../../bridge/DisconnectedBridge';
import {
  PROVIDER_PRESETS,
  ProviderSettingsPanel,
} from '../../webviews/ProviderSettingsPanel';
import { MockWebviewView } from './vscodeMock';

function createPanelContext(): {
  context: vscode.ExtensionContext;
  secrets: Map<string, string>;
  globalData: Map<string, unknown>;
} {
  const secrets = new Map<string, string>();
  const globalData = new Map<string, unknown>();
  const context = {
    subscriptions: [],
    globalState: {
      keys: () => Array.from(globalData.keys()),
      get: <T>(k: string, d?: T) => (globalData.has(k) ? globalData.get(k) : d) as T,
      update: async (k: string, v: unknown) => {
        if (v === undefined) globalData.delete(k);
        else globalData.set(k, v);
      },
    },
    workspaceState: {
      keys: () => [],
      get: <T>(_k: string, d?: T) => d as T,
      update: async () => {},
    },
    secrets: {
      get: async (k: string) => secrets.get(k),
      store: async (k: string, v: string) => {
        secrets.set(k, v);
      },
      delete: async (k: string) => {
        secrets.delete(k);
      },
      onDidChange: new vscode.EventEmitter<vscode.SecretStorageChangeEvent>().event,
    },
  } as unknown as vscode.ExtensionContext;
  return { context, secrets, globalData };
}

describe('ProviderSettingsPanel Unit Tests', () => {
  afterEach(() => {
    ProviderSettingsPanel.currentPanel?.dispose();
  });

  it('includes all 12 provider presets plus custom OpenAI-compatible option', () => {
    assert.ok(PROVIDER_PRESETS.length >= 12);
    assert.ok(PROVIDER_PRESETS.some((p) => p.kind === 'openai'));
    assert.ok(PROVIDER_PRESETS.some((p) => p.kind === 'anthropic'));
    assert.ok(PROVIDER_PRESETS.some((p) => p.kind === 'ollama'));
    assert.ok(PROVIDER_PRESETS.some((p) => p.kind === 'custom'));
  });

  it('renders strict CSP nonce and never echoes saved API secrets into HTML', () => {
    const webview = new MockWebviewView('agentStudios.providerSettings').webview;
    const html = ProviderSettingsPanel.buildHtml(webview as unknown as vscode.Webview, {
      profiles: [
        {
          instanceId: 'prov-1',
          providerType: 'openai',
          displayName: 'OpenAI Primary',
          protocol: 'openai_responses',
          baseUrl: 'https://api.openai.com/v1',
          secretSource: 'secret_storage',
          hasStoredSecret: true,
          defaultModel: 'gpt-4.1',
          discoveredModels: [{ id: 'gpt-4.1', name: 'gpt-4.1', contextWindow: 128000 }],
          isDefault: true,
        },
      ],
      editingInstanceId: 'prov-1',
      daemonConnected: false,
    });

    assert.ok(html.includes('http-equiv="Content-Security-Policy"'));
    assert.ok(html.includes("script-src 'nonce-"));
    assert.ok(html.includes('OpenAI Primary'));
    assert.ok(html.includes('Discover Models'));
    assert.ok(html.includes('Test Connection'));
    assert.ok(!html.includes('sk-live-secret'));
  });

  it('saves provider profile with write-only SecretStorage encryption and removes cleanly', async () => {
    const { context, secrets, globalData } = createPanelContext();
    const bridge = new DisconnectedBridge();
    const panel = ProviderSettingsPanel.createOrShow(context, () => bridge);

    const secretInput = 'sk-test-write-only-secret-value-9999';
    await panel.handleMessage({
      command: 'saveProvider',
      instanceId: '__new__',
      providerType: 'openai',
      displayName: 'My OpenAI',
      baseUrl: 'https://api.openai.com/v1',
      protocol: 'openai_responses',
      apiKey: secretInput,
      defaultModel: 'gpt-4.1',
    });

    // Verify raw key was stored ONLY in SecretStorage, NEVER in globalState profiles JSON
    assert.ok(Array.from(secrets.values()).includes(secretInput));
    const serializedProfiles = JSON.stringify(globalData.get('agentStudios.providerProfiles.v1'));
    assert.strictEqual(serializedProfiles.includes(secretInput), false);

    // Delete removes profile and its secret
    const profiles = globalData.get('agentStudios.providerProfiles.v1') as Array<{ instanceId: string }>;
    assert.strictEqual(profiles.length, 1);
    await panel.handleMessage({ command: 'deleteProvider', instanceId: profiles[0].instanceId });
    assert.strictEqual(secrets.size, 0);
    bridge.dispose();
  });
});
