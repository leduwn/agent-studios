/**
 * Agent Studios — ChatPresenter Unit Tests
 */

import * as assert from 'assert';
import {
  filterCompatibleRuntimes,
  pickDefaultRuntime,
  resolveSendReadiness,
  resolveUiConnectionStatus,
} from '../../chat/ChatPresenter';
import { ProviderSummary } from '../../bridge/types';

const sampleOpenAI: ProviderSummary = {
  instanceId: 'openai-1',
  providerType: 'openai',
  displayName: 'OpenAI',
  status: 'available',
  models: [{ id: 'gpt-4.1', name: 'gpt-4.1', contextWindow: 128000, isDefault: true }],
};

describe('ChatPresenter Unit Tests', () => {
  it('resolves all 9 clean user-facing UI connection states without technical jargon', () => {
    assert.strictEqual(
      resolveUiConnectionStatus({ connectionState: 'connected', providers: [] }).status,
      'not_configured'
    );
    assert.strictEqual(
      resolveUiConnectionStatus({ connectionState: 'disconnected', providers: [], isStartingEngine: true }).status,
      'starting'
    );
    assert.strictEqual(
      resolveUiConnectionStatus({ connectionState: 'connecting', providers: [sampleOpenAI] }).status,
      'connecting'
    );
    assert.strictEqual(
      resolveUiConnectionStatus({ connectionState: 'connected', providers: [sampleOpenAI] }).status,
      'ready'
    );
    assert.strictEqual(
      resolveUiConnectionStatus({ connectionState: 'connected', providers: [sampleOpenAI], isRunning: true }).status,
      'running'
    );
    assert.strictEqual(
      resolveUiConnectionStatus({ connectionState: 'connected', providers: [sampleOpenAI], pendingApprovalCount: 1 }).status,
      'waiting_for_approval'
    );
    assert.strictEqual(
      resolveUiConnectionStatus({ connectionState: 'reconnecting', providers: [sampleOpenAI] }).status,
      'reconnecting'
    );
    assert.strictEqual(
      resolveUiConnectionStatus({ connectionState: 'authentication_failed', providers: [sampleOpenAI] }).status,
      'error'
    );
    assert.strictEqual(
      resolveUiConnectionStatus({ connectionState: 'disconnected', providers: [sampleOpenAI] }).status,
      'disconnected'
    );
  });

  it('prevents Claude Code runtime from pairing with DeepSeek, Ollama, OpenAI, or Gemini models', () => {
    const runtimes = [
      { id: 'claude-code', name: 'Claude Code CLI', enabled: true },
      { id: 'internal-codex', name: 'Agent Studios Engine', enabled: true },
    ];

    const ollamaCompat = filterCompatibleRuntimes(runtimes, 'ollama', 'deepseek-coder:6.7b');
    assert.strictEqual(ollamaCompat.some((r) => r.id === 'claude-code'), false);
    assert.strictEqual(pickDefaultRuntime(runtimes, 'claude-code', 'ollama', 'deepseek-coder:6.7b'), 'internal-codex');

    const anthropicCompat = filterCompatibleRuntimes(runtimes, 'anthropic', 'claude-sonnet-4-5');
    assert.strictEqual(anthropicCompat.some((r) => r.id === 'claude-code'), true);
  });

  it('returns actionable recovery CTAs when ask/plan/agent prerequisites are missing', () => {
    const needProvider = resolveSendReadiness({
      uiStatus: 'not_configured',
      mode: 'ask',
      hasProviders: false,
      hasWorkspace: false,
      isWorkspaceTrusted: true,
    });
    assert.strictEqual(needProvider.canSendImmediately, false);
    assert.strictEqual(needProvider.action?.id, 'configure_provider');

    const needFolderForAgent = resolveSendReadiness({
      uiStatus: 'ready',
      mode: 'agent',
      hasProviders: true,
      hasWorkspace: false,
      isWorkspaceTrusted: true,
    });
    assert.strictEqual(needFolderForAgent.canSendImmediately, false);
    assert.strictEqual(needFolderForAgent.action?.id, 'open_folder');

    const askWithoutFolder = resolveSendReadiness({
      uiStatus: 'ready',
      mode: 'ask',
      hasProviders: true,
      hasWorkspace: false,
      isWorkspaceTrusted: true,
    });
    assert.strictEqual(askWithoutFolder.canSendImmediately, true);
  });
});
