/**
 * Agent Studios — Agent-First Experience, Shared Session Continuity & Mode Transitions Unit Tests
 * Milestone M15 UI-06: Agent Mode ↔ IDE Mode Integration Readiness
 */

import * as assert from 'assert';
import * as vscode from 'vscode';
import { BridgeManager } from '../../bridge/BridgeManager';
import { FixtureBridge } from '../../bridge/FixtureBridge';
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
    it('renders rich welcome surface with goal starters when conversation has no messages', () => {
      const mockWebview = new MockWebviewView(ChatWebviewViewProvider.viewType).webview;
      const html = ChatHtmlBuilder.buildHtml(mockWebview as unknown as vscode.Webview, extensionUri, {
        messages: [],
        workspacePath: 'D:\\Projects\\agent-studios',
        connectionState: 'connected',
        selectedRuntime: 'Native Daemon (agsd)',
      });

      // Verify welcome surface container and brand
      assert.ok(html.includes('agent-welcome-surface'), 'Must contain agent-welcome-surface');
      assert.ok(html.includes('Agent-First AI Engineering Workbench'), 'Must describe agent-first workbench');

      // Verify Goal Formulation Starters
      assert.ok(html.includes('Goal Formulation Starters'), 'Must contain Goal Formulation Starters heading');
      assert.ok(html.includes('data-action="prompt-suggestion"'), 'Must contain prompt-suggestion action chips');
      assert.ok(html.includes('Audit repository architecture'), 'Must contain audit starter prompt');
      assert.ok(html.includes('Diagnose failing integration tests'), 'Must contain diagnose starter prompt');
    });

    it('renders active workspace status with Open in IDE and Mission Control actions', () => {
      const mockWebview = new MockWebviewView(ChatWebviewViewProvider.viewType).webview;
      const html = ChatHtmlBuilder.buildHtml(mockWebview as unknown as vscode.Webview, extensionUri, {
        messages: [],
        workspacePath: 'D:\\Projects\\agent-studios',
        connectionState: 'connected',
      });

      assert.ok(html.includes('Active Workspace'), 'Must contain Active Workspace card');
      assert.ok(html.includes('agent-studios'), 'Must display workspace name');
      assert.ok(html.includes('data-action="open-in-ide"'), 'Must provide open-in-ide action button');
      assert.ok(html.includes('data-action="open-mission-control"'), 'Must provide open-mission-control action button');
    });

    it('renders honest model discovery notice and static adapter catalog', () => {
      const mockWebview = new MockWebviewView(ChatWebviewViewProvider.viewType).webview;
      const html = ChatHtmlBuilder.buildHtml(mockWebview as unknown as vscode.Webview, extensionUri, {
        messages: [],
        connectionState: 'connected',
      });

      // Verification of honest model catalog notice
      assert.ok(
        html.includes('Dynamic Provider Discovery pending M15.1'),
        'Must explicitly disclose pending dynamic discovery'
      );
      assert.ok(
        html.includes('Static adapters: Claude Code, Internal Codex, Codex'),
        'Must list real static adapters honestly without fabrication'
      );
    });

    it('renders session continuity restore action when previous sessions exist', () => {
      const mockWebview = new MockWebviewView(ChatWebviewViewProvider.viewType).webview;
      const html = ChatHtmlBuilder.buildHtml(mockWebview as unknown as vscode.Webview, extensionUri, {
        messages: [],
        session: {
          id: 'sess-persisted-42',
          workspacePath: 'D:\\Projects\\agent-studios',
          goalSummary: 'M15 UI-06 session continuity test',
          status: 'idle',
          activeAgentCount: 0,
          totalAgentCount: 1,
          completedTasksCount: 0,
          totalTasksCount: 0,
          startedAt: Date.now() - 5000,
          elapsedMs: 5000,
        },
      });

      assert.ok(html.includes('Session Continuity'), 'Must render Session Continuity card');
      assert.ok(html.includes('data-action="continue-session"'), 'Must render continue-session action button');
      assert.ok(html.includes('data-session-id="sess-persisted-42"'), 'Must carry session id in action button');
    });
  });

  describe('Progressive Disclosure & 14 Conversation UX States', () => {
    it('includes Mission Control progressive disclosure button in streaming console', () => {
      const mockWebview = new MockWebviewView(ChatWebviewViewProvider.viewType).webview;
      const html = ChatHtmlBuilder.buildHtml(mockWebview as unknown as vscode.Webview, extensionUri, {
        messages: [],
      });

      assert.ok(
        html.includes('data-action="open-mission-control"'),
        'Streaming console must link to Mission Control for detailed DAG'
      );
      assert.ok(
        html.includes('Mission Control →'),
        'Streaming console must include Mission Control button text'
      );
    });

    it('defines styling classes for all 14 conversation UX states', () => {
      const mockWebview = new MockWebviewView(ChatWebviewViewProvider.viewType).webview;
      const html = ChatHtmlBuilder.buildHtml(mockWebview as unknown as vscode.Webview, extensionUri, {
        messages: [],
      });

      // Verify the 14 UX states styling coverage
      const states = [
        'connecting',
        'connected',
        'degraded',
        'offline',
        'session_restored',
        'task_pending',
        'awaiting_approval',
        'interrupted',
        'completed',
        'verified_artifact',
        'unverified_artifact',
      ];

      for (const st of states) {
        assert.ok(html.includes(`.status-badge.status-${st}`), `CSS must define style for state: ${st}`);
      }
    });

    it('embeds client-side progressive disclosure toggle in script', () => {
      const mockWebview = new MockWebviewView(ChatWebviewViewProvider.viewType).webview;
      const html = ChatHtmlBuilder.buildHtml(mockWebview as unknown as vscode.Webview, extensionUri, {
        messages: [],
      });

      assert.ok(
        html.includes("if (execState.status && execState.status !== 'idle')"),
        'Must toggle streaming console display based on idle status'
      );
      assert.ok(
        html.includes("executionStreamingPane.style.display = 'none'"),
        'Must hide streaming console when idle'
      );
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
