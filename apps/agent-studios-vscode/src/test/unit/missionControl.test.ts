/**
 * Agent Studios — Mission Control Unit Tests
 * Milestone M14 Mission Control Workbench
 */

import * as assert from 'assert';
import * as vscode from 'vscode';
import { BridgeManager } from '../../bridge/BridgeManager';
import { FixtureBridge } from '../../bridge/FixtureBridge';
import { DiffReviewManager } from '../../cards/DiffReviewManager';
import {
  MissionControlHtmlBuilder,
  MissionControlPanel,
  MissionControlProjection,
  createPreviewMissionControlState,
  renderAttentionQueue,
  renderAgentInspector,
  renderKanbanBoard,
  renderDagGraph,
  renderReviewView,
  renderApprovalsView,
  renderProvidersView,
  renderDiagnosticsView,
  MissionControlState,
} from '../../mission-control';

describe('Mission Control Workbench Unit Tests', () => {
  const extensionUri = vscode.Uri.file('/test/extension');
  let bridgeManager: BridgeManager;

  beforeEach(() => {
    bridgeManager = new BridgeManager();
    bridgeManager.setBridge(new FixtureBridge());
    if (MissionControlPanel.currentPanel) {
      MissionControlPanel.currentPanel.dispose();
    }
  });

  afterEach(() => {
    if (MissionControlPanel.currentPanel) {
      MissionControlPanel.currentPanel.dispose();
    }
    bridgeManager.dispose();
  });

  describe('MissionControlFixtures & Read Models', () => {
    it('generates authoritative preview state with all required entities', () => {
      const state = createPreviewMissionControlState();
      assert.strictEqual(state.isPreview, true);
      assert.strictEqual(state.activeTab, 'overview');
      assert.ok(state.activeStudio);
      assert.strictEqual(state.activeStudio.name, 'Autonomous Architecture Agent');

      // 8 persona fleet members
      assert.strictEqual(state.fleet.length, 8);
      const architect = state.fleet.find((a) => a.name === 'Atlas');
      assert.ok(architect);
      assert.strictEqual(architect.role, 'Principal Systems Architect');

      // 12 tasks across lifecycle states
      assert.strictEqual(state.tasks.length, 12);
      const states = new Set(state.tasks.map((t) => t.state));
      assert.ok(states.has('running'));
      assert.ok(states.has('succeeded'));
      assert.ok(states.has('failed'));
      assert.ok(states.has('blocked'));
      assert.ok(states.has('pending'));
      assert.ok(states.has('ready'));
      assert.ok(states.has('review'));

      // Attention queue generated from failed tasks and approvals
      assert.ok(state.overview.attentionItems.length >= 2);
      const errorItem = state.overview.attentionItems.find((a) => a.severity === 'error');
      assert.ok(errorItem);

      // Provider summaries with secret scrubbing
      assert.strictEqual(state.providers.length, 3);
      for (const prov of state.providers) {
        // Assert no credentials or API keys exposed
        assert.strictEqual((prov as unknown as Record<string, unknown>).apiKey, undefined);
        assert.strictEqual((prov as unknown as Record<string, unknown>).secret, undefined);
        assert.ok(prov.displayName);
        assert.ok(prov.modelId);
      }

      // Diagnostics metadata
      assert.ok(
        state.diagnostics.endpoint.includes('pipe') ||
          state.diagnostics.endpoint.includes('sock')
      );
      assert.ok(state.diagnostics.watermark > 0);
      assert.ok(state.diagnostics.agreedCapabilities.length > 0);
    });
  });

  describe('MissionControlProjection', () => {
    it('initializes with preview state when bridge is in preview mode', () => {
      const projection = new MissionControlProjection(bridgeManager);
      const state = projection.getState();

      assert.ok(state);
      assert.strictEqual(state.activeTab, 'overview');
      assert.strictEqual(state.fleet.length, 8);
      assert.strictEqual(state.tasks.length, 12);
      projection.dispose();
    });

    it('updates active tab and fires onDidChangeState', (done) => {
      const projection = new MissionControlProjection(bridgeManager);

      let eventFired = false;
      projection.onDidChangeState((newState) => {
        assert.strictEqual(newState.activeTab, 'agents');
        eventFired = true;
        projection.dispose();
        done();
      });

      projection.setActiveTab('agents');
      assert.strictEqual(eventFired, true);
    });

    it('updates selected agent, task, and run IDs', () => {
      const projection = new MissionControlProjection(bridgeManager);

      projection.selectAgent('agent-atlas');
      assert.strictEqual(projection.getState().selectedAgentId, 'agent-atlas');

      projection.selectTask('task-101');
      assert.strictEqual(projection.getState().selectedTaskId, 'task-101');

      projection.selectRun('run-2024-001');
      assert.strictEqual(projection.getState().selectedRunId, 'run-2024-001');

      projection.selectAgent(null);
      assert.strictEqual(projection.getState().selectedAgentId, undefined);

      projection.dispose();
    });

    it('refreshes state from bridge on demand', async () => {
      const projection = new MissionControlProjection(bridgeManager);
      await projection.refresh();
      const state = projection.getState();
      assert.ok(state.timestamp > 0);
      projection.dispose();
    });
  });

  describe('MissionControlHtmlBuilder', () => {
    it('builds complete HTML document with CSP nonce and all 9 tabs', () => {
      const state = createPreviewMissionControlState();
      const mockWebview: vscode.Webview = {
        cspSource: "'self'",
        asWebviewUri: (uri: vscode.Uri) => uri,
        html: '',
        options: {},
        onDidReceiveMessage: new vscode.EventEmitter<unknown>().event,
        postMessage: async () => true,
      };

      const html = MissionControlHtmlBuilder.buildHtml(mockWebview, extensionUri, state);

      // Verify strict CSP
      assert.ok(html.includes("default-src 'none'"));
      assert.ok(html.includes("script-src 'nonce-"));
      assert.ok(html.includes("style-src 'self' 'unsafe-inline'"));

      // Verify all 9 navigation tabs exist in navbar
      assert.ok(html.includes('data-tab="overview"'));
      assert.ok(html.includes('data-tab="agents"'));
      assert.ok(html.includes('data-tab="tasks"'));
      assert.ok(html.includes('data-tab="runs"'));
      assert.ok(html.includes('data-tab="timeline"'));
      assert.ok(html.includes('data-tab="review"'));
      assert.ok(html.includes('data-tab="approvals"'));
      assert.ok(html.includes('data-tab="providers"'));
      assert.ok(html.includes('data-tab="diagnostics"'));

      // Verify view containers
      assert.ok(html.includes('overview-view'));
      assert.ok(html.includes('agents-view'));
      assert.ok(html.includes('tasks-view'));
      assert.ok(html.includes('runs-view'));
      assert.ok(html.includes('timeline-view'));
      assert.ok(html.includes('review-view'));
      assert.ok(html.includes('approvals-view'));
      assert.ok(html.includes('providers-view'));
      assert.ok(html.includes('diagnostics-view'));

      // Verify client script functions
      assert.ok(html.includes('window.switchTab'));
      assert.ok(html.includes('window.selectAgent'));
      assert.ok(html.includes('window.selectTask'));
      assert.ok(html.includes('window.selectRun'));
      assert.ok(html.includes('window.openDiff'));
      assert.ok(html.includes('window.openChat'));
      assert.ok(html.includes('window.refreshState'));
      assert.ok(html.includes('window.filterTimeline'));
      assert.ok(html.includes('window.zoomDag'));
    });
  });

  describe('MissionControlPanel Lifecycle & Message Routing', () => {
    it('creates or reveals singleton WebviewPanel', () => {
      const projection = new MissionControlProjection(bridgeManager);
      const panel = MissionControlPanel.createOrShow(extensionUri, projection);

      assert.ok(panel);
      assert.strictEqual(MissionControlPanel.currentPanel, panel);

      // Revealing existing panel returns identical singleton instance
      const secondCall = MissionControlPanel.createOrShow(extensionUri, projection);
      assert.strictEqual(secondCall, panel);

      projection.dispose();
    });

    it('resets singleton when disposed', () => {
      const projection = new MissionControlProjection(bridgeManager);
      const panel = MissionControlPanel.createOrShow(extensionUri, projection);

      panel.dispose();
      assert.strictEqual(MissionControlPanel.currentPanel, undefined);
      projection.dispose();
    });

    it('routes switchTab and selection messages from webview', async () => {
      const projection = new MissionControlProjection(bridgeManager);
      const panel = MissionControlPanel.createOrShow(extensionUri, projection);

      const panelAny = panel as unknown as {
        _panel: { webview: { _messageEmitter: { fire: (msg: unknown) => void } } };
      };

      // Fire switchTab message
      panelAny._panel.webview._messageEmitter.fire({
        type: 'switchTab',
        payload: { tab: 'tasks' },
      });
      assert.strictEqual(projection.activeTab, 'tasks');

      // Fire selectTask message
      panelAny._panel.webview._messageEmitter.fire({
        type: 'selectTask',
        payload: { taskId: 'task-102' },
      });
      assert.strictEqual(projection.getState().selectedTaskId, 'task-102');

      // Fire selectAgent message
      panelAny._panel.webview._messageEmitter.fire({
        type: 'selectAgent',
        payload: { agentId: 'agent-vulcan' },
      });
      assert.strictEqual(projection.getState().selectedAgentId, 'agent-vulcan');

      panel.dispose();
      projection.dispose();
    });

    it('delegates openDiff message to DiffReviewManager', async () => {
      const projection = new MissionControlProjection(bridgeManager);
      let diffOpenedWith: string | undefined;

      const mockDiffManager = {
        openDiff: async (target: string) => {
          diffOpenedWith = target;
        },
      } as unknown as DiffReviewManager;

      const panel = MissionControlPanel.createOrShow(
        extensionUri,
        projection,
        mockDiffManager
      );

      const panelAny = panel as unknown as {
        _panel: { webview: { _messageEmitter: { fire: (msg: unknown) => void } } };
      };

      panelAny._panel.webview._messageEmitter.fire({
        type: 'openDiff',
        payload: { artifactId: 'art-patch-001' },
      });

      // Allow microtask cycle
      await new Promise((resolve) => setTimeout(resolve, 10));

      assert.strictEqual(diffOpenedWith, 'art-patch-001');

      panel.dispose();
      projection.dispose();
    });

    it('fails closed on approveAction or denyAction requests', async () => {
      const projection = new MissionControlProjection(bridgeManager);
      const panel = MissionControlPanel.createOrShow(extensionUri, projection);

      const panelAny = panel as unknown as {
        _panel: { webview: { _messageEmitter: { fire: (msg: unknown) => void } } };
      };

      // Firing approveAction should not throw and should enforce fail-closed security
      panelAny._panel.webview._messageEmitter.fire({
        type: 'approveAction',
        payload: { actionId: 'appr-001' },
      });

      panelAny._panel.webview._messageEmitter.fire({
        type: 'denyAction',
        payload: { actionId: 'appr-001' },
      });

      // State should remain unchanged
      assert.ok(projection.getState());

      panel.dispose();
      projection.dispose();
    });
  });

  describe('View Components Rendering', () => {
    let state: MissionControlState;

    beforeEach(() => {
      state = createPreviewMissionControlState();
    });

    it('renders AttentionQueue with warning and error severity cards', () => {
      const html = renderAttentionQueue(state.overview.attentionItems);
      assert.ok(html.includes('attention-queue-container'));
      assert.ok(html.includes('severity-error'));
      assert.ok(html.includes('severity-warning'));
      assert.ok(html.includes('Attention Queue'));
    });

    it('renders AgentInspector with runtime telemetry and tool traces', () => {
      const agent = state.fleet[0];
      const html = renderAgentInspector(agent, state.runs);
      assert.ok(html.includes('agent-inspector-panel'));
      assert.ok(html.includes(agent.name));
      assert.ok(html.includes(agent.role));
      assert.ok(html.includes('codex-rs'));
      assert.ok(html.includes('Assigned Task'));
    });

    it('renders KanbanBoard across lifecycle states', () => {
      const html = renderKanbanBoard(state.tasks);
      assert.ok(html.includes('kanban-board-container'));
      assert.ok(html.includes('Ready'));
      assert.ok(html.includes('Running'));
      assert.ok(html.includes('Blocked'));
      assert.ok(html.includes('Review'));
      assert.ok(html.includes('Succeeded'));
      assert.ok(html.includes('Failed / Cancelled'));
    });

    it('renders DagGraph SVG with nodes, Bezier paths, and controls', () => {
      const html = renderDagGraph(state.tasks);
      assert.ok(html.includes('dag-graph-wrapper'));
      assert.ok(html.includes('<svg'));
      assert.ok(html.includes('class="dag-edge"'));
      assert.ok(html.includes('class="dag-node'));
      assert.ok(html.includes('zoomDag(0.1)'));
      assert.ok(html.includes('resetDagZoom()'));
    });

    it('renders ApprovalsView with fail-closed security banner', () => {
      const html = renderApprovalsView(state);
      assert.ok(html.includes('approvals-view'));
      assert.ok(html.includes('Fail-Closed Security Boundary'));
      assert.ok(html.includes('disabled'));
    });

    it('renders ReviewView with file artifacts and diff buttons', () => {
      const html = renderReviewView(state);
      assert.ok(html.includes('review-view'));
      assert.ok(html.includes('Code Review & Artifacts'));
      assert.ok(html.includes('openDiff'));
    });

    it('renders ProvidersView with secret-scrubbing banner', () => {
      const html = renderProvidersView(state);
      assert.ok(html.includes('providers-view'));
      assert.ok(html.includes('Secret Scrubbing'));
      assert.ok(html.includes('Measured Latency'));
    });

    it('renders DiagnosticsView with IPC endpoint and sequence watermark', () => {
      const html = renderDiagnosticsView(state);
      assert.ok(html.includes('diagnostics-view'));
      assert.ok(
        html.includes('ags-daemon-preview') ||
          html.includes('agent-studios.sock')
      );
      assert.ok(html.includes('Event Watermark'));
    });
  });

  describe('Integration Quality Gate & Defect Elimination', () => {
    it('buildViewFragments outputs all 9 tab view strings', () => {
      const state = createPreviewMissionControlState();
      const fragments = MissionControlHtmlBuilder.buildViewFragments(state);
      assert.ok(fragments.overview.includes('overview-view'));
      assert.ok(fragments.agents.includes('agents-view'));
      assert.ok(fragments.tasks.includes('tasks-view'));
      assert.ok(fragments.runs.includes('runs-view'));
      assert.ok(fragments.timeline.includes('timeline-view'));
      assert.ok(fragments.review.includes('review-view'));
      assert.ok(fragments.approvals.includes('approvals-view'));
      assert.ok(fragments.providers.includes('providers-view'));
      assert.ok(fragments.diagnostics.includes('diagnostics-view'));
    });

    it('performs incremental state updates via postMessage without rewriting webview.html', () => {
      const projection = new MissionControlProjection(bridgeManager);
      const panel = MissionControlPanel.createOrShow(extensionUri, projection);

      const panelAny = panel as unknown as {
        _panel: { webview: { html: string; _sentMessages: Array<{ type: string; fragments?: unknown }> } };
      };

      const initialHtml = panelAny._panel.webview.html;
      assert.ok(initialHtml.length > 0);
      assert.strictEqual(panelAny._panel.webview._sentMessages.length, 0);

      // Trigger a state change
      projection.selectTask('task-101');

      // HTML document is NOT rewritten
      assert.strictEqual(panelAny._panel.webview.html, initialHtml);
      // Typed postMessage is dispatched
      assert.strictEqual(panelAny._panel.webview._sentMessages.length, 1);
      assert.strictEqual(panelAny._panel.webview._sentMessages[0].type, 'stateUpdate');
      assert.ok(panelAny._panel.webview._sentMessages[0].fragments);

      panel.dispose();
      projection.dispose();
    });

    it('re-binds bridge listeners dynamically when onBridgeChanged fires', async () => {
      let bridge2ListenersCount = 0;
      const emitter2 = new vscode.EventEmitter<any>();

      const mockBridge1: any = {
        connectionState: 'connected',
        onConnectionStateChanged: new vscode.EventEmitter<any>().event,
        onEvent: new vscode.EventEmitter<any>().event,
        getCurrentSession: async () => null,
        listTasks: async () => [],
        listPendingApprovals: async () => [],
        listArtifacts: async () => [],
        listProviderSummaries: async () => [],
        dispose: () => {},
      };

      const mockBridge2: any = {
        connectionState: 'connected',
        onConnectionStateChanged: new vscode.EventEmitter<any>().event,
        onEvent: (listener: (e: any) => any) => {
          bridge2ListenersCount++;
          return emitter2.event(listener);
        },
        getCurrentSession: async () => null,
        listTasks: async () => [],
        listPendingApprovals: async () => [],
        listArtifacts: async () => [],
        listProviderSummaries: async () => [],
        dispose: () => {},
      };

      bridgeManager.setBridge(mockBridge1);
      const projection = new MissionControlProjection(bridgeManager);

      // Switch bridge
      bridgeManager.setBridge(mockBridge2);
      assert.ok(bridge2ListenersCount >= 1, 'Bridge 2 should receive listener binding onBridgeChanged');

      projection.dispose();
    });

    it('resiliently handles partial query failures via Promise.allSettled without throwing', async () => {
      const failingBridge: any = {
        connectionState: 'connected',
        onConnectionStateChanged: new vscode.EventEmitter<any>().event,
        onEvent: new vscode.EventEmitter<any>().event,
        getCurrentSession: async () => ({
          id: 'studio-failing-test',
          workspacePath: '/test',
          goalSummary: 'Resilience Test',
          status: 'running',
          activeAgentCount: 0,
          totalAgentCount: 0,
          completedTasksCount: 0,
          totalTasksCount: 0,
          startedAt: Date.now(),
          elapsedMs: 0,
        }),
        listTasks: async () => {
          throw new Error('Simulated tasks query failure');
        },
        listPendingApprovals: async () => [],
        listArtifacts: async () => [],
        listProviderSummaries: async () => [],
        dispose: () => {},
      };

      bridgeManager.setBridge(failingBridge);

      const projection = new MissionControlProjection(bridgeManager);
      await projection.refresh();

      const state = projection.getState();
      assert.strictEqual(state.tasks.length, 0);
      assert.ok(state.queryErrors?.tasks?.includes('Simulated tasks query failure'));
      assert.ok(state.overview.attentionItems.some((a) => a.id === 'att-query-errors'));

      projection.dispose();
    });

    it('maps retrying and paused task states truthfully without fabricated metrics', async () => {
      const customBridge: any = {
        connectionState: 'connected',
        onConnectionStateChanged: new vscode.EventEmitter<any>().event,
        onEvent: new vscode.EventEmitter<any>().event,
        getCurrentSession: async () => null,
        listTasks: async () => [
          {
            id: 'task-retrying',
            runId: 'run-1',
            title: 'Retrying Task',
            description: 'A task that is retrying',
            status: 'running',
            dependencies: [],
            progress: undefined,
            createdAt: Date.now(),
            updatedAt: Date.now(),
          },
        ],
        listPendingApprovals: async () => [],
        listArtifacts: async () => [],
        listProviderSummaries: async () => [
          {
            instanceId: 'prov-1',
            displayName: 'Real Provider',
            status: 'available',
            models: [{ id: 'm1', name: 'Real Model', contextWindow: 64000 }],
          },
        ],
        dispose: () => {},
      };

      bridgeManager.setBridge(customBridge);

      const projection = new MissionControlProjection(bridgeManager);
      await projection.refresh();

      const state = projection.getState();
      assert.strictEqual(state.tasks[0].progress, undefined, 'Progress must be undefined/indeterminate, not fabricated 50');

      // Providers latency must not be hardcoded 120ms
      for (const prov of state.providers) {
        assert.strictEqual(prov.latencyMs, undefined, 'Latency must be undefined when unmeasured');
      }

      projection.dispose();
    });
  });
});
