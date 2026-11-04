/**
 * Agent Studios — Commands Registry & Bridge Manager Unit Tests
 * Milestone M14-UI-00: Code-OSS Native Agents Workbench
 */

import * as assert from 'assert';
import * as vscode from 'vscode';
import { BridgeManager } from '../../bridge/BridgeManager';
import { DisconnectedBridge } from '../../bridge/DisconnectedBridge';
import { FixtureBridge } from '../../bridge/FixtureBridge';
import {
  registerAllCommands,
  ViewProviders,
  WebviewManagers,
} from '../../commands/CommandsRegistry';
import {
  executedCommands,
  mockWindowMessages,
  registeredCommands,
  resetMockState,
} from './vscodeMock';

describe('CommandsRegistry & BridgeManager Tests', () => {
  let bridgeManager: BridgeManager;
  let context: vscode.ExtensionContext;
  let compositeDisposable: vscode.Disposable;
  const subscriptions: vscode.Disposable[] = [];

  const mockViews: ViewProviders = {
    session: { refresh: () => {} } as any,
    tasks: { refresh: () => {} } as any,
    approvals: { refresh: () => {} } as any,
    artifacts: { refresh: () => {} } as any,
    timeline: { refresh: () => {} } as any,
    providers: { refresh: () => {} } as any,
  };

  const mockWebviews: WebviewManagers = {
    openSession: async () => {},
    showTaskGraph: async () => {},
  };

  beforeEach(() => {
    resetMockState();
    subscriptions.length = 0;
    context = {
      subscriptions,
    } as unknown as vscode.ExtensionContext;

    bridgeManager = new BridgeManager(new FixtureBridge());
    compositeDisposable = registerAllCommands(
      context,
      bridgeManager,
      mockViews,
      mockWebviews
    );
  });

  afterEach(() => {
    compositeDisposable.dispose();
    bridgeManager.dispose();
  });

  describe('BridgeManager', () => {
    it('initializes with provided bridge and exposes state projection', () => {
      assert.ok(bridgeManager.bridge instanceof FixtureBridge);
      assert.strictEqual(bridgeManager.isPreviewMode, true);
      assert.ok(bridgeManager.stateProjection !== undefined);
    });

    it('updates bridge, fires onBridgeChanged, and updates isPreviewMode', () => {
      let firedBridge: any = null;
      bridgeManager.onBridgeChanged((b) => {
        firedBridge = b;
      });

      const disconnected = new DisconnectedBridge();
      bridgeManager.setBridge(disconnected);

      assert.strictEqual(bridgeManager.bridge, disconnected);
      assert.strictEqual(bridgeManager.isPreviewMode, false);
      assert.strictEqual(firedBridge, disconnected);
    });

    it('ignores setBridge when setting the identical bridge instance', () => {
      let callCount = 0;
      bridgeManager.onBridgeChanged(() => {
        callCount++;
      });

      const current = bridgeManager.bridge;
      bridgeManager.setBridge(current);

      assert.strictEqual(callCount, 0);
    });
  });

  describe('Workbench Commands', () => {
    it('registers all required workbench and editor commands', () => {
      const required = [
        'agent-studios.openAgents',
        'agent-studios.openSession',
        'agent-studios.showTaskGraph',
        'agent-studios.showApprovals',
        'agent-studios.connectDaemon',
        'agent-studios.disconnectDaemon',
        'agent-studios.toggleFixturePreview',
        'agent-studios.refreshViews',
        'agent-studios.explainSelection',
        'agent-studios.refactorSelection',
        'agent-studios.generateTests',
      ];

      for (const cmd of required) {
        assert.ok(registeredCommands.has(cmd), `Command ${cmd} should be registered`);
      }
    });

    it('executes agent-studios.openAgents and invokes workbench focus command', async () => {
      await vscode.commands.executeCommand('agent-studios.openAgents');

      const called = executedCommands.some(
        (c) => c.command === 'workbench.view.extension.agent-studios-activitybar'
      );
      assert.ok(called, 'Should execute workbench.view.extension.agent-studios-activitybar');
    });

    it('executes agent-studios.showApprovals and focuses approvals view', async () => {
      await vscode.commands.executeCommand('agent-studios.showApprovals');

      const called = executedCommands.some(
        (c) => c.command === 'agent-studios.approvals.focus'
      );
      assert.ok(called, 'Should execute agent-studios.approvals.focus');
    });

    it('executes agent-studios.disconnectDaemon and switches to DisconnectedBridge', async () => {
      await vscode.commands.executeCommand('agent-studios.disconnectDaemon');

      assert.ok(bridgeManager.bridge instanceof DisconnectedBridge);
      assert.strictEqual(bridgeManager.isPreviewMode, false);
      assert.ok(
        mockWindowMessages.info.some((msg) =>
          msg.includes('disconnected from daemon')
        )
      );
    });

    it('executes agent-studios.toggleFixturePreview and toggles bridge mode', async () => {
      assert.strictEqual(bridgeManager.isPreviewMode, true);

      await vscode.commands.executeCommand('agent-studios.toggleFixturePreview');
      assert.strictEqual(bridgeManager.isPreviewMode, false);
      assert.ok(
        mockWindowMessages.info.some((msg) =>
          msg.includes('preview mode disabled')
        )
      );

      await vscode.commands.executeCommand('agent-studios.toggleFixturePreview');
      assert.strictEqual(bridgeManager.isPreviewMode, true);
      assert.ok(
        mockWindowMessages.info.some((msg) =>
          msg.includes('preview mode enabled')
        )
      );
    });

    it('executes agent-studios.refreshViews and calls refresh on views', async () => {
      let sessionRefreshed = false;
      let tasksRefreshed = false;

      const customViews: ViewProviders = {
        session: { refresh: () => { sessionRefreshed = true; } } as any,
        tasks: { refresh: () => { tasksRefreshed = true; } } as any,
      };

      const customDisposable = registerAllCommands(
        context,
        bridgeManager,
        customViews,
        mockWebviews
      );

      await vscode.commands.executeCommand('agent-studios.refreshViews');

      assert.strictEqual(sessionRefreshed, true);
      assert.strictEqual(tasksRefreshed, true);

      customDisposable.dispose();
    });
  });

  describe('Editor Context Menu Commands (Safety & Validation)', () => {
    it('warns when no active text editor is open', async () => {
      (vscode.window as any).activeTextEditor = undefined;

      await vscode.commands.executeCommand('agent-studios.explainSelection');

      assert.ok(
        mockWindowMessages.warning.some((msg) =>
          msg.includes('No active editor found')
        )
      );
    });

    it('warns when selection is empty', async () => {
      (vscode.window as any).activeTextEditor = {
        selection: { isEmpty: true },
        document: { uri: { toString: () => 'file:///test.ts' }, languageId: 'typescript', getText: () => '' },
      };

      await vscode.commands.executeCommand('agent-studios.refactorSelection');

      assert.ok(
        mockWindowMessages.warning.some((msg) =>
          msg.includes('Please select code to refactor')
        )
      );
    });

    it('blocks AI actions when workspace is untrusted', async () => {
      (vscode.window as any).activeTextEditor = {
        selection: {
          isEmpty: false,
          start: { line: 0, character: 0 },
          end: { line: 2, character: 10 },
        },
        document: {
          uri: { toString: () => 'file:///test.ts' },
          languageId: 'typescript',
          getText: () => 'const x = 1;',
        },
      };

      (vscode.workspace as any).isTrusted = false;

      await vscode.commands.executeCommand('agent-studios.generateTests');

      assert.ok(
        mockWindowMessages.error.some((msg) =>
          msg.includes('require a trusted workspace')
        )
      );

      (vscode.workspace as any).isTrusted = true;
    });

    it('blocks AI actions when bridge is disconnected', async () => {
      bridgeManager.setBridge(new DisconnectedBridge());

      (vscode.window as any).activeTextEditor = {
        selection: {
          isEmpty: false,
          start: { line: 0, character: 0 },
          end: { line: 2, character: 10 },
        },
        document: {
          uri: { toString: () => 'file:///test.ts' },
          languageId: 'typescript',
          getText: () => 'const x = 1;',
        },
      };

      await vscode.commands.executeCommand('agent-studios.explainSelection');

      assert.ok(
        mockWindowMessages.error.some((msg) =>
          msg.includes('daemon connection is required for AI actions')
        )
      );
    });

    it('dispatches editor action when connected and trusted with non-empty selection', async () => {
      (vscode.workspace as any).isTrusted = true;
      (vscode.window as any).activeTextEditor = {
        selection: {
          isEmpty: false,
          start: { line: 10, character: 2 },
          end: { line: 15, character: 4 },
        },
        document: {
          uri: { toString: () => 'file:///src/app.ts' },
          languageId: 'typescript',
          getText: () => 'function compute() { return 42; }',
        },
      };

      await vscode.commands.executeCommand('agent-studios.explainSelection');

      assert.ok(
        mockWindowMessages.info.some((msg) =>
          msg.includes("Editor action 'explain' accepted")
        )
      );
    });
  });
});
