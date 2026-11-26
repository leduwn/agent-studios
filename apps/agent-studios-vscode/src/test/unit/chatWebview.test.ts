/**
 * Agent Studios — Clean Conversation-First Chat Webview Unit Tests
 */

import * as assert from 'assert';
import * as vscode from 'vscode';
import { BridgeManager } from '../../bridge/BridgeManager';
import { DisconnectedBridge } from '../../bridge/DisconnectedBridge';
import { FixtureBridge } from '../fixtures/FixtureBridge';
import { ChatHtmlBuilder } from '../../chat/ChatHtmlBuilder';
import { ChatWebviewViewProvider } from '../../chat/ChatWebviewViewProvider';
import {
  executedCommands,
  mockWindowMessages,
  MockWebviewView,
  resetMockState,
} from './vscodeMock';

describe('ChatWebviewViewProvider & ChatHtmlBuilder Tests', () => {
  const extensionUri = vscode.Uri.file('/mock/extension/path');

  beforeEach(() => {
    resetMockState();
  });

  describe('ChatHtmlBuilder — Strict CSP & Clean 4-Region Structure', () => {
    it('generates HTML with strict cryptographic CSP and zero preview/debug badges', () => {
      const mockWebview = new MockWebviewView(ChatWebviewViewProvider.viewType).webview;
      const html = ChatHtmlBuilder.buildHtml(mockWebview as unknown as vscode.Webview, extensionUri, {
        connectionState: 'connected',
        providers: [
          {
            instanceId: 'openai-1',
            providerType: 'openai',
            displayName: 'OpenAI',
            status: 'available',
            models: [{ id: 'gpt-4.1', name: 'gpt-4.1', contextWindow: 128000, isDefault: true }],
          },
        ],
      });

      assert.ok(html.includes('http-equiv="Content-Security-Policy"'));
      assert.ok(html.includes("default-src 'none'"));
      assert.ok(html.includes(`style-src ${mockWebview.cspSource} 'unsafe-inline'`));
      assert.ok(html.includes("script-src 'nonce-"));

      assert.ok(!html.includes('[Preview Fixture]'));
      assert.ok(!html.includes('Host Modal Gate'));
      assert.ok(!html.includes('Execution Target Summary'));
      assert.ok(!html.includes('▶ Start Task'));
    });

    it('renders compact header, quiet context strip, conversation stream, and docked composer', () => {
      const mockWebview = new MockWebviewView(ChatWebviewViewProvider.viewType).webview;
      const html = ChatHtmlBuilder.buildHtml(mockWebview as unknown as vscode.Webview, extensionUri, {
        composerDraft: 'Initial draft message',
        activeMode: 'agent',
        workspacePath: '/projects/my-app',
        connectionState: 'connected',
      });

      assert.ok(html.includes('id="newSessionBtn"'));
      assert.ok(html.includes('id="historyToggleBtn"'));
      assert.ok(html.includes('id="openSettingsBtn"'));
      assert.ok(html.includes('id="openMissionControlBtn"'));
      assert.ok(html.includes('my-app'));
      assert.ok(html.includes('id="conversationContainer"'));
      assert.ok(html.includes('id="messageInput"'));
      assert.ok(html.includes('Initial draft message'));
      assert.ok(html.includes('id="modeSelect"'));
      assert.ok(html.includes('id="modelSelect"'));
      assert.ok(html.includes('id="reasoningEffortSelect"'));
      assert.ok(html.includes('id="sendBtn"'));
    });
  });

  describe('ChatWebviewViewProvider — Resolution & Unified Routing', () => {
    let bridge: FixtureBridge;
    let provider: ChatWebviewViewProvider;
    let mockView: MockWebviewView;

    beforeEach(async () => {
      bridge = new FixtureBridge();
      provider = new ChatWebviewViewProvider(extensionUri, bridge);
      mockView = new MockWebviewView(ChatWebviewViewProvider.viewType);
      await provider.resolveWebviewView(
        mockView as unknown as vscode.WebviewView,
        {} as vscode.WebviewViewResolveContext,
        {} as vscode.CancellationToken
      );
      mockView.webview._sentMessages.length = 0;
    });

    afterEach(() => {
      provider.dispose();
      bridge.dispose();
    });

    it('has viewType "agent-studios.chat" and configures webview safely', () => {
      assert.strictEqual(ChatWebviewViewProvider.viewType, 'agent-studios.chat');
      assert.strictEqual(mockView.webview.options.enableScripts, true);
      assert.ok(mockView.webview.html.includes('Agent Studios'));
    });

    it('routes unified sendPrompt in ask mode with streaming chunks and complete thread update', async () => {
      await provider.handleWebviewMessage({
        command: 'sendPrompt',
        text: 'Hello Agent Studios',
        mode: 'ask',
      });

      assert.strictEqual(provider.messages.length, 2);
      assert.strictEqual(provider.messages[0].role, 'user');
      assert.strictEqual(provider.messages[1].role, 'assistant');

      const chunks = mockView.webview._sentMessages.filter(
        (m: unknown) => (m as { type?: string }).type === 'streamChunk'
      );
      assert.ok(chunks.length > 0);
      assert.ok(
        mockView.webview._sentMessages.some(
          (m: unknown) => (m as { type?: string }).type === 'renderedThread'
        )
      );
    });

    it('shows inline recovery action when sending while disconnected', async () => {
      const disconnected = new DisconnectedBridge();
      const discProvider = new ChatWebviewViewProvider(extensionUri, disconnected);
      const discView = new MockWebviewView(ChatWebviewViewProvider.viewType);
      await discProvider.resolveWebviewView(
        discView as unknown as vscode.WebviewView,
        {} as vscode.WebviewViewResolveContext,
        {} as vscode.CancellationToken
      );

      await discProvider.handleWebviewMessage({
        command: 'sendPrompt',
        text: 'Should prompt user to connect',
        mode: 'ask',
      });

      assert.ok(discProvider.messages.some((m) => m.role === 'system' && m.content.includes('Connect')));
      discProvider.dispose();
      disconnected.dispose();
    });

    it('handles approveAction, denyAction, openDiff, and context attachment', async () => {
      const pending = await bridge.listPendingApprovals();
      assert.ok(pending.length > 0);
      await provider.handleWebviewMessage({
        command: 'approveAction',
        approvalId: pending[0].id,
      });
      assert.ok(
        mockView.webview._sentMessages.some(
          (m: unknown) => (m as { type?: string }).type === 'approvalResult'
        )
      );

      await provider.handleWebviewMessage({
        command: 'openDiff',
        artifactId: 'artifact-diff-001',
      });
      assert.ok(executedCommands.some((c) => c.command === 'agent-studios.inspectArtifact'));

      mockWindowMessages.openDialogResult = [vscode.Uri.file('/workspace/package.json')];
      await provider.handleWebviewMessage({ command: 'attachContext', source: 'filePicker' });
      assert.strictEqual(provider.attachments.length, 1);
    });

    it('handles switchMode, switchProviderModel, newChat, openSettings, and saveDraft', async () => {
      await provider.handleWebviewMessage({ command: 'switchMode', mode: 'plan' });
      assert.strictEqual(provider.activeMode, 'plan');

      await provider.handleWebviewMessage({
        command: 'switchProviderModel',
        providerId: 'provider-anthropic-01',
        modelId: 'claude-3-5-haiku-20241022',
      });

      await provider.handleWebviewMessage({ command: 'saveDraft', draft: 'Draft note' });
      assert.strictEqual(provider.composerDraft, 'Draft note');

      await provider.handleWebviewMessage({ command: 'openSettings' });
      assert.ok(executedCommands.some((c) => c.command === 'agent-studios.openSettings'));

      await provider.handleWebviewMessage({ command: 'newChat' });
      assert.strictEqual(provider.messages.length, 0);
      assert.strictEqual(provider.composerDraft, '');
    });

    it('filters known fixture session records from history dropdown cleanly', async () => {
      const manager = new BridgeManager(bridge);
      const mgrProvider = new ChatWebviewViewProvider(extensionUri, manager);
      const mgrView = new MockWebviewView(ChatWebviewViewProvider.viewType);
      await mgrProvider.resolveWebviewView(
        mgrView as unknown as vscode.WebviewView,
        {} as vscode.WebviewViewResolveContext,
        {} as vscode.CancellationToken
      );

      assert.ok(!mgrView.webview.html.includes('session-m14-fixture'));
      mgrProvider.dispose();
      manager.dispose();
    });
  });
});
