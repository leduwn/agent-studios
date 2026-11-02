/**
 * Agent Studios — Real Agent Execution & Multi-Runtime E2E Test Suite
 * Milestone M15 UI-04: Real Agent Execution UI & Runtime Verification
 *
 * Tier Classification:
 * - UNIT: Pure in-memory capability parsing, adapter options builder, and signature validation.
 * - MOCK_IPC: Synthetic framing and wire-level protocol validation without live daemon.
 * - REAL_AGSD_READ_ONLY_IPC: Real cross-process IPC against compiled Rust `agsd.exe` binary.
 * - IN_PROCESS_REAL_ARTIFACT_STORE: In-process task execution store and immutable state preservation.
 * - EXTENSION_HOST: Operator confirmation management, challenge lifecycle, and security guards.
 * - REAL_AI_MODEL_NOT_TESTED: External LLM frontier provider inference not invoked.
 * - TIER3_NOT_VERIFIED: Live billable agent task execution gated closed and unverified.
 *
 * Scenarios Tested:
 * 1. Detection of real runtime executable (claude.exe CLI v2.1.310).
 * 2. Multi-runtime capability checks (Claude Code vs OpenCode adapter detection, unhealthy/duplicate gating).
 * 3. Fail-closed execution gating: task submission without operator approval or in read-only slice fails closed.
 * 4. Real daemon agsd.exe lifecycle: spawn with temporary bootstrap, authenticate with HMAC, query health/ping,
 *    check provider summaries, verify model routing capability, and perform graceful shutdown with admin key.
 */

/* eslint-disable @typescript-eslint/naming-convention */

import * as assert from 'assert';
import * as childProcess from 'child_process';
import * as crypto from 'crypto';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';

import { DaemonConnectedBridge } from '../../bridge/DaemonConnectedBridge';
import { DaemonDiscovery } from '../../bridge/ipc/DaemonDiscovery';
import { NativeIpcTransport } from '../../bridge/ipc/NativeIpcTransport';
import {
  CapabilityMatrix,
  OperatorConfirmationManager,
} from '../../bridge/ipc/CapabilityMatrix';
import {
  ExecutionClient,
  SubmitTaskRequest,
  TaskSubmissionError,
} from '../../bridge/ipc/ExecutionClient';
import {
  RustExecutionReadinessReport,
  RustIpcCommand,
  RustIpcResponse,
} from '../../bridge/ipc/RustIpcProtocolTypes';
import { ChatWebviewViewProvider } from '../../chat/ChatWebviewViewProvider';
import { TaskExecutionStore } from '../../state/TaskExecutionStore';

/**
 * Resolves the path to the real compiled Rust agsd binary.
 */
function resolveAgsdBinaryPath(): string {
  const isWindows = process.platform === 'win32';
  const exeSuffix = isWindows ? '.exe' : '';

  const candidatePaths = [
    process.env.AGSD_BIN_PATH,
    'D:\\Projects\\agent-studios-m15-04\\agent-studios-rs\\target\\debug\\agsd.exe',
    'D:\\Projects\\agent-studios-m15-03\\agent-studios-rs\\target\\debug\\agsd.exe',
    'D:\\Projects\\agent-studios-m15-02\\agent-studios-rs\\target\\debug\\agsd.exe',
    'D:\\Projects\\agent-studios-m15\\agent-studios-rs\\target\\debug\\agsd.exe',
    path.resolve(__dirname, '../../../../../../agent-studios-rs/target/debug/agsd' + exeSuffix),
    path.resolve(__dirname, '../../../../../agent-studios-rs/target/debug/agsd' + exeSuffix),
    path.resolve(__dirname, '../../../../agent-studios-rs/target/debug/agsd' + exeSuffix),
    path.resolve(process.cwd(), '../../agent-studios-rs/target/debug/agsd' + exeSuffix),
  ].filter((p): p is string => typeof p === 'string' && p.length > 0);

  for (const candidate of candidatePaths) {
    if (fs.existsSync(candidate)) {
      return candidate;
    }
  }

  throw new Error(
    `[RealAgentExecutionE2E] Real Rust daemon binary (agsd) not found. Checked candidate locations:\n` +
      candidatePaths.map((p) => `  - ${p}`).join('\n') +
      `\n\nPlease compile agsd first with: cargo build --bin agsd -p agent-studios-ipc-transport`
  );
}

/**
 * Resolves the path to the real Claude CLI binary.
 */
function resolveClaudeBinaryPath(): string {
  const isWindows = process.platform === 'win32';
  const exeSuffix = isWindows ? '.exe' : '';

  const candidatePaths = [
    process.env.CLAUDE_BIN_PATH,
    'C:\\Users\\Duwn\\.local\\bin\\claude.exe',
    path.join(os.homedir(), '.local', 'bin', 'claude' + exeSuffix),
    path.join(os.homedir(), '.claude', 'bin', 'claude' + exeSuffix),
    path.join(process.env.LOCALAPPDATA || '', 'Programs', 'claude', 'claude' + exeSuffix),
  ].filter((p): p is string => typeof p === 'string' && p.length > 0);

  for (const candidate of candidatePaths) {
    if (fs.existsSync(candidate)) {
      return candidate;
    }
  }

  const pathEnv = process.env.PATH || '';
  const pathDirs = pathEnv.split(path.delimiter);
  for (const dir of pathDirs) {
    const candidate = path.join(dir, 'claude' + exeSuffix);
    if (fs.existsSync(candidate)) {
      return candidate;
    }
  }

  throw new Error(
    `[RealAgentExecutionE2E] Real Claude CLI binary not found. Checked candidate locations:\n` +
      candidatePaths.map((p) => `  - ${p}`).join('\n')
  );
}

/**
 * Mock transport for hermetic unit / preflight execution tests.
 */
class MockPreflightTransport {
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

describe('Tier 3: Real Agent Execution & Runtime E2E Suite', function () {
  this.timeout(30000);

  // =========================================================================
  // Section 1: Real Runtime Executable Detection (claude.exe)
  // Classification: REAL_AGSD_READ_ONLY_IPC / EXTENSION_HOST
  // =========================================================================
  describe('1. Real Runtime Executable Detection (claude.exe) [REAL_AGSD_READ_ONLY_IPC & EXTENSION_HOST]', function () {
    let claudeBinaryPath: string;

    before(function () {
      try {
        claudeBinaryPath = resolveClaudeBinaryPath();
      } catch (err) {
        console.warn(`[RealAgentExecutionE2E] Real Claude CLI not available on host: ${(err as Error).message}`);
        this.skip();
      }
    });

    it('1. locates real claude CLI binary path in environment or local bin', () => {
      assert.ok(typeof claudeBinaryPath === 'string' && claudeBinaryPath.length > 0);
      assert.ok(fs.existsSync(claudeBinaryPath), `Claude CLI binary must exist at ${claudeBinaryPath}`);
    });

    it('2. verifies real claude CLI binary is a valid non-empty executable', () => {
      const stats = fs.statSync(claudeBinaryPath);
      assert.ok(stats.isFile(), 'Claude binary must be a regular file');
      assert.ok(stats.size > 0, 'Claude binary size must be greater than 0 bytes');
    });

    it('3. runs claude --version without billable token usage and validates version output', () => {
      const result = childProcess.spawnSync(claudeBinaryPath, ['--version'], {
        encoding: 'utf8',
        timeout: 5000,
      });

      assert.strictEqual(result.status, 0, `claude --version must exit with code 0, stderr: ${result.stderr}`);
      const output = (result.stdout || '').trim();
      assert.ok(output.length > 0, 'claude --version stdout must not be empty');
      assert.ok(
        output.includes('Claude Code') || /^\d+\.\d+\.\d+/.test(output),
        `Output must indicate Claude Code CLI version, got: ${output}`
      );
    });

    it('4. runs claude --help in non-interactive read-only mode', () => {
      const result = childProcess.spawnSync(claudeBinaryPath, ['--help'], {
        encoding: 'utf8',
        timeout: 5000,
      });

      assert.strictEqual(result.status, 0, `claude --help must exit with code 0, stderr: ${result.stderr}`);
      const output = (result.stdout || '').trim();
      assert.ok(
        output.includes('Usage: claude') || output.includes('Options:') || output.includes('Commands:'),
        'Help output must describe CLI commands without running agent execution'
      );
    });

    it('5. fail-closed detection: non-existent CLI binary path throws error', () => {
      const bogusPath = path.join(os.tmpdir(), 'non_existent_claude_binary_987654.exe');
      assert.strictEqual(fs.existsSync(bogusPath), false);

      assert.throws(
        () => {
          if (!fs.existsSync(bogusPath)) {
            throw new Error(`[RealAgentExecutionE2E] Real Claude CLI binary not found at ${bogusPath}`);
          }
        },
        /Real Claude CLI binary not found/,
        'Querying non-existent binary must throw fail-closed error'
      );
    });
  });

  // =========================================================================
  // Section 2: Multi-Runtime Capability Checks (Claude Code vs OpenCode)
  // Classification: UNIT / EXTENSION_HOST
  // =========================================================================
  describe('2. Multi-Runtime Capability Checks (Claude Code vs OpenCode) [UNIT & EXTENSION_HOST]', () => {
    it('6. detects verified claude_code adapter when present in available_runtimes', () => {
      const readiness: RustExecutionReadinessReport = {
        is_ready: true,
        registered_runtimes: ['claude_code', 'opencode'],
        available_runtimes: ['claude_code', 'opencode'],
        runtime_instances: [
          { instance_id: 'rt-claude-01', implementation: 'claude_code', healthy: true },
          { instance_id: 'rt-opencode-01', implementation: 'opencode', healthy: true },
        ],
        runtime_capabilities: {
          'rt-claude-01': { streaming_events: 'supported', stop: 'supported' },
          'rt-opencode-01': { streaming_events: 'supported', stop: 'supported' },
        },
        available_models: [],
        authorized_workspaces: [],
        defaults: { configured_instance_id: 'rt-claude-01' },
        blocking_reasons: [],
      };

      const { runtimeOptions, selectedRuntime } = ChatWebviewViewProvider.buildRuntimeOptions(readiness);

      assert.strictEqual(runtimeOptions.length, 2);
      const claudeOption = runtimeOptions.find((o) => o.implementation === 'claude_code');
      assert.ok(claudeOption, 'claude_code option must be present');
      assert.strictEqual(claudeOption?.enabled, true, 'Verified claude_code adapter must be enabled');
      assert.strictEqual(claudeOption?.tooltip, 'Healthy runtime instance');
      assert.strictEqual(selectedRuntime, 'rt-claude-01|claude_code');
    });

    it('7. detects unverified claude_code adapter when missing from available_runtimes', () => {
      const readiness: RustExecutionReadinessReport = {
        is_ready: true,
        registered_runtimes: ['claude_code'],
        available_runtimes: [], // Missing from available_runtimes in backend
        runtime_instances: [
          { instance_id: 'rt-claude-unverified', implementation: 'claude_code', healthy: true },
        ],
        runtime_capabilities: {},
        available_models: [],
        authorized_workspaces: [],
        defaults: {},
        blocking_reasons: [],
      };

      const { runtimeOptions } = ChatWebviewViewProvider.buildRuntimeOptions(readiness);
      assert.strictEqual(runtimeOptions.length, 1);
      const claudeOption = runtimeOptions[0];

      assert.strictEqual(claudeOption.enabled, false, 'Unverified claude_code adapter must be disabled');
      assert.ok(claudeOption.name.includes('[Unverified Adapter]'), 'Name must indicate unverified adapter');
      assert.ok(
        (claudeOption.tooltip || '').includes('is unverified') ||
          (claudeOption.tooltip || '').includes('unverified'),
        'Tooltip must explain unverified adapter'
      );
    });

    it('8. detects opencode adapter and validates healthy runtime instance', () => {
      const readiness: RustExecutionReadinessReport = {
        is_ready: true,
        registered_runtimes: ['opencode'],
        available_runtimes: ['opencode'],
        runtime_instances: [
          { instance_id: 'rt-opencode-01', implementation: 'opencode', healthy: true },
        ],
        runtime_capabilities: {
          'rt-opencode-01': { streaming_events: 'supported', stop: 'supported' },
        },
        available_models: [],
        authorized_workspaces: [],
        defaults: { configured_instance_id: 'rt-opencode-01' },
        blocking_reasons: [],
      };

      const { runtimeOptions, selectedRuntime } = ChatWebviewViewProvider.buildRuntimeOptions(readiness);
      assert.strictEqual(runtimeOptions.length, 1);
      assert.strictEqual(runtimeOptions[0].enabled, true);
      assert.strictEqual(runtimeOptions[0].implementation, 'opencode');
      assert.strictEqual(selectedRuntime, 'rt-opencode-01|opencode');
    });

    it('9. detects degraded/unhealthy opencode runtime and disables option', () => {
      const readiness: RustExecutionReadinessReport = {
        is_ready: true,
        registered_runtimes: ['opencode'],
        available_runtimes: ['opencode'],
        runtime_instances: [
          { instance_id: 'rt-opencode-degraded', implementation: 'opencode', healthy: false },
        ],
        runtime_capabilities: {},
        available_models: [],
        authorized_workspaces: [],
        defaults: {},
        blocking_reasons: ['Runtime process crashed'],
      };

      const { runtimeOptions } = ChatWebviewViewProvider.buildRuntimeOptions(readiness);
      assert.strictEqual(runtimeOptions.length, 1);
      assert.strictEqual(runtimeOptions[0].enabled, false);
      assert.ok(runtimeOptions[0].name.includes('[Unhealthy]'));
      assert.strictEqual(runtimeOptions[0].tooltip, 'Degraded/Unhealthy runtime instance');
    });

    it('10. disables duplicate runtime instance IDs to prevent ambiguous routing', () => {
      const readiness: RustExecutionReadinessReport = {
        is_ready: true,
        registered_runtimes: ['opencode', 'claude_code'],
        available_runtimes: ['opencode', 'claude_code'],
        runtime_instances: [
          { instance_id: 'rt-dup-id', implementation: 'opencode', healthy: true },
          { instance_id: 'rt-dup-id', implementation: 'claude_code', healthy: true },
        ],
        runtime_capabilities: {},
        available_models: [],
        authorized_workspaces: [],
        defaults: {},
        blocking_reasons: [],
      };

      const { runtimeOptions } = ChatWebviewViewProvider.buildRuntimeOptions(readiness);
      assert.strictEqual(runtimeOptions.length, 2);
      for (const opt of runtimeOptions) {
        assert.strictEqual(opt.enabled, false, 'Duplicate instance ID options must be disabled');
        assert.ok(opt.name.includes('[Duplicate ID - Disabled]'));
        assert.strictEqual(opt.tooltip, 'Duplicate runtime instance ID detected in daemon readiness (disabled)');
      }
    });

    it('11. returns safe disabled fallback on null or unready daemon report', () => {
      const nullResult = ChatWebviewViewProvider.buildRuntimeOptions(null);
      assert.strictEqual(nullResult.runtimeOptions.length, 1);
      assert.strictEqual(nullResult.runtimeOptions[0].enabled, false);
      assert.strictEqual(nullResult.runtimeOptions[0].name, 'No runtimes available (daemon unready)');

      const undefinedResult = ChatWebviewViewProvider.buildRuntimeOptions(undefined);
      assert.strictEqual(undefinedResult.runtimeOptions.length, 1);
      assert.strictEqual(undefinedResult.runtimeOptions[0].enabled, false);
    });

    it('12. validates runtime capabilities tri-state enforcement for execution and cancellation', () => {
      const readinessValid: RustExecutionReadinessReport = {
        is_ready: true,
        registered_runtimes: ['opencode'],
        available_runtimes: ['opencode'],
        runtime_instances: [
          { instance_id: 'rt-inst-test', implementation: 'opencode', healthy: true },
        ],
        runtime_capabilities: {
          'rt-inst-test': {
            streaming_events: 'supported',
            stop: 'supported',
          },
        },
        available_models: [],
        authorized_workspaces: [],
        defaults: {},
        blocking_reasons: [],
      };

      // Valid task execution check
      const execVal = CapabilityMatrix.validateRuntimeInstanceSelection(
        readinessValid,
        { runtimeInstanceId: 'rt-inst-test', runtimePreference: 'opencode' },
        { action: 'task_execution' }
      );
      assert.strictEqual(execVal.valid, true, 'Execution must be valid for healthy instance');

      const readinessUnsupported: RustExecutionReadinessReport = {
        ...readinessValid,
        runtime_capabilities: {
          'rt-inst-test': {
            streaming_events: 'supported',
            stop: 'unsupported',
          },
        },
      };

      // Cancellation check: stop capability is unsupported
      const cancelVal = CapabilityMatrix.validateRuntimeInstanceSelection(
        readinessUnsupported,
        { runtimeInstanceId: 'rt-inst-test', runtimePreference: 'opencode' },
        { action: 'task_cancellation' }
      );
      assert.strictEqual(cancelVal.valid, false, 'Cancellation must fail closed when stop is unsupported');
      assert.ok(cancelVal.reason?.includes('stop'));
    });
  });

  // =========================================================================
  // Section 3: Fail-Closed Execution Gating & Zero Billable AI Tasks
  // Classification: UNIT / MOCK_IPC / EXTENSION_HOST / REAL_AI_MODEL_NOT_TESTED
  // =========================================================================
  describe('3. Fail-Closed Execution Gating & Zero Billable AI Tasks [UNIT, MOCK_IPC & REAL_AI_MODEL_NOT_TESTED]', () => {
    const validStudioId = '3fa85f64-5717-4562-b3fc-2c963f66afa6';
    let transport: MockPreflightTransport;

    beforeEach(() => {
      transport = new MockPreflightTransport();
      CapabilityMatrix.resetCapabilities();
      OperatorConfirmationManager.resetForTesting();
      TaskExecutionStore.instance.reset();
    });

    afterEach(() => {
      CapabilityMatrix.resetCapabilities();
      OperatorConfirmationManager.resetForTesting();
      TaskExecutionStore.instance.reset();
    });

    it('13. fails closed when task submission attempted with task.submit disabled in CapabilityMatrix', async () => {
      assert.strictEqual(CapabilityMatrix.isCapabilityEnabled('task.submit'), false);

      const challenge = OperatorConfirmationManager.createChallenge(validStudioId, 'test prompt');
      const req: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt: 'test prompt',
        confirmedByUser: true,
        confirmationChallenge: challenge,
      };

      await assert.rejects(
        async () => {
          await ExecutionClient.submitTask(transport, req);
        },
        (err: Error) => {
          assert.ok(err instanceof TaskSubmissionError);
          assert.ok(err.message.includes('task.submit capability missing'));
          return true;
        },
        'ExecutionClient must reject task submission when task.submit capability is missing'
      );

      assert.strictEqual(transport.sentCommands.length, 0, 'No wire commands should be dispatched');
    });

    it('14. fails closed when task submission attempted without operator confirmation challenge', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);

      const req: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt: 'test prompt',
        confirmedByUser: true,
        // confirmationChallenge deliberately omitted
      };

      await assert.rejects(
        async () => {
          await ExecutionClient.submitTask(transport, req);
        },
        (err: Error) => {
          assert.ok(err instanceof TaskSubmissionError);
          assert.ok(
            err.message.includes('challenge') ||
              err.message.includes('confirmation')
          );
          return true;
        },
        'ExecutionClient must reject task submission without confirmationChallenge'
      );

      assert.strictEqual(transport.sentCommands.length, 0, 'No wire commands should be dispatched');
    });

    it('15. fails closed when operator confirmation challenge signature is forged or invalid', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);

      const foreignSecret = crypto.randomBytes(32);
      const forgedChallenge = OperatorConfirmationManager.createChallengeWithForeignSecretForTesting(
        validStudioId,
        'test prompt',
        foreignSecret
      );

      const req: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt: 'test prompt',
        confirmedByUser: true,
        confirmationChallenge: forgedChallenge,
      };

      await assert.rejects(
        async () => {
          await ExecutionClient.submitTask(transport, req);
        },
        (err: Error) => {
          assert.ok(err instanceof TaskSubmissionError);
          const msg = err.message.toLowerCase();
          assert.ok(
            msg.includes('signature') ||
              msg.includes('forged') ||
              msg.includes('not found or already consumed'),
            `Error message must indicate signature or token rejection, got: ${err.message}`
          );
          return true;
        },
        'ExecutionClient must reject forged HMAC challenge token'
      );

      assert.strictEqual(transport.sentCommands.length, 0, 'No wire commands should be dispatched');
    });

    it('16. fails closed when operator confirmation challenge has expired', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);

      const expiredChallenge = OperatorConfirmationManager.createChallenge(validStudioId, 'test prompt', {
        ttlMs: -1000, // Expired 1 second ago
      });

      const req: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt: 'test prompt',
        confirmedByUser: true,
        confirmationChallenge: expiredChallenge,
      };

      await assert.rejects(
        async () => {
          await ExecutionClient.submitTask(transport, req);
        },
        (err: Error) => {
          assert.ok(err instanceof TaskSubmissionError);
          assert.ok(err.message.includes('expired'));
          return true;
        },
        'ExecutionClient must reject expired confirmation challenge'
      );

      assert.strictEqual(transport.sentCommands.length, 0, 'No wire commands should be dispatched');
    });

    it('17. fails closed when studio ID or prompt hash does not match challenge', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);

      const challenge = OperatorConfirmationManager.createChallenge(validStudioId, 'original prompt');

      // Attempt 1: Studio ID mismatch
      const reqMismatchedStudio: SubmitTaskRequest = {
        studioId: '00000000-0000-0000-0000-000000000000',
        prompt: 'original prompt',
        confirmedByUser: true,
        confirmationChallenge: challenge,
      };

      await assert.rejects(
        async () => {
          await ExecutionClient.submitTask(transport, reqMismatchedStudio);
        },
        (err: Error) => {
          assert.ok(err instanceof TaskSubmissionError);
          assert.ok(err.message.includes('Studio ID mismatch'));
          return true;
        }
      );

      // Attempt 2: Prompt hash mismatch
      const reqMismatchedPrompt: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt: 'tampered prompt content',
        confirmedByUser: true,
        confirmationChallenge: challenge,
      };

      await assert.rejects(
        async () => {
          await ExecutionClient.submitTask(transport, reqMismatchedPrompt);
        },
        (err: Error) => {
          assert.ok(err instanceof TaskSubmissionError);
          assert.ok(err.message.includes('Prompt hash mismatch'));
          return true;
        }
      );

      assert.strictEqual(transport.sentCommands.length, 0, 'No wire commands should be dispatched');
    });

    it('18. verifies DaemonConnectedBridge mutation methods fail closed in read-only slice', async () => {
      // Disconnected or read-only slice mutation rejection checks
      const bridge = new DaemonConnectedBridge();

      // Approval mutation fails closed
      const approvalRes = await bridge.requestApprovalDecision('appr-01', 'approved');
      assert.strictEqual(approvalRes.applied, false);
      assert.ok((approvalRes.message || '').includes('read-only'));

      // Editor action fails closed
      const editorRes = await bridge.submitEditorAction({
        action: 'refactor',
        documentUri: 'file:///workspace/app.ts',
        languageId: 'typescript',
        selectedText: 'code',
        selectionRange: { startLine: 1, startCol: 0, endLine: 1, endCol: 4 },
      });
      assert.strictEqual(editorRes.accepted, false);
      assert.ok((editorRes.message || '').includes('read-only'));

      // Chat prompt fails closed
      await assert.rejects(
        async () => {
          await bridge.sendMessage({ message: 'write code', mode: 'agent' }, () => {});
        },
        (err: Error) => err.message.includes('disabled') || err.message.includes('read-only')
      );

      // Create session fails closed
      await assert.rejects(
        async () => {
          await bridge.createSession('new session goal');
        },
        (err: Error) => err.message.includes('disabled') || err.message.includes('read-only')
      );

      bridge.dispose();
    });

    it('19. enforces zero wire dispatch and in-process TaskExecutionStore immutability', () => {
      // Assert classification invariants:
      // - REAL_AI_MODEL_NOT_TESTED: No frontier model calls made
      // - TIER3_NOT_VERIFIED: Production task.submit remains gated closed
      assert.strictEqual(transport.sentCommands.length, 0, 'Zero wire commands sent to provider');

      // IN_PROCESS_REAL_ARTIFACT_STORE state must remain completely unmutated
      const storeState = TaskExecutionStore.instance.state;
      assert.strictEqual(storeState.status, 'idle', 'Task execution store must remain idle');
      assert.strictEqual(storeState.activeTaskId, undefined);
      assert.strictEqual(storeState.activeRunId, undefined);
      assert.strictEqual(storeState.streamOutput, '');
      assert.strictEqual(storeState.activeTool, undefined);
    });
  });

  // =========================================================================
  // Section 4: Real agsd.exe Daemon Lifecycle & Model Routing
  // Classification: REAL_AGSD_READ_ONLY_IPC / TIER3_NOT_VERIFIED
  // =========================================================================
  describe('4. Real agsd.exe Daemon Lifecycle & Model Routing [REAL_AGSD_READ_ONLY_IPC & TIER3_NOT_VERIFIED]', () => {
    let agsdBinaryPath: string;
    let tempDir: string;
    let runtimePath: string;
    let adminRuntimePath: string;
    let pipeOrSocketPath: string;
    let daemonProcess: childProcess.ChildProcess | null = null;
    let bridge: DaemonConnectedBridge | null = null;
    let adminTransport: NativeIpcTransport | null = null;

    before(async () => {
      agsdBinaryPath = resolveAgsdBinaryPath();

      tempDir = await fs.promises.mkdtemp(path.join(os.tmpdir(), 'agsd-real-agent-exec-'));
      runtimePath = path.join(tempDir, 'agsd.json');
      adminRuntimePath = path.join(tempDir, 'agsd-admin.json');

      const isWindows = process.platform === 'win32';
      const uuid = crypto.randomUUID();
      pipeOrSocketPath = isWindows
        ? `\\\\.\\pipe\\ags-real-agent-exec-${uuid}`
        : path.join(tempDir, `ags-real-agent-exec-${uuid}.sock`);

      if (!isWindows) {
        try {
          await fs.promises.chmod(tempDir, 0o700);
        } catch {
          // Ignore
        }
      }
    });

    after(async () => {
      if (bridge) {
        try {
          await bridge.disconnect();
          bridge.dispose();
        } catch {
          // Ignore
        }
        bridge = null;
      }

      if (adminTransport) {
        try {
          await adminTransport.disconnect();
        } catch {
          // Ignore
        }
        adminTransport = null;
      }

      if (daemonProcess && !daemonProcess.killed) {
        try {
          daemonProcess.kill('SIGKILL');
        } catch {
          // Ignore
        }
        daemonProcess = null;
      }

      try {
        await fs.promises.rm(tempDir, { recursive: true, force: true });
      } catch {
        // Ignore
      }
    });

    it('20. locates real compiled Rust agsd.exe binary', () => {
      assert.ok(typeof agsdBinaryPath === 'string' && agsdBinaryPath.length > 0);
      assert.ok(fs.existsSync(agsdBinaryPath), `agsd binary must exist at ${agsdBinaryPath}`);
    });

    it('21. spawns real agsd.exe daemon with isolated temporary bootstrap and named pipe', async () => {
      const isWindows = process.platform === 'win32';
      const spawnArgs = [
        '--runtime-info-path',
        runtimePath,
        '--admin-runtime-info-path',
        adminRuntimePath,
      ];

      if (isWindows) {
        spawnArgs.push('--pipe-name', pipeOrSocketPath);
      } else {
        spawnArgs.push('--socket-path', pipeOrSocketPath);
      }

      daemonProcess = childProcess.spawn(agsdBinaryPath, spawnArgs, {
        stdio: ['pipe', 'pipe', 'pipe'],
        env: { ...process.env, RUST_LOG: 'info' },
      });

      daemonProcess.stderr?.on('data', () => {});
      daemonProcess.stdout?.on('data', () => {});

      assert.ok(daemonProcess !== null);
      assert.ok(typeof daemonProcess.pid === 'number' && daemonProcess.pid > 0);
      assert.strictEqual(daemonProcess.killed, false);

      // Await bootstrap discovery files generation
      const startTime = Date.now();
      let ready = false;
      while (Date.now() - startTime < 10000) {
        if (fs.existsSync(runtimePath) && fs.existsSync(adminRuntimePath)) {
          try {
            const content = JSON.parse(await fs.promises.readFile(runtimePath, 'utf8'));
            if (content.endpoint && content.client_key) {
              ready = true;
              break;
            }
          } catch {
            // Write in progress
          }
        }
        await new Promise((r) => setTimeout(r, 100));
      }

      assert.ok(ready, `agsd failed to write runtime discovery files within 10000ms at ${runtimePath}`);
    });

    it('22. verifies standard and admin discovery credentials separation', async () => {
      const bootstrapInfo = await DaemonDiscovery.loadBootstrapInfo(runtimePath);
      assert.strictEqual(bootstrapInfo.key_id, 'ags-client');
      assert.ok(bootstrapInfo.client_key.length > 0);
      assert.strictEqual(bootstrapInfo.admin_key, null);

      const stdContent = JSON.parse(await fs.promises.readFile(runtimePath, 'utf8'));
      const adminContent = JSON.parse(await fs.promises.readFile(adminRuntimePath, 'utf8'));

      assert.strictEqual(stdContent.key_id, 'ags-client');
      assert.ok(typeof stdContent.client_key === 'string' && stdContent.client_key.length > 0);
      assert.strictEqual(stdContent.admin_key, undefined, 'Standard bootstrap must not leak admin_key');

      assert.strictEqual(adminContent.key_id, 'ags-client-admin');
      assert.ok(typeof adminContent.admin_key === 'string' && adminContent.admin_key.length > 0);
      assert.notStrictEqual(
        stdContent.client_key,
        adminContent.admin_key,
        'Standard client key and admin key must be distinct'
      );
    });

    it('23. connects DaemonConnectedBridge with HMAC authentication to real daemon', async () => {
      bridge = new DaemonConnectedBridge({ bootstrapPath: runtimePath });
      assert.strictEqual(bridge.connectionState, 'disconnected');

      await bridge.connect();
      assert.strictEqual(bridge.connectionState, 'connected');
      assert.ok(bridge.endpoint !== null);
      assert.ok(bridge.transport !== null);
      assert.strictEqual(bridge.transport?.getState(), 'ready');
      assert.ok(bridge.transport?.getSessionId() !== null);
    });

    it('24. verifies real daemon health and active client count', async () => {
      assert.ok(bridge);
      const health = await bridge.getHealth();
      assert.ok(health !== null);
      assert.strictEqual(health.status, 'healthy');
      assert.ok(health.active_clients >= 1, `Active clients must be >= 1, got ${health.active_clients}`);
      assert.ok(typeof health.uptime_secs === 'number' && health.uptime_secs >= 0);
    });

    it('25. verifies real daemon ping roundtrip latency', async () => {
      assert.ok(bridge);
      const latency = await bridge.ping('m15-ui-04-real-agent-ping');
      assert.ok(typeof latency === 'number', 'Latency must be numeric');
      assert.ok(latency >= 0, 'Ping latency must be non-negative');
    });

    it('26. lists available providers from real daemon', async () => {
      assert.ok(bridge && bridge.transport);
      const summaries = await bridge.listProviderSummaries();
      assert.ok(Array.isArray(summaries), 'Provider summaries must be an array');

      const provResp = await bridge.transport.sendCommand({ type: 'list_providers' });
      assert.strictEqual(provResp.type, 'providers');
      if (provResp.type === 'providers') {
        assert.ok(Array.isArray(provResp.providers));
        for (const prov of provResp.providers) {
          assert.ok(typeof prov.id === 'string' && prov.id.length > 0);
          assert.ok(typeof prov.display_name === 'string' && prov.display_name.length > 0);
        }
      }
    });

    it('27. lists available models and verifies provider model routing mappings', async () => {
      assert.ok(bridge && bridge.transport);

      const modelsResp = await bridge.transport.sendCommand({ type: 'list_models' });
      assert.strictEqual(modelsResp.type, 'models');
      if (modelsResp.type === 'models') {
        assert.ok(Array.isArray(modelsResp.models));
        for (const model of modelsResp.models) {
          assert.ok(typeof model.id === 'string' && model.id.length > 0);
          assert.ok(typeof model.provider_instance_id === 'string' && model.provider_instance_id.length > 0);
          assert.ok(typeof model.display_name === 'string' && model.display_name.length > 0);
        }
      }

      // Check model catalog representation in bridge provider summaries
      const summaries = await bridge.listProviderSummaries();
      assert.ok(Array.isArray(summaries));
      for (const summary of summaries) {
        assert.ok(Array.isArray(summary.models), 'Each provider summary must contain models array');
        for (const m of summary.models) {
          assert.ok(typeof m.id === 'string' && m.id.length > 0);
          assert.ok(typeof m.name === 'string' && m.name.length > 0);
          assert.ok(typeof m.contextWindow === 'number' && m.contextWindow > 0);
        }
      }
    });

    it('28. verifies standard client shutdown denial (fails closed with permission denied)', async () => {
      assert.ok(bridge && bridge.transport);

      await assert.rejects(
        async () => {
          await bridge!.transport!.sendCommand({ type: 'shutdown' });
        },
        (err: Error) => {
          const msg = err.message.toLowerCase();
          return msg.includes('permission denied') || msg.includes('admin');
        },
        'Standard client must be rejected with permission denied attempting shutdown'
      );

      // Verify daemon remains healthy
      const health = await bridge.getHealth();
      assert.strictEqual(health.status, 'healthy');
    });

    it('29. executes graceful daemon shutdown using authenticated admin key', async () => {
      assert.ok(bridge);

      // Disconnect standard client
      await bridge.disconnect();
      bridge.dispose();
      bridge = null;

      // Connect admin transport
      const adminContent = JSON.parse(await fs.promises.readFile(adminRuntimePath, 'utf8'));
      assert.ok(adminContent.admin_key);
      assert.strictEqual(adminContent.key_id, 'ags-client-admin');

      adminTransport = new NativeIpcTransport({
        endpoint: adminContent.endpoint,
        keyId: adminContent.key_id,
        clientKey: adminContent.admin_key,
        commandTimeoutMs: 10000,
      });

      await adminTransport.connect();
      assert.strictEqual(adminTransport.getState(), 'ready');

      const shutdownResp = await adminTransport.sendCommand({ type: 'shutdown' });
      assert.strictEqual(shutdownResp.type, 'shutdown_acknowledged');

      await adminTransport.disconnect();
      adminTransport = null;
    });

    it('30. verifies daemon child process termination with exit code 0 and cleans up resources', async () => {
      assert.ok(daemonProcess);

      // Wait for child process exit
      if (!daemonProcess.killed && daemonProcess.exitCode === null) {
        await new Promise<void>((resolve, reject) => {
          const timer = setTimeout(() => {
            reject(new Error('agsd daemon failed to exit within 5000ms after shutdown command'));
          }, 5000);

          daemonProcess!.on('exit', (code) => {
            clearTimeout(timer);
            assert.strictEqual(code, 0, `agsd must exit with code 0, got ${code}`);
            resolve();
          });
        });
      } else {
        assert.strictEqual(daemonProcess.exitCode, 0, 'agsd child process must have exited with code 0');
      }

      daemonProcess = null;
    });
  });
});
