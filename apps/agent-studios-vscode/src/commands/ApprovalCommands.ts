/**
 * Agent Studios — Approval Commands
 * Milestone M14-UI-00: Code-OSS Native Agents Workbench
 */

import * as vscode from 'vscode';
import { AgentStudiosBridge } from '../bridge/AgentStudiosBridge';
import { ApprovalRequest } from '../bridge/types';
import { SessionStateProjection } from '../state/SessionStateProjection';

/**
 * Resolves an ApprovalRequest from command arguments or prompts the user via QuickPick.
 */
async function resolveApproval(
  arg: unknown,
  bridge: AgentStudiosBridge,
  stateProjection: SessionStateProjection,
  actionLabel: string
): Promise<ApprovalRequest | undefined> {
  // 1. Direct approval object
  if (arg && typeof arg === 'object') {
    if ('id' in arg && 'riskLevel' in arg && 'commandOrTarget' in arg) {
      return arg as ApprovalRequest;
    }
    if ('approval' in arg && typeof (arg as { approval: unknown }).approval === 'object') {
      return (arg as { approval: ApprovalRequest }).approval;
    }
    if ('approvalRequest' in arg && typeof (arg as { approvalRequest: unknown }).approvalRequest === 'object') {
      return (arg as { approvalRequest: ApprovalRequest }).approvalRequest;
    }
    if ('id' in arg && typeof (arg as { id: unknown }).id === 'string') {
      const id = (arg as { id: string }).id;
      const found = stateProjection.approvals.find(a => a.id === id);
      if (found) {
        return found;
      }
    }
  }

  // 2. String ID passed
  if (typeof arg === 'string') {
    const found = stateProjection.approvals.find(a => a.id === arg);
    if (found) {
      return found;
    }
    const pending = await bridge.listPendingApprovals();
    const fromBridge = pending.find(a => a.id === arg);
    if (fromBridge) {
      return fromBridge;
    }
    vscode.window.showErrorMessage(`Approval request '${arg}' not found.`);
    return undefined;
  }

  // 3. Prompt user from pending approvals
  const pending = stateProjection.pendingApprovals.length > 0
    ? stateProjection.pendingApprovals
    : await bridge.listPendingApprovals();

  if (pending.length === 0) {
    vscode.window.showInformationMessage('No pending approvals requiring decision.');
    return undefined;
  }

  const pick = await vscode.window.showQuickPick(
    pending.map(a => ({
      label: `[${a.riskLevel.toUpperCase()}] ${a.commandOrTarget || a.actionType}`,
      description: a.taskTitle ? `Task: ${a.taskTitle}` : a.id,
      detail: a.requestedPermissions?.length
        ? `Permissions: ${a.requestedPermissions.join(', ')}`
        : undefined,
      approval: a,
    })),
    {
      placeHolder: `Select pending action to ${actionLabel}`,
    }
  );

  return pick?.approval;
}

function isBridgeDisconnected(bridge: AgentStudiosBridge): boolean {
  return (bridge.connectionState as string) === 'disconnected';
}

/**
 * Registers commands for human-in-the-loop approvals and permission delegation.
 */
export function registerApprovalCommands(
  context: vscode.ExtensionContext,
  bridge: AgentStudiosBridge,
  stateProjection: SessionStateProjection
): vscode.Disposable {
  const disposables: vscode.Disposable[] = [];

  // Command: agent-studios.approveAction
  const approveCommand = vscode.commands.registerCommand(
    'agent-studios.approveAction',
    async (arg?: unknown) => {
      // Security safeguard: fail closed if disconnected
      if (isBridgeDisconnected(bridge)) {
        vscode.window.showErrorMessage(
          'Cannot approve action: Agent Studios daemon is disconnected. Approvals fail closed.'
        );
        return;
      }

      const approval = await resolveApproval(arg, bridge, stateProjection, 'approve');
      if (!approval) {
        return;
      }

      // Re-verify connection state after user interaction
      if (isBridgeDisconnected(bridge)) {
        vscode.window.showErrorMessage(
          'Cannot approve action: Agent Studios daemon is disconnected. Approvals fail closed.'
        );
        return;
      }

      // Security safeguard: high-risk and critical actions require explicit modal confirmation
      if (approval.riskLevel === 'critical' || approval.riskLevel === 'high') {
        const actionTarget = approval.commandOrTarget || approval.actionType;
        const confirmChoice = await vscode.window.showWarningMessage(
          `Approve high-risk action: ${actionTarget} ?`,
          { modal: true },
          'Confirm Approve',
          'Cancel'
        );

        if (confirmChoice !== 'Confirm Approve') {
          return;
        }
      }

      try {
        const result = await bridge.requestApprovalDecision(approval.id, 'approved');
        if (result.applied) {
          await stateProjection.initializeFromBridge();
          vscode.window.showInformationMessage(
            `Approved action: ${approval.commandOrTarget || approval.id}`
          );
        } else {
          vscode.window.showErrorMessage(
            result.message || `Failed to approve action '${approval.id}'.`
          );
        }
      } catch (err: unknown) {
        vscode.window.showErrorMessage(
          `Cannot approve action: ${err instanceof Error ? err.message : String(err)}`
        );
      }
    }
  );

  // Command: agent-studios.denyAction
  const denyCommand = vscode.commands.registerCommand(
    'agent-studios.denyAction',
    async (arg?: unknown) => {
      // Security safeguard: fail closed if disconnected
      if (isBridgeDisconnected(bridge)) {
        vscode.window.showErrorMessage(
          'Cannot deny action: Agent Studios daemon is disconnected. Approvals fail closed.'
        );
        return;
      }

      const approval = await resolveApproval(arg, bridge, stateProjection, 'deny');
      if (!approval) {
        return;
      }

      if (isBridgeDisconnected(bridge)) {
        vscode.window.showErrorMessage(
          'Cannot deny action: Agent Studios daemon is disconnected. Approvals fail closed.'
        );
        return;
      }

      try {
        const result = await bridge.requestApprovalDecision(approval.id, 'denied');
        if (result.applied) {
          await stateProjection.initializeFromBridge();
          vscode.window.showInformationMessage(
            `Denied action: ${approval.commandOrTarget || approval.id}`
          );
        } else {
          vscode.window.showErrorMessage(
            result.message || `Failed to deny action '${approval.id}'.`
          );
        }
      } catch (err: unknown) {
        vscode.window.showErrorMessage(
          `Cannot deny action: ${err instanceof Error ? err.message : String(err)}`
        );
      }
    }
  );

  disposables.push(approveCommand, denyCommand);

  // Command: agent-studios.showApprovals (safe check if already registered by CommandsRegistry)
  try {
    const showApprovalsCommand = vscode.commands.registerCommand(
      'agent-studios.showApprovals',
      async () => {
        await vscode.commands.executeCommand('agent-studios.approvals.focus');
      }
    );
    disposables.push(showApprovalsCommand);
  } catch {
    // Already registered in CommandsRegistry
  }

  const compositeDisposable = vscode.Disposable.from(...disposables);
  context.subscriptions.push(compositeDisposable);
  return compositeDisposable;
}

// eslint-disable-next-line @typescript-eslint/naming-convention
export const ApprovalCommands = {
  register: registerApprovalCommands,
};
