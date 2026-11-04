/**
 * Agent Studios — TreeView Providers Unit Tests
 * Milestone M14-UI-00: Code-OSS Native Agents Workbench
 */

import * as assert from 'assert';
import { BridgeManager } from '../../bridge/BridgeManager';
import { DisconnectedBridge } from '../../bridge/DisconnectedBridge';
import { FixtureBridge } from '../../bridge/FixtureBridge';
import { TaskNode } from '../../bridge/types';
import { SessionStateProjection } from '../../state/SessionStateProjection';
import {
  ApprovalsTreeProvider,
  ApprovalTreeItem,
  ArtifactsTreeProvider,
  ArtifactTreeItem,
  ProvidersTreeProvider,
  ProviderTreeItem,
  SessionTreeItem,
  SessionTreeProvider,
  TasksTreeProvider,
  TaskTreeItem,
  TimelineTreeItem,
  TimelineTreeProvider,
} from '../../views';

describe('TreeView Providers Tests', () => {
  let fixtureBridge: FixtureBridge;
  let projection: SessionStateProjection;
  let bridgeManager: BridgeManager;

  beforeEach(async () => {
    fixtureBridge = new FixtureBridge();
    projection = new SessionStateProjection(fixtureBridge);
    await projection.initializeFromBridge();
    projection.seedEvents(fixtureBridge.timeline);
    bridgeManager = new BridgeManager(fixtureBridge);
    await bridgeManager.stateProjection.initializeFromBridge();
    bridgeManager.stateProjection.seedEvents(fixtureBridge.timeline);
  });

  afterEach(() => {
    projection.dispose();
    fixtureBridge.dispose();
    bridgeManager.dispose();
  });

  function itemLabel(item?: { label?: unknown }): string {
    if (!item?.label) {
      return '';
    }
    if (typeof item.label === 'string') {
      return item.label;
    }
    if (typeof item.label === 'object' && 'label' in (item.label as Record<string, unknown>)) {
      return String((item.label as { label: string }).label);
    }
    return String(item.label);
  }

  function itemDesc(item?: { description?: unknown }): string {
    return item?.description ? String(item.description) : '';
  }

  describe('SessionTreeProvider', () => {
    it('renders all required session items with fixture preview tags', async () => {
      const provider = new SessionTreeProvider(bridgeManager);
      const items = (await provider.getChildren()) as SessionTreeItem[];

      assert.ok(items.length >= 7, `Expected at least 7 items, got ${items.length}`);

      // 1. Preview mode indicator
      const previewItem = items.find((i) => itemLabel(i).includes('[MOCK / PREVIEW FIXTURE]'));
      assert.ok(previewItem, 'Should have preview mode item tagged [MOCK / PREVIEW FIXTURE]');
      assert.strictEqual(previewItem?.contextValue, 'sessionPreviewMode');

      // 2. Connection status
      const connItem = items.find((i) => itemLabel(i).startsWith('Status:'));
      assert.ok(connItem, 'Should have connection status item');
      assert.strictEqual(connItem?.contextValue, 'sessionConnected');

      // 3. Workspace
      const wsItem = items.find((i) => itemLabel(i) === 'Workspace');
      assert.ok(wsItem, 'Should have workspace item');
      assert.ok(itemDesc(wsItem).includes('agent-studios-vscode'));

      // 4. Goal summary
      const goalItem = items.find((i) => itemLabel(i) === 'Goal');
      assert.ok(goalItem, 'Should have goal item');
      const goalChildren = (await provider.getChildren(goalItem)) as SessionTreeItem[];
      assert.strictEqual(goalChildren.length, 1);
      assert.ok(itemLabel(goalChildren[0]).includes('Code-OSS native agents workbench'));

      // 5. Active run status and progress
      const runItem = items.find((i) => itemLabel(i).startsWith('Run:'));
      assert.ok(runItem, 'Should have run item');
      assert.ok(itemDesc(runItem).includes('Running: 3/5 tasks completed'));

      // 6. Active agent count and names
      const agentsItem = items.find((i) => itemLabel(i).startsWith('Active Agents:'));
      assert.ok(agentsItem, 'Should have agents item');
      const agentChildren = (await provider.getChildren(agentsItem)) as SessionTreeItem[];
      assert.ok(agentChildren.length >= 2, 'Should list active agent names');
      const agentNames = agentChildren.map((c) => itemLabel(c));
      assert.ok(agentNames.includes('Architect Agent'));
      assert.ok(agentNames.includes('Coder Agent'));

      // 7. Provider / model summary
      const providerItem = items.find((i) => itemLabel(i) === 'Provider / Model');
      assert.ok(providerItem, 'Should have provider/model item');
      assert.ok(itemDesc(providerItem).includes('Anthropic Claude 3.5 Sonnet'));

      // 8. Elapsed time
      const elapsedItem = items.find((i) => itemLabel(i) === 'Elapsed Time');
      assert.ok(elapsedItem, 'Should have elapsed time item');
      assert.ok(itemDesc(elapsedItem).includes('6m 0s'));

      provider.dispose();
    });

    it('renders empty placeholder when session is disconnected/absent', async () => {
      const disconnectedBridge = new DisconnectedBridge();
      const emptyProjection = new SessionStateProjection(disconnectedBridge);
      const provider = new SessionTreeProvider(disconnectedBridge, emptyProjection);

      const items = (await provider.getChildren()) as SessionTreeItem[];
      assert.ok(items.length >= 2);

      const connItem = items.find((i) => itemLabel(i).startsWith('Status: Disconnected'));
      assert.ok(connItem);

      const emptyItem = items.find((i) => itemLabel(i) === 'No Active Session');
      assert.ok(emptyItem);

      provider.dispose();
      emptyProjection.dispose();
      disconnectedBridge.dispose();
    });
  });

  describe('TasksTreeProvider', () => {
    it('renders tasks with custom ThemeIcons, agent badges, and progress', async () => {
      const provider = new TasksTreeProvider(projection);
      const items = (await provider.getChildren()) as TaskTreeItem[];

      assert.strictEqual(items.length, 5, 'Should render 5 root tasks');

      // Task 1: succeeded
      const task1 = items.find((t) => t.task?.id === 'task-1');
      assert.ok(task1);
      assert.strictEqual(task1?.task?.status, 'succeeded');
      assert.strictEqual(task1?.description, '(Architect Agent) 100%');

      // Task 4: running
      const task4 = items.find((t) => t.task?.id === 'task-4');
      assert.ok(task4);
      assert.strictEqual(task4?.task?.status, 'running');
      assert.strictEqual(task4?.description, '(Coder Agent) 60%');
      assert.strictEqual(task4?.command?.command, 'agent-studios.showTaskGraph');

      // Task 5: pending
      const task5 = items.find((t) => t.task?.id === 'task-5');
      assert.ok(task5);
      assert.strictEqual(task5?.task?.status, 'pending');
      assert.strictEqual(task5?.description, '(Coder Agent) 0%');

      provider.dispose();
    });

    it('renders custom ThemeIcon correctly for each task status', () => {
      const runningIcon = TaskTreeItem.getIconForStatus('running');
      assert.strictEqual(runningIcon.id, 'sync~spin');

      const succeededIcon = TaskTreeItem.getIconForStatus('succeeded');
      assert.strictEqual(succeededIcon.id, 'check');

      const failedIcon = TaskTreeItem.getIconForStatus('failed');
      assert.strictEqual(failedIcon.id, 'error');

      const blockedIcon = TaskTreeItem.getIconForStatus('blocked');
      assert.strictEqual(blockedIcon.id, 'lock');

      const pendingIcon = TaskTreeItem.getIconForStatus('pending');
      assert.strictEqual(pendingIcon.id, 'circle-outline');

      const readyIcon = TaskTreeItem.getIconForStatus('ready');
      assert.strictEqual(readyIcon.id, 'circle-filled');
    });

    it('supports hierarchical subtasks presentation', async () => {
      const disconnectedBridge = new DisconnectedBridge();
      const customProjection = new SessionStateProjection(disconnectedBridge);

      const parentTask: TaskNode = {
        id: 'parent-1',
        runId: 'run-1',
        title: 'Parent Task',
        description: 'Parent with 2 subtasks',
        status: 'running',
        dependencies: [],
        subtasks: ['sub-1', 'sub-2'],
        progress: 50,
        createdAt: 1000,
        updatedAt: 1000,
      };
      const subtask1: TaskNode = {
        id: 'sub-1',
        runId: 'run-1',
        title: 'Subtask 1',
        description: 'First subtask',
        status: 'succeeded',
        dependencies: [],
        progress: 100,
        createdAt: 1000,
        updatedAt: 1000,
      };
      const subtask2: TaskNode = {
        id: 'sub-2',
        runId: 'run-1',
        title: 'Subtask 2',
        description: 'Second subtask',
        status: 'running',
        dependencies: ['sub-1'],
        progress: 0,
        createdAt: 1000,
        updatedAt: 1000,
      };

      // Seed tasks into projection via event
      customProjection.handleEvent({
        sequenceNumber: 1,
        id: 'e1',
        timestamp: 1000,
        type: 'session_started',
        summary: 'Start',
      });
      // Directly inject into private tasks via reflection for test
      (customProjection as unknown as { _tasks: TaskNode[] })._tasks = [
        parentTask,
        subtask1,
        subtask2,
      ];

      const provider = new TasksTreeProvider(customProjection);
      const rootItems = (await provider.getChildren()) as TaskTreeItem[];

      // Only parent-1 should be at root because sub-1 and sub-2 are in parent's subtasks
      assert.strictEqual(rootItems.length, 1);
      assert.strictEqual(rootItems[0].task?.id, 'parent-1');

      // Children of parent-1
      const subItems = (await provider.getChildren(rootItems[0])) as TaskTreeItem[];
      assert.strictEqual(subItems.length, 2);
      assert.strictEqual(subItems[0].task?.id, 'sub-1');
      assert.strictEqual(subItems[1].task?.id, 'sub-2');

      provider.dispose();
      customProjection.dispose();
      disconnectedBridge.dispose();
    });

    it('renders empty placeholder when no tasks exist', async () => {
      const disconnectedBridge = new DisconnectedBridge();
      const emptyProjection = new SessionStateProjection(disconnectedBridge);
      const provider = new TasksTreeProvider(emptyProjection);

      const items = (await provider.getChildren()) as TaskTreeItem[];
      assert.strictEqual(items.length, 1);
      assert.strictEqual(items[0].label, 'No tasks found');
      assert.strictEqual(items[0].contextValue, 'taskEmpty');

      provider.dispose();
      emptyProjection.dispose();
      disconnectedBridge.dispose();
    });
  });

  describe('ApprovalsTreeProvider', () => {
    it('displays pending approvals with correct contextValue and descriptions', async () => {
      const provider = new ApprovalsTreeProvider(projection);
      const items = (await provider.getChildren()) as ApprovalTreeItem[];

      assert.strictEqual(items.length, 1);
      const item = items[0];

      assert.strictEqual(item.label, 'npm install @vscode/test-electron');
      assert.strictEqual(item.description, '[MEDIUM] exec_command');
      assert.strictEqual(item.contextValue, 'approvalItemPending');
      assert.ok(item.tooltip?.toString().includes('Approval Request: approval-m14-001'));
      assert.ok(item.tooltip?.toString().includes('Permissions: shell:exec'));

      provider.dispose();
    });

    it('displays placeholder item when no approvals are pending', async () => {
      // Approve the only pending item
      await fixtureBridge.requestApprovalDecision('approval-m14-001', 'approved');

      const provider = new ApprovalsTreeProvider(projection);
      const items = (await provider.getChildren()) as ApprovalTreeItem[];

      assert.strictEqual(items.length, 1);
      assert.strictEqual(items[0].label, 'No pending approvals');
      assert.strictEqual(items[0].contextValue, 'noApprovals');

      provider.dispose();
    });
  });

  describe('ArtifactsTreeProvider', () => {
    it('displays patch artifacts with metadata and diff inspection command', async () => {
      const provider = new ArtifactsTreeProvider(projection);
      const items = (await provider.getChildren()) as ArtifactTreeItem[];

      assert.strictEqual(items.length, 2);

      const diff1 = items.find((a) => a.artifact?.id === 'artifact-diff-001');
      assert.ok(diff1);
      assert.strictEqual(itemLabel(diff1), 'task-m14-workbench-diff.patch');
      assert.strictEqual(diff1?.contextValue, 'artifactItem');
      assert.ok(itemDesc(diff1).includes('v1 (4 files) - 4f53cda1...'));
      assert.strictEqual(diff1?.command?.command, 'agent-studios.inspectArtifact');
      assert.deepStrictEqual(diff1?.command?.arguments, ['artifact-diff-001']);

      const diff2 = items.find((a) => a.artifact?.id === 'artifact-diff-002');
      assert.ok(diff2);
      assert.strictEqual(itemLabel(diff2), 'task-m14-bridge-contracts.patch');
      assert.ok(itemDesc(diff2).includes('v1 (1 files) - b5a2c9b6...'));

      provider.dispose();
    });

    it('displays placeholder when no artifacts exist', async () => {
      const disconnectedBridge = new DisconnectedBridge();
      const emptyProjection = new SessionStateProjection(disconnectedBridge);
      const provider = new ArtifactsTreeProvider(emptyProjection);

      const items = (await provider.getChildren()) as ArtifactTreeItem[];
      assert.strictEqual(items.length, 1);
      assert.strictEqual(itemLabel(items[0]), 'No artifacts generated');
      assert.strictEqual(items[0].contextValue, 'noArtifacts');

      provider.dispose();
      emptyProjection.dispose();
      disconnectedBridge.dispose();
    });
  });

  describe('TimelineTreeProvider', () => {
    it('displays chronological timeline events newest first with agent badge and icons', async () => {
      const provider = new TimelineTreeProvider(projection);
      const items = (await provider.getChildren()) as TimelineTreeItem[];

      assert.strictEqual(items.length, 10, 'Fixture timeline has 10 events');

      // Newest first: first item should be event sequence 10
      assert.strictEqual(items[0].event?.sequenceNumber, 10);
      assert.strictEqual(items[0].event?.type, 'approval_requested');
      assert.ok(itemDesc(items[0]).includes('[Coder Agent]'));

      // Last item should be event sequence 1
      assert.strictEqual(items[9].event?.sequenceNumber, 1);
      assert.strictEqual(items[9].event?.type, 'session_started');

      provider.dispose();
    });

    it('assigns correct contextual icons for event types', () => {
      assert.strictEqual(TimelineTreeItem.getIconForEventType('session_started').id, 'play');
      assert.strictEqual(TimelineTreeItem.getIconForEventType('task_completed').id, 'check');
      assert.strictEqual(TimelineTreeItem.getIconForEventType('task_failed').id, 'error');
      assert.strictEqual(TimelineTreeItem.getIconForEventType('task_blocked').id, 'lock');
      assert.strictEqual(TimelineTreeItem.getIconForEventType('tool_executing').id, 'tools');
      assert.strictEqual(TimelineTreeItem.getIconForEventType('approval_requested').id, 'shield');
      assert.strictEqual(TimelineTreeItem.getIconForEventType('artifact_created').id, 'diff');
    });

    it('displays placeholder when timeline has no events', async () => {
      const disconnectedBridge = new DisconnectedBridge();
      const emptyProjection = new SessionStateProjection(disconnectedBridge);
      const provider = new TimelineTreeProvider(emptyProjection);

      const items = (await provider.getChildren()) as TimelineTreeItem[];
      assert.strictEqual(items.length, 1);
      assert.strictEqual(items[0].label, 'No activity recorded');
      assert.strictEqual(items[0].contextValue, 'noActivity');

      provider.dispose();
      emptyProjection.dispose();
      disconnectedBridge.dispose();
    });
  });

  describe('ProvidersTreeProvider (Read-Only)', () => {
    it('displays read-only providers and models without mutation commands', async () => {
      const provider = new ProvidersTreeProvider(fixtureBridge);
      const providerItems = (await provider.getChildren()) as ProviderTreeItem[];

      assert.strictEqual(providerItems.length, 3, 'Anthropic, OpenAI, Ollama');

      // Check Anthropic provider
      const anthropic = providerItems.find((p) => p.provider?.instanceId === 'provider-anthropic-01');
      assert.ok(anthropic);
      assert.strictEqual(anthropic?.label, 'Anthropic Claude 3.5 Sonnet');
      assert.strictEqual(anthropic?.description, '[AVAILABLE] 2 models');
      assert.strictEqual(anthropic?.contextValue, 'providerItemReadOnly');
      assert.ok(anthropic?.tooltip?.toString().includes('(Read-Only: Managed by Rust backend)'));

      // Check Anthropic models
      const models = (await provider.getChildren(anthropic)) as ProviderTreeItem[];
      assert.strictEqual(models.length, 2);

      const sonnet = models.find((m) => m.model?.id === 'claude-3-5-sonnet-20241022');
      assert.ok(sonnet);
      assert.strictEqual(sonnet?.label, 'Claude 3.5 Sonnet (Default)');
      assert.strictEqual(sonnet?.description, '200k ctx [claude-3-5-sonnet-20241022]');
      assert.strictEqual(sonnet?.contextValue, 'providerModelReadOnly');
      assert.ok(sonnet?.tooltip?.toString().includes('(Read-Only: Managed by Rust backend)'));

      provider.dispose();
    });

    it('displays placeholder when no providers returned', async () => {
      const disconnectedBridge = new DisconnectedBridge();
      const provider = new ProvidersTreeProvider(disconnectedBridge);
      const items = (await provider.getChildren()) as ProviderTreeItem[];

      assert.strictEqual(items.length, 1);
      assert.strictEqual(items[0].label, 'No providers configured');
      assert.strictEqual(items[0].contextValue, 'noProviders');

      provider.dispose();
      disconnectedBridge.dispose();
    });
  });
});
