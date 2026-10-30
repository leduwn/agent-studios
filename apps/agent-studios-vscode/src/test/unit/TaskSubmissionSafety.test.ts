/**
 * Agent Studios — M14.1 Task Submission Safety Regression Tests
 * Milestone M14 Integration Freeze & M15 UI Execution Preparation
 *
 * Verifies strict security boundaries and fail-closed invariants for task submission:
 * 1. Direct SubmitTaskAdapter bypass test (authoritative capability check & forged confirmation fail closed).
 * 2. Operator confirmation challenge test (untrusted raw boolean confirmedByUser: true rejected without token).
 * 3. Model routing mismatch test (requested model diverging from daemon effective model rejected with clear error).
 * 4. Workspace mismatch test (workspace directory not matching daemon repository root fails closed).
 * 5. Prompt integrity test (exact before/after bytes for multiline Markdown, code fences with blank lines, Unicode characters, and bounded frame size limit).
 */

import * as assert from 'assert';
import * as crypto from 'crypto';
import * as path from 'path';
import * as os from 'os';
import * as fs from 'fs';
import {
  SubmitTaskAdapter,
  SubmitTaskRequest,
  TaskSubmissionError,
} from '../../bridge/ipc/SubmitTaskAdapter';
import {
  CapabilityMatrix,
  OperatorConfirmationManager,
  OperatorConfirmationChallenge,
} from '../../bridge/ipc/CapabilityMatrix';
import {
  PromptSanitizer,
  PromptSanitizationError,
  MAX_PROMPT_BYTES,
} from '../../bridge/ipc/PromptSanitizer';
import {
  RustIpcCommand,
  RustIpcResponse,
} from '../../bridge/ipc/RustIpcProtocolTypes';
import {
  RustIpcCodec,
  RUST_DEFAULT_MAX_FRAME_SIZE,
} from '../../bridge/ipc/RustIpcCodec';

class MockTransport {
  public sentCommands: RustIpcCommand[] = [];
  public responseGenerator: (cmd: RustIpcCommand) => RustIpcResponse = () => ({
    type: 'tasks_submitted',
    task_ids: ['a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d'],
  });

  public async sendCommand(cmd: RustIpcCommand): Promise<RustIpcResponse> {
    this.sentCommands.push(cmd);
    return this.responseGenerator(cmd);
  }
}

describe('M14.1 Task Submission Safety & Fail-Closed Regression Tests', () => {
  const validStudioId = '3fa85f64-5717-4562-b3fc-2c963f66afa6';
  let transport: MockTransport;

  beforeEach(() => {
    transport = new MockTransport();
    CapabilityMatrix.resetCapabilities();
    OperatorConfirmationManager.resetForTesting();
  });

  afterEach(() => {
    CapabilityMatrix.resetCapabilities();
    OperatorConfirmationManager.resetForTesting();
  });

  // =========================================================================
  // 1. Direct SubmitTaskAdapter Bypass Tests
  // =========================================================================
  describe('1. Direct SubmitTaskAdapter Bypass & Fail-Closed Enforcement', () => {
    it('fails closed when authoritative capability check fails (task.submit disabled)', async () => {
      // Capability is disabled by default in M14 read-only slice
      assert.strictEqual(CapabilityMatrix.isCapabilityEnabled('task.submit'), false);

      const challenge = OperatorConfirmationManager.createChallenge(
        validStudioId,
        'Write an integration test'
      );

      const req: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt: 'Write an integration test',
        confirmedByUser: true,
        confirmationChallenge: challenge,
      };

      await assert.rejects(
        async () => {
          await SubmitTaskAdapter.submitTask(transport, req);
        },
        (err: unknown) => {
          assert.ok(err instanceof TaskSubmissionError);
          assert.strictEqual(err.studioId, validStudioId);
          assert.ok(
            err.message.includes(
              "Authoritative capability check failed: capability 'task.submit' is not enabled or authorized"
            )
          );
          return true;
        }
      );

      // Verify transport.sendCommand was NEVER invoked (fail closed)
      assert.strictEqual(transport.sentCommands.length, 0);
    });

    it('fails closed when confirmation challenge signature is forged or tampered', async () => {
      // Enable task.submit capability for this test
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);

      const validChallenge = OperatorConfirmationManager.createChallenge(
        validStudioId,
        'Refactor memory manager'
      );

      // Tamper with signature (flip last hex char)
      const lastChar = validChallenge.signature.slice(-1);
      const forgedChar = lastChar === 'a' ? 'b' : 'a';
      const forgedChallenge: OperatorConfirmationChallenge = {
        ...validChallenge,
        signature: validChallenge.signature.slice(0, -1) + forgedChar,
      };

      const req: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt: 'Refactor memory manager',
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
            err.message.includes('Forged confirmation challenge: invalid cryptographic signature')
          );
          return true;
        }
      );

      // Wire transport must not have received any command
      assert.strictEqual(transport.sentCommands.length, 0);
    });

    it('fails closed when challenge token was signed with different secret key', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);

      const alienSecret = crypto.randomBytes(32);
      const alienChallenge = OperatorConfirmationManager.createChallengeWithForeignSecretForTesting(
        validStudioId,
        'Deploy smart contract',
        alienSecret
      );

      const req: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt: 'Deploy smart contract',
        confirmedByUser: true,
        confirmationChallenge: alienChallenge,
      };

      await assert.rejects(
        async () => {
          await SubmitTaskAdapter.submitTask(transport, req);
        },
        (err: unknown) => {
          assert.ok(err instanceof TaskSubmissionError);
          assert.ok(
            err.message.includes('Forged confirmation challenge: invalid cryptographic signature')
          );
          return true;
        }
      );

      assert.strictEqual(transport.sentCommands.length, 0);
    });
  });

  // =========================================================================
  // 2. Operator Confirmation Challenge Tests
  // =========================================================================
  describe('2. Operator Confirmation Challenge Verification', () => {
    it('rejects raw untrusted boolean confirmedByUser: true without verification challenge token', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);

      const req: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt: 'Delete all test databases',
        confirmedByUser: true,
        // confirmationChallenge deliberately omitted
      };

      await assert.rejects(
        async () => {
          await SubmitTaskAdapter.submitTask(transport, req);
        },
        (err: unknown) => {
          assert.ok(err instanceof TaskSubmissionError);
          assert.ok(
            err.message.includes(
              'Untrusted raw boolean confirmedByUser: true rejected without trusted verification challenge token'
            )
          );
          return true;
        }
      );

      assert.strictEqual(transport.sentCommands.length, 0);
    });

    it('rejects request when confirmedByUser is false even if challenge is provided', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);

      const challenge = OperatorConfirmationManager.createChallenge(
        validStudioId,
        'Mutate production files'
      );

      const req: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt: 'Mutate production files',
        confirmedByUser: false,
        confirmationChallenge: challenge,
      };

      await assert.rejects(
        async () => {
          await SubmitTaskAdapter.submitTask(transport, req);
        },
        (err: unknown) => {
          assert.ok(err instanceof TaskSubmissionError);
          assert.ok(
            err.message.includes('Operator confirmation is required before starting mutating coding tasks')
          );
          return true;
        }
      );

      assert.strictEqual(transport.sentCommands.length, 0);
    });

    it('rejects expired confirmation challenge tokens (TTL expiration)', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);

      // Create challenge that is already expired (negative TTL)
      const expiredChallenge = OperatorConfirmationManager.createChallenge(
        validStudioId,
        'Fix compiler warnings',
        { ttlMs: -1000 }
      );

      const req: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt: 'Fix compiler warnings',
        confirmedByUser: true,
        confirmationChallenge: expiredChallenge,
      };

      await assert.rejects(
        async () => {
          await SubmitTaskAdapter.submitTask(transport, req);
        },
        (err: unknown) => {
          assert.ok(err instanceof TaskSubmissionError);
          assert.ok(err.message.includes('Confirmation challenge expired'));
          return true;
        }
      );

      assert.strictEqual(transport.sentCommands.length, 0);
    });

    it('rejects prompt hash mismatch when prompt is altered after challenge issuance', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);

      const challenge = OperatorConfirmationManager.createChallenge(
        validStudioId,
        'Original benign prompt: run lint checks'
      );

      // Attacker substitutes prompt with malicious instructions
      const alteredReq: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt: 'Altered malicious prompt: rm -rf /',
        confirmedByUser: true,
        confirmationChallenge: challenge,
      };

      await assert.rejects(
        async () => {
          await SubmitTaskAdapter.submitTask(transport, alteredReq);
        },
        (err: unknown) => {
          assert.ok(err instanceof TaskSubmissionError);
          assert.ok(
            err.message.includes('Prompt hash mismatch: prompt was altered after confirmation challenge issued')
          );
          return true;
        }
      );

      assert.strictEqual(transport.sentCommands.length, 0);
    });

    it('rejects studio ID mismatch between challenge and submission', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);

      const studioA = '11111111-1111-4111-8111-111111111111';
      const studioB = '22222222-2222-4222-8222-222222222222';

      const challengeForStudioA = OperatorConfirmationManager.createChallenge(
        studioA,
        'Run test suite'
      );

      const reqForStudioB: SubmitTaskRequest = {
        studioId: studioB,
        prompt: 'Run test suite',
        confirmedByUser: true,
        confirmationChallenge: challengeForStudioA,
      };

      await assert.rejects(
        async () => {
          await SubmitTaskAdapter.submitTask(transport, reqForStudioB);
        },
        (err: unknown) => {
          assert.ok(err instanceof TaskSubmissionError);
          assert.ok(
            err.message.includes('Studio ID mismatch in confirmation challenge')
          );
          return true;
        }
      );

      assert.strictEqual(transport.sentCommands.length, 0);
    });

    it('accepts valid confirmation challenge when all cryptographic bindings match', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);

      const prompt = 'Run test suite and verify coverage';
      const challenge = OperatorConfirmationManager.createChallenge(
        validStudioId,
        prompt
      );

      const req: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt,
        confirmedByUser: true,
        confirmationChallenge: challenge,
      };

      const result = await SubmitTaskAdapter.submitTask(transport, req);
      assert.strictEqual(result.success, true);
      assert.strictEqual(result.studioId, validStudioId);
      assert.strictEqual(transport.sentCommands.length, 1);
      assert.strictEqual(transport.sentCommands[0].type, 'submit_task');
    });

    it('accepts challenge serialized as JSON string', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);

      const prompt = 'Format codebase with prettier';
      const challenge = OperatorConfirmationManager.createChallenge(
        validStudioId,
        prompt
      );
      const jsonChallenge = JSON.stringify(challenge);

      const req: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt,
        confirmedByUser: true,
        confirmationChallenge: jsonChallenge,
      };

      const result = await SubmitTaskAdapter.submitTask(transport, req);
      assert.strictEqual(result.success, true);
      assert.strictEqual(transport.sentCommands.length, 1);
    });

    it('fails closed when attempting to replay a previously consumed challenge token', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);

      const prompt = 'Deploy database migrations';
      const challenge = OperatorConfirmationManager.createChallenge(
        validStudioId,
        prompt
      );

      const req: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt,
        confirmedByUser: true,
        confirmationChallenge: challenge,
      };

      // First submission consumes the token
      const result = await SubmitTaskAdapter.submitTask(transport, req);
      assert.strictEqual(result.success, true);
      assert.strictEqual(transport.sentCommands.length, 1);

      // Replay attempt with the EXACT SAME consumed challenge token must fail closed
      await assert.rejects(
        async () => {
          await SubmitTaskAdapter.submitTask(transport, req);
        },
        (err: unknown) => {
          assert.ok(err instanceof TaskSubmissionError);
          assert.ok(
            err.message.includes('Challenge token has already been consumed (replay prevented)')
          );
          return true;
        }
      );

      // Command count must not increase
      assert.strictEqual(transport.sentCommands.length, 1);
    });

    it('rejects challenge token not present in Extension Host registry (unrecognized token)', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);

      // Craft challenge with unknown token
      const forgedChallenge: OperatorConfirmationChallenge = {
        token: 'deadbeefdeadbeefdeadbeefdeadbeef',
        studioId: validStudioId,
        promptHash: crypto.createHash('sha256').update('Exploit payload', 'utf8').digest('hex'),
        expiresAt: Date.now() + 60000,
        signature: '0000000000000000000000000000000000000000000000000000000000000000',
      };

      const req: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt: 'Exploit payload',
        confirmedByUser: true,
        confirmationChallenge: forgedChallenge,
      };

      await assert.rejects(
        async () => {
          await SubmitTaskAdapter.submitTask(transport, req);
        },
        (err: unknown) => {
          assert.ok(err instanceof TaskSubmissionError);
          return true;
        }
      );

      assert.strictEqual(transport.sentCommands.length, 0);
    });

    it('rejects submission when workspace presence mismatches confirmation challenge', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);

      // Challenge created with workspacePath, but request omits it
      const prompt = 'Run test job';
      const challengeWithWs = OperatorConfirmationManager.createChallenge(
        validStudioId,
        prompt,
        { workspacePath: path.resolve('/some/test/workspace') }
      );

      const reqWithoutWs: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt,
        confirmedByUser: true,
        confirmationChallenge: challengeWithWs,
        // workspacePath deliberately omitted
      };

      await assert.rejects(
        async () => {
          await SubmitTaskAdapter.submitTask(transport, reqWithoutWs);
        },
        (err: unknown) => {
          assert.ok(err instanceof TaskSubmissionError);
          assert.ok(err.message.includes('Workspace directory presence mismatch'));
          return true;
        }
      );

      assert.strictEqual(transport.sentCommands.length, 0);
    });

    it('rejects submission when model presence mismatches confirmation challenge', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);

      // Challenge created with modelId, but request omits it
      const prompt = 'Run test job';
      const challengeWithModel = OperatorConfirmationManager.createChallenge(
        validStudioId,
        prompt,
        { modelId: 'claude-3-5-sonnet' }
      );

      const reqWithoutModel: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt,
        confirmedByUser: true,
        confirmationChallenge: challengeWithModel,
        // requestedModelId deliberately omitted
      };

      await assert.rejects(
        async () => {
          await SubmitTaskAdapter.submitTask(transport, reqWithoutModel);
        },
        (err: unknown) => {
          assert.ok(err instanceof TaskSubmissionError);
          assert.ok(err.message.includes('Model ID presence mismatch'));
          return true;
        }
      );

      assert.strictEqual(transport.sentCommands.length, 0);
    });

    it('fails closed when pending authorizations are cleared on session or bridge reset', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);

      const prompt = 'Run migration';
      const challenge = OperatorConfirmationManager.createChallenge(
        validStudioId,
        prompt
      );

      // Simulate bridge disconnect or session switch clearing pending confirmations
      OperatorConfirmationManager.clearPendingChallenges();

      const req: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt,
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
            err.message.includes('Challenge token not found in Extension Host registry')
          );
          return true;
        }
      );

      assert.strictEqual(transport.sentCommands.length, 0);
    });
  });

  // =========================================================================
  // 3. Model Routing Mismatch Tests
  // =========================================================================
  describe('3. Model Routing Mismatch Enforcement', () => {
    it('rejects per-task model override differing from daemon effective configuration with clear error', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);

      const prompt = 'Analyze heap dump';
      const challenge = OperatorConfirmationManager.createChallenge(
        validStudioId,
        prompt,
        { modelId: 'claude-3-opus' }
      );

      const req: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt,
        confirmedByUser: true,
        confirmationChallenge: challenge,
        daemonEffectiveModel: 'claude-3-5-sonnet',
        requestedModelId: 'claude-3-opus', // Diverges from daemon effective model!
      };

      await assert.rejects(
        async () => {
          await SubmitTaskAdapter.submitTask(transport, req);
        },
        (err: unknown) => {
          assert.ok(err instanceof TaskSubmissionError);
          // Invariant: MUST be a clear rejection error, not a silent warning or partial fallback
          assert.ok(
            err.message.includes(
              "Model routing mismatch: Requested model 'claude-3-opus' differs from daemon effective configuration ('claude-3-5-sonnet')"
            )
          );
          return true;
        }
      );

      assert.strictEqual(transport.sentCommands.length, 0);
    });

    it('CapabilityMatrix.evaluateTaskSubmissionPreflight rejects model mismatch with allowed: false', () => {
      const prompt = 'Analyze profiling data';
      const preflight = CapabilityMatrix.evaluateTaskSubmissionPreflight({
        studioId: validStudioId,
        prompt,
        confirmedByUser: true,
        daemonEffectiveModel: 'gemini-1.5-pro',
        requestedModelId: 'gpt-4o',
      });

      assert.strictEqual(preflight.allowed, false);
      assert.strictEqual(preflight.tier, 'TASK_SUBMISSION_READY');
      assert.ok(
        preflight.error?.includes(
          "Model routing mismatch: Requested model 'gpt-4o' differs from daemon effective configuration ('gemini-1.5-pro')"
        )
      );
    });

    it('permits task submission when requestedModelId matches daemon effective model', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);

      const prompt = 'Analyze CPU profile';
      const effectiveModel = 'claude-3-5-sonnet';
      const challenge = OperatorConfirmationManager.createChallenge(
        validStudioId,
        prompt,
        { modelId: effectiveModel }
      );

      const req: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt,
        confirmedByUser: true,
        confirmationChallenge: challenge,
        daemonEffectiveModel: effectiveModel,
        requestedModelId: effectiveModel,
      };

      const result = await SubmitTaskAdapter.submitTask(transport, req);
      assert.strictEqual(result.success, true);
      assert.strictEqual(transport.sentCommands.length, 1);
    });

    it('permits task submission when requestedModelId is omitted (inherits daemon default)', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);

      const prompt = 'Analyze network traces';
      const challenge = OperatorConfirmationManager.createChallenge(
        validStudioId,
        prompt
      );

      const req: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt,
        confirmedByUser: true,
        confirmationChallenge: challenge,
        daemonEffectiveModel: 'claude-3-5-sonnet',
        // requestedModelId omitted
      };

      const result = await SubmitTaskAdapter.submitTask(transport, req);
      assert.strictEqual(result.success, true);
      assert.strictEqual(transport.sentCommands.length, 1);
    });
  });

  // =========================================================================
  // 4. Workspace Mismatch Tests
  // =========================================================================
  describe('4. Workspace Mismatch & Boundary Enforcement', () => {
    let tempDaemonDir: string;
    let tempForeignDir: string;
    let daemonRoot: string;
    let foreignWorkspace: string;

    before(() => {
      tempDaemonDir = fs.mkdtempSync(path.join(os.tmpdir(), 'ags-daemon-'));
      tempForeignDir = fs.mkdtempSync(path.join(os.tmpdir(), 'ags-foreign-'));
      daemonRoot = fs.realpathSync(tempDaemonDir);
      foreignWorkspace = fs.realpathSync(tempForeignDir);
    });

    after(() => {
      try {
        fs.rmSync(tempDaemonDir, { recursive: true, force: true });
        fs.rmSync(tempForeignDir, { recursive: true, force: true });
      } catch {
        // Ignore cleanup errors
      }
    });

    it('fails closed when workspace directory does not match daemon repository root', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);

      const prompt = 'Audit dependencies';
      const challenge = OperatorConfirmationManager.createChallenge(
        validStudioId,
        prompt,
        { workspacePath: foreignWorkspace }
      );

      const req: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt,
        confirmedByUser: true,
        confirmationChallenge: challenge,
        daemonRepoRoot: daemonRoot,
        workspacePath: foreignWorkspace,
      };

      await assert.rejects(
        async () => {
          await SubmitTaskAdapter.submitTask(transport, req);
        },
        (err: unknown) => {
          assert.ok(err instanceof TaskSubmissionError);
          assert.ok(err.message.includes('Workspace mismatch'));
          return true;
        }
      );

      assert.strictEqual(transport.sentCommands.length, 0);
    });

    it('rejects path traversal sequences (..) in workspacePath attempting breakout', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);

      const traversalPath = `${daemonRoot}/../unauthorized_directory`;
      const prompt = 'Inspect secrets';
      const challenge = OperatorConfirmationManager.createChallenge(
        validStudioId,
        prompt
      );

      const req: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt,
        confirmedByUser: true,
        confirmationChallenge: challenge,
        daemonRepoRoot: daemonRoot,
        workspacePath: traversalPath,
      };

      await assert.rejects(
        async () => {
          await SubmitTaskAdapter.submitTask(transport, req);
        },
        (err: unknown) => {
          assert.ok(err instanceof TaskSubmissionError);
          assert.ok(err.message.includes('Path traversal sequences rejected in workspace path'));
          return true;
        }
      );

      assert.strictEqual(transport.sentCommands.length, 0);
    });

    it('rejects null byte injection in workspacePath', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);

      const nullByteInjection = `${daemonRoot}\0/etc/shadow`;
      const prompt = 'Read shadow file';
      const challenge = OperatorConfirmationManager.createChallenge(
        validStudioId,
        prompt
      );

      const req: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt,
        confirmedByUser: true,
        confirmationChallenge: challenge,
        daemonRepoRoot: daemonRoot,
        workspacePath: nullByteInjection,
      };

      await assert.rejects(
        async () => {
          await SubmitTaskAdapter.submitTask(transport, req);
        },
        (err: unknown) => {
          assert.ok(err instanceof TaskSubmissionError);
          assert.ok(err.message.includes('Null byte injection detected in workspace path'));
          return true;
        }
      );

      assert.strictEqual(transport.sentCommands.length, 0);
    });

    it('rejects Windows UNC network paths in workspacePath', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);

      const uncPath = '\\\\remote-server\\share\\agent-repo';
      const prompt = 'Remote execution';
      const challenge = OperatorConfirmationManager.createChallenge(
        validStudioId,
        prompt
      );

      const req: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt,
        confirmedByUser: true,
        confirmationChallenge: challenge,
        daemonRepoRoot: daemonRoot,
        workspacePath: uncPath,
      };

      await assert.rejects(
        async () => {
          await SubmitTaskAdapter.submitTask(transport, req);
        },
        (err: unknown) => {
          assert.ok(err instanceof TaskSubmissionError);
          assert.ok(err.message.includes('UNC network paths rejected in workspace path'));
          return true;
        }
      );

      assert.strictEqual(transport.sentCommands.length, 0);
    });

    it('rejects empty workspace path when daemon repo root is configured', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);

      const prompt = 'Execute task';
      const challenge = OperatorConfirmationManager.createChallenge(
        validStudioId,
        prompt
      );

      const req: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt,
        confirmedByUser: true,
        confirmationChallenge: challenge,
        daemonRepoRoot: daemonRoot,
        workspacePath: '',
      };

      await assert.rejects(
        async () => {
          await SubmitTaskAdapter.submitTask(transport, req);
        },
        (err: unknown) => {
          assert.ok(err instanceof TaskSubmissionError);
          assert.ok(
            err.message.includes(
              'Workspace directory is required when daemon repository root is configured'
            )
          );
          return true;
        }
      );

      assert.strictEqual(transport.sentCommands.length, 0);
    });

    it('allows submission when workspace directory matches daemon repository root across trailing slashes', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);

      const wsPathWithSlash = `${daemonRoot}${path.sep}`;
      const prompt = 'Clean build cache';
      const challenge = OperatorConfirmationManager.createChallenge(
        validStudioId,
        prompt,
        { workspacePath: wsPathWithSlash }
      );

      const req: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt,
        confirmedByUser: true,
        confirmationChallenge: challenge,
        daemonRepoRoot: daemonRoot,
        workspacePath: wsPathWithSlash,
      };

      const result = await SubmitTaskAdapter.submitTask(transport, req);
      assert.strictEqual(result.success, true);
      assert.strictEqual(transport.sentCommands.length, 1);
    });

    if (process.platform === 'win32') {
      it('allows submission on Windows with alternate slash separators', async () => {
        CapabilityMatrix.setCapabilityEnabled('task.submit', true);

        // Windows slash variation: forward slash vs backslash
        const wsForwardSlash = daemonRoot.replace(/\\/g, '/');
        const prompt = 'Compile Windows artifacts';
        const challenge = OperatorConfirmationManager.createChallenge(
          validStudioId,
          prompt,
          { workspacePath: wsForwardSlash }
        );

        const req: SubmitTaskRequest = {
          studioId: validStudioId,
          prompt,
          confirmedByUser: true,
          confirmationChallenge: challenge,
          daemonRepoRoot: daemonRoot,
          workspacePath: wsForwardSlash,
        };

        const result = await SubmitTaskAdapter.submitTask(transport, req);
        assert.strictEqual(result.success, true);
        assert.strictEqual(transport.sentCommands.length, 1);
      });

      it('allows submission on Windows with drive letter casing variations', async () => {
        CapabilityMatrix.setCapabilityEnabled('task.submit', true);

        const wsUpper = daemonRoot.toUpperCase();
        const prompt = 'Build release';
        const challenge = OperatorConfirmationManager.createChallenge(
          validStudioId,
          prompt,
          { workspacePath: wsUpper }
        );

        const req: SubmitTaskRequest = {
          studioId: validStudioId,
          prompt,
          confirmedByUser: true,
          confirmationChallenge: challenge,
          daemonRepoRoot: daemonRoot,
          workspacePath: wsUpper,
        };

        const result = await SubmitTaskAdapter.submitTask(transport, req);
        assert.strictEqual(result.success, true);
        assert.strictEqual(transport.sentCommands.length, 1);
      });
    }

    if (process.platform !== 'win32') {
      it('allows submission on POSIX when workspace is a canonical symlink to daemon repository', async () => {
        CapabilityMatrix.setCapabilityEnabled('task.submit', true);

        const symlinkPath = path.join(os.tmpdir(), `ags-symlink-${Date.now()}`);
        fs.symlinkSync(daemonRoot, symlinkPath);

        try {
          const prompt = 'Run POSIX build';
          const challenge = OperatorConfirmationManager.createChallenge(
            validStudioId,
            prompt,
            { workspacePath: symlinkPath }
          );

          const req: SubmitTaskRequest = {
            studioId: validStudioId,
            prompt,
            confirmedByUser: true,
            confirmationChallenge: challenge,
            daemonRepoRoot: daemonRoot,
            workspacePath: symlinkPath,
          };

          const result = await SubmitTaskAdapter.submitTask(transport, req);
          assert.strictEqual(result.success, true);
          assert.strictEqual(transport.sentCommands.length, 1);
        } finally {
          try {
            fs.unlinkSync(symlinkPath);
          } catch {
            // Ignore cleanup error
          }
        }
      });

      it('rejects foreign-platform Windows drive paths on POSIX', async () => {
        CapabilityMatrix.setCapabilityEnabled('task.submit', true);

        const foreignWindowsPath = 'C:\\Projects\\agent-studios';
        const prompt = 'Run foreign path';
        const challenge = OperatorConfirmationManager.createChallenge(
          validStudioId,
          prompt
        );

        const req: SubmitTaskRequest = {
          studioId: validStudioId,
          prompt,
          confirmedByUser: true,
          confirmationChallenge: challenge,
          daemonRepoRoot: daemonRoot,
          workspacePath: foreignWindowsPath,
        };

        await assert.rejects(
          async () => {
            await SubmitTaskAdapter.submitTask(transport, req);
          },
          (err: unknown) => {
            assert.ok(err instanceof TaskSubmissionError);
            assert.ok(
              err.message.includes('Foreign-platform Windows path') ||
              err.message.includes('Workspace normalization error')
            );
            return true;
          }
        );

        assert.strictEqual(transport.sentCommands.length, 0);
      });
    }
  });

  // =========================================================================
  // 5. Prompt Integrity Tests
  // =========================================================================
  describe('5. Prompt Byte Integrity & Bounded Wire Framing', () => {
    it('preserves multiline Markdown bytes verbatim (headings, blank lines, quotes, lists)', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);

      const multilineMarkdown = [
        '# Milestone M14 Integration Spec',
        '',
        '## Overview',
        'Verify authoritative wire contracts between VS Code and Rust agsd:',
        '',
        '> Security note: Fail-closed boundary must hold unconditionally.',
        '',
        '- [x] Capability matrix enforcement',
        '- [x] HMAC challenge tokens',
        '- [ ] Diff streaming over IPC',
        '',
        '---',
        'End of spec.',
      ].join('\n');

      const challenge = OperatorConfirmationManager.createChallenge(
        validStudioId,
        multilineMarkdown
      );

      const req: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt: multilineMarkdown,
        confirmedByUser: true,
        confirmationChallenge: challenge,
      };

      const result = await SubmitTaskAdapter.submitTask(transport, req);
      assert.strictEqual(result.success, true);

      // Inspect dispatched wire command
      const cmd = transport.sentCommands[0];
      assert.strictEqual(cmd.type, 'submit_task');
      if (cmd.type !== 'submit_task') {
        throw new Error('Expected submit_task command');
      }
      assert.strictEqual(cmd.task_spec.title, multilineMarkdown);
      assert.strictEqual(cmd.task_spec.description, '');

      // Verify exact byte equality
      const inputBytes = Buffer.from(multilineMarkdown, 'utf8');
      const titleBytes = Buffer.from(cmd.task_spec.title, 'utf8');
      assert.strictEqual(inputBytes.compare(titleBytes), 0);

      // Verify Rust backend prompt resolution reproduces exact input
      const resolved = PromptSanitizer.simulateRustPromptResolution({
        title: cmd.task_spec.title,
        description: cmd.task_spec.description,
      });
      assert.strictEqual(resolved, multilineMarkdown);
    });

    it('preserves code fences with internal blank lines, tabs, and indentation intact', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);

      const codeFencePrompt = [
        'Implement the following Rust function in `crates/ipc/src/auth.rs`:',
        '',
        '```rust',
        'pub fn compute_canonical_digest(parts: &[&str]) -> String {',
        '    let mut hasher = Sha256::new();',
        '',
        '    for (idx, part) in parts.iter().enumerate() {',
        '        if idx > 0 {',
        '            hasher.update(b"\\n");',
        '        }',
        '        hasher.update(part.as_bytes());',
        '    }',
        '',
        '    format!("{:x}", hasher.finalize())',
        '}',
        '```',
        '',
        '```python',
        'def verify_signature(secret: bytes, payload: str, sig: str) -> bool:',
        '    expected = hmac.new(secret, payload.encode("utf-8"), hashlib.sha256).hexdigest()',
        '    return hmac.compare_digest(expected, sig)',
        '```',
        '',
        'All unit tests must pass.',
      ].join('\n');

      const challenge = OperatorConfirmationManager.createChallenge(
        validStudioId,
        codeFencePrompt
      );

      const req: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt: codeFencePrompt,
        confirmedByUser: true,
        confirmationChallenge: challenge,
      };

      await SubmitTaskAdapter.submitTask(transport, req);
      const cmd = transport.sentCommands[0];
      assert.strictEqual(cmd.type, 'submit_task');
      if (cmd.type !== 'submit_task') {
        throw new Error('Expected submit_task command');
      }

      // Exact string equality
      assert.strictEqual(cmd.task_spec.title, codeFencePrompt);

      // Verify internal blank lines were NOT stripped
      const originalBlankCount = (codeFencePrompt.match(/\n\n/g) || []).length;
      const dispatchedBlankCount = (cmd.task_spec.title.match(/\n\n/g) || []).length;
      assert.strictEqual(dispatchedBlankCount, originalBlankCount);
      assert.strictEqual(originalBlankCount, 5);
    });

    it('preserves multibyte UTF-8 characters faithfully across diverse scripts', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);

      const unicodePrompt = [
        'Task: Kiểm tra tính toàn vẹn ký tự Unicode & Đa ngữ (UTF-8 Fidelity)',
        '• Vietnamese: Xin chào, hệ thống Agent Studios sẵn sàng hoạt động với bảo mật tối đa!',
        '• Japanese: エージェントスタジオへようこそ。システムテストを実行してください。',
        '• Emojis: 🛡️ Security • 🚀 Performance • ⚡ Low-latency • 🔒 Cryptographic Bindings',
        '• Math symbols: ∑(x_i) = ∫ f(t) dt ≈ 3.14159 ≠ 0',
        '• French/German: Déjà vu, naïve café & Über-großes Phänomen',
      ].join('\n');

      const challenge = OperatorConfirmationManager.createChallenge(
        validStudioId,
        unicodePrompt
      );

      const req: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt: unicodePrompt,
        confirmedByUser: true,
        confirmationChallenge: challenge,
      };

      await SubmitTaskAdapter.submitTask(transport, req);
      const cmd = transport.sentCommands[0];
      assert.strictEqual(cmd.type, 'submit_task');
      if (cmd.type !== 'submit_task') {
        throw new Error('Expected submit_task command');
      }

      const inputBuffer = Buffer.from(unicodePrompt, 'utf8');
      const outputBuffer = Buffer.from(cmd.task_spec.title, 'utf8');

      // Byte-for-byte identity
      assert.strictEqual(inputBuffer.length, outputBuffer.length);
      assert.strictEqual(inputBuffer.compare(outputBuffer), 0);
      assert.strictEqual(cmd.task_spec.title, unicodePrompt);
    });

    it('normalizes CRLF to LF while strictly preserving byte content and blank lines', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);

      const crlfPrompt = 'First line\r\n\r\nSecond line with indent\r\n    code_call();\r\n\r\nThird line';
      const expectedLf = 'First line\n\nSecond line with indent\n    code_call();\n\nThird line';

      const challenge = OperatorConfirmationManager.createChallenge(
        validStudioId,
        crlfPrompt
      );

      const req: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt: crlfPrompt,
        confirmedByUser: true,
        confirmationChallenge: challenge,
      };

      await SubmitTaskAdapter.submitTask(transport, req);
      const cmd = transport.sentCommands[0];
      assert.strictEqual(cmd.type, 'submit_task');
      if (cmd.type !== 'submit_task') {
        throw new Error('Expected submit_task command');
      }

      assert.strictEqual(cmd.task_spec.title, expectedLf);
      assert.ok(!cmd.task_spec.title.includes('\r'));
    });

    it('enforces bounded frame size limit (< 4 MiB frame boundary) and fails closed on oversized payload', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);

      // Prompt exceeding MAX_PROMPT_BYTES (~3.94 MiB)
      const oversizedPrompt = Buffer.alloc(MAX_PROMPT_BYTES + 1024, 'X').toString('utf8');

      const req: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt: oversizedPrompt,
        confirmedByUser: true,
      };

      // 1. PromptSanitizer rejects before preflight completes
      assert.throws(
        () => PromptSanitizer.sanitizePrompt(oversizedPrompt),
        (err: unknown) => {
          assert.ok(err instanceof PromptSanitizationError);
          assert.strictEqual(err.field, 'prompt');
          assert.ok(err.message.includes('exceeds maximum allowed size'));
          return true;
        }
      );

      // 2. SubmitTaskAdapter fails closed
      await assert.rejects(
        async () => {
          await SubmitTaskAdapter.submitTask(transport, req);
        },
        (err: unknown) => {
          assert.ok(err instanceof TaskSubmissionError);
          assert.ok(err.message.includes('exceeds maximum allowed size'));
          return true;
        }
      );

      // 3. Ensure transport never received any command
      assert.strictEqual(transport.sentCommands.length, 0);

      // 4. Verify wire codec rejects command exceeding RUST_DEFAULT_MAX_FRAME_SIZE
      const oversizedCommand: RustIpcCommand = {
        type: 'submit_task',
        studio_id: validStudioId,
        task_spec: {
          key: 'overflow-test',
          title: Buffer.alloc(RUST_DEFAULT_MAX_FRAME_SIZE, 'y').toString('utf8'),
          description: '',
          assigned_agent_id: null,
          parent_task_key: null,
          parent_task_id: null,
          dependency_keys: [],
          dependency_task_ids: [],
          dependency_output_policy: 'task_success',
        },
      };

      assert.throws(
        () => RustIpcCodec.encodeCommand(oversizedCommand),
        (err: unknown) => {
          assert.ok(err instanceof Error);
          assert.ok(
            err.message.includes('exceeds maximum frame size') ||
            err.message.includes('exceeds maximum allowed size')
          );
          return true;
        }
      );
    });
  });
});
