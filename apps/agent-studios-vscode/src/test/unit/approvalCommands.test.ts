import * as assert from 'assert';
import * as vscode from 'vscode';
import { DisconnectedBridge } from '../../bridge/DisconnectedBridge';
import { FixtureBridge } from '../../bridge/FixtureBridge';
import { registerApprovalCommands } from '../../commands/ApprovalCommands';
import { SessionStateProjection } from '../../state/SessionStateProjection';
import {
  executedCommands,
  mockWindowMessages,
  resetMockState,
} from './vscodeMock';

describe('ApprovalCommands Tests', () => {
  describe('Connected FixtureBridge', () => {
    let bridge: FixtureBridge;
    let projection: SessionStateProjection;
    let context: vscode.ExtensionContext;
    let disposable: vscode.Disposable;

    beforeEach(async () => {
      resetMockState();
      bridge = new FixtureBridge();
      projection = new SessionStateProjection(bridge);
      await projection.initializeFromBridge();

      const subscriptions: vscode.Disposable[] = [];
      context = { subscriptions } as unknown as vscode.ExtensionContext;

      disposable = registerApprovalCommands(context, bridge, projection);
    });

    afterEach(() => {
      disposable.dispose();
      projection.dispose();
      bridge.dispose();
      resetMockState();
    });

    it('approves medium risk action directly without modal warning', async () => {
      const pending = projection.pendingApprovals;
      assert.strictEqual(pending.length, 1);
      const approval = pending[0];
      assert.strictEqual(approval.riskLevel, 'medium');

      await vscode.commands.executeCommand('agent-studios.approveAction', approval);

      assert.strictEqual(mockWindowMessages.warning.length, 0);
      assert.ok(mockWindowMessages.info.some(msg => msg.includes('Approved action')));
      assert.strictEqual(projection.pendingApprovals.length, 0);
    });

    it('prompts modal warning for high risk action and approves on confirmation', async () => {
      // Create high-risk approval
      const highRiskApproval = {
        id: 'approval-high-risk',
        sessionId: 'session-m14-fixture',
        actionType: 'exec_command' as const,
        commandOrTarget: 'rm -rf /critical/path',
        riskLevel: 'high' as const,
        requestedPermissions: ['shell:exec'],
        status: 'pending' as const,
        createdAt: Date.now(),
      };
      (bridge as unknown as { _approvals: unknown[] })._approvals.push(highRiskApproval);

      // Mock user confirms approval
      mockWindowMessages.modalWarningResponse = 'Confirm Approve';

      await vscode.commands.executeCommand('agent-studios.approveAction', highRiskApproval);

      assert.ok(mockWindowMessages.warning.some(msg => msg.includes('Approve high-risk action')));
      assert.ok(mockWindowMessages.info.some(msg => msg.includes('Approved action')));
    });

    it('aborts high risk approval when user cancels modal confirmation', async () => {
      const highRiskApproval = {
        id: 'approval-high-risk-2',
        sessionId: 'session-m14-fixture',
        actionType: 'exec_command' as const,
        commandOrTarget: 'drop database',
        riskLevel: 'critical' as const,
        requestedPermissions: ['shell:exec'],
        status: 'pending' as const,
        createdAt: Date.now(),
      };
      (bridge as unknown as { _approvals: unknown[] })._approvals.push(highRiskApproval);

      // User cancels
      mockWindowMessages.modalWarningResponse = 'Cancel';

      await vscode.commands.executeCommand('agent-studios.approveAction', highRiskApproval);

      assert.ok(mockWindowMessages.warning.some(msg => msg.includes('Approve high-risk action')));
      // No info message should be shown indicating approval
      assert.ok(!mockWindowMessages.info.some(msg => msg.includes('Approved action')));
    });

    it('denies action and updates projection state', async () => {
      const pending = projection.pendingApprovals;
      assert.strictEqual(pending.length, 1);
      const approval = pending[0];

      await vscode.commands.executeCommand('agent-studios.denyAction', approval);

      assert.ok(mockWindowMessages.info.some(msg => msg.includes('Denied action')));
      assert.strictEqual(projection.pendingApprovals.length, 0);
    });

    it('focuses approvals sidebar view via agent-studios.showApprovals', async () => {
      await vscode.commands.executeCommand('agent-studios.showApprovals');

      const focusCall = executedCommands.find(c => c.command === 'agent-studios.approvals.focus');
      assert.ok(focusCall !== undefined, 'agent-studios.approvals.focus should have been called');
    });
  });

  describe('Disconnected Bridge (Fail Closed)', () => {
    let bridge: DisconnectedBridge;
    let projection: SessionStateProjection;
    let context: vscode.ExtensionContext;
    let disposable: vscode.Disposable;

    beforeEach(() => {
      resetMockState();
      bridge = new DisconnectedBridge();
      projection = new SessionStateProjection(bridge);

      const subscriptions: vscode.Disposable[] = [];
      context = { subscriptions } as unknown as vscode.ExtensionContext;

      disposable = registerApprovalCommands(context, bridge, projection);
    });

    afterEach(() => {
      disposable.dispose();
      projection.dispose();
      bridge.dispose();
      resetMockState();
    });

    it('fails closed when approving while disconnected', async () => {
      const approval = {
        id: 'approval-disc-1',
        sessionId: 'session-disc',
        actionType: 'exec_command' as const,
        commandOrTarget: 'npm test',
        riskLevel: 'low' as const,
        requestedPermissions: [],
        status: 'pending' as const,
        createdAt: Date.now(),
      };

      await vscode.commands.executeCommand('agent-studios.approveAction', approval);

      assert.ok(
        mockWindowMessages.error.some(msg =>
          msg.includes(
            'Cannot approve action: Agent Studios daemon is disconnected. Approvals fail closed.'
          )
        )
      );
    });

    it('fails closed when denying while disconnected', async () => {
      const approval = {
        id: 'approval-disc-2',
        sessionId: 'session-disc',
        actionType: 'exec_command' as const,
        commandOrTarget: 'npm test',
        riskLevel: 'low' as const,
        requestedPermissions: [],
        status: 'pending' as const,
        createdAt: Date.now(),
      };

      await vscode.commands.executeCommand('agent-studios.denyAction', approval);

      assert.ok(
        mockWindowMessages.error.some(msg =>
          msg.includes(
            'Cannot deny action: Agent Studios daemon is disconnected. Approvals fail closed.'
          )
        )
      );
    });
  });
});
