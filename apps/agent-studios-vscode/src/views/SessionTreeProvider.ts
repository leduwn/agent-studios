/**
 * Agent Studios — Session TreeDataProvider
 * Milestone M14-UI-00: Code-OSS Native Agents Workbench
 */

import * as vscode from 'vscode';
import { AgentStudiosBridge } from '../bridge/AgentStudiosBridge';
import { BridgeManager } from '../bridge/BridgeManager';
import { ConnectionState, SessionInfo } from '../bridge/types';
import { SessionStateProjection } from '../state/SessionStateProjection';

export class SessionTreeItem extends vscode.TreeItem {
  public children?: SessionTreeItem[];

  constructor(
    label: string,
    collapsibleState: vscode.TreeItemCollapsibleState = vscode.TreeItemCollapsibleState.None,
    options?: {
      description?: string;
      tooltip?: string | vscode.MarkdownString;
      icon?: vscode.ThemeIcon;
      contextValue?: string;
      command?: vscode.Command;
      children?: SessionTreeItem[];
    }
  ) {
    super(label, collapsibleState);
    if (options?.description !== undefined) {
      this.description = options.description;
    }
    if (options?.tooltip !== undefined) {
      this.tooltip = options.tooltip;
    }
    if (options?.icon !== undefined) {
      this.iconPath = options.icon;
    }
    if (options?.contextValue !== undefined) {
      this.contextValue = options.contextValue;
    }
    if (options?.command !== undefined) {
      this.command = options.command;
    }
    if (options?.children !== undefined) {
      this.children = options.children;
    }
  }
}

export class SessionTreeProvider
  implements vscode.TreeDataProvider<SessionTreeItem>, vscode.Disposable
{
  private _bridgeManager?: BridgeManager;
  private _bridge?: AgentStudiosBridge;
  private _projection?: SessionStateProjection;

  private readonly _disposables: vscode.Disposable[] = [];
  private readonly _bridgeDisposables: vscode.Disposable[] = [];

  private readonly _onDidChangeTreeData = new vscode.EventEmitter<
    SessionTreeItem | undefined | void
  >();
  public readonly onDidChangeTreeData: vscode.Event<SessionTreeItem | undefined | void> =
    this._onDidChangeTreeData.event;

  constructor(
    bridgeOrManager: AgentStudiosBridge | BridgeManager | SessionStateProjection,
    projectionOrBridge?: SessionStateProjection | AgentStudiosBridge
  ) {
    this._disposables.push(this._onDidChangeTreeData);

    if ('stateProjection' in bridgeOrManager) {
      // BridgeManager instance
      this._bridgeManager = bridgeOrManager as BridgeManager;
      this._bridge = this._bridgeManager.bridge;
      this._projection = this._bridgeManager.stateProjection;
      this._disposables.push(
        this._bridgeManager.onBridgeChanged((newBridge) => {
          this._bridge = newBridge;
          this.bindBridgeListeners();
          this.refresh();
        })
      );
    } else if ('getCurrentSession' in bridgeOrManager) {
      // AgentStudiosBridge instance
      this._bridge = bridgeOrManager as AgentStudiosBridge;
      if (projectionOrBridge && 'handleEvent' in projectionOrBridge) {
        this._projection = projectionOrBridge as SessionStateProjection;
      }
    } else if ('handleEvent' in bridgeOrManager) {
      // SessionStateProjection instance
      this._projection = bridgeOrManager as SessionStateProjection;
      if (projectionOrBridge && 'getCurrentSession' in projectionOrBridge) {
        this._bridge = projectionOrBridge as AgentStudiosBridge;
      }
    }

    if (this._projection) {
      this._disposables.push(
        this._projection.onSessionChanged(() => this.refresh()),
        this._projection.onTasksChanged(() => this.refresh())
      );
    }

    this.bindBridgeListeners();
  }

  private bindBridgeListeners(): void {
    while (this._bridgeDisposables.length > 0) {
      const d = this._bridgeDisposables.pop();
      d?.dispose();
    }
    if (this._bridge) {
      this._bridgeDisposables.push(
        this._bridge.onConnectionStateChanged(() => this.refresh())
      );
    }
  }

  public refresh(): void {
    this._onDidChangeTreeData.fire();
  }

  public getTreeItem(element: SessionTreeItem): vscode.TreeItem {
    return element;
  }

  public getChildren(element?: SessionTreeItem): vscode.ProviderResult<SessionTreeItem[]> {
    if (element) {
      return element.children ?? [];
    }

    const items: SessionTreeItem[] = [];
    const isPreview = this.checkIsPreview();

    // 1. Preview mode indicator if active
    if (isPreview) {
      items.push(
        new SessionTreeItem(
          'Mode: [MOCK / PREVIEW FIXTURE]',
          vscode.TreeItemCollapsibleState.None,
          {
            description: 'Preview Active',
            tooltip:
              'Developer fixture preview mode active. Displaying mock session data [MOCK / PREVIEW FIXTURE]. Click to toggle.',
            icon: new vscode.ThemeIcon(
              'beaker',
              new vscode.ThemeColor('problemsWarningIcon.foreground')
            ),
            contextValue: 'sessionPreviewMode',
            command: {
              command: 'agent-studios.toggleFixturePreview',
              title: 'Toggle Developer Fixture Preview',
            },
          }
        )
      );
    }

    // 2. Connection status
    const connState: ConnectionState = this._bridge?.connectionState ?? 'disconnected';
    items.push(this.createConnectionItem(connState));

    const session: SessionInfo | null = this._projection?.session ?? null;

    if (!session) {
      items.push(
        new SessionTreeItem(
          'No Active Session',
          vscode.TreeItemCollapsibleState.None,
          {
            description: connState === 'connected' ? 'Idle' : 'Disconnected',
            tooltip: 'No active session projection available.',
            icon: new vscode.ThemeIcon('info'),
            contextValue: 'sessionEmpty',
          }
        )
      );
      return items;
    }

    // 3. Workspace path
    items.push(
      new SessionTreeItem('Workspace', vscode.TreeItemCollapsibleState.None, {
        description: session.workspacePath,
        tooltip: `Workspace root: ${session.workspacePath}`,
        icon: new vscode.ThemeIcon('folder'),
        contextValue: 'sessionWorkspace',
      })
    );

    // 4. Goal summary (expandable)
    items.push(
      new SessionTreeItem('Goal', vscode.TreeItemCollapsibleState.Collapsed, {
        description: session.goalSummary,
        tooltip: `Session Goal: ${session.goalSummary}`,
        icon: new vscode.ThemeIcon('target'),
        contextValue: 'sessionGoal',
        children: [
          new SessionTreeItem(
            session.goalSummary,
            vscode.TreeItemCollapsibleState.None,
            {
              tooltip: session.goalSummary,
              icon: new vscode.ThemeIcon('checklist'),
            }
          ),
        ],
      })
    );

    // 5. Active run status and progress
    const runProgress = `${session.completedTasksCount}/${session.totalTasksCount} tasks completed`;
    const runStatusLabel = session.status.charAt(0).toUpperCase() + session.status.slice(1);
    const runIcon = this.getRunIcon(session.status);

    items.push(
      new SessionTreeItem(
        `Run: ${session.activeRunId ?? 'None'}`,
        vscode.TreeItemCollapsibleState.None,
        {
          description: `${runStatusLabel}: ${runProgress}`,
          tooltip: `Run ID: ${session.activeRunId ?? 'None'}\nStatus: ${session.status}\nProgress: ${runProgress}`,
          icon: runIcon,
          contextValue: 'sessionRun',
          command: {
            command: 'agent-studios.openSession',
            title: 'Open Session Details',
          },
        }
      )
    );

    // 6. Active agent count and names
    const agentNames = this.resolveAgentNames(session);
    items.push(
      new SessionTreeItem(
        `Active Agents: ${session.activeAgentCount}/${session.totalAgentCount}`,
        agentNames.length > 0
          ? vscode.TreeItemCollapsibleState.Collapsed
          : vscode.TreeItemCollapsibleState.None,
        {
          description: agentNames.join(', ') || undefined,
          tooltip: `Active Agents (${session.activeAgentCount}):\n${agentNames.join('\n') || 'None'}`,
          icon: new vscode.ThemeIcon('organization'),
          contextValue: 'sessionAgents',
          children: agentNames.map(
            (name) =>
              new SessionTreeItem(name, vscode.TreeItemCollapsibleState.None, {
                tooltip: `Agent: ${name}`,
                icon: new vscode.ThemeIcon('person'),
              })
          ),
        }
      )
    );

    // 7. Provider / model summary
    if (session.providerSummary) {
      items.push(
        new SessionTreeItem('Provider / Model', vscode.TreeItemCollapsibleState.None, {
          description: session.providerSummary,
          tooltip: `Configured Model Provider: ${session.providerSummary}`,
          icon: new vscode.ThemeIcon('server'),
          contextValue: 'sessionProvider',
        })
      );
    }

    // 8. Elapsed time
    items.push(
      new SessionTreeItem('Elapsed Time', vscode.TreeItemCollapsibleState.None, {
        description: this.formatElapsedTime(session.elapsedMs),
        tooltip: `Elapsed run time: ${session.elapsedMs} ms`,
        icon: new vscode.ThemeIcon('clock'),
        contextValue: 'sessionElapsed',
      })
    );

    return items;
  }

  private checkIsPreview(): boolean {
    if (this._bridgeManager?.isPreviewMode) {
      return true;
    }
    if (this._projection?.session?.isPreview) {
      return true;
    }
    const config = vscode.workspace.getConfiguration('agentStudios');
    return config.get<boolean>('previewMode', false);
  }

  private createConnectionItem(state: ConnectionState): SessionTreeItem {
    let icon: vscode.ThemeIcon;
    let label = `Status: ${state.charAt(0).toUpperCase() + state.slice(1).replace('_', ' ')}`;
    let contextValue = 'sessionConnection';
    let command: vscode.Command | undefined;

    switch (state) {
      case 'connected':
        icon = new vscode.ThemeIcon('pass-filled', new vscode.ThemeColor('testing.iconPassed'));
        contextValue = 'sessionConnected';
        command = {
          command: 'agent-studios.disconnectDaemon',
          title: 'Disconnect from Daemon',
        };
        break;
      case 'connecting':
      case 'reconnecting':
        icon = new vscode.ThemeIcon('sync~spin');
        break;
      case 'degraded':
        icon = new vscode.ThemeIcon(
          'warning',
          new vscode.ThemeColor('problemsWarningIcon.foreground')
        );
        break;
      case 'auth_required':
        icon = new vscode.ThemeIcon('key', new vscode.ThemeColor('problemsWarningIcon.foreground'));
        break;
      case 'incompatible':
        icon = new vscode.ThemeIcon('error', new vscode.ThemeColor('testing.iconFailed'));
        break;
      case 'disconnected':
      default:
        icon = new vscode.ThemeIcon('circle-slash');
        contextValue = 'sessionDisconnected';
        command = {
          command: 'agent-studios.connectDaemon',
          title: 'Connect to Daemon',
        };
        break;
    }

    return new SessionTreeItem(label, vscode.TreeItemCollapsibleState.None, {
      tooltip: `Connection state: ${state}`,
      icon,
      contextValue,
      command,
    });
  }

  private getRunIcon(status: SessionInfo['status']): vscode.ThemeIcon {
    switch (status) {
      case 'running':
        return new vscode.ThemeIcon('sync~spin');
      case 'completed':
        return new vscode.ThemeIcon('check', new vscode.ThemeColor('testing.iconPassed'));
      case 'failed':
        return new vscode.ThemeIcon('error', new vscode.ThemeColor('testing.iconFailed'));
      case 'paused':
        return new vscode.ThemeIcon(
          'debug-pause',
          new vscode.ThemeColor('problemsWarningIcon.foreground')
        );
      case 'planning':
        return new vscode.ThemeIcon('edit');
      case 'idle':
      default:
        return new vscode.ThemeIcon('circle-outline');
    }
  }

  private resolveAgentNames(session: SessionInfo): string[] {
    const names = new Set<string>();
    if (this._projection) {
      for (const t of this._projection.tasks) {
        if (t.assignedAgentName) {
          names.add(t.assignedAgentName);
        }
      }
    }
    if (names.size === 0 && session.activeAgentCount > 0) {
      names.add('Assigned Agent');
    }
    return Array.from(names);
  }

  private formatElapsedTime(elapsedMs: number): string {
    const totalSeconds = Math.floor(elapsedMs / 1000);
    const hours = Math.floor(totalSeconds / 3600);
    const minutes = Math.floor((totalSeconds % 3600) / 60);
    const seconds = totalSeconds % 60;

    if (hours > 0) {
      return `${hours}h ${minutes}m ${seconds}s`;
    }
    if (minutes > 0) {
      return `${minutes}m ${seconds}s`;
    }
    return `${seconds}s`;
  }

  public dispose(): void {
    while (this._bridgeDisposables.length > 0) {
      const d = this._bridgeDisposables.pop();
      d?.dispose();
    }
    while (this._disposables.length > 0) {
      const d = this._disposables.pop();
      d?.dispose();
    }
  }
}
