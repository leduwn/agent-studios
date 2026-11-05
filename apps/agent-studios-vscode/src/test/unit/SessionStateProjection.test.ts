import * as assert from 'assert';
import { FixtureBridge } from '../../bridge/FixtureBridge';
import { TimelineEvent } from '../../bridge/types';
import { SequenceGapDetails, SessionStateProjection } from '../../state/SessionStateProjection';

describe('SessionStateProjection Pure Unit Tests', () => {
  let bridge: FixtureBridge;
  let projection: SessionStateProjection;

  beforeEach(() => {
    bridge = new FixtureBridge();
    projection = new SessionStateProjection(bridge);
  });

  afterEach(() => {
    projection.dispose();
    bridge.dispose();
  });

  it('bounded event buffer caps at 1000 events and prunes oldest', () => {
    for (let i = 1; i <= 1050; i++) {
      projection.handleEvent({
        sequenceNumber: i,
        id: `evt-${i}`,
        timestamp: 1000 + i,
        type: 'task_started',
        summary: `Event ${i}`,
      });
    }

    assert.strictEqual(projection.timeline.length, 1000);
    // Oldest 50 events pruned (1 to 50), first element should be 51
    assert.strictEqual(projection.timeline[0].sequenceNumber, 51);
    assert.strictEqual(projection.timeline[999].sequenceNumber, 1050);
  });

  it('monotonic sequence ordering correctly processes events', () => {
    const receivedSessionChanges: number[] = [];
    projection.onTasksChanged((tasks) => {
      receivedSessionChanges.push(tasks.length);
    });

    const evt1: TimelineEvent = {
      sequenceNumber: 1,
      id: 'e1',
      timestamp: 1000,
      type: 'session_started',
      summary: 'Session started',
    };
    const evt2: TimelineEvent = {
      sequenceNumber: 2,
      id: 'e2',
      timestamp: 2000,
      type: 'run_started',
      summary: 'Run started',
    };
    const evt3: TimelineEvent = {
      sequenceNumber: 3,
      id: 'e3',
      timestamp: 3000,
      type: 'task_started',
      summary: 'Task 1 started',
      taskId: 'task-1',
    };

    projection.handleEvent(evt1);
    assert.strictEqual(projection.lastProcessedSequence, 1);
    assert.strictEqual(projection.healthState, 'nominal');

    projection.handleEvent(evt2);
    assert.strictEqual(projection.lastProcessedSequence, 2);
    assert.strictEqual(projection.healthState, 'nominal');

    projection.handleEvent(evt3);
    assert.strictEqual(projection.lastProcessedSequence, 3);
    assert.strictEqual(projection.healthState, 'nominal');

    assert.strictEqual(projection.timeline.length, 3);
    assert.strictEqual(projection.timeline[0].sequenceNumber, 1);
    assert.strictEqual(projection.timeline[1].sequenceNumber, 2);
    assert.strictEqual(projection.timeline[2].sequenceNumber, 3);
  });

  it('duplicate sequence numbers are ignored without side effects', () => {
    const event1: TimelineEvent = {
      sequenceNumber: 1,
      id: 'e1',
      timestamp: 1000,
      type: 'session_started',
      summary: 'Session 1',
    };
    const event2: TimelineEvent = {
      sequenceNumber: 2,
      id: 'e2',
      timestamp: 2000,
      type: 'run_started',
      summary: 'Run 1',
    };
    const duplicateEvent2: TimelineEvent = {
      sequenceNumber: 2,
      id: 'e2-dup',
      timestamp: 2050,
      type: 'run_started',
      summary: 'Run 1 duplicate',
    };
    const staleEvent1: TimelineEvent = {
      sequenceNumber: 1,
      id: 'e1-stale',
      timestamp: 2100,
      type: 'session_started',
      summary: 'Stale 1',
    };

    projection.handleEvent(event1);
    projection.handleEvent(event2);
    assert.strictEqual(projection.timeline.length, 2);
    assert.strictEqual(projection.lastProcessedSequence, 2);

    // Feed duplicate and stale
    projection.handleEvent(duplicateEvent2);
    projection.handleEvent(staleEvent1);

    // Timeline length and sequence remain completely unchanged
    assert.strictEqual(projection.timeline.length, 2);
    assert.strictEqual(projection.lastProcessedSequence, 2);
    assert.strictEqual(projection.timeline[1].id, 'e2');
  });

  it('sequence number gaps trigger degraded state', () => {
    const gaps: SequenceGapDetails[] = [];
    const sub = projection.onStateDegraded((g) => gaps.push(g));

    const event1: TimelineEvent = {
      sequenceNumber: 1,
      id: 'e1',
      timestamp: 1000,
      type: 'session_started',
      summary: 'Session 1',
    };
    const event2: TimelineEvent = {
      sequenceNumber: 2,
      id: 'e2',
      timestamp: 2000,
      type: 'run_started',
      summary: 'Run 1',
    };
    const event3: TimelineEvent = {
      sequenceNumber: 3,
      id: 'e3',
      timestamp: 3000,
      type: 'task_started',
      summary: 'Task 3',
    };
    // Gap: sequence 5 arrives after sequence 3 (sequence 4 missing)
    const event5: TimelineEvent = {
      sequenceNumber: 5,
      id: 'e5',
      timestamp: 5000,
      type: 'task_completed',
      summary: 'Task 5',
    };

    projection.handleEvent(event1);
    projection.handleEvent(event2);
    projection.handleEvent(event3);

    assert.strictEqual(projection.healthState, 'nominal');
    assert.strictEqual(projection.isDegraded, false);
    assert.strictEqual(gaps.length, 0);

    projection.handleEvent(event5);

    assert.strictEqual(projection.healthState, 'degraded');
    assert.strictEqual(projection.isDegraded, true);
    assert.strictEqual(gaps.length, 1);
    assert.strictEqual(gaps[0].expected, 4);
    assert.strictEqual(gaps[0].received, 5);

    projection.resetHealthState();
    assert.strictEqual(projection.healthState, 'nominal');
    assert.strictEqual(projection.isDegraded, false);

    sub.dispose();
  });

  it('disposal unbinds all listeners and cleans state', async () => {
    await projection.initializeFromBridge();
    assert.ok(projection.session !== null);
    assert.ok(projection.tasks.length > 0);

    let eventFiredAfterDisposal = false;
    projection.onTimelineChanged(() => {
      eventFiredAfterDisposal = true;
    });

    // Call dispose
    projection.dispose();

    // Verify state cleanup
    assert.strictEqual(projection.session, null);
    assert.deepStrictEqual(projection.tasks, []);
    assert.deepStrictEqual(projection.approvals, []);
    assert.deepStrictEqual(projection.artifacts, []);
    assert.deepStrictEqual(projection.timeline, []);

    // Emitting bridge events should no longer affect disposed projection
    bridge.emitEvent({
      sequenceNumber: 99,
      id: 'evt-post-dispose',
      timestamp: Date.now(),
      type: 'task_started',
      summary: 'Post-dispose task',
    });

    assert.strictEqual(eventFiredAfterDisposal, false);
  });

  it('initializes from bridge snapshot correctly', async () => {
    await projection.initializeFromBridge();

    assert.strictEqual(projection.session?.status, 'running');
    assert.strictEqual(projection.tasks.length, 5);
    assert.strictEqual(projection.pendingApprovals.length, 1);
    assert.strictEqual(projection.artifacts.length, 2);
    assert.strictEqual(projection.healthState, 'nominal');
    assert.strictEqual(projection.isDegraded, false);
  });

  it('updates task completion and session metrics incrementally', async () => {
    await projection.initializeFromBridge();
    assert.strictEqual(projection.session?.completedTasksCount, 3);

    const task4 = projection.tasks.find((t) => t.id === 'task-4');
    assert.strictEqual(task4?.status, 'running');

    // Emit task_completed for task-4
    projection.handleEvent({
      sequenceNumber: 11,
      id: 'evt-11',
      timestamp: 1716000130000,
      type: 'task_completed',
      summary: 'Task 4 finished',
      taskId: 'task-4',
    });

    const updatedTask4 = projection.tasks.find((t) => t.id === 'task-4');
    assert.strictEqual(updatedTask4?.status, 'succeeded');
    assert.strictEqual(updatedTask4?.progress, 100);
    assert.strictEqual(projection.session?.completedTasksCount, 4);
  });

  it('handles bridge events when connected to FixtureBridge', async () => {
    await projection.initializeFromBridge();

    const approvalResult = await bridge.requestApprovalDecision('approval-m14-001', 'approved');
    assert.strictEqual(approvalResult.applied, true);

    const approvedItem = projection.approvals.find((a) => a.id === 'approval-m14-001');
    assert.strictEqual(approvedItem?.status, 'approved');
    assert.strictEqual(projection.pendingApprovals.length, 0);
  });

  it('manages chat messages and fires chat message events', () => {
    const addedMessages: string[] = [];
    const updatedMessages: string[] = [];

    const sub1 = projection.onChatMessageAdded((m) => addedMessages.push(m.id));
    const sub2 = projection.onChatMessageUpdated((m) => updatedMessages.push(m.id));

    projection.addChatMessage({
      id: 'msg-1',
      role: 'user',
      content: 'Fix the bug',
      timestamp: Date.now(),
      mode: 'agent',
    });

    assert.strictEqual(projection.chatMessages.length, 1);
    assert.strictEqual(projection.chatMessages[0].content, 'Fix the bug');
    assert.strictEqual(addedMessages.length, 1);
    assert.strictEqual(addedMessages[0], 'msg-1');

    projection.updateChatMessage('msg-1', { content: 'Fix the bug now', isStreaming: true });
    assert.strictEqual(projection.chatMessages[0].content, 'Fix the bug now');
    assert.strictEqual(projection.chatMessages[0].isStreaming, true);
    assert.strictEqual(updatedMessages.length, 1);
    assert.strictEqual(updatedMessages[0], 'msg-1');

    projection.setChatMessages([
      { id: 'm-a', role: 'user', content: 'A', timestamp: 1 },
      { id: 'm-b', role: 'assistant', content: 'B', timestamp: 2 },
    ]);
    assert.strictEqual(projection.chatMessages.length, 2);

    projection.clearChatMessages();
    assert.strictEqual(projection.chatMessages.length, 0);

    sub1.dispose();
    sub2.dispose();
  });

  it('tracks active session and fires onSessionSwitched and onSessionChanged', async () => {
    await projection.initializeFromBridge();
    assert.strictEqual(projection.activeSessionId, 'session-m14-fixture');

    const switchedSessions: (string | undefined)[] = [];
    const changedSessions: (string | undefined)[] = [];

    const sub1 = projection.onSessionSwitched((s) => switchedSessions.push(s?.id));
    const sub2 = projection.onSessionChanged((s) => changedSessions.push(s?.id));

    const newMockSession = {
      id: 'session-switched-test',
      workspacePath: '/test',
      goalSummary: 'Test goal',
      status: 'idle' as const,
      activeAgentCount: 0,
      totalAgentCount: 0,
      completedTasksCount: 0,
      totalTasksCount: 0,
      startedAt: Date.now(),
      elapsedMs: 0,
    };

    projection.switchSession(newMockSession);

    assert.strictEqual(projection.activeSessionId, 'session-switched-test');
    assert.strictEqual(projection.session?.id, 'session-switched-test');
    assert.strictEqual(switchedSessions.length, 1);
    assert.strictEqual(switchedSessions[0], 'session-switched-test');
    assert.strictEqual(changedSessions.length, 1);
    assert.strictEqual(changedSessions[0], 'session-switched-test');

    sub1.dispose();
    sub2.dispose();
  });

  it('cleans up chat messages and activeSessionId on disposal', async () => {
    await projection.initializeFromBridge();
    assert.ok(projection.activeSessionId !== null);
    assert.ok(projection.chatMessages.length > 0);

    projection.dispose();

    assert.strictEqual(projection.activeSessionId, null);
    assert.strictEqual(projection.chatMessages.length, 0);
  });
});
