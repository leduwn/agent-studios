/**
 * Agent Studios — Webviews & Layout Unit Tests
 * Milestone M14-UI-00: Code-OSS Native Agents Workbench
 */

import * as assert from 'assert';
import * as vscode from 'vscode';
import { FixtureBridge } from '../../bridge/FixtureBridge';
import { TaskGraph, TaskNode } from '../../bridge/types';
import {
  computeTopologicalLayout,
  detectCycle,
  normalizeEdges,
} from '../../webviews/graphLayout';
import { escapeHtml, renderSafeMarkdown } from '../../webviews/markdownRenderer';
import { SessionDetailWebview } from '../../webviews/SessionDetailWebview';
import { TaskGraphWebview } from '../../webviews/TaskGraphWebview';

describe('Webviews & Graph Layout Tests', () => {
  describe('graphLayout — Cycle Detection & Layout', () => {
    const createMockNode = (id: string, dependencies: string[] = []): TaskNode => ({
      id,
      runId: 'run-1',
      title: `Task ${id}`,
      description: `Description for ${id}`,
      status: 'pending',
      dependencies,
      progress: 0,
      createdAt: 1000,
      updatedAt: 1000,
    });

    it('handles empty nodes gracefully', () => {
      const layout = computeTopologicalLayout([]);
      assert.strictEqual(layout.hasCycle, false);
      assert.strictEqual(layout.nodes.length, 0);
      assert.strictEqual(layout.edges.length, 0);
      assert.strictEqual(layout.levelCount, 0);
    });

    it('positions a single node at level 0', () => {
      const node = createMockNode('t1');
      const layout = computeTopologicalLayout([node]);
      assert.strictEqual(layout.hasCycle, false);
      assert.strictEqual(layout.nodes.length, 1);
      assert.strictEqual(layout.nodes[0].level, 0);
      assert.strictEqual(layout.levelCount, 1);
    });

    it('computes correct topological levels for a linear pipeline (A -> B -> C)', () => {
      const t1 = createMockNode('t1', []);
      const t2 = createMockNode('t2', ['t1']);
      const t3 = createMockNode('t3', ['t2']);

      const layout = computeTopologicalLayout([t1, t2, t3]);
      assert.strictEqual(layout.hasCycle, false);
      assert.strictEqual(layout.levelCount, 3);

      const nodeMap = new Map(layout.nodes.map((n) => [n.id, n]));
      assert.strictEqual(nodeMap.get('t1')?.level, 0);
      assert.strictEqual(nodeMap.get('t2')?.level, 1);
      assert.strictEqual(nodeMap.get('t3')?.level, 2);

      // Verify X coordinates increase with level
      assert.ok(nodeMap.get('t1')!.x < nodeMap.get('t2')!.x);
      assert.ok(nodeMap.get('t2')!.x < nodeMap.get('t3')!.x);

      // Verify edges
      assert.strictEqual(layout.edges.length, 2);
      assert.ok(layout.edges.some((e) => e.from === 't1' && e.to === 't2'));
      assert.ok(layout.edges.some((e) => e.from === 't2' && e.to === 't3'));
    });

    it('computes correct levels for diamond graph (A -> B, A -> C, B -> D, C -> D)', () => {
      const a = createMockNode('A', []);
      const b = createMockNode('B', ['A']);
      const c = createMockNode('C', ['A']);
      const d = createMockNode('D', ['B', 'C']);

      const layout = computeTopologicalLayout([a, b, c, d]);
      assert.strictEqual(layout.hasCycle, false);

      const nodeMap = new Map(layout.nodes.map((n) => [n.id, n]));
      assert.strictEqual(nodeMap.get('A')?.level, 0);
      assert.strictEqual(nodeMap.get('B')?.level, 1);
      assert.strictEqual(nodeMap.get('C')?.level, 1);
      assert.strictEqual(nodeMap.get('D')?.level, 2);

      assert.strictEqual(layout.edges.length, 4);
    });

    it('detects direct self-loop cycle (A -> A)', () => {
      const a = createMockNode('A', ['A']);
      const cycle = detectCycle([a], [{ from: 'A', to: 'A' }]);
      assert.strictEqual(cycle.hasCycle, true);
    });

    it('detects two-node cycle (A -> B -> A)', () => {
      const a = createMockNode('A', ['B']);
      const b = createMockNode('B', ['A']);
      const cycle = detectCycle([a, b], [
        { from: 'A', to: 'B' },
        { from: 'B', to: 'A' },
      ]);
      assert.strictEqual(cycle.hasCycle, true);
      assert.ok(cycle.cyclePath && cycle.cyclePath.length >= 2);
    });

    it('detects three-node cycle (A -> B -> C -> A)', () => {
      const a = createMockNode('A');
      const b = createMockNode('B');
      const c = createMockNode('C');
      const edges = [
        { from: 'A', to: 'B' },
        { from: 'B', to: 'C' },
        { from: 'C', to: 'A' },
      ];
      const cycle = detectCycle([a, b, c], edges);
      assert.strictEqual(cycle.hasCycle, true);
      assert.ok(cycle.cyclePath);
      assert.deepStrictEqual(cycle.cyclePath, ['A', 'B', 'C', 'A']);
    });

    it('falls back to grid layout with warning flag when cycle detected', () => {
      const a = createMockNode('A', ['B']);
      const b = createMockNode('B', ['A']);
      const layout = computeTopologicalLayout([a, b]);

      assert.strictEqual(layout.hasCycle, true);
      assert.ok(layout.cyclePath);
      assert.strictEqual(layout.nodes.length, 2);
    });

    it('normalizes edges by deduplicating and removing invalid node references', () => {
      const a = createMockNode('A', ['B', 'NONEXISTENT']);
      const b = createMockNode('B', []);
      const edges = normalizeEdges([a, b], [
        { from: 'B', to: 'A' },
        { from: 'B', to: 'A' }, // Duplicate
        { from: 'A', to: 'UNKNOWN' }, // Invalid
      ]);

      assert.strictEqual(edges.length, 1);
      assert.strictEqual(edges[0].from, 'B');
      assert.strictEqual(edges[0].to, 'A');
    });

    it('lays out FixtureBridge task graph without cycles', async () => {
      const bridge = new FixtureBridge();
      const graph = await bridge.getTaskGraph();
      assert.ok(graph);

      const layout = computeTopologicalLayout(graph.nodes, graph.edges);
      assert.strictEqual(layout.hasCycle, false);
      assert.strictEqual(layout.nodes.length, 5);
      assert.ok(layout.levelCount >= 3);
      bridge.dispose();
    });
  });

  describe('markdownRenderer — Safe Markdown & XSS Sanitization', () => {
    it('escapes dangerous HTML characters', () => {
      assert.strictEqual(escapeHtml('<script>alert("xss")</script>'), '&lt;script&gt;alert(&quot;xss&quot;)&lt;/script&gt;');
      assert.strictEqual(escapeHtml('foo & bar \'baz\''), 'foo &amp; bar &#39;baz&#39;');
      assert.strictEqual(escapeHtml(null), '');
      assert.strictEqual(escapeHtml(undefined), '');
    });

    it('sanitizes script tags inside markdown', () => {
      const input = '# Heading\n<script>alert(1)</script>\n**bold**';
      const html = renderSafeMarkdown(input);
      assert.ok(!html.includes('<script>'));
      assert.ok(html.includes('&lt;script&gt;alert(1)&lt;/script&gt;'));
      assert.ok(html.includes('<h1>Heading</h1>'));
      assert.ok(html.includes('<strong>bold</strong>'));
    });

    it('renders code blocks, lists, blockquotes, and inline formatting safely', () => {
      const md = [
        '# Goal',
        'Here is `inline code` and *italic text*.',
        '> Important quote',
        '- Item 1',
        '- Item 2',
        '```typescript',
        'const x = 42;',
        '```',
      ].join('\n');

      const html = renderSafeMarkdown(md);
      assert.ok(html.includes('<h1>Goal</h1>'));
      assert.ok(html.includes('<code class="inline-code">inline code</code>'));
      assert.ok(html.includes('<em>italic text</em>'));
      assert.ok(html.includes('<blockquote>Important quote</blockquote>'));
      assert.ok(html.includes('<ul>'));
      assert.ok(html.includes('<li>Item 1</li>'));
      assert.ok(html.includes('<li>Item 2</li>'));
      assert.ok(html.includes('<pre class="code-block"><code class="language-typescript">const x = 42;</code></pre>'));
    });

    it('handles empty markdown input with fallback', () => {
      const html = renderSafeMarkdown('');
      assert.ok(html.includes('No details provided.'));
    });
  });

  describe('TaskGraphWebview — WebviewPanel Management & Interactivity', () => {
    const extensionUri = vscode.Uri.file('/test/extension');
    let bridge: FixtureBridge;

    beforeEach(() => {
      bridge = new FixtureBridge();
      if (TaskGraphWebview.currentPanel) {
        TaskGraphWebview.currentPanel.dispose();
      }
    });

    afterEach(() => {
      if (TaskGraphWebview.currentPanel) {
        TaskGraphWebview.currentPanel.dispose();
      }
      bridge.dispose();
    });

    it('creates or reveals singleton WebviewPanel with strict CSP', async () => {
      const graph = await bridge.getTaskGraph();
      const webviewInstance = TaskGraphWebview.createOrShow(extensionUri, graph, bridge);
      assert.ok(webviewInstance);
      assert.strictEqual(TaskGraphWebview.currentPanel, webviewInstance);

      // Verify same instance returned on repeat call
      const secondCall = TaskGraphWebview.createOrShow(extensionUri);
      assert.strictEqual(secondCall, webviewInstance);

      // Verify CSP header in webview HTML
      // Accessing internal panel webview HTML through reflection for verification
      const panelAny = webviewInstance as unknown as { _panel: { webview: { html: string } } };
      const html = panelAny._panel.webview.html;
      assert.ok(html.includes("default-src 'none'"));
      assert.ok(html.includes("style-src"));
      assert.ok(html.includes("script-src 'nonce-"));
      assert.ok(html.includes("Task Graph (DAG)"));
    });

    it('fires onDidSelectTask event when selectTask message received from webview', async () => {
      const graph = await bridge.getTaskGraph();
      const webviewInstance = TaskGraphWebview.createOrShow(extensionUri, graph, bridge);

      let selectedId: string | undefined;
      webviewInstance.onDidSelectTask((taskId) => {
        selectedId = taskId;
      });

      // Simulate message from webview client
      const panelAny = webviewInstance as unknown as {
        _panel: { webview: { _messageEmitter: { fire: (msg: unknown) => void } } };
      };
      panelAny._panel.webview._messageEmitter.fire({ command: 'selectTask', taskId: 'task-1' });

      assert.strictEqual(selectedId, 'task-1');
      assert.strictEqual(webviewInstance.selectedTaskId, 'task-1');
    });

    it('handles disposal and resets currentPanel singleton', () => {
      const webviewInstance = TaskGraphWebview.createOrShow(extensionUri, null, bridge);
      assert.strictEqual(TaskGraphWebview.currentPanel, webviewInstance);

      webviewInstance.dispose();
      assert.strictEqual(TaskGraphWebview.currentPanel, undefined);
    });

    it('updates graph data dynamically via setGraph', () => {
      const webviewInstance = TaskGraphWebview.createOrShow(extensionUri, null, bridge);
      const newGraph: TaskGraph = {
        runId: 'run-custom',
        nodes: [
          {
            id: 'task-dyn-1',
            runId: 'run-custom',
            title: 'Dynamic Task',
            description: 'Dynamic desc',
            status: 'running',
            dependencies: [],
            progress: 50,
            createdAt: 1000,
            updatedAt: 1000,
          },
        ],
        edges: [],
      };

      webviewInstance.setGraph(newGraph);
      assert.strictEqual(webviewInstance.currentGraph?.runId, 'run-custom');

      const panelAny = webviewInstance as unknown as { _panel: { webview: { html: string } } };
      assert.ok(panelAny._panel.webview.html.includes('Dynamic Task'));
    });
  });

  describe('SessionDetailWebview — Deep Session Overview & Metrics', () => {
    const extensionUri = vscode.Uri.file('/test/extension');
    let bridge: FixtureBridge;

    beforeEach(() => {
      bridge = new FixtureBridge();
      if (SessionDetailWebview.currentPanel) {
        SessionDetailWebview.currentPanel.dispose();
      }
    });

    afterEach(() => {
      if (SessionDetailWebview.currentPanel) {
        SessionDetailWebview.currentPanel.dispose();
      }
      bridge.dispose();
    });

    it('creates SessionDetailWebview and renders session metadata & metric cards', async () => {
      const session = await bridge.getCurrentSession();
      const tasks = await bridge.listTasks();
      const webview = SessionDetailWebview.createOrShow(extensionUri, session, tasks, undefined, bridge);

      assert.ok(webview);
      assert.strictEqual(SessionDetailWebview.currentPanel, webview);

      const panelAny = webview as unknown as { _panel: { webview: { html: string } } };
      const html = panelAny._panel.webview.html;

      // Verify Session ID and status badge
      assert.ok(html.includes(session!.id));
      assert.ok(html.includes('badge-status-running'));
      // Verify metrics
      assert.ok(html.includes('Total Tasks'));
      assert.ok(html.includes('In-Flight'));
      assert.ok(html.includes('Blocked'));
      assert.ok(html.includes('Elapsed Time'));
      // Verify Goal & Scope
      assert.ok(html.includes('Goal &amp; Scope Overview'));
      // Verify Agent Roster
      assert.ok(html.includes('Active Agent Roster'));

      // Check developer fixture preview banner is displayed when isPreview = true
      assert.ok(html.includes('DEVELOPER FIXTURE PREVIEW'));
    });

    it('updates session data and respects non-preview sessions', () => {
      const webview = SessionDetailWebview.createOrShow(extensionUri, null, [], undefined, bridge);
      webview.setData(
        {
          id: 'prod-session',
          workspacePath: '/prod/workspace',
          goalSummary: 'Production deployment goal',
          status: 'completed',
          activeAgentCount: 1,
          totalAgentCount: 1,
          completedTasksCount: 10,
          totalTasksCount: 10,
          startedAt: 1000,
          elapsedMs: 60000,
          isPreview: false,
        },
        []
      );

      const panelAny = webview as unknown as { _panel: { webview: { html: string } } };
      const html = panelAny._panel.webview.html;
      assert.ok(html.includes('prod-session'));
      assert.ok(html.includes('badge-status-completed'));
      assert.ok(!html.includes('DEVELOPER FIXTURE PREVIEW'));
    });

    it('handles disposal cleanly', () => {
      const webview = SessionDetailWebview.createOrShow(extensionUri, null, [], undefined, bridge);
      assert.strictEqual(SessionDetailWebview.currentPanel, webview);
      webview.dispose();
      assert.strictEqual(SessionDetailWebview.currentPanel, undefined);
    });
  });
});
