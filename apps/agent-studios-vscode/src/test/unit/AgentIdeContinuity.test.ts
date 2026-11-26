/**
 * Agent Studios â€” Agent-First Experience, Shared Session Continuity & Mode Transitions Unit Tests
 * Milestone M15 UI-06: Agent Mode â†” IDE Mode Integration Readiness
 */

import * as assert from 'assert';
import * as vscode from 'vscode';
import { BridgeManager } from '../../bridge/BridgeManager';
import { FixtureBridge } from '../fixtures/FixtureBridge';
import { ChatHtmlBuilder } from '../../chat/ChatHtmlBuilder';
import { ChatSecurityGuards } from '../../chat/ChatSecurityGuards';
import { ChatWebviewViewProvider } from '../../chat/ChatWebviewViewProvider';
import { registerAllCommands } from '../../commands/CommandsRegistry';
import {
  executedCommands,
  MockWebviewView,
  registeredCommands,
  resetMockState,
} from './vscodeMock';

describe('Agent-First Experience & Mode Continuity Tests (M15 UI-06)', () => {
  const extensionUri = vscode.Uri.file('/mock/extension/path');

  beforeEach(() => {
    resetMockState();
  });

  describe('Agent-First Welcome Surface Rendering', () => {
    it('renders clean empty state with starter prompts when connected and provider configured', () => {
      const mockWebview = new MockWebviewView(ChatWebviewViewProvider.viewType).webview;
      const html = ChatHtmlBuilder.buildHtml(mockWebview as unknown as vscode.Webview, extensionUri, {
        messages: [],
        workspacePath: 'D:\\Projects\\agent-studios',
        connectionState: 'connected',
        providers: [
          {
            instanceId: 'openai-1',
            providerType: 'openai',
            displayName: 'OpenAI',
            status: 'available',
            models: [{ id: 'gpt-4.1', name: 'gpt-4.1', contextWindow: 128000 }],
          },
        ],
      });

      assert.ok(html.includes('empty-state'), 'Must contain empty-state container');
      assert.ok(html.includes('What would you like to work on?'), 'Must display clean welcome headline');
      assert.ok(html.includes('data-starter='), 'Must include clickable starter prompts');
      assert.ok(html.includes('Explain how this workspace is structured'));
      assert.ok(!html.includes('Dynamic Provider Discovery pending M15.1'), 'Must not contain stale dev notices');
    });

    it('renders clean context strip with workspace basename and Mission Control link in header', () => {
      const mockWebview = new MockWebviewView(ChatWebviewViewProvider.viewType).webview;
      const html = ChatHtmlBuilder.buildHtml(mockWebview as unknown as vscode.Webview, extensionUri, {
        messages: [],
        workspacePath: 'D:\\Projects\\agent-studios',
        connectionState: 'connected',
      });

      assert.ok(html.includes('agent-studios'), 'Must display workspace name');
      assert.ok(html.includes('id="openMissionControlBtn"'), 'Must provide Mission Control toggle button');
      assert.ok(html.includes('id="openSettingsBtn"'), 'Must provide Settings button');
    });

    it('renders recent conversation history items cleanly without fixture jargon', () => {
      const mockWebview = new MockWebviewView(ChatWebviewViewProvider.viewType).webview;
      const html = ChatHtmlBuilder.buildHtml(mockWebview as unknown as vscode.Webview, extensionUri, {
        messages: [],
        sessions: [
          {
            id: 'sess-persisted-42',
            title: 'Previous work session',
            status: 'idle',
            updatedAt: Date.now() - 5000,
          },
        ],
      });

      assert.ok(html.includes('Recent Conversations'));
      assert.ok(html.includes('data-session-id="sess-persisted-42"'));
    });
  });

  describe('Progressive Disclosure & 9 Connection UX States', () => {
    it('renders Mission Control link inside multi-agent card only when subtasks exist', () => {
      const singleHtml = ChatHtmlBuilder.renderMessagesHtml(
        [{ id: 'm1', role: 'assistant', content: 'Hello', timestamp: 1 }],
        [],
        []
      );
      assert.ok(!singleHtml.includes('multi-agent-card'), 'Single-agent chat must not show multi-agent card');

      const multiHtml = ChatHtmlBuilder.renderMessagesHtml(
        [{ id: 'm1', role: 'assistant', content: 'Coordinating', timestamp: 1 }],
        [
          { id: 't1', runId: 'r1', title: 'Subtask 1', description: '', status: 'succeeded', dependencies: [], createdAt: 1, updatedAt: 2 },
          { id: 't2', runId: 'r1', title: 'Subtask 2', description: '', status: 'running', dependencies: [], createdAt: 1, updatedAt: 2 },
        ],
        []
      );
      assert.ok(multiHtml.includes('multi-agent-card'));
      assert.ok(multiHtml.includes('data-ui-action="open_mission_control"'));
    });

    it('defines styling classes for calm connection status tones', () => {
      const mockWebview = new MockWebviewView(ChatWebviewViewProvider.viewType).webview;
      const html = ChatHtmlBuilder.buildHtml(mockWebview as unknown as vscode.Webview, extensionUri, {
        messages: [],
      });

      for (const tone of ['ok', 'active', 'warn', 'error', 'neutral']) {
        assert.ok(html.includes(`.tone-${tone}`), `CSS must define style for tone-${tone}`);
      }
    });
  });

  describe('Mode Transitions & Commands Registry Preservation', () => {
    let bridgeManager: BridgeManager;
    let context: vscode.ExtensionContext;
    let compositeDisposable: vscode.Disposable;
    const subscriptions: vscode.Disposable[] = [];

    beforeEach(() => {
      subscriptions.length = 0;
      context = { subscriptions } as unknown as vscode.ExtensionContext;
      bridgeManager = new BridgeManager(new FixtureBridge());
      compositeDisposable = registerAllCommands(
        context,
        bridgeManager,
        {},
        {}
      );
    });

    afterEach(() => {
      compositeDisposable.dispose();
    });

    it('registers agent-studios.openInIde command and focuses explorer view', async () => {
      const handler = registeredCommands.get('agent-studios.openInIde');
      assert.ok(handler, 'agent-studios.openInIde must be registered');

      await handler();
      assert.ok(
        executedCommands.some(c => c.command === 'workbench.view.explorer'),
        'openInIde must execute workbench.view.explorer'
      );
    });

    it('registers agent-studios.switchToAgentMode command and reveals Agents container', async () => {
      const handler = registeredCommands.get('agent-studios.switchToAgentMode');
      assert.ok(handler, 'agent-studios.switchToAgentMode must be registered');

      await handler();
      assert.ok(
        executedCommands.some(c => c.command === 'workbench.view.extension.agent-studios-activitybar'),
        'switchToAgentMode must reveal agent-studios-activitybar'
      );
      assert.ok(
        executedCommands.some(c => c.command === 'agent-studios.chat.focus'),
        'switchToAgentMode must focus agent-studios.chat'
      );
    });

    it('registers agent-studios.continueSession and updates projection without restarting daemon', async () => {
      const handler = registeredCommands.get('agent-studios.continueSession');
      assert.ok(handler, 'agent-studios.continueSession must be registered');

      // Hydrate state projection from bridge
      await bridgeManager.stateProjection.initializeFromBridge();
      const initialSessionId = bridgeManager.stateProjection.session?.id;
      assert.ok(initialSessionId, 'Projection must have initial session');

      const restored: any = await handler();
      assert.ok(restored, 'continueSession must return restored session');
      assert.strictEqual(
        bridgeManager.stateProjection.session?.id,
        restored.id,
        'State projection must preserve restored session id'
      );
    });
  });

  describe('ChatSecurityGuards Webview Command Validation', () => {
    it('permits openInIde and openMissionControl commands', () => {
      const resIde = ChatSecurityGuards.validateWebviewMessage({ command: 'openInIde' });
      assert.strictEqual(resIde.valid, true);

      const resMc = ChatSecurityGuards.validateWebviewMessage({ command: 'openMissionControl' });
      assert.strictEqual(resMc.valid, true);
    });

    it('permits continueSession with valid string sessionId', () => {
      const res = ChatSecurityGuards.validateWebviewMessage({
        command: 'continueSession',
        sessionId: 'sess-abc-123',
      });
      assert.strictEqual(res.valid, true);
    });

    it('rejects continueSession with invalid non-string sessionId or oversized id', () => {
      const resNonString = ChatSecurityGuards.validateWebviewMessage({
        command: 'continueSession',
        sessionId: 12345,
      });
      assert.strictEqual(resNonString.valid, false);

      const resOversized = ChatSecurityGuards.validateWebviewMessage({
        command: 'continueSession',
        sessionId: 'a'.repeat(200),
      });
      assert.strictEqual(resOversized.valid, false);
    });

    it('rejects prototype pollution on mode navigation commands', () => {
      const payload = JSON.parse('{"command": "openInIde", "__proto__": {"polluted": true}}');
      const res = ChatSecurityGuards.validateWebviewMessage(payload);
      assert.strictEqual(res.valid, false);
      assert.ok(res.error?.includes('prototype pollution'));
    });
  });
});
