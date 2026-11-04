/**
 * Agent Studios — Operator Confirmation Safety & Fail-Closed Unit Tests
 * Milestone M15 UI-05A: UX & Operator Safety Hardening
 *
 * Verifies strict Phase D security invariants:
 * 1. Single-use challenge token consumption (consumed on attempt, replay prevented).
 * 2. TTL expiration rejection (expired challenge tokens fail closed).
 * 3. Studio/prompt tampering rejection (bound studioId and promptHash cannot be altered).
 * 4. Signature tampering rejection (forged or altered HMAC fails closed with TaskSubmissionError).
 * 5. Capability disabled gating (task.submit capability remains strictly gated).
 * 6. Readiness report fallback on null, degraded, duplicate, or unverified runtime adapters.
 * 7. Accessibility and operator controls (modal focus trapping, Esc cancellation, ARIA announcements).
 */

import * as assert from 'assert';
import * as path from 'path';
import * as os from 'os';
import * as fs from 'fs';
import {
  CapabilityMatrix,
  OperatorConfirmationManager,
  OperatorConfirmationChallenge,
} from '../../bridge/ipc/CapabilityMatrix';
import {
  SubmitTaskAdapter,
  SubmitTaskRequest,
  TaskSubmissionError,
} from '../../bridge/ipc/SubmitTaskAdapter';
import { ExecutionClient } from '../../bridge/ipc/ExecutionClient';
import {
  RustExecutionReadinessReport,
  RustIpcCommand,
  RustIpcResponse,
} from '../../bridge/ipc/RustIpcProtocolTypes';
import {
  OperatorConfirmationModal,
  OperatorConfirmationDialogRequest,
} from '../../security/OperatorConfirmationModal';
import { ChatHtmlBuilder } from '../../chat/ChatHtmlBuilder';

class MockTransport {
  public sentCommands: RustIpcCommand[] = [];
  /* eslint-disable @typescript-eslint/naming-convention */
  public responseGenerator: (_cmd: RustIpcCommand) => RustIpcResponse = () => ({
    type: 'tasks_submitted',
    task_ids: ['3fa85f64-5717-4562-b3fc-2c963f66afa6'],
  });
  /* eslint-enable @typescript-eslint/naming-convention */

  public async sendCommand(cmd: RustIpcCommand): Promise<RustIpcResponse> {
    this.sentCommands.push(cmd);
    return this.responseGenerator(cmd);
  }
}

describe('M15 UI-05A: OperatorConfirmationSafety Unit Tests', () => {
  const validStudioId = '3fa85f64-5717-4562-b3fc-2c963f66afa6';
  const validPrompt = 'Refactor authentication middleware with fail-closed security invariants';
  let transport: MockTransport;
  let tempDir: string;
  let testWorkspace: string;

  beforeEach(() => {
    transport = new MockTransport();
    CapabilityMatrix.resetCapabilities();
    OperatorConfirmationManager.resetForTesting();
    OperatorConfirmationModal.resetDialogHandlerForTesting();
    tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'ag-safety-test-'));
    testWorkspace = fs.realpathSync(tempDir);
  });

  afterEach(() => {
    CapabilityMatrix.resetCapabilities();
    OperatorConfirmationManager.resetForTesting();
    OperatorConfirmationModal.resetDialogHandlerForTesting();
    if (tempDir && fs.existsSync(tempDir)) {
      try {
        fs.rmSync(tempDir, { recursive: true, force: true });
      } catch {
        // cleanup ignore
      }
    }
  });

  // =========================================================================
  // 1. Single-Use Challenge Token Consumption
  // =========================================================================
  describe('1. Single-Use Challenge Token Consumption', () => {
    it('consumes challenge token on successful verification and prevents replay', () => {
      const challenge = OperatorConfirmationManager.createChallenge(validStudioId, validPrompt, {
        workspacePath: testWorkspace,
      });

      // First verification with consumption enabled should succeed
      const firstResult = OperatorConfirmationManager.verifyAndConsumeChallenge(
        challenge,
        {
          studioId: validStudioId,
          prompt: validPrompt,
          workspacePath: testWorkspace,
        },
        { consume: true }
      );
      assert.strictEqual(firstResult.valid, true);

      // Second verification attempt with same token must fail due to replay prevention
      const secondResult = OperatorConfirmationManager.verifyAndConsumeChallenge(
        challenge,
        {
          studioId: validStudioId,
          prompt: validPrompt,
          workspacePath: testWorkspace,
        },
        { consume: true }
      );
      assert.strictEqual(secondResult.valid, false);
      assert.ok(
        secondResult.reason?.includes('already been consumed') ||
        secondResult.reason?.includes('replay prevented'),
        `Unexpected failure reason: ${secondResult.reason}`
      );
    });

    it('consumes challenge token on attempt even when parameter verification fails', () => {
      const challenge = OperatorConfirmationManager.createChallenge(validStudioId, validPrompt);

      // Attempt verification with tampered prompt
      const tamperedAttempt = OperatorConfirmationManager.verifyAndConsumeChallenge(
        challenge,
        {
          studioId: validStudioId,
          prompt: 'Tampered malicious prompt attempt',
        },
        { consume: true }
      );
      assert.strictEqual(tamperedAttempt.valid, false);

      // Subsequent attempt with original prompt must now fail because token was consumed on attempt
      const retryWithOriginal = OperatorConfirmationManager.verifyAndConsumeChallenge(
        challenge,
        {
          studioId: validStudioId,
          prompt: validPrompt,
        },
        { consume: true }
      );
      assert.strictEqual(retryWithOriginal.valid, false);
      assert.ok(
        retryWithOriginal.reason?.includes('already been consumed') ||
        retryWithOriginal.reason?.includes('replay prevented'),
        `Expected token to be consumed on attempt, got: ${retryWithOriginal.reason}`
      );
    });

    it('distinguishes read-only inspection (consume: false) from execution consumption (consume: true)', () => {
      const challenge = OperatorConfirmationManager.createChallenge(validStudioId, validPrompt);

      // Read-only inspection (consume: false)
      const inspect1 = OperatorConfirmationManager.verifyChallenge(challenge, {
        studioId: validStudioId,
        prompt: validPrompt,
      });
      assert.strictEqual(inspect1.valid, true);

      const inspect2 = OperatorConfirmationManager.verifyChallenge(challenge, {
        studioId: validStudioId,
        prompt: validPrompt,
      });
      assert.strictEqual(inspect2.valid, true);

      // Now consume for execution
      const consumeResult = OperatorConfirmationManager.verifyAndConsumeChallenge(challenge, {
        studioId: validStudioId,
        prompt: validPrompt,
      });
      assert.strictEqual(consumeResult.valid, true);

      // Any subsequent inspection or consumption must fail
      const inspect3 = OperatorConfirmationManager.verifyChallenge(challenge, {
        studioId: validStudioId,
        prompt: validPrompt,
      });
      assert.strictEqual(inspect3.valid, false);
      assert.ok(inspect3.reason?.includes('already been consumed'));
    });
  });

  // =========================================================================
  // 2. TTL Expiration Rejection
  // =========================================================================
  describe('2. TTL Expiration Rejection', () => {
    it('rejects challenge token when TTL has expired', async () => {
      // Create challenge with 20ms TTL
      const challenge = OperatorConfirmationManager.createChallenge(
        validStudioId,
        validPrompt,
        {
          ttlMs: 20,
        }
      );

      // Wait 30ms for TTL expiration
      await new Promise((resolve) => setTimeout(resolve, 30));

      const result = OperatorConfirmationManager.verifyAndConsumeChallenge(challenge, {
        studioId: validStudioId,
        prompt: validPrompt,
      });
      assert.strictEqual(result.valid, false);
      assert.ok(
        result.reason?.includes('expired') || result.reason?.includes('TTL'),
        `Unexpected failure reason: ${result.reason}`
      );
    });

    it('fails closed in SubmitTaskAdapter when challenge has expired', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);

      const challenge = OperatorConfirmationManager.createChallenge(
        validStudioId,
        validPrompt,
        {
          ttlMs: 20,
        }
      );

      await new Promise((resolve) => setTimeout(resolve, 30));

      const req: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt: validPrompt,
        confirmedByUser: true,
        confirmationChallenge: challenge,
      };

      await assert.rejects(
        async () => {
          await SubmitTaskAdapter.submitTask(transport, req);
        },
        (err: unknown) => {
          assert.ok(err instanceof TaskSubmissionError);
          assert.ok(
            (err as TaskSubmissionError).message.includes('expired') ||
            (err as TaskSubmissionError).message.includes('challenge verification failed')
          );
          return true;
        }
      );
    });
  });

  // =========================================================================
  // 3. Studio/Prompt Tampering Rejection
  // =========================================================================
  describe('3. Studio and Prompt Tampering Rejection', () => {
    it('rejects challenge token when studio ID does not match bound identity', () => {
      const challenge = OperatorConfirmationManager.createChallenge(validStudioId, validPrompt);
      const forgedStudioId = 'e1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d';

      const result = OperatorConfirmationManager.verifyAndConsumeChallenge(challenge, {
        studioId: forgedStudioId,
        prompt: validPrompt,
      });
      assert.strictEqual(result.valid, false);
      assert.ok(
        result.reason?.includes('Studio ID mismatch') ||
        result.reason?.includes('tampered'),
        `Unexpected failure reason: ${result.reason}`
      );
    });

    it('rejects challenge token when prompt hash does not match bound prompt', () => {
      const challenge = OperatorConfirmationManager.createChallenge(validStudioId, validPrompt);
      const alteredPrompt = 'Altered malicious prompt to run rm -rf /';

      const result = OperatorConfirmationManager.verifyAndConsumeChallenge(challenge, {
        studioId: validStudioId,
        prompt: alteredPrompt,
      });
      assert.strictEqual(result.valid, false);
      assert.ok(
        result.reason?.includes('Prompt hash mismatch') ||
        result.reason?.includes('tampered'),
        `Unexpected failure reason: ${result.reason}`
      );
    });

    it('fails closed with TaskSubmissionError when tampered studio ID is submitted', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);

      const challenge = OperatorConfirmationManager.createChallenge(validStudioId, validPrompt);
      const tamperedStudioId = 'a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d';

      const req: SubmitTaskRequest = {
        studioId: tamperedStudioId,
        prompt: validPrompt,
        confirmedByUser: true,
        confirmationChallenge: challenge,
      };

      await assert.rejects(
        async () => {
          await SubmitTaskAdapter.submitTask(transport, req);
        },
        (err: unknown) => {
          assert.ok(err instanceof TaskSubmissionError);
          assert.strictEqual((err as TaskSubmissionError).studioId, tamperedStudioId);
          assert.ok(
            (err as TaskSubmissionError).message.includes('Studio ID mismatch') ||
            (err as TaskSubmissionError).message.includes('challenge verification failed')
          );
          return true;
        }
      );
    });

    it('fails closed with TaskSubmissionError when tampered prompt is submitted', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);

      const challenge = OperatorConfirmationManager.createChallenge(validStudioId, validPrompt);
      const tamperedPrompt = 'Malicious code injection attempt';

      const req: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt: tamperedPrompt,
        confirmedByUser: true,
        confirmationChallenge: challenge,
      };

      await assert.rejects(
        async () => {
          await SubmitTaskAdapter.submitTask(transport, req);
        },
        (err: unknown) => {
          assert.ok(err instanceof TaskSubmissionError);
          assert.ok(
            (err as TaskSubmissionError).message.includes('Prompt hash mismatch') ||
            (err as TaskSubmissionError).message.includes('challenge verification failed')
          );
          return true;
        }
      );
    });
  });

  // =========================================================================
  // 4. Signature Tampering Rejection
  // =========================================================================
  describe('4. Signature Tampering Rejection', () => {
    it('rejects challenge token with tampered HMAC signature', () => {
      const challenge = OperatorConfirmationManager.createChallenge(validStudioId, validPrompt);

      // Invert characters of signature
      const tamperedSig = challenge.signature.slice(0, -1) + (challenge.signature.slice(-1) === 'a' ? 'b' : 'a');
      const forgedChallenge: OperatorConfirmationChallenge = {
        ...challenge,
        signature: tamperedSig,
      };

      const result = OperatorConfirmationManager.verifyAndConsumeChallenge(forgedChallenge, {
        studioId: validStudioId,
        prompt: validPrompt,
      });
      assert.strictEqual(result.valid, false);
      assert.ok(
        result.reason?.includes('invalid cryptographic signature') ||
        result.reason?.includes('Forged'),
        `Unexpected failure reason: ${result.reason}`
      );
    });

    it('burns registry challenge entry when forged signature is detected on attempt', () => {
      const challenge = OperatorConfirmationManager.createChallenge(validStudioId, validPrompt);
      const tamperedSig = challenge.signature.slice(0, -1) + (challenge.signature.slice(-1) === '0' ? '1' : '0');
      const forgedChallenge: OperatorConfirmationChallenge = {
        ...challenge,
        signature: tamperedSig,
      };

      // Attempt forged verification with consumption enabled
      const forgedResult = OperatorConfirmationManager.verifyAndConsumeChallenge(
        forgedChallenge,
        {
          studioId: validStudioId,
          prompt: validPrompt,
        },
        { consume: true }
      );
      assert.strictEqual(forgedResult.valid, false);

      // Original challenge token must now also be rejected
      const retryResult = OperatorConfirmationManager.verifyAndConsumeChallenge(challenge, {
        studioId: validStudioId,
        prompt: validPrompt,
      });
      assert.strictEqual(retryResult.valid, false);
    });

    it('fails closed with TaskSubmissionError when forged signature is submitted', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);

      const challenge = OperatorConfirmationManager.createChallenge(validStudioId, validPrompt);
      const tamperedSig = 'deadbeef' + challenge.signature.substring(8);
      const forgedChallenge: OperatorConfirmationChallenge = {
        ...challenge,
        signature: tamperedSig,
      };

      const req: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt: validPrompt,
        confirmedByUser: true,
        confirmationChallenge: forgedChallenge,
      };

      await assert.rejects(
        async () => {
          await SubmitTaskAdapter.submitTask(transport, req);
        },
        (err: unknown) => {
          assert.ok(err instanceof TaskSubmissionError);
          assert.ok(
            (err as TaskSubmissionError).message.includes('invalid cryptographic signature') ||
            (err as TaskSubmissionError).message.includes('challenge verification failed')
          );
          return true;
        }
      );
    });
  });

  // =========================================================================
  // 5. Capability Disabled Gating
  // =========================================================================
  describe('5. Capability Disabled Gating', () => {
    it('verifies task.submit capability is statically disabled by default', () => {
      assert.strictEqual(CapabilityMatrix.isCapabilityEnabled('task.submit'), false);
    });

    it('rejects task submission via ExecutionClient when task.submit capability is disabled', async () => {
      assert.strictEqual(CapabilityMatrix.isCapabilityEnabled('task.submit'), false);

      const challenge = OperatorConfirmationManager.createChallenge(validStudioId, validPrompt);

      const req: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt: validPrompt,
        confirmedByUser: true,
        confirmationChallenge: challenge,
      };

      await assert.rejects(
        async () => {
          await ExecutionClient.submitTask(transport, req);
        },
        (err: unknown) => {
          assert.ok(err instanceof TaskSubmissionError);
          assert.ok((err as TaskSubmissionError).message.includes('task.submit capability missing'));
          return true;
        }
      );
    });

    it('rejects task submission via SubmitTaskAdapter when task.submit capability is disabled', async () => {
      assert.strictEqual(CapabilityMatrix.isCapabilityEnabled('task.submit'), false);

      const challenge = OperatorConfirmationManager.createChallenge(validStudioId, validPrompt);

      const req: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt: validPrompt,
        confirmedByUser: true,
        confirmationChallenge: challenge,
      };

      await assert.rejects(
        async () => {
          await SubmitTaskAdapter.submitTask(transport, req);
        },
        (err: unknown) => {
          assert.ok(err instanceof TaskSubmissionError);
          assert.ok(
            (err as TaskSubmissionError).message.includes("capability 'task.submit' is not enabled or authorized")
          );
          return true;
        }
      );
    });
  });

  // =========================================================================
  // 6. Readiness Report Fallback on Null / Degraded / Unverified
  // =========================================================================
  describe('6. Readiness Report Fallback on Null/Degraded', () => {
    it('fails closed when readiness report is null or undefined for runtime instance selection', () => {
      const result = CapabilityMatrix.validateRuntimeInstanceSelection(
        null as unknown as RustExecutionReadinessReport,
        { runtimePreference: 'claude_code' }
      );
      assert.strictEqual(result.valid, false);
      assert.ok(result.reason?.includes('readiness report is unavailable') || result.reason?.includes('required'));
    });

    it('fails closed when selected runtime instance is degraded or unhealthy', () => {
      /* eslint-disable @typescript-eslint/naming-convention */
      const degradedReadiness: RustExecutionReadinessReport = {
        is_ready: false,
        available_runtimes: ['mock', 'claude_code'],
        runtime_instances: [
          {
            instance_id: 'inst-1',
            implementation: 'claude_code',
            healthy: false,
          },
        ],
        available_models: [],
        authorized_workspaces: [],
        defaults: {},
        blocking_reasons: ['Daemon process crashed or unresponsive'],
      };
      /* eslint-enable @typescript-eslint/naming-convention */

      const result = CapabilityMatrix.validateRuntimeInstanceSelection(
        degradedReadiness,
        { runtimeInstanceId: 'inst-1' }
      );
      assert.strictEqual(result.valid, false);
      assert.ok(result.reason?.includes('degraded or unhealthy'));
    });

    it('fails closed when native claude_code adapter is absent from available_runtimes', () => {
      /* eslint-disable @typescript-eslint/naming-convention */
      const unverifiedReadiness: RustExecutionReadinessReport = {
        is_ready: true,
        available_runtimes: ['mock'], // claude_code absent
        runtime_instances: [
          {
            instance_id: 'inst-claude',
            implementation: 'claude_code',
            healthy: true,
          },
        ],
        available_models: [],
        authorized_workspaces: [],
        defaults: {},
        blocking_reasons: [],
      };
      /* eslint-enable @typescript-eslint/naming-convention */

      const result = CapabilityMatrix.validateRuntimeInstanceSelection(
        unverifiedReadiness,
        { runtimeInstanceId: 'inst-claude' }
      );
      assert.strictEqual(result.valid, false);
      assert.ok(result.reason?.includes('not implemented') || result.reason?.includes('unverified'));
    });

    it('fails closed when duplicate runtime instance IDs exist in daemon readiness', () => {
      /* eslint-disable @typescript-eslint/naming-convention */
      const duplicateReadiness: RustExecutionReadinessReport = {
        is_ready: true,
        available_runtimes: ['mock'],
        runtime_instances: [
          {
            instance_id: 'inst-dup',
            implementation: 'mock',
            healthy: true,
          },
          {
            instance_id: 'inst-dup',
            implementation: 'mock',
            healthy: true,
          },
        ],
        available_models: [],
        authorized_workspaces: [],
        defaults: {},
        blocking_reasons: [],
      };
      /* eslint-enable @typescript-eslint/naming-convention */

      const result = CapabilityMatrix.validateRuntimeInstanceSelection(
        duplicateReadiness,
        { runtimeInstanceId: 'inst-dup' }
      );
      assert.strictEqual(result.valid, false);
      assert.ok(result.reason?.includes('Duplicate runtime instance identity detected'));
    });

    it('renders safe fallback options in ChatHtmlBuilder when runtimes are empty or unready', () => {
      const htmlFallback = ChatHtmlBuilder.renderRuntimeOptions([]);
      assert.ok(htmlFallback.includes('disabled'));
      assert.ok(htmlFallback.includes('No runtimes available (daemon unready)'));

      const htmlWithDisabled = ChatHtmlBuilder.renderRuntimeOptions([
        {
          id: 'claude_code',
          name: 'Claude Code (Native CLI)',
          enabled: false,
          tooltip: 'Claude Code CLI not found in system PATH.',
        },
      ]);
      assert.ok(htmlWithDisabled.includes('disabled'));
      assert.ok(htmlWithDisabled.includes('title="Claude Code CLI not found in system PATH."'));
    });
  });

  // =========================================================================
  // 7. Accessibility and Operator Controls
  // =========================================================================
  describe('7. Accessibility and Operator Controls', () => {
    it('wraps focus circularly forward and backward (trapFocus)', () => {
      const total = 3;
      assert.strictEqual(OperatorConfirmationModal.trapFocus(0, total, 'forward'), 1);
      assert.strictEqual(OperatorConfirmationModal.trapFocus(1, total, 'forward'), 2);
      assert.strictEqual(OperatorConfirmationModal.trapFocus(2, total, 'forward'), 0); // wrap to 0

      assert.strictEqual(OperatorConfirmationModal.trapFocus(0, total, 'backward'), 2); // wrap to 2
      assert.strictEqual(OperatorConfirmationModal.trapFocus(2, total, 'backward'), 1);
      assert.strictEqual(OperatorConfirmationModal.trapFocus(1, total, 'backward'), 0);
    });

    it('formats accessible screen reader announcement with complete security context', () => {
      const req: OperatorConfirmationDialogRequest = {
        studioId: validStudioId,
        studioName: 'Production Safety Studio',
        runtime: 'claude_code',
        providerId: 'anthropic',
        modelId: 'claude-3-5-sonnet',
        workspacePath: testWorkspace,
        prompt: 'Clean and format authentication controllers',
      };

      const announcement = OperatorConfirmationModal.formatScreenReaderAnnouncement(req);
      assert.ok(announcement.includes('Security Alert: Operator confirmation required'));
      assert.ok(announcement.includes('Production Safety Studio'));
      assert.ok(announcement.includes('claude_code'));
      assert.ok(announcement.includes('anthropic / claude-3-5-sonnet'));
      assert.ok(announcement.includes('Clean and format authentication controllers'));
      assert.ok(announcement.includes('Press Enter to Confirm and Execute, or Escape to Cancel'));
    });

    it('cancels modal and purges pending challenges when Escape is pressed', () => {
      let cancelled = false;
      let confirmed = false;
      const handlers = {
        onCancel: () => { cancelled = true; },
        onConfirm: () => { confirmed = true; },
      };

      // Create a pending challenge to verify purging
      OperatorConfirmationManager.createChallenge(validStudioId, validPrompt);
      assert.strictEqual(OperatorConfirmationManager.getActiveChallengesCountForTesting(), 1);

      const handled = OperatorConfirmationModal.handleModalKeyDown(
        { key: 'Escape', preventDefault: () => {} },
        handlers
      );

      assert.strictEqual(handled, true);
      assert.strictEqual(cancelled, true);
      assert.strictEqual(confirmed, false);
      assert.strictEqual(OperatorConfirmationManager.getActiveChallengesCountForTesting(), 0);
    });

    it('confirms modal when Enter is pressed without modifiers', () => {
      let cancelled = false;
      let confirmed = false;
      const handlers = {
        onCancel: () => { cancelled = true; },
        onConfirm: () => { confirmed = true; },
      };

      const handled = OperatorConfirmationModal.handleModalKeyDown(
        { key: 'Enter', preventDefault: () => {} },
        handlers
      );

      assert.strictEqual(handled, true);
      assert.strictEqual(confirmed, true);
      assert.strictEqual(cancelled, false);
    });

    it('navigates focus on Tab and Shift+Tab', () => {
      let focused = 0;
      const handlers = {
        onCancel: () => {},
        onConfirm: () => {},
        focusedIndex: 0,
        totalElements: 2,
        onNavigate: (next: number) => { focused = next; },
      };

      // Tab forward
      OperatorConfirmationModal.handleModalKeyDown(
        { key: 'Tab', shiftKey: false, preventDefault: () => {} },
        handlers
      );
      assert.strictEqual(focused, 1);

      // Shift+Tab backward from index 0
      handlers.focusedIndex = 0;
      OperatorConfirmationModal.handleModalKeyDown(
        { key: 'Tab', shiftKey: true, preventDefault: () => {} },
        handlers
      );
      assert.strictEqual(focused, 1); // wraps backward from 0 to 1
    });
  });
});
