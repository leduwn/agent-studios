/**
 * Agent Studios — Controls & Styles Unit Tests
 * Milestone M14-UI-01: Professional AI Coding Assistant Experience
 */

import * as assert from 'assert';
import * as vscode from 'vscode';
import {
  AgentMode,
  AGENT_MODES,
  AGENT_STUDIOS_LAST_MODE_KEY,
  ModeSelector,
  normalizeAgentMode,
} from '../../controls/ModeSelector';
import {
  AGENT_STUDIOS_PROVIDER_MODEL_KEY,
  formatContextWindow,
  getProviderCategoryLabel,
  ProviderModelSelector,
  sanitizeProviderSummary,
} from '../../controls/ProviderModelSelector';
import { CHAT_STYLES, getChatStyles } from '../../chat/styles.css';
import { AgentStudiosBridge } from '../../bridge/AgentStudiosBridge';
import { ConnectionState, ProviderSummary } from '../../bridge/types';

/**
 * In-memory Memento implementation for unit tests.
 */
class TestMemento implements vscode.Memento {
  private readonly _map = new Map<string, unknown>();

  public keys(): readonly string[] {
    return Array.from(this._map.keys());
  }

  public get<T>(key: string): T | undefined;
  public get<T>(key: string, defaultValue: T): T;
  public get<T>(key: string, defaultValue?: T): T | undefined {
    return this._map.has(key) ? (this._map.get(key) as T) : defaultValue;
  }

  public async update(key: string, value: unknown): Promise<void> {
    if (value === undefined) {
      this._map.delete(key);
    } else {
      this._map.set(key, value);
    }
  }

  public dump(): Record<string, unknown> {
    return Object.fromEntries(this._map.entries());
  }
}

/**
 * Mock bridge for provider summary tests.
 */
class TestBridge implements AgentStudiosBridge {
  public connectionState: ConnectionState = 'connected';
  public readonly onConnectionStateChanged = new vscode.EventEmitter<ConnectionState>().event;
  public readonly onEvent = new vscode.EventEmitter<never>().event;

  public providerSummaries: ProviderSummary[] = [];

  public async connect(): Promise<void> {}
  public async disconnect(): Promise<void> {}
  public async getCurrentSession(): Promise<null> { return null; }
  public async getTaskGraph(): Promise<null> { return null; }
  public async listTasks(): Promise<[]> { return []; }
  public async listPendingApprovals(): Promise<[]> { return []; }
  public async requestApprovalDecision(): Promise<never> { throw new Error('Not implemented'); }
  public async listArtifacts(): Promise<[]> { return []; }
  public async getArtifactDiff(): Promise<null> { return null; }
  public async listProviderSummaries(): Promise<ProviderSummary[]> {
    return [...this.providerSummaries];
  }
  public async submitEditorAction(): Promise<never> { throw new Error('Not implemented'); }
  public async sendMessage(): Promise<never> { throw new Error('Not implemented'); }
  public async cancelCurrentRun(): Promise<void> {}
  public async listSessions(): Promise<[]> { return []; }
  public async createSession(): Promise<never> { throw new Error('Not implemented'); }
  public async switchSession(): Promise<never> { throw new Error('Not implemented'); }
  public dispose(): void {}
}

describe('M14-UI-01: Controls and Styles Tests', () => {

  describe('ModeSelector — 3 Agent Modes & Validation', () => {
    let memento: TestMemento;

    beforeEach(() => {
      memento = new TestMemento();
    });

    it('defines exactly 3 canonical agent modes: Ask, Plan, Agent', () => {
      assert.strictEqual(AGENT_MODES.ask.name, 'Ask');
      assert.strictEqual(AGENT_MODES.plan.name, 'Plan');
      assert.strictEqual(AGENT_MODES.agent.name, 'Agent');

      // Ask: Read-only explanation, code search, non-mutating queries
      assert.strictEqual(AGENT_MODES.ask.isReadOnly, true);
      assert.strictEqual(AGENT_MODES.ask.capabilities.canEditFiles, false);
      assert.strictEqual(AGENT_MODES.ask.capabilities.canRunCommands, false);
      assert.strictEqual(AGENT_MODES.ask.capabilities.canProposeDag, false);

      // Plan: Architecture breakdown, dependency planning, DAG proposal without writing files
      assert.strictEqual(AGENT_MODES.plan.isReadOnly, true);
      assert.strictEqual(AGENT_MODES.plan.capabilities.canEditFiles, false);
      assert.strictEqual(AGENT_MODES.plan.capabilities.canRunCommands, false);
      assert.strictEqual(AGENT_MODES.plan.capabilities.canProposeDag, true);

      // Agent: Full execution mode (file edits, running tests, invoking tools via daemon)
      assert.strictEqual(AGENT_MODES.agent.isReadOnly, false);
      assert.strictEqual(AGENT_MODES.agent.capabilities.canEditFiles, true);
      assert.strictEqual(AGENT_MODES.agent.capabilities.canRunCommands, true);
      assert.strictEqual(AGENT_MODES.agent.capabilities.canProposeDag, true);
    });

    it('normalizes case-insensitive mode strings', () => {
      assert.strictEqual(normalizeAgentMode('Ask'), 'ask');
      assert.strictEqual(normalizeAgentMode('PLAN'), 'plan');
      assert.strictEqual(normalizeAgentMode('Agent'), 'agent');
      assert.strictEqual(normalizeAgentMode('  ask  '), 'ask');
      assert.strictEqual(normalizeAgentMode('invalid'), null);
      assert.strictEqual(normalizeAgentMode(null), null);
      assert.strictEqual(normalizeAgentMode(123), null);
    });

    it('initializes with default mode when no persisted mode exists', () => {
      const selector = new ModeSelector(memento, 'agent');
      assert.strictEqual(selector.currentMode, 'agent');
      assert.strictEqual(selector.isCurrentModeSupported, true);
      assert.strictEqual(selector.currentDefinition.name, 'Agent');
      selector.dispose();
    });

    it('restores persisted mode from memento', async () => {
      await memento.update(AGENT_STUDIOS_LAST_MODE_KEY, 'plan');
      const selector = new ModeSelector(memento, 'agent');
      assert.strictEqual(selector.currentMode, 'plan');
      assert.strictEqual(selector.currentDefinition.name, 'Plan');
      selector.dispose();
    });

    it('handles mode transitions and fires onDidChangeMode', () => {
      const selector = new ModeSelector(memento, 'agent');
      const transitions: AgentMode[] = [];
      selector.onDidChangeMode((m) => transitions.push(m));

      const r1 = selector.setMode('ask');
      assert.strictEqual(r1.supported, true);
      assert.strictEqual(selector.currentMode, 'ask');

      const r2 = selector.setMode('plan');
      assert.strictEqual(r2.supported, true);
      assert.strictEqual(selector.currentMode, 'plan');

      const r3 = selector.setMode('agent');
      assert.strictEqual(r3.supported, true);
      assert.strictEqual(selector.currentMode, 'agent');

      assert.deepStrictEqual(transitions, ['ask', 'plan', 'agent']);
      assert.strictEqual(memento.get(AGENT_STUDIOS_LAST_MODE_KEY), 'agent');
      selector.dispose();
    });

    it('indicates unsupported state when backend does not support a mode', () => {
      // Backend only supports 'ask' and 'plan', but not 'agent'
      const selector = new ModeSelector(memento, 'ask', ['ask', 'plan']);

      assert.strictEqual(selector.isModeSupported('ask'), true);
      assert.strictEqual(selector.isModeSupported('plan'), true);
      assert.strictEqual(selector.isModeSupported('agent'), false);

      const validation = selector.setMode('agent');
      assert.strictEqual(validation.supported, false);
      assert.strictEqual(selector.currentMode, 'agent');
      assert.strictEqual(selector.isCurrentModeSupported, false);
      assert.ok(validation.reason?.includes('not supported'));

      // Mode validation event also fires when supported modes dynamically updated
      let validatedEventReceived = false;
      selector.onDidValidateMode((v) => {
        if (v.mode === 'agent' && !v.supported) {
          validatedEventReceived = true;
        }
      });
      selector.setSupportedModes(['ask']);
      assert.strictEqual(validatedEventReceived, true);

      selector.dispose();
    });

    it('renders accessible HTML radio pills for webview', () => {
      const selector = new ModeSelector(memento, 'plan', ['ask', 'plan']);
      const html = selector.renderHtml();

      assert.ok(html.includes('role="radiogroup"'));
      assert.ok(html.includes('data-mode="plan"'));
      assert.ok(html.includes('aria-checked="true"'));
      assert.ok(html.includes('data-mode="ask"'));
      assert.ok(html.includes('aria-checked="false"'));
      assert.ok(html.includes('data-unsupported="true"'));
      assert.ok(html.includes('mode-unsupported-badge'));
      selector.dispose();
    });
  });

  describe('ProviderModelSelector — Provider Summaries, Fallback & Security', () => {
    let bridge: TestBridge;
    let memento: TestMemento;

    const fullProviderCatalog: ProviderSummary[] = [
      {
        instanceId: 'inst-anthropic',
        providerType: 'anthropic',
        displayName: 'Anthropic Official',
        status: 'available',
        models: [
          { id: 'claude-3-5-sonnet-20241022', name: 'Claude 3.5 Sonnet', contextWindow: 200000, isDefault: true },
          { id: 'claude-3-5-haiku-20241022', name: 'Claude 3.5 Haiku', contextWindow: 200000, isDefault: false },
        ],
      },
      {
        instanceId: 'inst-openai',
        providerType: 'openai',
        displayName: 'OpenAI Cloud',
        status: 'available',
        models: [
          { id: 'gpt-4o', name: 'GPT-4o', contextWindow: 128000, isDefault: true },
          { id: 'o1', name: 'o1 Reasoning', contextWindow: 200000, isDefault: false },
        ],
      },
      {
        instanceId: 'inst-gemini',
        providerType: 'gemini',
        displayName: 'Google Gemini',
        status: 'available',
        models: [
          { id: 'gemini-2.5-pro', name: 'Gemini 2.5 Pro', contextWindow: 1000000, isDefault: true },
          { id: 'gemini-2.5-flash', name: 'Gemini 2.5 Flash', contextWindow: 1000000, isDefault: false },
        ],
      },
      {
        instanceId: 'inst-openrouter',
        providerType: 'openrouter',
        displayName: 'OpenRouter Unified',
        status: 'available',
        models: [
          { id: 'openrouter/auto', name: 'OpenRouter Auto', contextWindow: 128000, isDefault: true },
        ],
      },
      {
        instanceId: 'inst-ollama',
        providerType: 'ollama',
        displayName: 'Local Ollama Daemon',
        status: 'offline',
        models: [
          { id: 'deepseek-coder:6.7b', name: 'DeepSeek Coder 6.7B', contextWindow: 16000, isDefault: true },
          { id: 'llama3:8b', name: 'Llama 3 8B', contextWindow: 8000, isDefault: false },
        ],
      },
      {
        instanceId: 'inst-lmstudio',
        providerType: 'lmstudio',
        displayName: 'LM Studio Desktop',
        status: 'unconfigured',
        models: [
          { id: 'local-model', name: 'LM Studio Default', contextWindow: 32000, isDefault: true },
        ],
      },
      {
        instanceId: 'inst-custom',
        providerType: 'custom',
        displayName: 'Custom Enterprise Gateway',
        status: 'available',
        models: [
          { id: 'enterprise-llama', name: 'Enterprise Llama 70B', contextWindow: 64000, isDefault: true },
        ],
      },
    ];

    beforeEach(() => {
      bridge = new TestBridge();
      memento = new TestMemento();
      bridge.providerSummaries = fullProviderCatalog;
    });

    afterEach(() => {
      bridge.dispose();
    });

    it('formats context window token counts correctly', () => {
      assert.strictEqual(formatContextWindow(200000), '200k tokens');
      assert.strictEqual(formatContextWindow(128000), '128k tokens');
      assert.strictEqual(formatContextWindow(16000), '16k tokens');
      assert.strictEqual(formatContextWindow(8000), '8k tokens');
      assert.strictEqual(formatContextWindow(1000000), '1M tokens');
      assert.strictEqual(formatContextWindow(2500000), '2.5M tokens');
      assert.strictEqual(formatContextWindow(500), '500 tokens');
      assert.strictEqual(formatContextWindow(0), '0 tokens');
    });

    it('categorizes known provider types accurately', () => {
      assert.strictEqual(getProviderCategoryLabel('anthropic'), 'Anthropic');
      assert.strictEqual(getProviderCategoryLabel('openai'), 'OpenAI');
      assert.strictEqual(getProviderCategoryLabel('gemini'), 'Google Gemini');
      assert.strictEqual(getProviderCategoryLabel('google'), 'Google Gemini');
      assert.strictEqual(getProviderCategoryLabel('openrouter'), 'OpenRouter');
      assert.strictEqual(getProviderCategoryLabel('ollama'), 'Local Ollama');
      assert.strictEqual(getProviderCategoryLabel('lmstudio'), 'LM Studio');
      assert.strictEqual(getProviderCategoryLabel('custom'), 'Custom Endpoint');
    });

    it('reads provider summaries from bridge across all 7 supported providers', async () => {
      const selector = new ProviderModelSelector(bridge, memento);
      const providers = await selector.refresh();

      assert.strictEqual(providers.length, 7);
      const types = providers.map((p) => p.providerType);
      assert.ok(types.includes('anthropic'));
      assert.ok(types.includes('openai'));
      assert.ok(types.includes('gemini'));
      assert.ok(types.includes('openrouter'));
      assert.ok(types.includes('ollama'));
      assert.ok(types.includes('lmstudio'));
      assert.ok(types.includes('custom'));

      // Check models on Anthropic
      const anthropic = providers.find((p) => p.providerType === 'anthropic');
      assert.strictEqual(anthropic?.models.length, 2);
      assert.strictEqual(anthropic?.models[0].name, 'Claude 3.5 Sonnet');
      assert.strictEqual(anthropic?.models[0].formattedContextWindow, '200k tokens');
      assert.strictEqual(anthropic?.models[1].name, 'Claude 3.5 Haiku');

      // Check models on OpenAI
      const openai = providers.find((p) => p.providerType === 'openai');
      assert.strictEqual(openai?.models[0].name, 'GPT-4o');
      assert.strictEqual(openai?.models[1].name, 'o1 Reasoning');

      // Check models on Gemini
      const gemini = providers.find((p) => p.providerType === 'gemini');
      assert.strictEqual(gemini?.models[0].name, 'Gemini 2.5 Pro');
      assert.strictEqual(gemini?.models[0].formattedContextWindow, '1M tokens');
      assert.strictEqual(gemini?.models[1].name, 'Gemini 2.5 Flash');

      // Check models on Ollama
      const ollama = providers.find((p) => p.providerType === 'ollama');
      assert.strictEqual(ollama?.status, 'offline');
      assert.strictEqual(ollama?.models[0].name, 'DeepSeek Coder 6.7B');
      assert.strictEqual(ollama?.models[1].name, 'Llama 3 8B');

      selector.dispose();
    });

    it('strictly sanitizes raw provider summaries to prevent secrets in webview', () => {
      const contaminatedRaw = {
        instanceId: 'inst-leaked',
        providerType: 'anthropic',
        displayName: 'Leaked Secrets Provider',
        status: 'available',
        apiKey: 'sk-ant-api03-SECRET-KEY-12345-MUST-NOT-LEAK',
        clientSecret: 'secret_passphrase_67890',
        bearerToken: 'Bearer eyJhbGciOi...',
        password: 'super-secret-password',
        models: [
          {
            id: 'claude-3-5-sonnet',
            name: 'Claude 3.5 Sonnet',
            contextWindow: 200000,
            apiEndpointAuth: 'Basic dXNlcjpwYXNz',
          },
        ],
      };

      const sanitized = sanitizeProviderSummary(contaminatedRaw);
      assert.ok(sanitized !== null);

      // Verify sanitized object has only safe keys
      const json = JSON.stringify(sanitized);
      assert.ok(!json.includes('sk-ant-api03'));
      assert.ok(!json.includes('secret_passphrase'));
      assert.ok(!json.includes('Bearer'));
      assert.ok(!json.includes('super-secret-password'));
      assert.ok(!json.includes('apiKey'));
      assert.ok(!json.includes('clientSecret'));
      assert.ok(!json.includes('bearerToken'));
      assert.ok(!json.includes('apiEndpointAuth'));

      assert.strictEqual(sanitized?.instanceId, 'inst-leaked');
      assert.strictEqual(sanitized?.models[0].name, 'Claude 3.5 Sonnet');
    });

    it('renders HTML with zero secrets and correct accessibility attributes', async () => {
      const selector = new ProviderModelSelector(bridge, memento);
      await selector.refresh();

      const html = selector.renderHtml();
      assert.ok(html.includes('class="provider-model-selector"'));
      assert.ok(html.includes('class="status-dot status-available"'));
      assert.ok(html.includes('role="combobox"') || html.includes('<select'));
      assert.ok(html.includes('200k tokens'));
      assert.ok(html.includes('Claude 3.5 Sonnet'));
      assert.ok(html.includes('Google Gemini'));
      assert.ok(html.includes('Local Ollama'));

      // Ensure no apiKey or token attribute exists
      assert.ok(!html.includes('apiKey'));
      assert.ok(!html.includes('secret'));
      selector.dispose();
    });

    it('falls back to default model of first available provider on fresh start', async () => {
      const selector = new ProviderModelSelector(bridge, memento);
      await selector.refresh();

      const selected = selector.selected;
      assert.ok(selected !== null);
      assert.strictEqual(selected?.instanceId, 'inst-anthropic');
      assert.strictEqual(selected?.modelId, 'claude-3-5-sonnet-20241022');
      assert.strictEqual(selected?.status, 'available');
      selector.dispose();
    });

    it('persists user selection and restores it on subsequent instance', async () => {
      const selector1 = new ProviderModelSelector(bridge, memento);
      await selector1.refresh();

      const success = selector1.select('inst-gemini', 'gemini-2.5-flash');
      assert.strictEqual(success, true);
      assert.strictEqual(selector1.selected?.modelId, 'gemini-2.5-flash');

      const saved = memento.get(AGENT_STUDIOS_PROVIDER_MODEL_KEY);
      assert.deepStrictEqual(saved, {
        instanceId: 'inst-gemini',
        modelId: 'gemini-2.5-flash',
      });
      selector1.dispose();

      // Launch second selector with same memento
      const selector2 = new ProviderModelSelector(bridge, memento);
      await selector2.refresh();
      assert.strictEqual(selector2.selected?.instanceId, 'inst-gemini');
      assert.strictEqual(selector2.selected?.modelId, 'gemini-2.5-flash');
      selector2.dispose();
    });

    it('selection fallback: if persisted model disappears, falls back to provider default', async () => {
      await memento.update(AGENT_STUDIOS_PROVIDER_MODEL_KEY, {
        instanceId: 'inst-openai',
        modelId: 'old-deprecated-gpt-3.5',
      });

      const selector = new ProviderModelSelector(bridge, memento);
      await selector.refresh();

      // Provider exists, model does not -> falls back to default model of OpenAI (gpt-4o)
      assert.strictEqual(selector.selected?.instanceId, 'inst-openai');
      assert.strictEqual(selector.selected?.modelId, 'gpt-4o');
      selector.dispose();
    });

    it('selection fallback: if persisted provider disappears, falls back to first available', async () => {
      await memento.update(AGENT_STUDIOS_PROVIDER_MODEL_KEY, {
        instanceId: 'deleted-provider-999',
        modelId: 'deleted-model-888',
      });

      const selector = new ProviderModelSelector(bridge, memento);
      await selector.refresh();

      assert.strictEqual(selector.selected?.instanceId, 'inst-anthropic');
      assert.strictEqual(selector.selected?.modelId, 'claude-3-5-sonnet-20241022');
      selector.dispose();
    });

    it('selection fallback: handles empty providers list gracefully', async () => {
      bridge.providerSummaries = [];
      const selector = new ProviderModelSelector(bridge, memento);
      await selector.refresh();

      assert.strictEqual(selector.selected, null);
      const html = selector.renderHtml();
      assert.ok(html.includes('No AI providers connected'));
      selector.dispose();
    });
  });

  describe('chatStyles — CSS Tokens, Accessibility & Responsiveness', () => {
    it('contains all required native VS Code CSS tokens', () => {
      const css = getChatStyles();
      assert.strictEqual(css, CHAT_STYLES);

      const requiredTokens = [
        'var(--vscode-editor-background',
        'var(--vscode-editor-foreground',
        'var(--vscode-input-background',
        'var(--vscode-button-background',
        'var(--vscode-charts-green',
        'var(--vscode-charts-yellow',
        'var(--vscode-charts-red',
        'var(--vscode-sideBar-background',
        'var(--vscode-focusBorder',
      ];

      for (const token of requiredTokens) {
        assert.ok(css.includes(token), `Expected CSS to contain token: ${token}`);
      }
    });

    it('supports responsive layouts: narrow (<=300px), standard, wide (>=600px)', () => {
      const css = getChatStyles();

      assert.ok(css.includes('@media (max-width: 300px)'));
      assert.ok(css.includes('@media (min-width: 301px) and (max-width: 599px)'));
      assert.ok(css.includes('@media (min-width: 600px)'));
    });

    it('supports high-contrast themes and forced-colors accessibility mode', () => {
      const css = getChatStyles();

      assert.ok(css.includes('@media (forced-colors: active)'));
      assert.ok(css.includes('body.vscode-high-contrast'));
      assert.ok(css.includes('body.vscode-high-contrast-light'));
      assert.ok(css.includes(':focus-visible'));
    });

    it('supports reduced motion accessibility preference', () => {
      const css = getChatStyles();

      assert.ok(css.includes('@media (prefers-reduced-motion: reduce)'));
      assert.ok(css.includes('animation-duration: 0.01ms !important'));
    });

    it('implements sticky bottom prompt composer and scrollable conversation', () => {
      const css = getChatStyles();

      // Conversation message container scrolls smoothly
      assert.ok(css.includes('.chat-messages'));
      assert.ok(css.includes('overflow-y: auto'));
      assert.ok(css.includes('scroll-behavior: smooth'));

      // Sticky prompt composer remains pinned at bottom
      assert.ok(css.includes('.chat-composer'));
      assert.ok(css.includes('position: sticky'));
      assert.ok(css.includes('bottom: 0'));
    });

    it('provides clean typography, 1px borders, and collapsible sections', () => {
      const css = getChatStyles();

      assert.ok(css.includes('details.collapsible'));
      assert.ok(css.includes('details.collapsible summary'));
      assert.ok(css.includes('border: 1px solid var(--border-subtle)'));
    });
  });

});
