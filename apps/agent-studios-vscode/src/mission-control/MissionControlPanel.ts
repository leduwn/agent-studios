/**
 * Agent Studios — Mission Control Panel
 * Milestone M14 Mission Control Workbench
 *
 * Implements editor WebviewPanel singleton managing:
 * - Panel lifecycle (createOrShow, revive, dispose)
 * - Strict Content Security Policy
 * - Reactive state projection synchronization
 * - Client-host message handling (openDiff, openChat, selectAgent, selectTask, refresh)
 * - Fail-closed security boundary for mutations
 */

import * as vscode from 'vscode';
import { DiffReviewManager } from '../cards/DiffReviewManager';
import { MissionControlHtmlBuilder } from './MissionControlHtmlBuilder';
import { MissionControlState, MissionControlWebviewMessage } from './models/types';
import { MissionControlProjection } from './state/MissionControlProjection';

export class MissionControlPanel implements vscode.Disposable {
  public static readonly viewType = 'agent-studios.missionControl';
  public static currentPanel: MissionControlPanel | undefined;

  private readonly _panel: vscode.WebviewPanel;
  private readonly _extensionUri: vscode.Uri;
  private readonly _projection: MissionControlProjection;
  private readonly _diffReviewManager?: DiffReviewManager;
  private _disposables: vscode.Disposable[] = [];
  private _isHtmlInitialized = false;

  public static createOrShow(
    extensionUri: vscode.Uri,
    projection: MissionControlProjection,
    diffReviewManager?: DiffReviewManager
  ): MissionControlPanel {
    const column = vscode.window.activeTextEditor
      ? vscode.window.activeTextEditor.viewColumn
      : undefined;

    // If panel already exists, reveal it
    if (MissionControlPanel.currentPanel) {
      MissionControlPanel.currentPanel._panel.reveal(column);
      MissionControlPanel.currentPanel._projection.refresh();
      return MissionControlPanel.currentPanel;
    }

    // Otherwise create a new WebviewPanel
    const panel = vscode.window.createWebviewPanel(
      MissionControlPanel.viewType,
      'Agent Studios: Mission Control',
      column || vscode.ViewColumn.One,
      {
        enableScripts: true,
        retainContextWhenHidden: true,
        localResourceRoots: [extensionUri],
      }
    );

    panel.iconPath = vscode.Uri.joinPath(extensionUri, 'media', 'agents-activitybar.svg');

    MissionControlPanel.currentPanel = new MissionControlPanel(
      panel,
      extensionUri,
      projection,
      diffReviewManager
    );
    return MissionControlPanel.currentPanel;
  }

  public static revive(
    panel: vscode.WebviewPanel,
    extensionUri: vscode.Uri,
    projection: MissionControlProjection,
    diffReviewManager?: DiffReviewManager
  ): MissionControlPanel {
    MissionControlPanel.currentPanel = new MissionControlPanel(
      panel,
      extensionUri,
      projection,
      diffReviewManager
    );
    return MissionControlPanel.currentPanel;
  }

  private constructor(
    panel: vscode.WebviewPanel,
    extensionUri: vscode.Uri,
    projection: MissionControlProjection,
    diffReviewManager?: DiffReviewManager
  ) {
    this._panel = panel;
    this._extensionUri = extensionUri;
    this._projection = projection;
    this._diffReviewManager = diffReviewManager;

    // Render initial state
    this._update(this._projection.getState());

    // Listen for disposal
    this._panel.onDidDispose(() => this.dispose(), null, this._disposables);

    // Listen for panel visibility change
    if (typeof this._panel.onDidChangeViewState === 'function') {
      this._panel.onDidChangeViewState(
        (e) => {
          if (e.webviewPanel.visible) {
            this._projection.refresh();
          }
        },
        null,
        this._disposables
      );
    }

    // Listen for incoming messages from webview
    this._panel.webview.onDidReceiveMessage(
      (message: MissionControlWebviewMessage) => this._handleMessage(message),
      null,
      this._disposables
    );

    // Listen for state changes from projection
    this._disposables.push(
      this._projection.onDidChangeState((state) => {
        this._update(state);
      })
    );
  }

  private _update(state: MissionControlState): void {
    this._panel.title = state.activeStudio
      ? `Mission Control: ${state.activeStudio.name}`
      : 'Agent Studios: Mission Control';

    if (!this._isHtmlInitialized) {
      this._panel.webview.html = MissionControlHtmlBuilder.buildHtml(
        this._panel.webview,
        this._extensionUri,
        state
      );
      this._isHtmlInitialized = true;
    } else {
      const fragments = MissionControlHtmlBuilder.buildViewFragments(state);
      const pendingApprovalsCount = state.approvals.filter((a) => a.status === 'pending').length;
      this._panel.webview.postMessage({
        type: 'stateUpdate',
        state,
        fragments,
        badges: {
          agents: state.fleet.length,
          tasks: state.tasks.length,
          review: state.artifacts.length,
          approvals: pendingApprovalsCount,
        },
      });
    }
  }

  private async _handleMessage(message: MissionControlWebviewMessage): Promise<void> {
    if (!message || typeof message.type !== 'string') {
      return;
    }

    switch (message.type) {
      case 'switchTab':
        if (message.payload?.tab) {
          this._projection.setActiveTab(message.payload.tab);
        }
        break;

      case 'selectAgent':
        this._projection.selectAgent(message.payload?.agentId ?? null);
        break;

      case 'selectTask':
        this._projection.selectTask(message.payload?.taskId ?? null);
        break;

      case 'selectRun':
        this._projection.selectRun(message.payload?.runId ?? null);
        break;

      case 'openDiff':
        if (message.payload?.artifactId) {
          await this._openDiff(message.payload.artifactId);
        }
        break;

      case 'openChat':
        await this._openChat(message.payload?.sessionId);
        break;

      case 'refresh':
        await this._projection.refresh();
        break;

      case 'approveAction':
      case 'denyAction':
        vscode.window.showWarningMessage(
          'Approval decisions are read-only in this vertical slice. Fail-closed security boundary is active.'
        );
        break;

      default:
        break;
    }
  }

  private async _openDiff(artifactId: string): Promise<void> {
    try {
      if (this._diffReviewManager) {
        await this._diffReviewManager.openDiff(artifactId);
      } else {
        await vscode.commands.executeCommand('agent-studios.inspectArtifact', artifactId);
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      vscode.window.showErrorMessage(`Failed to open artifact diff: ${message}`);
    }
  }

  private async _openChat(sessionId?: string): Promise<void> {
    try {
      await vscode.commands.executeCommand('agent-studios.chat.focus');
      if (sessionId) {
        await vscode.commands.executeCommand('agent-studios.openSession', sessionId);
      }
    } catch {
      // Chat view focus fallback
    }
  }

  public dispose(): void {
    MissionControlPanel.currentPanel = undefined;

    this._panel.dispose();

    while (this._disposables.length) {
      const x = this._disposables.pop();
      if (x) {
        x.dispose();
      }
    }
  }
}
