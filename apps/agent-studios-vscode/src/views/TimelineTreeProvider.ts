/**
 * Agent Studios — Timeline TreeDataProvider
 * Milestone M14-UI-00: Code-OSS Native Agents Workbench
 */

import * as vscode from 'vscode';
import { AgentStudiosBridge } from '../bridge/AgentStudiosBridge';
import { BridgeManager } from '../bridge/BridgeManager';
import { TimelineEvent, TimelineEventType } from '../bridge/types';
import { SessionStateProjection } from '../state/SessionStateProjection';

export class TimelineTreeItem extends vscode.TreeItem {
  public readonly event?: TimelineEvent;

  constructor(
    eventOrLabel: TimelineEvent | string,
    collapsibleState: vscode.TreeItemCollapsibleState = vscode.TreeItemCollapsibleState.None
  ) {
    if (typeof eventOrLabel === 'string') {
      super(eventOrLabel, collapsibleState);
      return;
    }

    const event = eventOrLabel;
    super(event.summary, collapsibleState);
    this.event = event;
    this.id = event.id || `evt-${event.sequenceNumber}`;

    this.contextValue = 'timelineItem';

    // Formatted timestamp (HH:MM:SS) and agent name badge
    const timeFormatted = TimelineTreeItem.formatTimestamp(event.timestamp);
    const agentBadge = event.agentName ? ` [${event.agentName}]` : '';
    this.description = `${timeFormatted}${agentBadge}`;

    // Contextual icon based on event type
    this.iconPath = TimelineTreeItem.getIconForEventType(event.type);

    // Tooltip
    const tooltipParts = [
      `[#${event.sequenceNumber}] ${event.type}`,
      `Time: ${new Date(event.timestamp).toISOString()}`,
      `Summary: ${event.summary}`,
      event.agentName ? `Agent: ${event.agentName}` : null,
      event.taskId ? `Task ID: ${event.taskId}` : null,
    ].filter(Boolean);
    this.tooltip = tooltipParts.join('\n');

    if (event.taskId) {
      this.command = {
        command: 'agent-studios.showTaskGraph',
        title: 'Show Task in Graph',
        arguments: [event.taskId],
      };
    }
  }

  public static formatTimestamp(timestamp: number): string {
    const d = new Date(timestamp);
    const pad = (n: number) => n.toString().padStart(2, '0');
    return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
  }

  public static getIconForEventType(type: TimelineEventType): vscode.ThemeIcon {
    switch (type) {
      case 'session_started':
      case 'run_started':
      case 'task_started':
        return new vscode.ThemeIcon('play');

      case 'session_ended':
      case 'run_completed':
      case 'task_completed':
        return new vscode.ThemeIcon(
          'check',
          new vscode.ThemeColor('testing.iconPassed')
        );

      case 'run_failed':
      case 'task_failed':
        return new vscode.ThemeIcon(
          'error',
          new vscode.ThemeColor('testing.iconFailed')
        );

      case 'task_blocked':
        return new vscode.ThemeIcon(
          'lock',
          new vscode.ThemeColor('problemsWarningIcon.foreground')
        );

      case 'agent_assigned':
        return new vscode.ThemeIcon('person');

      case 'tool_executing':
      case 'tool_completed':
        return new vscode.ThemeIcon('tools');

      case 'approval_requested':
      case 'approval_decided':
        return new vscode.ThemeIcon('shield');

      case 'artifact_created':
        return new vscode.ThemeIcon('diff');

      default:
        return new vscode.ThemeIcon('circle-outline');
    }
  }
}

export class TimelineTreeProvider
  implements vscode.TreeDataProvider<TimelineTreeItem>, vscode.Disposable
{
  private _projection?: SessionStateProjection;
  private readonly _disposables: vscode.Disposable[] = [];

  private readonly _onDidChangeTreeData = new vscode.EventEmitter<
    TimelineTreeItem | undefined | void
  >();
  public readonly onDidChangeTreeData: vscode.Event<TimelineTreeItem | undefined | void> =
    this._onDidChangeTreeData.event;

  constructor(
    bridgeOrManager: AgentStudiosBridge | BridgeManager | SessionStateProjection,
    projection?: SessionStateProjection
  ) {
    this._disposables.push(this._onDidChangeTreeData);

    if ('stateProjection' in bridgeOrManager) {
      const manager = bridgeOrManager as BridgeManager;
      this._projection = manager.stateProjection;
      this._disposables.push(
        manager.onBridgeChanged(() => {
          this._projection = manager.stateProjection;
          this.refresh();
        })
      );
    } else if ('handleEvent' in bridgeOrManager) {
      this._projection = bridgeOrManager as SessionStateProjection;
    } else if (projection) {
      this._projection = projection;
    }

    if (this._projection) {
      this._disposables.push(
        this._projection.onTimelineChanged(() => this.refresh())
      );
    }
  }

  public refresh(): void {
    this._onDidChangeTreeData.fire();
  }

  public getTreeItem(element: TimelineTreeItem): vscode.TreeItem {
    return element;
  }

  public getChildren(element?: TimelineTreeItem): vscode.ProviderResult<TimelineTreeItem[]> {
    if (element) {
      return [];
    }

    const events = this._projection?.timeline ?? [];

    if (events.length === 0) {
      const placeholder = new TimelineTreeItem(
        'No activity recorded',
        vscode.TreeItemCollapsibleState.None
      );
      placeholder.iconPath = new vscode.ThemeIcon('history');
      placeholder.description = 'Activity stream will appear here';
      placeholder.tooltip = 'No timeline events have been emitted for current session.';
      placeholder.contextValue = 'noActivity';
      return [placeholder];
    }

    // Chronological feed: newest first
    const sorted = [...events].sort((a, b) => b.sequenceNumber - a.sequenceNumber);
    return sorted.map((evt) => new TimelineTreeItem(evt));
  }

  public dispose(): void {
    while (this._disposables.length > 0) {
      const d = this._disposables.pop();
      d?.dispose();
    }
  }
}
