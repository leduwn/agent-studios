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

    it('DisconnectedBridge fails closed on sendMessage', async () => {
      await assert.rejects(
        async () => {
          await bridge.sendMessage(
            { message: 'Hello', mode: 'ask' },
            () => {}
          );
        },
        (err: unknown) => {
          assert.ok(err instanceof BridgeDisconnectedError);
          assert.strictEqual((err as BridgeDisconnectedError).applied, false);
          return true;
        }
      );
    });

    it('DisconnectedBridge cancelCurrentRun is a safe no-op', async () => {
      await assert.doesNotReject(async () => {
        await bridge.cancelCurrentRun();
      });
    });

    it('DisconnectedBridge returns empty array for listSessions', async () => {
      const sessions = await bridge.listSessions();
      assert.deepStrictEqual(sessions, []);
    });

    it('DisconnectedBridge fails closed on createSession', async () => {
      await assert.rejects(
        async () => {
          await bridge.createSession('Test goal');
        },
        (err: unknown) => {
          assert.ok(err instanceof BridgeDisconnectedError);
          assert.strictEqual((err as BridgeDisconnectedError).applied, false);
          return true;
        }
      );
    });

    it('DisconnectedBridge fails closed on switchSession', async () => {
      await assert.rejects(
        async () => {
          await bridge.switchSession('session-1');
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

    it('FixtureBridge listSessions returns 3 realistic mock sessions', async () => {
      const sessions = await bridge.listSessions();
      assert.strictEqual(sessions.length, 3);
      assert.strictEqual(sessions[0].id, 'session-m14-fixture');
      assert.strictEqual(sessions[0].status, 'running');
      assert.strictEqual(sessions[1].id, 'session-m13-refactor');
      assert.strictEqual(sessions[1].status, 'completed');
      assert.strictEqual(sessions[2].id, 'session-m12-dag-layout');
      assert.strictEqual(sessions[2].status, 'completed');
    });

    it('FixtureBridge createSession creates new mock session and fires event', async () => {
      const receivedEvents: TimelineEvent[] = [];
      const sub = bridge.onEvent((e) => receivedEvents.push(e));

      const newSession = await bridge.createSession('Build new feature');
      assert.ok(newSession.id.startsWith('session-preview-'));
      assert.strictEqual(newSession.goalSummary, 'Build new feature');
      assert.strictEqual(newSession.status, 'idle');
      assert.strictEqual(newSession.isPreview, true);

      const current = await bridge.getCurrentSession();
      assert.strictEqual(current?.id, newSession.id);

      const sessions = await bridge.listSessions();
      assert.strictEqual(sessions.length, 4);
      assert.strictEqual(sessions[0].id, newSession.id);

      assert.strictEqual(receivedEvents.length, 1);
      assert.strictEqual(receivedEvents[0].type, 'session_started');

      sub.dispose();
    });

    it('FixtureBridge switchSession switches active session', async () => {
      const switched = await bridge.switchSession('session-m13-refactor');
      assert.strictEqual(switched.id, 'session-m13-refactor');
      assert.strictEqual(switched.goalSummary, 'Refactor bridge interfaces and add state projection unit tests');

      const current = await bridge.getCurrentSession();
      assert.strictEqual(current?.id, 'session-m13-refactor');
    });

    it('FixtureBridge switchSession throws on unknown session id', async () => {
      await assert.rejects(
        async () => {
          await bridge.switchSession('non-existent-session');
        },
        /not found/
      );
    });

    it('FixtureBridge sendMessage in ask mode streams text_delta and tool_call', async () => {
      const chunks: unknown[] = [];
      const msg = await bridge.sendMessage(
        {
          message: 'Explain architecture',
          mode: 'ask',
          attachments: [
            {
              type: 'file',
              uri: 'file:///src/bridge/types.ts',
              relativePath: 'src/bridge/types.ts',
            },
          ],
        },
        (chunk) => chunks.push(chunk)
      );

      assert.strictEqual(msg.role, 'assistant');
      assert.strictEqual(msg.mode, 'ask');
      assert.ok(msg.content.includes('architecture'));
      assert.ok(msg.toolCalls && msg.toolCalls.length > 0);
      assert.strictEqual(msg.toolCalls[0].name, 'read_file');

      // Verify streamed chunks
      assert.ok(chunks.some((c: any) => c.type === 'tool_call'));
      assert.ok(chunks.some((c: any) => c.type === 'text_delta'));
      assert.ok(chunks.some((c: any) => c.type === 'done'));

      // Check in-memory history
      assert.strictEqual(bridge.chatMessages.length, 3); // initial + user + assistant
    });

    it('FixtureBridge sendMessage in plan mode streams task_progress and plan steps', async () => {
      const chunks: unknown[] = [];
      const msg = await bridge.sendMessage(
        {
          message: 'Plan M14 features',
          mode: 'plan',
        },
        (chunk) => chunks.push(chunk)
      );

      assert.strictEqual(msg.role, 'assistant');
      assert.strictEqual(msg.mode, 'plan');
      assert.ok(msg.content.includes('Proposed Execution Plan'));

      assert.ok(chunks.some((c: any) => c.type === 'task_progress'));
      assert.ok(chunks.some((c: any) => c.type === 'tool_call'));
      assert.ok(chunks.some((c: any) => c.type === 'text_delta'));
      assert.ok(chunks.some((c: any) => c.type === 'done'));
    });

    it('FixtureBridge sendMessage in agent mode executes tools and generates approval request', async () => {
      const chunks: unknown[] = [];
      const msg = await bridge.sendMessage(
        {
          message: 'Execute build and apply patch',
          mode: 'agent',
        },
        (chunk) => chunks.push(chunk)
      );

      assert.strictEqual(msg.role, 'assistant');
      assert.strictEqual(msg.mode, 'agent');
      assert.ok(msg.approvalId !== undefined);

      assert.ok(chunks.some((c: any) => c.type === 'approval_required'));
      assert.ok(chunks.some((c: any) => c.type === 'tool_call'));
      assert.ok(chunks.some((c: any) => c.type === 'done'));

      // Approval should be in pending approvals
      const pending = await bridge.listPendingApprovals();
      assert.ok(pending.some((a) => a.id === msg.approvalId));
    });

    it('FixtureBridge cancelCurrentRun aborts active message execution', async () => {
      const chunks: unknown[] = [];
      // Cancel immediately before or during
      const sendPromise = bridge.sendMessage(
        {
          message: 'Long running task',
          mode: 'ask',
        },
        (chunk) => {
          chunks.push(chunk);
          bridge.cancelCurrentRun();
        }
      );

      const msg = await sendPromise;
      assert.ok(msg.content.includes('[Execution cancelled by user]'));
      assert.ok(chunks.some((c: any) => c.type === 'error'));
    });
  });
});
