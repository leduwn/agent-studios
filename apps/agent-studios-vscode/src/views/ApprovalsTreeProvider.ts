/**
 * Agent Studios — Approvals TreeDataProvider
 * Milestone M14-UI-00: Code-OSS Native Agents Workbench
 */

import * as vscode from 'vscode';
import { AgentStudiosBridge } from '../bridge/AgentStudiosBridge';
import { BridgeManager } from '../bridge/BridgeManager';
import { ApprovalRequest } from '../bridge/types';
import { SessionStateProjection } from '../state/SessionStateProjection';

export class ApprovalTreeItem extends vscode.TreeItem {
  public readonly approval?: ApprovalRequest;

  constructor(
    approvalOrLabel: ApprovalRequest | string,
    collapsibleState: vscode.TreeItemCollapsibleState = vscode.TreeItemCollapsibleState.None
  ) {
    if (typeof approvalOrLabel === 'string') {
      super(approvalOrLabel, collapsibleState);
      return;
    }

    const approval = approvalOrLabel;
    super(approval.commandOrTarget, collapsibleState);
    this.approval = approval;
    this.id = approval.id;

    // contextValue = 'approvalItemPending' (enables inline Approve/Deny buttons from package.json menus!)
    this.contextValue = 'approvalItemPending';

    // Description: [${riskLevel.toUpperCase()}] ${actionType}
    this.description = `[${approval.riskLevel.toUpperCase()}] ${approval.actionType}`;

    // Tooltip: full details, requested permissions, session/task ID, expiration
    const tooltipParts = [
      `Approval Request: ${approval.id}`,
      `Action: ${approval.actionType}`,
      `Target: ${approval.commandOrTarget}`,
      `Risk Level: ${approval.riskLevel.toUpperCase()}`,
      `Requested Permissions: ${
        approval.requestedPermissions.length > 0
          ? approval.requestedPermissions.join(', ')
          : 'None'
      }`,
      `Session ID: ${approval.sessionId}`,
      approval.taskTitle
        ? `Task: ${approval.taskTitle}`
        : approval.taskId
          ? `Task ID: ${approval.taskId}`
          : null,
      `Status: ${approval.status}`,
      `Created: ${new Date(approval.createdAt).toISOString()}`,
      approval.expiresAt
        ? `Expires: ${new Date(approval.expiresAt).toISOString()}`
        : 'Expires: Never',
    ].filter(Boolean);

    this.tooltip = tooltipParts.join('\n');

    // Icon: new vscode.ThemeIcon('shield') or warning icon for high/critical risk
    if (approval.riskLevel === 'critical' || approval.riskLevel === 'high') {
      this.iconPath = new vscode.ThemeIcon(
        'warning',
        new vscode.ThemeColor('problemsWarningIcon.foreground')
      );
    } else {
      this.iconPath = new vscode.ThemeIcon('shield');
    }

    // Default click command to show approvals inbox
    this.command = {
      command: 'agent-studios.showApprovals',
      title: 'Show Approvals Inbox',
      arguments: [approval.id],
    };
  }
}

export class ApprovalsTreeProvider
  implements vscode.TreeDataProvider<ApprovalTreeItem>, vscode.Disposable
{
  private _projection?: SessionStateProjection;
  private readonly _disposables: vscode.Disposable[] = [];

  private readonly _onDidChangeTreeData = new vscode.EventEmitter<
    ApprovalTreeItem | undefined | void
  >();
  public readonly onDidChangeTreeData: vscode.Event<ApprovalTreeItem | undefined | void> =
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
        this._projection.onApprovalsChanged(() => this.refresh())
      );
    }
  }

  public refresh(): void {
    this._onDidChangeTreeData.fire();
  }

  public getTreeItem(element: ApprovalTreeItem): vscode.TreeItem {
    return element;
  }

  public getChildren(element?: ApprovalTreeItem): vscode.ProviderResult<ApprovalTreeItem[]> {
    if (element) {
      return [];
    }

    const pending = this._projection?.pendingApprovals ?? [];

    if (pending.length === 0) {
      const placeholder = new ApprovalTreeItem(
        'No pending approvals',
        vscode.TreeItemCollapsibleState.None
      );
      placeholder.iconPath = new vscode.ThemeIcon(
        'check',
        new vscode.ThemeColor('testing.iconPassed')
      );
      placeholder.description = 'All clear';
      placeholder.tooltip = 'There are no pending actions waiting for human approval.';
      placeholder.contextValue = 'noApprovals';
      return [placeholder];
    }

    return pending.map((item) => new ApprovalTreeItem(item));
  }

  public dispose(): void {
    while (this._disposables.length > 0) {
      const d = this._disposables.pop();
      d?.dispose();
    }
  }
}
