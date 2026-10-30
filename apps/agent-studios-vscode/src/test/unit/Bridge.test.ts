import * as assert from 'assert';
import { BridgeDisconnectedError } from '../../bridge/AgentStudiosBridge';
import { DisconnectedBridge } from '../../bridge/DisconnectedBridge';
import { FixtureBridge } from '../../bridge/FixtureBridge';
import { TimelineEvent } from '../../bridge/types';

describe('Bridge Pure Unit Tests', () => {
  describe('DisconnectedBridge', () => {
    let bridge: DisconnectedBridge;

    beforeEach(() => {
      bridge = new DisconnectedBridge();
    });

    afterEach(() => {
      bridge.dispose();
    });

    it('DisconnectedBridge starts disconnected', () => {
      assert.strictEqual(bridge.connectionState, 'disconnected');
    });

    it('DisconnectedBridge fails closed on approval decision (throws BridgeDisconnectedError or applied: false)', async () => {
      await assert.rejects(
        async () => {
          await bridge.requestApprovalDecision('approval-test', 'approved');
        },
        (err: unknown) => {
          assert.ok(err instanceof BridgeDisconnectedError);
          assert.strictEqual((err as BridgeDisconnectedError).applied, false);
          return true;
        }
      );
    });

    it('DisconnectedBridge fails closed on editor action', async () => {
      await assert.rejects(
        async () => {
          await bridge.submitEditorAction({
            action: 'explain',
            documentUri: 'file:///test.ts',
            languageId: 'typescript',
            selectedText: 'const x = 1;',
            selectionRange: { startLine: 1, startCol: 1, endLine: 1, endCol: 13 },
          });
        },
        (err: unknown) => {
          assert.ok(err instanceof BridgeDisconnectedError);
          assert.strictEqual((err as BridgeDisconnectedError).applied, false);
          return true;
        }
      );
    });
  });

  describe('FixtureBridge', () => {
    let bridge: FixtureBridge;

    beforeEach(() => {
      bridge = new FixtureBridge();
    });

    afterEach(() => {
      bridge.dispose();
    });

    it('FixtureBridge starts connected in preview mode', async () => {
      assert.strictEqual(bridge.connectionState, 'connected');
      const session = await bridge.getCurrentSession();
      assert.ok(session !== null);
      assert.strictEqual(session?.isPreview, true);
      assert.strictEqual(session?.status, 'running');
    });

    it('FixtureBridge provides valid session, tasks, approvals, artifacts, timeline, providers', async () => {
      const session = await bridge.getCurrentSession();
      assert.ok(session !== null);
      assert.strictEqual(session?.id, 'session-m14-fixture');
      assert.strictEqual(session?.activeAgentCount, 2);

      const tasks = await bridge.listTasks();
      assert.strictEqual(tasks.length, 5);
      assert.strictEqual(tasks[0].id, 'task-1');

      const approvals = await bridge.listPendingApprovals();
      assert.strictEqual(approvals.length, 1);
      assert.strictEqual(approvals[0].id, 'approval-m14-001');

      const artifacts = await bridge.listArtifacts();
      assert.strictEqual(artifacts.length, 2);
      assert.strictEqual(artifacts[0].name, 'task-m14-workbench-diff.patch');

      const timeline = bridge.timeline;
      assert.strictEqual(timeline.length, 10);
      assert.strictEqual(timeline[0].sequenceNumber, 1);

      const providers = await bridge.listProviderSummaries();
      assert.strictEqual(providers.length, 3);
      assert.strictEqual(providers[0].displayName, 'Anthropic Claude 3.5 Sonnet');
    });

    it('FixtureBridge approval decision updates pending approval status and emits event', async () => {
      const pending = await bridge.listPendingApprovals();
      assert.strictEqual(pending.length, 1);
      const approvalId = pending[0].id;

      const receivedEvents: TimelineEvent[] = [];
      const sub = bridge.onEvent((e) => receivedEvents.push(e));

      const result = await bridge.requestApprovalDecision(approvalId, 'approved');
      assert.strictEqual(result.applied, true);
      assert.strictEqual(result.decision, 'approved');

      const after = await bridge.listPendingApprovals();
      assert.strictEqual(after.length, 0);

      assert.strictEqual(receivedEvents.length, 1);
      assert.strictEqual(receivedEvents[0].type, 'approval_decided');
      assert.strictEqual(receivedEvents[0].sequenceNumber, 11);

      sub.dispose();
    });
  });
});
