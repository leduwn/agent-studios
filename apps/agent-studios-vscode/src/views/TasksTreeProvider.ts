/**
 * Agent Studios — Tasks TreeDataProvider
 * Milestone M14-UI-00: Code-OSS Native Agents Workbench
 */

import * as vscode from 'vscode';
import { AgentStudiosBridge } from '../bridge/AgentStudiosBridge';
import { BridgeManager } from '../bridge/BridgeManager';
import { TaskNode, TaskStatus } from '../bridge/types';
import { SessionStateProjection } from '../state/SessionStateProjection';

export class TaskTreeItem extends vscode.TreeItem {
  public readonly task?: TaskNode;

  constructor(
    taskOrLabel: TaskNode | string,
    collapsibleState: vscode.TreeItemCollapsibleState = vscode.TreeItemCollapsibleState.None
  ) {
    if (typeof taskOrLabel === 'string') {
      super(taskOrLabel, collapsibleState);
      return;
    }

    const task = taskOrLabel;
    super(task.title, collapsibleState);
    this.task = task;
    this.id = task.id;

    // Custom ThemeIcon for task status
    this.iconPath = TaskTreeItem.getIconForStatus(task.status);

    // Description shows assigned agent (e.g. (Coder Agent)) and progress percentage
    const agentPart = task.assignedAgentName ? `(${task.assignedAgentName}) ` : '';
    this.description = `${agentPart}${task.progress}%`;

    // Tooltip shows task description, dependencies, error details if failed
    const tooltipParts = [
      `Task: ${task.title} [${task.status.toUpperCase()}]`,
      `ID: ${task.id}`,
      `Progress: ${task.progress}%`,
      task.assignedAgentName ? `Assigned Agent: ${task.assignedAgentName}` : null,
      `Description: ${task.description}`,
      task.dependencies && task.dependencies.length > 0
        ? `Dependencies: ${task.dependencies.join(', ')}`
        : 'Dependencies: None',
      task.error ? `Error: ${task.error}` : null,
    ].filter(Boolean);

    this.tooltip = tooltipParts.join('\n');
    this.contextValue = `taskItem_${task.status}`;

    // Clicking a task can trigger detail inspection or open DAG
    this.command = {
      command: 'agent-studios.showTaskGraph',
      title: 'Show Task Graph',
      arguments: [task.id],
    };
  }

  public static getIconForStatus(status: TaskStatus): vscode.ThemeIcon {
    switch (status) {
      case 'running':
        return new vscode.ThemeIcon('sync~spin');
      case 'succeeded':
        return new vscode.ThemeIcon('check', new vscode.ThemeColor('testing.iconPassed'));
      case 'failed':
        return new vscode.ThemeIcon('error', new vscode.ThemeColor('testing.iconFailed'));
      case 'blocked':
        return new vscode.ThemeIcon(
          'lock',
          new vscode.ThemeColor('problemsWarningIcon.foreground')
        );
      case 'ready':
        return new vscode.ThemeIcon('circle-filled');
      case 'cancelled':
        return new vscode.ThemeIcon('circle-slash');
      case 'pending':
      default:
        return new vscode.ThemeIcon('circle-outline');
    }
  }
}

export class TasksTreeProvider
  implements vscode.TreeDataProvider<TaskTreeItem>, vscode.Disposable
{
  private _projection?: SessionStateProjection;
  private readonly _disposables: vscode.Disposable[] = [];

  private readonly _onDidChangeTreeData = new vscode.EventEmitter<
    TaskTreeItem | undefined | void
  >();
  public readonly onDidChangeTreeData: vscode.Event<TaskTreeItem | undefined | void> =
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
        this._projection.onTasksChanged(() => this.refresh())
      );
    }
  }

  public refresh(): void {
    this._onDidChangeTreeData.fire();
  }

  public getTreeItem(element: TaskTreeItem): vscode.TreeItem {
    return element;
  }

  public getChildren(element?: TaskTreeItem): vscode.ProviderResult<TaskTreeItem[]> {
    const tasks = this._projection?.tasks ?? [];

    if (tasks.length === 0) {
      if (element) {
        return [];
      }
      const emptyItem = new TaskTreeItem(
        'No tasks found',
        vscode.TreeItemCollapsibleState.None
      );
      emptyItem.iconPath = new vscode.ThemeIcon('checklist');
      emptyItem.description = 'Waiting for execution run';
      emptyItem.tooltip = 'No tasks have been scheduled or projected yet.';
      emptyItem.contextValue = 'taskEmpty';
      return [emptyItem];
    }

    // Children of a task
    if (element) {
      if (!element.task || !element.task.subtasks || element.task.subtasks.length === 0) {
        return [];
      }
      const subtaskIds = new Set(element.task.subtasks);
      const childNodes = tasks.filter((t) => subtaskIds.has(t.id));
      return childNodes.map((child) => this.createTaskTreeItem(child, tasks));
    }

    // Root tasks: tasks that are not subtasks of any other task
    const allSubtaskIds = new Set<string>();
    for (const t of tasks) {
      if (t.subtasks) {
        for (const subId of t.subtasks) {
          allSubtaskIds.add(subId);
        }
      }
    }

    const rootTasks = tasks.filter((t) => !allSubtaskIds.has(t.id));
    const itemsToDisplay = rootTasks.length > 0 ? rootTasks : tasks;

    return itemsToDisplay.map((t) => this.createTaskTreeItem(t, tasks));
  }

  private createTaskTreeItem(task: TaskNode, allTasks: TaskNode[]): TaskTreeItem {
    const hasSubtasks =
      task.subtasks &&
      task.subtasks.length > 0 &&
      task.subtasks.some((id) => allTasks.some((t) => t.id === id));

    const collapsibleState = hasSubtasks
      ? vscode.TreeItemCollapsibleState.Expanded
      : vscode.TreeItemCollapsibleState.None;

    return new TaskTreeItem(task, collapsibleState);
  }

  public dispose(): void {
    while (this._disposables.length > 0) {
      const d = this._disposables.pop();
      d?.dispose();
    }
  }
}
