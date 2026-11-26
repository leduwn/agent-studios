/**
 * Agent Studios — Clean Composer Controls, Unified Send & Streaming Tests
 */

import * as assert from 'assert';
import * as vscode from 'vscode';
import { BridgeManager } from '../../bridge/BridgeManager';
import { FixtureBridge } from '../fixtures/FixtureBridge';
import { ChatHtmlBuilder } from '../../chat/ChatHtmlBuilder';
import { ChatWebviewViewProvider } from '../../chat/ChatWebviewViewProvider';
import { filterCompatibleRuntimes } from '../../chat/ChatPresenter';
import { TaskExecutionStore } from '../../state/TaskExecutionStore';
import { MockWebviewView, resetMockState } from './vscodeMock';

describe('ExecutionWorkbenchUI Tests', () => {
  const extensionUri = vscode.Uri.file('/mock/extension/path');

  beforeEach(() => {
    resetMockState();
    TaskExecutionStore.instance.reset();
  });

  afterEach(() => {
    TaskExecutionStore.instance.reset();
  });

  it('filters runtime options cleanly by active provider compatibility', () => {
    const mockWebview = new MockWebviewView(ChatWebviewViewProvider.viewType).webview;
    const html = ChatHtmlBuilder.buildHtml(mockWebview as unknown as vscode.Webview, extensionUri, {
      connectionState: 'connected',
      selectedProviderId: 'ollama-1',
      selectedModelId: 'deepseek-coder:6.7b',
      providers: [
        {
          instanceId: 'ollama-1',
          providerType: 'ollama',
          displayName: 'Ollama',
          status: 'available',
          models: [{ id: 'deepseek-coder:6.7b', name: 'deepseek-coder:6.7b', contextWindow: 32000 }],
        },
      ],
      runtimes: [
        { id: 'claude-code', name: 'Claude Code CLI', enabled: true },
        { id: 'internal-codex', name: 'Agent Studios Engine', enabled: true },
      ],
    });

    assert.ok(html.includes('id="runtimeSelect"'));
    assert.ok(html.includes('Agent Studios Engine'));
    assert.ok(!html.includes('Claude Code CLI'), 'Claude Code CLI must not pair with DeepSeek/Ollama');

    const compatibleAnthropic = filterCompatibleRuntimes(
      [
        { id: 'claude-code', name: 'Claude Code CLI', enabled: true },
        { id: 'internal-codex', name: 'Agent Studios Engine', enabled: true },
      ],
      'anthropic',
      'claude-sonnet-4-5'
    );
    assert.ok(compatibleAnthropic.some((r) => r.id === 'claude-code'));
  });

  it('renders clean Send / Stop state without disabled Start Task clutter', () => {
    const mockWebview = new MockWebviewView(ChatWebviewViewProvider.viewType).webview;
    const idleHtml = ChatHtmlBuilder.buildHtml(mockWebview as unknown as vscode.Webview, extensionUri, {
      connectionState: 'connected',
    });
    assert.ok(idleHtml.includes('id="sendBtn"'));
    assert.ok(idleHtml.includes('aria-label="Send message"'));
    assert.ok(!idleHtml.includes('▶ Start Task'));
    assert.ok(!idleHtml.includes('Host Modal Gate'));

    const runningHtml = ChatHtmlBuilder.buildHtml(mockWebview as unknown as vscode.Webview, extensionUri, {
      connectionState: 'connected',
      executionState: {
        status: 'running',
        elapsedSeconds: 5,
        streamOutput: '',
      },
    });
    assert.ok(runningHtml.includes('is-stop'));
    assert.ok(runningHtml.includes('aria-label="Stop generation"'));
  });

  it('renders grouped collapsible tool calls, diff cards, approval cards, and progressive multi-agent card', () => {
    const threadHtml = ChatHtmlBuilder.renderMessagesHtml(
      [
        { id: 'u1', role: 'user', content: 'Refactor utils.ts', timestamp: 1 },
        {
          id: 'a1',
          role: 'assistant',
          content: 'Updated `utils.ts` cleanly.\n\n```ts\nexport const ok = true;\n```',
          timestamp: 2,
          toolCalls: [
            { id: 't1', name: 'read_file', target: 'src/utils.ts', status: 'completed' },
            { id: 't2', name: 'edit_file', target: 'src/utils.ts', status: 'completed' },
          ],
          artifacts: [{ id: 'art-1', path: 'src/utils.ts' }],
        },
      ],
      [
        {
          id: 'task-1',
          runId: 'run-1',
          title: 'Inspect files',
          description: '',
          status: 'succeeded',
          dependencies: [],
          createdAt: 1,
          updatedAt: 2,
        },
        {
          id: 'task-2',
          runId: 'run-1',
          title: 'Apply refactor',
          description: '',
          status: 'running',
          dependencies: ['task-1'],
          createdAt: 1,
          updatedAt: 2,
        },
      ],
      [
        {
          id: 'apr-1',
          sessionId: 's1',
          actionType: 'exec_command',
          commandOrTarget: 'npm test',
          riskLevel: 'medium',
          requestedPermissions: ['terminal'],
          status: 'pending',
          createdAt: 1,
        },
      ]
    );

    assert.ok(threadHtml.includes('class="tool-group"'));
    assert.ok(threadHtml.includes('2 tool calls'));
    assert.ok(threadHtml.includes('class="copy-code-btn"'));
    assert.ok(threadHtml.includes('View Diff'));
    assert.ok(threadHtml.includes('data-approve-id="apr-1"'));
    assert.ok(threadHtml.includes('Plan &amp; Agents (1/2 done)'));
  });

  it('routes unified sendPrompt in agent mode through BridgeManager', async () => {
    const manager = new BridgeManager(new FixtureBridge());
    const provider = new ChatWebviewViewProvider(extensionUri, manager);
    const mockView = new MockWebviewView(ChatWebviewViewProvider.viewType);
    await provider.resolveWebviewView(
      mockView as unknown as vscode.WebviewView,
      {} as vscode.WebviewViewResolveContext,
      {} as vscode.CancellationToken
    );

    (vscode.workspace as { workspaceFolders?: readonly vscode.WorkspaceFolder[] }).workspaceFolders = [
      {
        uri: vscode.Uri.file('/test/workspace'),
        name: 'test',
        index: 0,
      },
    ];

    await provider.handleWebviewMessage({
      command: 'sendPrompt',
      text: 'Update README',
      mode: 'agent',
    });

    assert.ok(provider.messages.some((m) => m.role === 'user' && m.content === 'Update README'));
    assert.ok(provider.messages.some((m) => m.role === 'assistant'));
    provider.dispose();
    manager.dispose();
  });
});
