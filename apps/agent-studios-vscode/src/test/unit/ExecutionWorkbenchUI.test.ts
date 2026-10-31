/**
 * Agent Studios — Execution Workbench UI, Accessibility & Streaming Tests
 * Milestone M15 UI-01: Interactive Agent Execution Workbench
 *
 * Verifies:
 * 1. Workbench Toolbar & Selectors (Runtime defaults, disabled M16 options, Studio selector).
 * 2. Workspace Containment & Equality Verification (matched badge vs mismatch warning).
 * 3. Multiline Composer & Frame Boundary Guards (>100 KB warning, >4 MiB error rejection).
 * 4. Execution Summary Card & Configuration Transparency.
 * 5. Live Streaming Pane, Tool Activity Banner & Truthful Metrics (zero fabrication).
 * 6. Accessibility Compliance (ARIA roles, live regions, high-contrast support).
 * 7. Webview Provider Execution Command Handling & Operator Confirmation Flow.
 */

import * as assert from 'assert';
import * as vscode from 'vscode';
import { CapabilityMatrix } from '../../bridge/ipc/CapabilityMatrix';
import { FixtureBridge } from '../../bridge/FixtureBridge';
import { ChatHtmlBuilder } from '../../chat/ChatHtmlBuilder';
import { ChatWebviewViewProvider } from '../../chat/ChatWebviewViewProvider';
import { OperatorConfirmationModal } from '../../security/OperatorConfirmationModal';
import { TaskExecutionStore } from '../../state/TaskExecutionStore';
import {
  MockWebviewView,
  resetMockState,
} from './vscodeMock';

describe('ExecutionWorkbenchUI Tests', () => {
  const extensionUri = vscode.Uri.file('/mock/extension/path');

  beforeEach(() => {
    resetMockState();
    OperatorConfirmationModal.resetDialogHandlerForTesting();
    TaskExecutionStore.instance.reset();
    CapabilityMatrix.resetCapabilities();
  });

  afterEach(() => {
    OperatorConfirmationModal.resetDialogHandlerForTesting();
    TaskExecutionStore.instance.reset();
    CapabilityMatrix.resetCapabilities();
  });

  describe('1. Workbench Toolbar & Selectors', () => {
    it('renders Runtime selector with Claude Code default/enabled and Codex/Custom Shell disabled', () => {
      const mockWebview = new MockWebviewView(ChatWebviewViewProvider.viewType).webview;
      const html = ChatHtmlBuilder.buildHtml(mockWebview as unknown as vscode.Webview, extensionUri, {
        connectionState: 'connected',
        selectedRuntime: 'claude_code',
        runtimes: [
          { id: 'claude_code', name: 'Claude Code (Native CLI)', enabled: true },
          { id: 'codex_external', name: 'Codex Runtime (External — Coming in M16)', enabled: false, tooltip: 'Coming in M16: External process runner support' },
          { id: 'custom_shell', name: 'Custom Shell (Disabled)', enabled: false, tooltip: 'Coming in future milestone' },
        ],
      });

      assert.ok(html.includes('id="runtimeSelect"'), 'Must render runtimeSelect');
      assert.ok(html.includes('value="claude_code" selected>Claude Code (Native CLI)</option>'), 'Claude Code must be selected');
      assert.ok(
        html.includes('value="codex_external" disabled title="Coming in M16: External process runner support">Codex Runtime (External — Coming in M16)</option>'),
        'Codex Runtime must be disabled with M16 tooltip'
      );
      assert.ok(
        html.includes('value="custom_shell" disabled title="Coming in future milestone">Custom Shell (Disabled)</option>'),
        'Custom shell must be disabled'
      );
    });

    it('renders Studio selector with default options', () => {
      const mockWebview = new MockWebviewView(ChatWebviewViewProvider.viewType).webview;
      const html = ChatHtmlBuilder.buildHtml(mockWebview as unknown as vscode.Webview, extensionUri, {
        connectionState: 'connected',
        selectedStudioId: 'default-studio',
        studios: [
          { id: 'default-studio', name: 'default-studio' },
          { id: 'agent-studios-core', name: 'agent-studios-core' },
        ],
      });

      assert.ok(html.includes('id="studioSelect"'), 'Must render studioSelect');
      assert.ok(html.includes('value="default-studio"'), 'Must include default-studio');
      assert.ok(html.includes('value="agent-studios-core"'), 'Must include agent-studios-core');
    });

    it('renders verified workspace badge when local workspace matches daemon root', () => {
      const mockWebview = new MockWebviewView(ChatWebviewViewProvider.viewType).webview;
      const html = ChatHtmlBuilder.buildHtml(mockWebview as unknown as vscode.Webview, extensionUri, {
        connectionState: 'connected',
        workspacePath: '/projects/agent-studios',
        isWorkspaceMatched: true,
      });

      assert.ok(html.includes('workspace-badge--authorized') || html.includes('workspace-badge--matched'));
      assert.ok(html.includes('✓ Authorized') || html.includes('✓ Verified'));
      assert.ok(html.includes('/projects/agent-studios'));
    });

    it('renders warning badge and mismatch details when workspace differs from daemon root', () => {
      const mockWebview = new MockWebviewView(ChatWebviewViewProvider.viewType).webview;
      const mismatchReason = 'Daemon root is bound to /repo/a but VS Code is in /repo/b';
      const html = ChatHtmlBuilder.buildHtml(mockWebview as unknown as vscode.Webview, extensionUri, {
        connectionState: 'connected',
        workspacePath: '/repo/b',
        isWorkspaceMatched: false,
        workspaceMismatchReason: mismatchReason,
      });

      assert.ok(html.includes('workspace-badge--mismatch') || html.includes('workspace-badge--mismatched'));
      assert.ok(html.includes('⚠ Mismatch'));
      assert.ok(html.includes(mismatchReason));
    });
  });

  describe('2. Execution Configuration Summary Card', () => {
    it('renders runtime, model, workspace, and host confirmation policy in summary card', () => {
      const mockWebview = new MockWebviewView(ChatWebviewViewProvider.viewType).webview;
      const html = ChatHtmlBuilder.buildHtml(mockWebview as unknown as vscode.Webview, extensionUri, {
        connectionState: 'connected',
        selectedRuntime: 'claude_code',
        selectedModelId: 'claude-3-7-sonnet',
        selectedProviderId: 'anthropic',
        workspacePath: '/workspace/agent-studios',
      });

      assert.ok(html.includes('class="execution-summary-card"'));
      assert.ok(html.includes('id="summaryRuntime"'));
      assert.ok(html.includes('id="summaryModel"'));
      assert.ok(html.includes('id="summaryWorkspace"'));
      assert.ok(html.includes('Host-Owned Modal Dialog'));
    });
  });

  describe('3. Wire Framing Boundary & Prompt Composer', () => {
    it('renders warning banner for prompts >100 KB and error banner for >4 MiB', () => {
      const mockWebview = new MockWebviewView(ChatWebviewViewProvider.viewType).webview;
      const html = ChatHtmlBuilder.buildHtml(mockWebview as unknown as vscode.Webview, extensionUri, {
        connectionState: 'connected',
      });

      assert.ok(html.includes('id="composerWarning"'));
      assert.ok(html.includes('Prompt exceeds 100 KB'));
      assert.ok(html.includes('id="composerError"'));
      assert.ok(html.includes('Prompt exceeds 4 MiB wire frame limit'));
    });

    it('renders execution control buttons with keyboard shortcuts', () => {
      const mockWebview = new MockWebviewView(ChatWebviewViewProvider.viewType).webview;
      const html = ChatHtmlBuilder.buildHtml(mockWebview as unknown as vscode.Webview, extensionUri, {
        connectionState: 'connected',
      });

      assert.ok(html.includes('id="sendBtn"'));
      assert.ok(html.includes('▶ Start Task'));
      assert.ok(html.includes('id="stopBtn"'));
      assert.ok(html.includes('■ Cancel Task'));
      assert.ok(html.includes('Ctrl+Enter to submit, Shift+Enter for newline'));
    });
  });

  describe('4. Live Execution Streaming Pane & Truthful Metrics', () => {
    it('renders live streaming console with status badge and elapsed timer', () => {
      const mockWebview = new MockWebviewView(ChatWebviewViewProvider.viewType).webview;
      const html = ChatHtmlBuilder.buildHtml(mockWebview as unknown as vscode.Webview, extensionUri, {
        connectionState: 'connected',
        executionState: {
          status: 'running',
          elapsedSeconds: 42,
          streamOutput: 'Compiling module...\n',
          activeTool: {
            name: 'bash',
            status: 'executing',
            timestamp: Date.now(),
          },
        },
      });

      assert.ok(html.includes('id="executionStreamingPane"'));
      assert.ok(html.includes('id="executionStatusBadge"'));
      assert.ok(html.includes('RUNNING'));
      assert.ok(html.includes('id="executionTimer"'));
      assert.ok(html.includes('00:42'));
      assert.ok(html.includes('id="activeToolBanner"'));
      assert.ok(html.includes('bash'));
      assert.ok(html.includes('(executing)'));
      assert.ok(html.includes('id="streamingConsoleOutput"'));
      assert.ok(html.includes('Compiling module...'));
    });

    it('renders unpopulated notice when metrics are not reported by daemon', () => {
      const metricsHtml = ChatHtmlBuilder.renderMetricsHtml(undefined);
      assert.ok(metricsHtml.includes('Metrics: Pending daemon telemetry'));
      assert.ok(metricsHtml.includes('Truthful reporting: daemon has not emitted token or cost telemetry'));

      const unreportedHtml = ChatHtmlBuilder.renderMetricsHtml({ isReported: false });
      assert.ok(unreportedHtml.includes('Metrics: Pending daemon telemetry'));
    });

    it('renders exact metrics when authoritatively reported by daemon', () => {
      const reportedHtml = ChatHtmlBuilder.renderMetricsHtml({
        isReported: true,
        totalTokens: 2500,
        costUsd: 0.035,
        latencyMs: 1850,
      });

      assert.ok(reportedHtml.includes('Tokens: 2,500'));
      assert.ok(reportedHtml.includes('Cost: $0.0350'));
      assert.ok(reportedHtml.includes('Duration: 1.9s'));
    });
  });

  describe('5. Accessibility & ARIA Compliance', () => {
    it('includes ARIA roles, live regions and accessible labels', () => {
      const mockWebview = new MockWebviewView(ChatWebviewViewProvider.viewType).webview;
      const html = ChatHtmlBuilder.buildHtml(mockWebview as unknown as vscode.Webview, extensionUri, {
        connectionState: 'connected',
      });

      assert.ok(html.includes('role="region" aria-label="Execution Workbench"'));
      assert.ok(html.includes('role="region" aria-label="Live Execution Streaming Output"'));
      assert.ok(html.includes('role="log" aria-live="polite"'));
      assert.ok(html.includes('role="status" aria-live="polite"'));
      assert.ok(html.includes('role="alert" aria-live="assertive"'));
    });
  });

  describe('6. ChatWebviewViewProvider Execution Dispatching', () => {
    it('blocks task submission when task.submit capability is disabled (fail-closed)', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', false);

      const bridge = new FixtureBridge();
      const provider = new ChatWebviewViewProvider(extensionUri, bridge);
      const mockView = new MockWebviewView(ChatWebviewViewProvider.viewType);
      await provider.resolveWebviewView(
        mockView as unknown as vscode.WebviewView,
        {} as vscode.WebviewViewResolveContext,
        {} as vscode.CancellationToken
      );

      const postedMessages = mockView.webview._sentMessages;

      await provider.handleWebviewMessage({
        command: 'submitTask',
        prompt: 'Run test command',
        studioId: 'default-studio',
      });

      const err = postedMessages.find((m) => (m as { type?: string }).type === 'error') as { error?: string } | undefined;
      assert.ok(err, 'Must receive error message');
      assert.ok(err.error?.includes('task.submit capability is not enabled'));
    });

    it('blocks task submission when workspace path mismatches daemon root', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);

      const bridge = new FixtureBridge();
      // Configure bridge session with mismatched root
      bridge.setSessionWorkspacePathForTesting('/daemon/foreign/path');

      const provider = new ChatWebviewViewProvider(extensionUri, bridge);
      const mockView = new MockWebviewView(ChatWebviewViewProvider.viewType);
      await provider.resolveWebviewView(
        mockView as unknown as vscode.WebviewView,
        {} as vscode.WebviewViewResolveContext,
        {} as vscode.CancellationToken
      );

      // Force workspace folder mock
      (vscode.workspace as { workspaceFolders?: readonly vscode.WorkspaceFolder[] }).workspaceFolders = [
        {
          uri: vscode.Uri.file('/local/workbench/path'),
          name: 'workbench',
          index: 0,
        },
      ];

      const postedMessages = mockView.webview._sentMessages;

      await provider.handleWebviewMessage({
        command: 'submitTask',
        prompt: 'Run test command',
        studioId: 'default-studio',
      });

      const err = postedMessages.find((m) => (m as { type?: string }).type === 'error') as { error?: string } | undefined;
      assert.ok(err, 'Must reject on workspace mismatch');
      assert.ok(err.error?.includes('Workspace mismatch'));
    });

    it('submits task when operator confirms modal dialog and capability is enabled', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);

      let modalPrompted = false;
      OperatorConfirmationModal.setDialogHandlerForTesting(async (_req, _detail) => {
        modalPrompted = true;
        return true; // Confirmed
      });

      const bridge = new FixtureBridge();
      bridge.setSessionWorkspacePathForTesting('/test/workspace');

      (vscode.workspace as { workspaceFolders?: readonly vscode.WorkspaceFolder[] }).workspaceFolders = [
        {
          uri: vscode.Uri.file('/test/workspace'),
          name: 'test',
          index: 0,
        },
      ];

      const provider = new ChatWebviewViewProvider(extensionUri, bridge);
      const mockView = new MockWebviewView(ChatWebviewViewProvider.viewType);
      await provider.resolveWebviewView(
        mockView as unknown as vscode.WebviewView,
        {} as vscode.WebviewViewResolveContext,
        {} as vscode.CancellationToken
      );

      const postedMessages = mockView.webview._sentMessages;

      await provider.handleWebviewMessage({
        command: 'submitTask',
        prompt: 'Build new feature in workspace',
        studioId: 'default-studio',
      });

      assert.strictEqual(modalPrompted, true, 'Must prompt operator confirmation modal');

      const submitMsg = postedMessages.find((m) => (m as { type?: string }).type === 'taskSubmitted') as { taskId?: string } | undefined;
      assert.ok(submitMsg, 'Must emit taskSubmitted message');
      assert.ok(submitMsg.taskId, 'Must have valid task ID');

      const execState = TaskExecutionStore.instance.getState();
      assert.strictEqual(execState.status, 'queued');
      assert.strictEqual(execState.activeTaskId, submitMsg.taskId);
    });

    it('cancels task execution cleanly and updates store', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);
      CapabilityMatrix.setCapabilityEnabled('task.cancel', true);

      const bridge = new FixtureBridge();
      const provider = new ChatWebviewViewProvider(extensionUri, bridge);
      const mockView = new MockWebviewView(ChatWebviewViewProvider.viewType);
      await provider.resolveWebviewView(
        mockView as unknown as vscode.WebviewView,
        {} as vscode.WebviewViewResolveContext,
        {} as vscode.CancellationToken
      );

      TaskExecutionStore.instance.taskSubmitted('task-to-cancel-1', 'default-studio');

      const postedMessages = mockView.webview._sentMessages;

      await provider.handleWebviewMessage({
        command: 'cancelTask',
        taskId: 'task-to-cancel-1',
        studioId: 'default-studio',
      });

      const cancelMsg = postedMessages.find((m) => (m as { type?: string }).type === 'taskCancelled') as { taskId?: string } | undefined;
      assert.ok(cancelMsg, 'Must post taskCancelled');
      assert.strictEqual(cancelMsg.taskId, 'task-to-cancel-1');

      const state = TaskExecutionStore.instance.getState();
      assert.strictEqual(state.status, 'cancelled');
    });

    it('fails closed when attempting task cancellation without task.cancel capability', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.cancel', false);

      const bridge = new FixtureBridge();
      const provider = new ChatWebviewViewProvider(extensionUri, bridge);
      const mockView = new MockWebviewView(ChatWebviewViewProvider.viewType);
      await provider.resolveWebviewView(
        mockView as unknown as vscode.WebviewView,
        {} as vscode.WebviewViewResolveContext,
        {} as vscode.CancellationToken
      );

      const postedMessages = mockView.webview._sentMessages;

      await provider.handleWebviewMessage({
        command: 'cancelTask',
        taskId: 'task-123',
      });

      const err = postedMessages.find((m) => (m as { type?: string }).type === 'error') as { error?: string } | undefined;
      assert.ok(err, 'Must return error');
      assert.ok(err.error?.includes('task.cancel capability is not enabled'));
    });

    it('preserves prompt whitespace integrity verbatim and rejects whitespace-only prompts', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);

      let capturedPrompt = '';
      OperatorConfirmationModal.setDialogHandlerForTesting(async (req) => {
        capturedPrompt = req.prompt;
        return true;
      });

      const bridge = new FixtureBridge();
      bridge.setSessionWorkspacePathForTesting('/test/workspace');

      (vscode.workspace as { workspaceFolders?: readonly vscode.WorkspaceFolder[] }).workspaceFolders = [
        {
          uri: vscode.Uri.file('/test/workspace'),
          name: 'test',
          index: 0,
        },
      ];

      const provider = new ChatWebviewViewProvider(extensionUri, bridge);
      const mockView = new MockWebviewView(ChatWebviewViewProvider.viewType);
      await provider.resolveWebviewView(
        mockView as unknown as vscode.WebviewView,
        {} as vscode.WebviewViewResolveContext,
        {} as vscode.CancellationToken
      );

      // 1. Whitespace-only prompt must be rejected
      await provider.handleWebviewMessage({
        command: 'submitTask',
        prompt: '   \n\t  \n  ',
        studioId: 'default-studio',
      });
      const err = mockView.webview._sentMessages.find((m) => (m as { type?: string }).type === 'error') as { error?: string } | undefined;
      assert.ok(err, 'Must reject whitespace-only prompt');
      assert.ok(
        err.error?.includes("Command 'submitTask' requires non-empty string 'prompt'") ||
          err.error?.includes('Prompt cannot be empty.'),
        'Must report non-empty prompt requirement'
      );

      // 2. Multiline indented prompt must preserve indentation and leading/trailing newlines
      const rawPrompt = '  \nfn hello() {\n    println!("hello");\n  }\n  ';
      await provider.handleWebviewMessage({
        command: 'submitTask',
        prompt: rawPrompt,
        studioId: 'default-studio',
      });

      assert.strictEqual(capturedPrompt, rawPrompt, 'Prompt whitespace must be preserved verbatim without destructive trimming');
    });
  });
});
