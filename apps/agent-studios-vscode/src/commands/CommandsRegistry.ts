/**
 * Agent Studios — Commands Registry
 * Milestone M14-UI-00: Code-OSS Native Agents Workbench
 */

import * as vscode from 'vscode';
import { BridgeManager } from '../bridge/BridgeManager';
import { DisconnectedBridge } from '../bridge/DisconnectedBridge';
import { FixtureBridge } from '../bridge/FixtureBridge';
import { EditorActionRequest } from '../bridge/types';
import {
  ApprovalsTreeProvider,
  ArtifactsTreeProvider,
  ProvidersTreeProvider,
  SessionTreeProvider,
  TasksTreeProvider,
  TimelineTreeProvider,
} from '../views';
import { SessionDetailWebview } from '../webviews/SessionDetailWebview';
import { TaskGraphWebview } from '../webviews/TaskGraphWebview';

export interface ViewProviders {
  session?: SessionTreeProvider;
  tasks?: TasksTreeProvider;
  approvals?: ApprovalsTreeProvider;
  artifacts?: ArtifactsTreeProvider;
  timeline?: TimelineTreeProvider;
  providers?: ProvidersTreeProvider;
  refreshAll?: () => void;
}

export interface WebviewManagers {
  openSession?: () => void | Promise<void>;
  showTaskGraph?: () => void | Promise<void>;
}

/**
 * Executes an editor action with workspace trust and connectivity checks.
 */
async function handleEditorAction(
  action: 'explain' | 'refactor' | 'generateTests',
  bridgeManager: BridgeManager
): Promise<void> {
  const editor = vscode.window.activeTextEditor;
  if (!editor) {
    vscode.window.showWarningMessage('No active editor found.');
    return;
  }

  const selection = editor.selection;
  if (!selection || selection.isEmpty) {
    vscode.window.showWarningMessage(
      `Please select code to ${action === 'generateTests' ? 'generate tests for' : action}.`
    );
    return;
  }

  // Check workspace trust
  if (vscode.workspace.isTrusted === false) {
    vscode.window.showErrorMessage('Agent Studios AI actions require a trusted workspace.');
    return;
  }

  // Validate bridge is connected
  if (bridgeManager.bridge.connectionState !== 'connected') {
    vscode.window.showErrorMessage(
      'Agent Studios daemon connection is required for AI actions.'
    );
    return;
  }

  const document = editor.document;
  const selectedText = document.getText(selection);
  const actionRequest: EditorActionRequest = {
    action,
    documentUri: document.uri.toString(),
    languageId: document.languageId,
    selectedText,
    selectionRange: {
      startLine: selection.start.line + 1,
      startCol: selection.start.character + 1,
      endLine: selection.end.line + 1,
      endCol: selection.end.character + 1,
    },
  };

  try {
    const result = await bridgeManager.bridge.submitEditorAction(actionRequest);
    if (result.accepted) {
      vscode.window.showInformationMessage(result.message);
    } else {
      vscode.window.showErrorMessage(result.message || `Action '${action}' was rejected.`);
    }
  } catch (err: unknown) {
    vscode.window.showErrorMessage(
      `Cannot execute ${action}: ${err instanceof Error ? err.message : String(err)}`
    );
  }
}

/**
 * Registers all workbench and editor context menu commands.
 */
export function registerAllCommands(
  context: vscode.ExtensionContext,
  bridgeManager: BridgeManager,
  views: ViewProviders,
  webviews: WebviewManagers
): vscode.Disposable {
  const disposables: vscode.Disposable[] = [];

  // 1. agent-studios.openAgents: focuses activity bar view
  disposables.push(
    vscode.commands.registerCommand('agent-studios.openAgents', async () => {
      await vscode.commands.executeCommand(
        'workbench.view.extension.agent-studios-activitybar'
      );
    })
  );

  // 2. agent-studios.openSession: opens session detail webview
  disposables.push(
    vscode.commands.registerCommand('agent-studios.openSession', async () => {
      if (webviews.openSession) {
        await webviews.openSession();
        return;
      }
      const session = await bridgeManager.bridge.getCurrentSession();
      const tasks = await bridgeManager.bridge.listTasks();
      SessionDetailWebview.createOrShow(
        context.extensionUri,
        session,
        tasks,
        undefined,
        bridgeManager.bridge
      );
    })
  );

  // 3. agent-studios.showTaskGraph: opens interactive DAG webview
  disposables.push(
    vscode.commands.registerCommand('agent-studios.showTaskGraph', async () => {
      if (webviews.showTaskGraph) {
        await webviews.showTaskGraph();
        return;
      }
      const graph = await bridgeManager.bridge.getTaskGraph();
      TaskGraphWebview.createOrShow(
        context.extensionUri,
        graph,
        bridgeManager.bridge
      );
    })
  );

  // 4. agent-studios.showApprovals: focuses approvals view
  try {
    disposables.push(
      vscode.commands.registerCommand('agent-studios.showApprovals', async () => {
        await vscode.commands.executeCommand('agent-studios.approvals.focus');
      })
    );
  } catch {
    // If already registered by registerApprovalCommands, don't crash
  }

  // 5. agent-studios.connectDaemon: prompts user for endpoint or connects to configured daemon endpoint; updates bridge
  disposables.push(
    vscode.commands.registerCommand('agent-studios.connectDaemon', async () => {
      const config = vscode.workspace.getConfiguration('agentStudios');
      const defaultEndpoint = config.get<string>(
        'daemonEndpoint',
        'ipc://agent-studios.sock'
      );

      const endpoint = await vscode.window.showInputBox({
        prompt: 'Connect to Agent Studios authoritative daemon endpoint',
        value: defaultEndpoint,
        placeHolder: 'ipc://agent-studios.sock or http://127.0.0.1:4000',
      });

      if (endpoint === undefined) {
        return;
      }

      try {
        await bridgeManager.bridge.connect();
        vscode.window.showInformationMessage(
          `Agent Studios connected to daemon at ${endpoint}.`
        );
      } catch (err: unknown) {
        vscode.window.showErrorMessage(
          `Failed to connect to daemon at ${endpoint}: ${
            err instanceof Error ? err.message : String(err)
          }`
        );
      }
    })
  );

  // 6. agent-studios.disconnectDaemon: disconnects bridge, returns to safe disconnected state
  disposables.push(
    vscode.commands.registerCommand('agent-studios.disconnectDaemon', async () => {
      await bridgeManager.bridge.disconnect();
      bridgeManager.setBridge(new DisconnectedBridge());
      vscode.window.showInformationMessage('Agent Studios disconnected from daemon.');
    })
  );

  // 7. agent-studios.toggleFixturePreview: toggles preview mode between DisconnectedBridge and FixtureBridge. Shows visible notification!
  disposables.push(
    vscode.commands.registerCommand('agent-studios.toggleFixturePreview', async () => {
      const currentlyPreview = bridgeManager.isPreviewMode;
      const nextIsPreview = !currentlyPreview;

      if (nextIsPreview) {
        bridgeManager.setBridge(new FixtureBridge());
        vscode.window.showInformationMessage(
          'Agent Studios developer preview mode enabled (Fixture Bridge).'
        );
      } else {
        bridgeManager.setBridge(new DisconnectedBridge());
        vscode.window.showInformationMessage(
          'Agent Studios developer preview mode disabled (Disconnected).'
        );
      }

      const config = vscode.workspace.getConfiguration('agentStudios');
      if (config.get<boolean>('previewMode') !== nextIsPreview) {
        await config.update('previewMode', nextIsPreview, vscode.ConfigurationTarget.Global);
      }

      await vscode.commands.executeCommand('agent-studios.refreshViews');
    })
  );

  // 8. agent-studios.refreshViews: refreshes all sidebar tree views
  disposables.push(
    vscode.commands.registerCommand('agent-studios.refreshViews', () => {
      views.session?.refresh?.();
      views.tasks?.refresh?.();
      views.approvals?.refresh?.();
      views.artifacts?.refresh?.();
      views.timeline?.refresh?.();
      views.providers?.refresh?.();
      views.refreshAll?.();
    })
  );

  // 9. Editor context menu: explainSelection
  disposables.push(
    vscode.commands.registerCommand('agent-studios.explainSelection', async () => {
      await handleEditorAction('explain', bridgeManager);
    })
  );

  // 10. Editor context menu: refactorSelection
  disposables.push(
    vscode.commands.registerCommand('agent-studios.refactorSelection', async () => {
      await handleEditorAction('refactor', bridgeManager);
    })
  );

  // 11. Editor context menu: generateTests
  disposables.push(
    vscode.commands.registerCommand('agent-studios.generateTests', async () => {
      await handleEditorAction('generateTests', bridgeManager);
    })
  );

  const composite = vscode.Disposable.from(...disposables);
  context.subscriptions.push(composite);
  return composite;
}
