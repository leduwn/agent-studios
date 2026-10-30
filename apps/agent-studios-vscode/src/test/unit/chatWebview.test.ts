/**
 * Agent Studios — Chat WebviewView & HTML Builder Unit Tests
 * Milestone M14-UI-01: Professional AI Coding Assistant Experience
 */

import * as assert from 'assert';
import * as vscode from 'vscode';
import { BridgeManager } from '../../bridge/BridgeManager';
import { DisconnectedBridge } from '../../bridge/DisconnectedBridge';
import { FixtureBridge } from '../../bridge/FixtureBridge';
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

  describe('ChatHtmlBuilder — Strict CSP & Structure', () => {
    it('generates HTML containing strict Content Security Policy with nonce', () => {
      const mockWebview = new MockWebviewView(ChatWebviewViewProvider.viewType).webview;
      const html = ChatHtmlBuilder.buildHtml(mockWebview as unknown as vscode.Webview, extensionUri, {
        connectionState: 'connected',
        isPreview: true,
      });

      assert.ok(html.includes('http-equiv="Content-Security-Policy"'));
      assert.ok(html.includes("default-src 'none'"));
      assert.ok(html.includes(`style-src ${mockWebview.cspSource} 'unsafe-inline'`));
      assert.ok(html.includes("script-src 'nonce-"));
      assert.ok(html.includes(`font-src ${mockWebview.cspSource}`));
    });

    it('renders header with Agent Studios title, session name, new session button, history select and preview badge', () => {
      const mockWebview = new MockWebviewView(ChatWebviewViewProvider.viewType).webview;
      const html = ChatHtmlBuilder.buildHtml(mockWebview as unknown as vscode.Webview, extensionUri, {
        session: {
          id: 'session-123',
          workspacePath: '/ws',
          goalSummary: 'Build Agent Studios workbench',
          status: 'running',
          activeAgentCount: 1,
          totalAgentCount: 2,
          completedTasksCount: 1,
          totalTasksCount: 3,
          startedAt: 1000,
          elapsedMs: 200,
          isPreview: true,
        },
        sessions: [
          { id: 'session-123', title: 'Build Agent Studios workbench', status: 'running', updatedAt: 1200 },
          { id: 'session-prev', title: 'Previous Task Session', status: 'completed', updatedAt: 800 },
        ],
        connectionState: 'connected',
        isPreview: true,
      });

      assert.ok(html.includes('Agent Studios'));
      assert.ok(html.includes('Build Agent Studios workbench'));
      assert.ok(html.includes('id="newSessionBtn"'));
      assert.ok(html.includes('id="sessionHistorySelect"'));
      assert.ok(html.includes('session-123'));
      assert.ok(html.includes('session-prev'));
      assert.ok(html.includes('[Preview Fixture]'));
      assert.ok(html.includes('Connected'));
    });

    it('renders conversation container and sticky bottom composer with required controls', () => {
      const mockWebview = new MockWebviewView(ChatWebviewViewProvider.viewType).webview;
      const html = ChatHtmlBuilder.buildHtml(mockWebview as unknown as vscode.Webview, extensionUri, {
        composerDraft: 'Initial draft message',
        activeMode: 'agent',
      });

      // Conversation log container
      assert.ok(html.includes('id="conversationContainer"'));
      assert.ok(html.includes('class="chat-messages"'));

      // Sticky bottom composer & elements
      assert.ok(html.includes('class="chat-composer"'));
      assert.ok(html.includes('id="contextChipsBar"'));
      assert.ok(html.includes('id="addContextBtn"'));
      assert.ok(html.includes('id="composerInput"'));
      assert.ok(html.includes('Initial draft message'));
      assert.ok(html.includes('id="modeSelect"'));
      assert.ok(html.includes('id="modelSelect"'));
      assert.ok(html.includes('id="stopBtn"'));
      assert.ok(html.includes('id="sendBtn"'));
      assert.ok(html.includes('<script nonce='));
    });
  });

  describe('ChatWebviewViewProvider — Resolution & Lifecycle', () => {
    let bridgeManager: BridgeManager;
    let provider: ChatWebviewViewProvider;

    beforeEach(() => {
      bridgeManager = new BridgeManager(new FixtureBridge());
      provider = new ChatWebviewViewProvider(extensionUri, bridgeManager);
    });

    afterEach(() => {
      provider.dispose();
      bridgeManager.dispose();
    });

    it('has viewType "agent-studios.chat"', () => {
      assert.strictEqual(ChatWebviewViewProvider.viewType, 'agent-studios.chat');
    });

    it('resolves webview view, configures options, and sets HTML content', async () => {
      const mockView = new MockWebviewView(ChatWebviewViewProvider.viewType);
      await provider.resolveWebviewView(
        mockView as unknown as vscode.WebviewView,
        {} as vscode.WebviewViewResolveContext,
        {} as vscode.CancellationToken
      );

      assert.strictEqual(mockView.webview.options.enableScripts, true);
      assert.ok(Array.isArray(mockView.webview.options.localResourceRoots));
      assert.strictEqual(mockView.webview.options.localResourceRoots[0], extensionUri);
      assert.ok(mockView.webview.html.length > 0);
      assert.ok(mockView.webview.html.includes('Agent Studios'));
      assert.ok(mockView.webview.html.includes('id="conversationContainer"'));
    });

    it('reconnects state when webview visibility changes to true', async () => {
      const mockView = new MockWebviewView(ChatWebviewViewProvider.viewType);
      await provider.resolveWebviewView(
        mockView as unknown as vscode.WebviewView,
        {} as vscode.WebviewViewResolveContext,
        {} as vscode.CancellationToken
      );

      mockView.webview._sentMessages.length = 0;
      mockView.setVisible(true);
      await new Promise((resolve) => setTimeout(resolve, 10));

      // syncState sends syncState message
      assert.ok(mockView.webview._sentMessages.length > 0);
      const syncMsg = mockView.webview._sentMessages.find(
        (m: unknown) => (m as { type?: string }).type === 'syncState'
      ) as { type: string; state: { connectionState: string; isPreview: boolean } };
      assert.ok(syncMsg);
      assert.strictEqual(syncMsg.state.connectionState, 'connected');
      assert.strictEqual(syncMsg.state.isPreview, true);
    });

    it('disposes cleanly and stops firing events', async () => {
      const mockView = new MockWebviewView(ChatWebviewViewProvider.viewType);
      await provider.resolveWebviewView(
        mockView as unknown as vscode.WebviewView,
        {} as vscode.WebviewViewResolveContext,
        {} as vscode.CancellationToken
      );

      provider.dispose();
      mockView.webview._sentMessages.length = 0;
      mockView.setVisible(true);
      // No messages sent after dispose
      assert.strictEqual(mockView.webview._sentMessages.length, 0);
    });
  });

  describe('ChatWebviewViewProvider — Message Handling & Dispatch', () => {
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

    it('dispatches sendMessage with streaming chunks and messageComplete', async () => {
      await provider.handleWebviewMessage({
        command: 'sendMessage',
        text: 'Hello Agent Studios',
        mode: 'ask',
      });

      // User message is tracked in provider
      assert.strictEqual(provider.messages.length, 2); // 1 user + 1 assistant
      assert.strictEqual(provider.messages[0].role, 'user');
      assert.strictEqual(provider.messages[0].content, 'Hello Agent Studios');
      assert.strictEqual(provider.messages[1].role, 'assistant');

      // Verify stream chunks and complete message sent to webview
      const chunks = mockView.webview._sentMessages.filter(
        (m: unknown) => (m as { type?: string }).type === 'streamChunk'
      );
      assert.ok(chunks.length > 0);

      const complete = mockView.webview._sentMessages.find(
        (m: unknown) => (m as { type?: string }).type === 'messageComplete'
      );
      assert.ok(complete);
    });

    it('handles sendMessage failure on disconnected bridge gracefully', async () => {
      const disconnected = new DisconnectedBridge();
      const discProvider = new ChatWebviewViewProvider(extensionUri, disconnected);
      const discView = new MockWebviewView(ChatWebviewViewProvider.viewType);
      await discProvider.resolveWebviewView(
        discView as unknown as vscode.WebviewView,
        {} as vscode.WebviewViewResolveContext,
        {} as vscode.CancellationToken
      );
      discView.webview._sentMessages.length = 0;

      await discProvider.handleWebviewMessage({
        command: 'sendMessage',
        text: 'This should fail closed',
        mode: 'agent',
      });

      const errorMsg = discView.webview._sentMessages.find(
        (m: unknown) => (m as { type?: string }).type === 'error'
      ) as { type: string; error: string };
      assert.ok(errorMsg);
      assert.ok(errorMsg.error.includes('disconnected'));

      discProvider.dispose();
      disconnected.dispose();
    });

    it('handles cancelRun by invoking bridge.cancelCurrentRun and notifying webview', async () => {
      await provider.handleWebviewMessage({ command: 'cancelRun' });

      const cancelledMsg = mockView.webview._sentMessages.find(
        (m: unknown) => (m as { type?: string }).type === 'runCancelled'
      );
      assert.ok(cancelledMsg);
    });

    it('handles approveAction and posts decision result', async () => {
      const pending = await bridge.listPendingApprovals();
      assert.ok(pending.length > 0);
      const approvalId = pending[0].id;

      await provider.handleWebviewMessage({
        command: 'approveAction',
        approvalId,
      });

      const approvalResult = mockView.webview._sentMessages.find(
        (m: unknown) => (m as { type?: string }).type === 'approvalResult'
      ) as { type: string; approvalId: string; decision: string; applied: boolean };
      assert.ok(approvalResult);
      assert.strictEqual(approvalResult.approvalId, approvalId);
      assert.strictEqual(approvalResult.decision, 'approved');
      assert.strictEqual(approvalResult.applied, true);
    });

    it('handles denyAction and posts decision result', async () => {
      const pending = await bridge.listPendingApprovals();
      assert.ok(pending.length > 0);
      const approvalId = pending[0].id;

      await provider.handleWebviewMessage({
        command: 'denyAction',
        approvalId,
      });

      const approvalResult = mockView.webview._sentMessages.find(
        (m: unknown) => (m as { type?: string }).type === 'approvalResult'
      ) as { type: string; approvalId: string; decision: string; applied: boolean };
      assert.ok(approvalResult);
      assert.strictEqual(approvalResult.approvalId, approvalId);
      assert.strictEqual(approvalResult.decision, 'denied');
      assert.strictEqual(approvalResult.applied, true);
    });

    it('handles openDiff by executing inspectArtifact command', async () => {
      await provider.handleWebviewMessage({
        command: 'openDiff',
        artifactId: 'artifact-diff-001',
      });

      const diffCommand = executedCommands.find(
        (c) => c.command === 'agent-studios.inspectArtifact'
      );
      assert.ok(diffCommand);
      assert.strictEqual(diffCommand.args[0], 'artifact-diff-001');
    });

    it('handles attachContext from activeEditor', async () => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (vscode.window as any).activeTextEditor = {
        document: {
          uri: vscode.Uri.file('/workspace/src/app.ts'),
          languageId: 'typescript',
          getText: () => 'const greeting = "Hello";',
        },
        selection: { isEmpty: false },
      };

      await provider.handleWebviewMessage({
        command: 'attachContext',
        source: 'activeEditor',
      });

      assert.strictEqual(provider.attachments.length, 1);
      assert.strictEqual(provider.attachments[0].type, 'file');
      assert.ok(provider.attachments[0].uri.includes('app.ts'));

      const attachedEvent = mockView.webview._sentMessages.find(
        (m: unknown) => (m as { type?: string }).type === 'contextAttached'
      );
      assert.ok(attachedEvent);
    });

    it('handles attachContext from filePicker when files are chosen', async () => {
      mockWindowMessages.openDialogResult = [
        vscode.Uri.file('/workspace/package.json'),
        vscode.Uri.file('/workspace/tsconfig.json'),
      ];

      await provider.handleWebviewMessage({
        command: 'attachContext',
        source: 'filePicker',
      });

      assert.strictEqual(provider.attachments.length, 2);
      const attachedEvent = mockView.webview._sentMessages.find(
        (m: unknown) => (m as { type?: string }).type === 'contextAttached'
      );
      assert.ok(attachedEvent);
    });

    it('handles switchMode and updates provider mode', async () => {
      await provider.handleWebviewMessage({
        command: 'switchMode',
        mode: 'plan',
      });

      assert.strictEqual(provider.activeMode, 'plan');
      const modeChangedEvent = mockView.webview._sentMessages.find(
        (m: unknown) => (m as { type?: string }).type === 'modeChanged'
      ) as { type: string; mode: string };
      assert.ok(modeChangedEvent);
      assert.strictEqual(modeChangedEvent.mode, 'plan');
    });

    it('handles switchProviderModel and notifies webview', async () => {
      await provider.handleWebviewMessage({
        command: 'switchProviderModel',
        providerId: 'provider-anthropic-01',
        modelId: 'claude-3-5-haiku-20241022',
      });

      const modelChangedEvent = mockView.webview._sentMessages.find(
        (m: unknown) => (m as { type?: string }).type === 'modelChanged'
      ) as { type: string; modelId: string; providerId: string };
      assert.ok(modelChangedEvent);
      assert.strictEqual(modelChangedEvent.modelId, 'claude-3-5-haiku-20241022');
      assert.strictEqual(modelChangedEvent.providerId, 'provider-anthropic-01');
    });

    it('handles switchSession and clears existing message stream', async () => {
      await provider.handleWebviewMessage({
        command: 'switchSession',
        sessionId: 'session-m13-refactor',
      });

      const syncMsg = mockView.webview._sentMessages.find(
        (m: unknown) => (m as { type?: string }).type === 'syncState'
      ) as { type: string; state: { session: { id: string } } };
      assert.ok(syncMsg);
      assert.strictEqual(syncMsg.state.session.id, 'session-m13-refactor');
      assert.strictEqual(provider.messages.length, 0);
    });

    it('handles newSession by creating a new session on bridge and resetting composer', async () => {
      await provider.handleWebviewMessage({ command: 'newSession' });

      const syncMsg = mockView.webview._sentMessages.find(
        (m: unknown) => (m as { type?: string }).type === 'syncState'
      ) as { type: string; state: { session: { id: string } } };
      assert.ok(syncMsg);
      assert.ok(syncMsg.state.session.id.startsWith('session-preview-'));
      assert.strictEqual(provider.messages.length, 0);
      assert.strictEqual(provider.attachments.length, 0);
      assert.strictEqual(provider.composerDraft, '');
    });

    it('handles openTaskGraph by executing agent-studios.showTaskGraph command', async () => {
      await provider.handleWebviewMessage({ command: 'openTaskGraph' });

      const tgCommand = executedCommands.find(
        (c) => c.command === 'agent-studios.showTaskGraph'
      );
      assert.ok(tgCommand);
    });

    it('preserves composer drafts across message updates', async () => {
      await provider.handleWebviewMessage({
        command: 'saveDraft',
        draft: 'My saved draft prompt',
      });

      assert.strictEqual(provider.composerDraft, 'My saved draft prompt');

      // Draft is sent on state synchronization
      mockView.webview._sentMessages.length = 0;
      await provider.syncState();

      const syncMsg = mockView.webview._sentMessages.find(
        (m: unknown) => (m as { type?: string }).type === 'syncState'
      ) as { type: string; state: { composerDraft: string } };
      assert.ok(syncMsg);
      assert.strictEqual(syncMsg.state.composerDraft, 'My saved draft prompt');
    });
  });
});
