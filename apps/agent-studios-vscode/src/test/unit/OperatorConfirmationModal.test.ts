/**
 * Agent Studios — Operator Confirmation Modal Unit Tests
 * Milestone M15 UI-01: Interactive Agent Execution Workbench
 */

import * as assert from 'assert';
import {
  OperatorConfirmationModal,
  OperatorConfirmationDialogRequest,
} from '../../security/OperatorConfirmationModal';
import { OperatorConfirmationManager } from '../../bridge/ipc/CapabilityMatrix';

describe('M15 OperatorConfirmationModal Unit Tests', () => {
  beforeEach(() => {
    OperatorConfirmationManager.resetForTesting();
    OperatorConfirmationModal.resetDialogHandlerForTesting();
  });

  afterEach(() => {
    OperatorConfirmationManager.resetForTesting();
    OperatorConfirmationModal.resetDialogHandlerForTesting();
  });

  describe('Confirmation Approval & Challenge Binding', () => {
    it('returns cryptographically bound challenge when operator confirms modal', async () => {
      let capturedDetail = '';
      OperatorConfirmationModal.setDialogHandlerForTesting(async (_req, detail) => {
        capturedDetail = detail;
        return true;
      });

      const req: OperatorConfirmationDialogRequest = {
        studioId: 'studio-987',
        studioName: 'Main Dev Studio',
        runtime: 'Claude Code (Native CLI)',
        providerId: 'anthropic',
        modelId: 'claude-3-5-sonnet',
        workspacePath: 'D:\\Projects\\agent-studios-ide-ui',
        prompt: 'Refactor authentication middleware\nwith unit tests',
      };

      const result = await OperatorConfirmationModal.requestConfirmation(req);

      assert.strictEqual(result.confirmed, true);
      assert.ok(result.challenge);
      assert.strictEqual(result.challenge.studioId, 'studio-987');
      assert.strictEqual(result.challenge.modelId, 'claude-3-5-sonnet');
      assert.strictEqual(result.challenge.providerId, 'anthropic');
      assert.strictEqual(result.challenge.workspacePath, 'D:\\Projects\\agent-studios-ide-ui');
      assert.strictEqual(result.challenge.intent, 'task_execution');

      // Verify challenge signature and parameters in OperatorConfirmationManager
      const verification = OperatorConfirmationManager.verifyChallenge(
        result.challenge,
        {
          studioId: 'studio-987',
          prompt: req.prompt,
          workspacePath: req.workspacePath,
          modelId: req.modelId,
          providerId: req.providerId,
          intent: 'task_execution',
        },
        { consume: false }
      );
      assert.strictEqual(verification.valid, true);

      // Verify detail text content
      assert.ok(capturedDetail.includes('Studio: Main Dev Studio'));
      assert.ok(capturedDetail.includes('Runtime: Claude Code (Native CLI)'));
      assert.ok(capturedDetail.includes('Provider / Model: anthropic / claude-3-5-sonnet'));
      assert.ok(capturedDetail.includes('Workspace: D:\\Projects\\agent-studios-ide-ui'));
      assert.ok(capturedDetail.includes('Prompt Preview: "Refactor authentication middleware with unit tests"'));
    });

    it('truncates prompt preview to 200 characters and flattens newlines', async () => {
      let capturedDetail = '';
      OperatorConfirmationModal.setDialogHandlerForTesting(async (_req, detail) => {
        capturedDetail = detail;
        return true;
      });

      const longPrompt = 'Line 1\nLine 2\n' + 'A'.repeat(250);
      const req: OperatorConfirmationDialogRequest = {
        studioId: 'studio-123',
        runtime: 'Claude Code (Native CLI)',
        prompt: longPrompt,
      };

      const result = await OperatorConfirmationModal.requestConfirmation(req);
      assert.strictEqual(result.confirmed, true);

      // Check truncation
      assert.ok(capturedDetail.includes('...'));
      const previewMatch = capturedDetail.match(/Prompt Preview: "([^"]+)"/);
      assert.ok(previewMatch);
      assert.strictEqual(previewMatch[1].length, 203); // 200 chars + '...'
      assert.ok(!previewMatch[1].includes('\n'));
    });
  });

  describe('Cancellation & Challenge Purging', () => {
    it('purges pending challenge and returns cancelledByUser when operator cancels', async () => {
      OperatorConfirmationModal.setDialogHandlerForTesting(async () => {
        return false; // User clicked 'Cancel'
      });

      const req: OperatorConfirmationDialogRequest = {
        studioId: 'studio-123',
        runtime: 'Claude Code (Native CLI)',
        prompt: 'Mutating command to cancel',
      };

      const result = await OperatorConfirmationModal.requestConfirmation(req);

      assert.strictEqual(result.confirmed, false);
      assert.strictEqual(result.cancelledByUser, true);
      assert.strictEqual(result.challenge, undefined);
      assert.ok(result.reason?.includes('cancelled'));
    });
  });

  describe('Validation & Production Isolation', () => {
    it('fails closed when studioId is missing or empty', async () => {
      const result = await OperatorConfirmationModal.requestConfirmation({
        studioId: '   ',
        runtime: 'Claude Code (Native CLI)',
        prompt: 'Valid prompt',
      });

      assert.strictEqual(result.confirmed, false);
      assert.ok(result.reason?.includes('Studio ID is required'));
    });

    it('fails closed when prompt is missing or empty', async () => {
      const result = await OperatorConfirmationModal.requestConfirmation({
        studioId: 'studio-1',
        runtime: 'Claude Code (Native CLI)',
        prompt: '',
      });

      assert.strictEqual(result.confirmed, false);
      assert.ok(result.reason?.includes('Prompt text is required'));
    });

    it('prohibits test dialog handler injection in production mode', () => {
      const originalEnv = process.env.NODE_ENV;
      try {
        process.env.NODE_ENV = 'production';
        assert.throws(
          () => {
            OperatorConfirmationModal.setDialogHandlerForTesting(async () => true);
          },
          /Test dialog handler cannot be injected in production mode/
        );
      } finally {
        process.env.NODE_ENV = originalEnv;
      }
    });
  });
});
