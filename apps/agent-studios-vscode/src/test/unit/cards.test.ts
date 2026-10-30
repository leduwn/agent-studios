/**
 * Agent Studios — Card Components & Diff Review Unit Tests
 * Milestone M14-UI-01: In-Conversation Activities & Diff Review
 */

import * as assert from 'assert';
import * as vscode from 'vscode';
import { DisconnectedBridge } from '../../bridge/DisconnectedBridge';
import { FixtureBridge } from '../../bridge/FixtureBridge';
import { DiffDocumentProvider } from '../../bridge/DiffDocumentProvider';
import {
  ApprovalCardData,
  CardRenderer,
  ChangedFileEntry,
  ChangedFilesCardData,
  DiffReviewManager,
  normalizeRiskLevel,
  SupportedTool,
  TaskProgressCardData,
  ToolCallCardData,
} from '../../cards';
import {
  executedCommands,
  mockWindowMessages,
  resetMockState,
} from './vscodeMock';

describe('M14-UI-01: Card Components & Diff Review Tests', () => {

  describe('CardRenderer — Monochrome Styling Tokens', () => {
    it('generates monochrome VS Code CSS tokens', () => {
      const styles = CardRenderer.getStyles();
      assert.ok(styles.includes('--vscode-badge-background'));
      assert.ok(styles.includes('--vscode-editorWidget-background'));
      assert.ok(styles.includes('var(--vscode-editorWidget-border'));
      assert.ok(styles.includes('var(--vscode-testing-iconPassed'));
      assert.ok(styles.includes('var(--vscode-testing-iconFailed'));
      assert.ok(styles.includes('var(--vscode-editorWarning-foreground'));
      assert.ok(styles.includes('var(--vscode-progressBar-background'));
      assert.ok(styles.includes('agent-studios-spin'));
    });
  });

  describe('ToolCallCard — Tools, Status Badges & XSS Sanitization', () => {
    const supportedTools: SupportedTool[] = [
      'read_file',
      'edit_file',
      'search_codebase',
      'exec_command',
      'run_tests',
      'apply_patch',
      'review_code',
      'delegate_agent',
    ];

    it('renders each supported tool with appropriate label and metadata', () => {
      for (const tool of supportedTools) {
        const cardData: ToolCallCardData = {
          tool,
          status: 'completed',
          target: `test/target/${tool}.ts`,
          durationMs: 45,
        };

        const html = CardRenderer.renderToolCallCard(cardData);
        assert.ok(html.includes(`data-tool="${tool}"`));
        assert.ok(html.includes(`class="tool-name">${tool}</span>`));
        assert.ok(html.includes(`test/target/${tool}.ts`));
        assert.ok(html.includes('45 ms'));
      }
    });

    it('renders fallback representation for custom or unknown tools', () => {
      const html = CardRenderer.renderToolCallCard({
        tool: 'custom_debugger_tool',
        status: 'completed',
      });
      assert.ok(html.includes('custom_debugger_tool'));
      assert.ok(html.includes('data-tool="custom_debugger_tool"'));
    });

    it('renders running status with animated spinner badge', () => {
      const html = CardRenderer.renderToolCallCard({
        tool: 'exec_command',
        status: 'running',
        target: 'npm test',
      });

      assert.ok(html.includes('status-running'));
      assert.ok(html.includes('spinner'));
      assert.ok(html.includes('Running'));
      assert.ok(!html.includes('&#10003;'));
      assert.ok(!html.includes('&#10005;'));
    });

    it('renders completed status with green check icon badge', () => {
      const html = CardRenderer.renderToolCallCard({
        tool: 'run_tests',
        status: 'completed',
        durationMs: 240,
      });

      assert.ok(html.includes('status-completed'));
      assert.ok(html.includes('&#10003;'));
      assert.ok(html.includes('Completed'));
      assert.ok(html.includes('240 ms'));
    });

    it('renders failed status with red error icon badge', () => {
      const html = CardRenderer.renderToolCallCard({
        tool: 'apply_patch',
        status: 'failed',
        durationMs: 12,
        error: 'Hunk #1 failed at line 42',
      });

      assert.ok(html.includes('status-failed'));
      assert.ok(html.includes('&#10005;'));
      assert.ok(html.includes('Failed'));
      assert.ok(html.includes('Hunk #1 failed at line 42'));
    });

    it('formats duration as X ms and handles 0 ms duration', () => {
      const html0 = CardRenderer.renderToolCallCard({
        tool: 'read_file',
        status: 'completed',
        durationMs: 0,
      });
      assert.ok(html0.includes('0 ms'));

      const html142 = CardRenderer.renderToolCallCard({
        tool: 'read_file',
        status: 'completed',
        durationMs: 142,
      });
      assert.ok(html142.includes('142 ms'));

      const htmlNone = CardRenderer.renderToolCallCard({
        tool: 'read_file',
        status: 'completed',
      });
      assert.ok(!htmlNone.includes('tool-duration'));
    });

    it('renders collapsible summary/output and sanitizes malicious XSS payload', () => {
      const maliciousScript = '<script>alert("pwned")</script><img src="x" onerror="evil()" />';
      const html = CardRenderer.renderToolCallCard({
        tool: 'exec_command',
        status: 'failed',
        target: '<evil & unsafe>',
        summary: 'Failed with <unsafe> tag',
        output: maliciousScript,
      });

      // HTML tags must NOT appear unescaped
      assert.ok(!html.includes('<script>'));
      assert.ok(!html.includes('</script>'));
      assert.ok(!html.includes('<img src="x"'));

      // Escaped entities must be present
      assert.ok(html.includes('&lt;script&gt;alert(&quot;pwned&quot;)&lt;/script&gt;'));
      assert.ok(html.includes('&lt;img src=&quot;x&quot; onerror=&quot;evil()&quot; /&gt;'));
      assert.ok(html.includes('&lt;evil &amp; unsafe&gt;'));
      assert.ok(html.includes('&lt;unsafe&gt;'));

      // Collapsible details and summary must be present
      assert.ok(html.includes('<details class="tool-output-details">'));
      assert.ok(html.includes('<summary class="tool-output-summary">'));
    });
  });

  describe('ApprovalCard — States, Risk Badges, Modal Warning & Fail-Closed', () => {
    let bridge: FixtureBridge;

    beforeEach(() => {
      resetMockState();
      bridge = new FixtureBridge();
    });

    afterEach(() => {
      bridge.dispose();
      resetMockState();
    });

    it('normalizes risk levels correctly', () => {
      assert.strictEqual(normalizeRiskLevel('low'), 'LOW');
      assert.strictEqual(normalizeRiskLevel('LOW'), 'LOW');
      assert.strictEqual(normalizeRiskLevel('medium'), 'MEDIUM');
      assert.strictEqual(normalizeRiskLevel('MEDIUM'), 'MEDIUM');
      assert.strictEqual(normalizeRiskLevel('high'), 'HIGH');
      assert.strictEqual(normalizeRiskLevel('HIGH'), 'HIGH');
      assert.strictEqual(normalizeRiskLevel('critical'), 'CRITICAL');
      assert.strictEqual(normalizeRiskLevel('CRITICAL'), 'CRITICAL');
    });

    it('renders risk badges for all 4 classifications (LOW, MEDIUM, HIGH, CRITICAL)', () => {
      const lowHtml = CardRenderer.renderApprovalCard({
        id: 'app-low',
        status: 'pending',
        riskLevel: 'LOW',
        commandOrTarget: 'cat config.json',
      });
      assert.ok(lowHtml.includes('badge-risk-low'));
      assert.ok(lowHtml.includes('>LOW<'));

      const medHtml = CardRenderer.renderApprovalCard({
        id: 'app-med',
        status: 'pending',
        riskLevel: 'MEDIUM',
        commandOrTarget: 'npm install lodash',
      });
      assert.ok(medHtml.includes('badge-risk-medium'));
      assert.ok(medHtml.includes('>MEDIUM<'));

      const highHtml = CardRenderer.renderApprovalCard({
        id: 'app-high',
        status: 'pending',
        riskLevel: 'HIGH',
        commandOrTarget: 'git push --force',
      });
      assert.ok(highHtml.includes('badge-risk-high'));
      assert.ok(highHtml.includes('>HIGH<'));

      const critHtml = CardRenderer.renderApprovalCard({
        id: 'app-crit',
        status: 'pending',
        riskLevel: 'CRITICAL',
        commandOrTarget: 'rm -rf /production/data',
      });
      assert.ok(critHtml.includes('badge-risk-critical'));
      assert.ok(critHtml.includes('>CRITICAL<'));
    });

    it('renders [Approve] and [Deny] buttons only for pending state', () => {
      const pendingHtml = CardRenderer.renderApprovalCard({
        id: 'app-001',
        status: 'pending',
        riskLevel: 'low',
        commandOrTarget: 'ls -la',
      });
      assert.ok(pendingHtml.includes('class="btn btn-approve"'));
      assert.ok(pendingHtml.includes('class="btn btn-deny"'));
      assert.ok(pendingHtml.includes('data-approval-id="app-001"'));

      const approvedHtml = CardRenderer.renderApprovalCard({
        id: 'app-001',
        status: 'approved',
        riskLevel: 'low',
        commandOrTarget: 'ls -la',
      });
      assert.ok(!approvedHtml.includes('class="btn btn-approve"'));
      assert.ok(approvedHtml.includes('badge-approved'));
      assert.ok(approvedHtml.includes('Approved'));

      const deniedHtml = CardRenderer.renderApprovalCard({
        id: 'app-001',
        status: 'denied',
        riskLevel: 'low',
        commandOrTarget: 'ls -la',
      });
      assert.ok(!deniedHtml.includes('class="btn btn-deny"'));
      assert.ok(deniedHtml.includes('badge-denied'));
      assert.ok(deniedHtml.includes('Denied'));

      const expiredHtml = CardRenderer.renderApprovalCard({
        id: 'app-001',
        status: 'expired',
        riskLevel: 'low',
        commandOrTarget: 'ls -la',
      });
      assert.ok(!expiredHtml.includes('class="btn btn-approve"'));
      assert.ok(expiredHtml.includes('badge-expired'));
      assert.ok(expiredHtml.includes('Expired'));
    });

    it('approves LOW and MEDIUM risk actions directly without modal warning', async () => {
      const approval: ApprovalCardData = {
        id: 'approval-m14-001',
        status: 'pending',
        riskLevel: 'medium',
        commandOrTarget: 'npm install @vscode/test-electron',
      };

      const result = await CardRenderer.handleApprovalAction(approval, 'approved', bridge);
      assert.strictEqual(result.applied, true);
      assert.strictEqual(result.decision, 'approved');
      assert.strictEqual(mockWindowMessages.warning.length, 0);
    });

    it('prompts modal warning for HIGH and CRITICAL risks before approving', async () => {
      mockWindowMessages.modalWarningResponse = 'Confirm Approve';

      const highApproval: ApprovalCardData = {
        id: 'approval-m14-001',
        status: 'pending',
        riskLevel: 'HIGH',
        commandOrTarget: 'drop database test',
      };

      const result = await CardRenderer.handleApprovalAction(highApproval, 'approved', bridge);
      assert.strictEqual(result.applied, true);
      assert.strictEqual(mockWindowMessages.warning.length, 1);
      assert.ok(mockWindowMessages.warning[0].includes('Approve HIGH-risk action'));
    });

    it('aborts approval when modal confirmation is cancelled', async () => {
      mockWindowMessages.modalWarningResponse = 'Cancel';

      const critApproval: ApprovalCardData = {
        id: 'approval-m14-001',
        status: 'pending',
        riskLevel: 'CRITICAL',
        commandOrTarget: 'rm -rf /',
      };

      const result = await CardRenderer.handleApprovalAction(critApproval, 'approved', bridge);
      assert.strictEqual(result.applied, false);
      assert.ok(result.message?.includes('cancelled by user'));
    });

    it('denies actions without modal prompt', async () => {
      const approval: ApprovalCardData = {
        id: 'approval-m14-001',
        status: 'pending',
        riskLevel: 'CRITICAL',
        commandOrTarget: 'rm -rf /',
      };

      const result = await CardRenderer.handleApprovalAction(approval, 'denied', bridge);
      assert.strictEqual(result.applied, true);
      assert.strictEqual(result.decision, 'denied');
      assert.strictEqual(mockWindowMessages.warning.length, 0);
    });

    it('fails closed when approving while bridge is disconnected', async () => {
      const disconnected = new DisconnectedBridge();
      const approval: ApprovalCardData = {
        id: 'approval-m14-001',
        status: 'pending',
        riskLevel: 'LOW',
        commandOrTarget: 'echo ok',
      };

      const result = await CardRenderer.handleApprovalAction(approval, 'approved', disconnected);
      assert.strictEqual(result.applied, false);
      assert.ok(result.message?.includes('disconnected'));
      assert.ok(mockWindowMessages.error.some(msg => msg.includes('disconnected')));
    });

    it('fails closed when denying while bridge is disconnected', async () => {
      const disconnected = new DisconnectedBridge();
      const approval: ApprovalCardData = {
        id: 'approval-m14-001',
        status: 'pending',
        riskLevel: 'LOW',
        commandOrTarget: 'echo ok',
      };

      const result = await CardRenderer.handleApprovalAction(approval, 'denied', disconnected);
      assert.strictEqual(result.applied, false);
      assert.ok(result.message?.includes('disconnected'));
      assert.ok(mockWindowMessages.error.some(msg => msg.includes('disconnected')));
    });
  });

  describe('TaskProgressCard — Active Agent, Progress & DAG Trigger', () => {
    it('displays coordinator and active agent progress with task count and bar', () => {
      const data: TaskProgressCardData = {
        coordinatorName: 'Agent Studios Coordinator',
        activeAgentName: 'Coder Agent',
        completedTasks: 3,
        totalTasks: 5,
      };

      const html = CardRenderer.renderTaskProgressCard(data);
      assert.ok(html.includes('Agent Studios Coordinator'));
      assert.ok(html.includes('Coder Agent'));
      assert.ok(html.includes('3/5 complete'));
      assert.ok(html.includes('width: 60%;'));
      assert.ok(html.includes('60%'));
      assert.ok(html.includes('data-command="agent-studios.showTaskGraph"'));
      assert.ok(html.includes('Show Task Graph'));
    });

    it('handles zero total tasks and 100% completion cleanly', () => {
      const zeroHtml = CardRenderer.renderTaskProgressCard({
        completedTasks: 0,
        totalTasks: 0,
      });
      assert.ok(zeroHtml.includes('0/0 complete'));
      assert.ok(zeroHtml.includes('0%'));

      const completeHtml = CardRenderer.renderTaskProgressCard({
        completedTasks: 10,
        totalTasks: 10,
      });
      assert.ok(completeHtml.includes('10/10 complete'));
      assert.ok(completeHtml.includes('100%'));
      assert.ok(completeHtml.includes('width: 100%;'));
    });
  });

  describe('ChangedFilesCard — Additions/Deletions & Diff Inspection Trigger', () => {
    it('summarizes modified files with +X and -Y counts and diff trigger button', () => {
      const cardData: ChangedFilesCardData = {
        title: 'Session Artifact Diff',
        files: [
          { path: 'src/index.ts', additions: 15, deletions: 3, artifactId: 'artifact-diff-001' },
          { path: 'package.json', additions: 2, deletions: 1, artifactId: 'artifact-diff-001' },
        ],
      };

      const html = CardRenderer.renderChangedFilesCard(cardData);
      assert.ok(html.includes('Session Artifact Diff'));
      assert.ok(html.includes('src/index.ts'));
      assert.ok(html.includes('package.json'));
      assert.ok(html.includes('+17'));
      assert.ok(html.includes('-4'));
      assert.ok(html.includes('+15'));
      assert.ok(html.includes('-3'));
      assert.ok(html.includes('+2'));
      assert.ok(html.includes('-1'));
      assert.ok(html.includes('data-command="agent-studios.inspectArtifact"'));
      assert.ok(html.includes('data-artifact-id="artifact-diff-001"'));
    });

    it('escapes dangerous file paths safely in HTML output', () => {
      const cardData: ChangedFilesCardData = {
        files: [
          { path: 'src/<script>bad</script>.ts', additions: 1, deletions: 0 },
        ],
      };

      const html = CardRenderer.renderChangedFilesCard(cardData);
      assert.ok(!html.includes('<script>'));
      assert.ok(html.includes('src/&lt;script&gt;bad&lt;/script&gt;.ts'));
    });
  });

  describe('DiffReviewManager — Native Diff Inspection & Safe Review', () => {
    let bridge: FixtureBridge;
    let diffProvider: DiffDocumentProvider;
    let reviewManager: DiffReviewManager;

    beforeEach(() => {
      resetMockState();
      bridge = new FixtureBridge();
      diffProvider = new DiffDocumentProvider(bridge);
      reviewManager = new DiffReviewManager(bridge, diffProvider);
    });

    afterEach(() => {
      reviewManager.dispose();
      diffProvider.dispose();
      bridge.dispose();
      resetMockState();
    });

    it('parses unified diff into structured ChangedFileEntry records with additions/deletions', () => {
      const unifiedDiff = [
        '--- a/src/cards/CardRenderer.ts',
        '+++ b/src/cards/CardRenderer.ts',
        '@@ -10,3 +10,5 @@',
        ' context',
        '-old line',
        '+new line 1',
        '+new line 2',
        '--- a/src/cards/DiffReviewManager.ts',
        '+++ b/src/cards/DiffReviewManager.ts',
        '@@ -1,4 +1,2 @@',
        '-delete line 1',
        '-delete line 2',
        ' unchanged',
      ].join('\n');

      const parsed = reviewManager.parseDiffStats(unifiedDiff, 'art-123');
      assert.strictEqual(parsed.length, 2);

      assert.strictEqual(parsed[0].path, 'src/cards/CardRenderer.ts');
      assert.strictEqual(parsed[0].additions, 2);
      assert.strictEqual(parsed[0].deletions, 1);
      assert.strictEqual(parsed[0].artifactId, 'art-123');

      assert.strictEqual(parsed[1].path, 'src/cards/DiffReviewManager.ts');
      assert.strictEqual(parsed[1].additions, 0);
      assert.strictEqual(parsed[1].deletions, 2);
    });

    it('returns empty array when diff content is empty or blank', () => {
      const parsed = reviewManager.parseDiffStats('');
      assert.deepStrictEqual(parsed, []);
    });

    it('opens native diff view via openDiff with virtual agent-studios-diff URI', async () => {
      const fileEntry: ChangedFileEntry = {
        path: 'package.json',
        additions: 37,
        deletions: 0,
        artifactId: 'artifact-diff-001',
      };

      await reviewManager.openDiff(fileEntry);

      // Verify vscode.diff or openTextDocument command was executed
      const diffCommand = executedCommands.find(c => c.command === 'vscode.diff');
      assert.ok(diffCommand !== undefined);
      assert.strictEqual(diffCommand.command, 'vscode.diff');
      assert.strictEqual(diffCommand.args.length, 3);

      const virtualUri = diffCommand.args[1] as vscode.Uri;
      assert.strictEqual(virtualUri.scheme, 'agent-studios-diff');
      assert.ok(virtualUri.path.includes('artifact-diff-001'));
    });

    it('opens diff by string artifact identifier', async () => {
      await reviewManager.openDiff('artifact-diff-002');

      const diffCommand = executedCommands.find(c => c.command === 'vscode.diff');
      assert.ok(diffCommand !== undefined);
      const virtualUri = diffCommand.args[1] as vscode.Uri;
      assert.strictEqual(virtualUri.scheme, 'agent-studios-diff');
      assert.ok(virtualUri.path.includes('artifact-diff-002'));
    });

    it('SAFE REVIEW: strictly rejects unverified workspace file mutations on disk', async () => {
      // 1. Without proof
      const resNoProof = await reviewManager.reconcileWorkspaceFile('src/main.ts', 'code');
      assert.strictEqual(resNoProof.success, false);
      assert.strictEqual(resNoProof.reconciled, false);
      assert.ok(resNoProof.message.includes('rejected'));
      assert.ok(mockWindowMessages.error.some(msg => msg.includes('Safe review policy violation')));

      // 2. With unverified proof
      const resUnverified = await reviewManager.reconcileWorkspaceFile('src/main.ts', 'code', {
        verified: false,
        reconciliationId: 'fake-id',
      });
      assert.strictEqual(resUnverified.success, false);
      assert.strictEqual(resUnverified.reconciled, false);

      // 3. When bridge is disconnected
      const disconnectedReview = new DiffReviewManager(new DisconnectedBridge());
      const resDisconnected = await disconnectedReview.reconcileWorkspaceFile('src/main.ts', 'code', {
        verified: true,
        reconciliationId: 'verified-id',
      });
      assert.strictEqual(resDisconnected.success, false);
      assert.ok(resDisconnected.message.includes('disconnected'));
      disconnectedReview.dispose();

      // 4. With verified proof and connected bridge
      const resVerified = await reviewManager.reconcileWorkspaceFile('src/main.ts', 'code', {
        verified: true,
        reconciliationId: 'txn-recon-001',
      });
      assert.strictEqual(resVerified.success, true);
      assert.strictEqual(resVerified.reconciled, true);
    });

    it('dispatches card message commands correctly in handleCardAction', async () => {
      // openDiff action
      await reviewManager.handleCardAction({
        command: 'agent-studios.inspectArtifact',
        payload: 'artifact-diff-001',
      });
      assert.ok(executedCommands.some(c => c.command === 'vscode.diff'));

      // showTaskGraph action
      await reviewManager.handleCardAction({
        command: 'agent-studios.showTaskGraph',
      });
      assert.ok(executedCommands.some(c => c.command === 'agent-studios.showTaskGraph'));

      // approve action
      const approveRes = await reviewManager.handleCardAction({
        command: 'agent-studios.approveAction',
        payload: {
          id: 'approval-m14-001',
          status: 'pending',
          riskLevel: 'LOW',
          commandOrTarget: 'npm test',
        },
      }) as { applied: boolean };
      assert.strictEqual(approveRes.applied, true);

      // deny action
      const denyRes = await reviewManager.handleCardAction({
        command: 'agent-studios.denyAction',
        payload: {
          id: 'approval-m14-001',
          status: 'pending',
          riskLevel: 'LOW',
          commandOrTarget: 'npm test',
        },
      }) as { applied: boolean };
      assert.strictEqual(denyRes.applied, true);
    });
  });
});
