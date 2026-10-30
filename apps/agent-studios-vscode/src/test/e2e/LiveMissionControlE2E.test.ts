/**
 * Agent Studios — Live Mission Control Data Projection E2E Test Suite
 * Milestone M14 Mission Control Integration
 *
 * Verifies live Mission Control data projection against seeded Control Plane
 * and native daemon over authentic OS IPC (Named Pipes / Domain Sockets)
 * using HMAC-authenticated wire framing and role-based access control.
 */

import * as assert from 'assert';
import * as childProcess from 'child_process';
import * as crypto from 'crypto';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';

import { DaemonConnectedBridge } from '../../bridge/DaemonConnectedBridge';
import { BridgeManager } from '../../bridge/BridgeManager';
import { NativeIpcTransport } from '../../bridge/ipc/NativeIpcTransport';
import { MissionControlProjection } from '../../mission-control/state/MissionControlProjection';
import { MissionControlState } from '../../mission-control/models/types';
import {
  RustIpcResponse,
  RustStudio,
} from '../../bridge/ipc/RustIpcProtocolTypes';

function resolveAgsdBinaryPath(): string | null {
  const isWindows = process.platform === 'win32';
  const exeSuffix = isWindows ? '.exe' : '';

  const candidatePaths = [
    process.env.AGSD_BIN_PATH,
    isWindows ? 'D:\\Projects\\agent-studios-m13\\agent-studios-rs\\target\\debug\\agsd.exe' : undefined,
    path.resolve(__dirname, '../../../../../../agent-studios-m13/agent-studios-rs/target/debug/agsd' + exeSuffix),
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

  return null;
}

describe('Live Mission Control Data Projection E2E', () => {
  let tempDir: string;
  let runtimePath: string;
  let adminRuntimePath: string;
  let pipeOrSocketPath: string;
  let daemonProcess: childProcess.ChildProcess | null = null;
  let daemonPid: number = 0;

  let adminTransport: NativeIpcTransport | null = null;
  let bridge: DaemonConnectedBridge | null = null;
  let bridgeManager: BridgeManager | null = null;
  let projection: MissionControlProjection | null = null;

  let seededStudio: RustStudio | null = null;
  const clientKey = 'synthetic-client-key-1234567890123456';
  const adminKey = 'synthetic-admin-key-9876543210654321';

  before(async () => {
    // 1. Create isolated temporary directory
    tempDir = await fs.promises.mkdtemp(path.join(os.tmpdir(), 'ags-mc-e2e-'));
    runtimePath = path.join(tempDir, 'agsd.json');
    adminRuntimePath = path.join(tempDir, 'agsd-admin.json');

    // 2. Setup cross-platform IPC endpoint
    const isWindows = process.platform === 'win32';
    const uuid = crypto.randomUUID();
    pipeOrSocketPath = isWindows
      ? `\\\\.\\pipe\\ags-mc-e2e-${uuid}`
      : path.join(tempDir, `ags-mc-e2e-${uuid}.sock`);
    if (!isWindows) {
      try {
        await fs.promises.chmod(tempDir, 0o700);
      } catch {
        // Ignore
      }
    }

    const daemonScript = path.resolve(__dirname, 'StandaloneMockDaemon.js');
    assert.ok(fs.existsSync(daemonScript), `Mock daemon script must exist at ${daemonScript}`);

    // 3. Spawn cross-process daemon with separate PID and memory
    await new Promise<void>((resolve, reject) => {
      const child = childProcess.spawn(
        process.execPath,
        [
          daemonScript,
          '--bootstrap-path',
          runtimePath,
          '--admin-bootstrap-path',
          adminRuntimePath,
          isWindows ? '--pipe-name' : '--socket-path',
          pipeOrSocketPath,
          '--key-id',
          'ags-client',
          '--client-key',
          clientKey,
          '--admin-key',
          adminKey,
        ],
        {
          stdio: ['pipe', 'pipe', 'pipe'],
          env: { ...process.env },
        }
      );

      daemonProcess = child;

      let started = false;
      const timeout = setTimeout(() => {
        if (!started) {
          child.kill();
          reject(new Error('Cross-process daemon failed to launch within 6000ms.'));
        }
      }, 6000);

      child.stdout?.on('data', (chunk: Buffer) => {
        const text = chunk.toString('utf8');
        const match = text.match(/DAEMON_READY:(\d+):(.+)/);
        if (match) {
          started = true;
          clearTimeout(timeout);
          daemonPid = parseInt(match[1], 10);
          resolve();
        }
      });

      child.stderr?.on('data', (errChunk: Buffer) => {
        console.error('[MockDaemon Stderr]:', errChunk.toString('utf8'));
      });

      child.on('error', (err) => {
        clearTimeout(timeout);
        reject(err);
      });
    });

    assert.ok(daemonPid > 0, 'Daemon PID must be positive non-zero integer');
    assert.ok(fs.existsSync(runtimePath), 'agsd.json bootstrap file must exist on disk');
    assert.ok(fs.existsSync(adminRuntimePath), 'agsd-admin.json admin bootstrap must exist on disk');

    // 4. Initialize Admin IPC transport for privileged Control Plane seeding
    adminTransport = new NativeIpcTransport({
      endpoint: pipeOrSocketPath,
      keyId: 'ags-client-admin',
      clientKey: adminKey,
      commandTimeoutMs: 5000,
    });
    await adminTransport.connect();
    assert.strictEqual(adminTransport.getState(), 'ready', 'Admin transport must be ready');

    // 5. Initialize standard DaemonConnectedBridge
    bridge = new DaemonConnectedBridge({
      bootstrapPath: runtimePath,
      autoSubscribeEvents: true,
    });
    bridgeManager = new BridgeManager(bridge);
    projection = new MissionControlProjection(bridgeManager);
  });

  after(async () => {
    if (projection) {
      projection.dispose();
      projection = null;
    }
    if (bridgeManager) {
      bridgeManager.dispose();
      bridgeManager = null;
    }
    if (bridge) {
      try {
        await bridge.disconnect();
        bridge.dispose();
      } catch {
        // Safe ignore
      }
      bridge = null;
    }
    if (adminTransport) {
      try {
        await adminTransport.disconnect();
      } catch {
        // Safe ignore
      }
      adminTransport = null;
    }
    if (daemonProcess && !daemonProcess.killed) {
      try {
        daemonProcess.kill();
      } catch {
        // Safe ignore
      }
      daemonProcess = null;
    }
    try {
      await fs.promises.rm(tempDir, { recursive: true, force: true });
    } catch {
      // Safe ignore
    }
  });

  it('1. Standard client fails closed on mutations while admin/raw IPC can seed data', async () => {
    assert.ok(bridge, 'Bridge must be initialized');
    assert.ok(adminTransport, 'Admin transport must be initialized');

    // Standard client MUST fail closed on mutations (read-only slice)
    await assert.rejects(
      async () => {
        await bridge!.createSession('Standard Unauthorized Studio');
      },
      (err: Error) => {
        return err.message.includes('disabled') || err.message.includes('read-only');
      },
      'Standard client createSession must fail closed'
    );

    await assert.rejects(
      async () => {
        await bridge!.sendMessage(
          {
            mode: 'ask',
            message: 'Execute unauthorized task creation',
          },
          () => {}
        );
      },
      (err: Error) => {
        return err.message.includes('disabled') || err.message.includes('read-only');
      },
      'Standard client sendMessage must fail closed'
    );

    const approvalRes = await bridge!.requestApprovalDecision('mock-appr-01', 'approved');
    assert.strictEqual(approvalRes.applied, false, 'Standard client approval mutation must fail closed');
    assert.ok(
      approvalRes.message?.includes('disabled') || approvalRes.message?.includes('read-only'),
      'Approval mutation response must explain read-only restriction'
    );

    // Admin / raw IPC can seed real data into the daemon's Control Plane
    const studioResp = (await adminTransport.sendCommand({
      type: 'create_studio',
      name: 'Mission Control E2E Studio',
    })) as RustIpcResponse;

    assert.strictEqual(studioResp.type, 'studio_created', 'Admin studio creation must succeed');
    if (studioResp.type === 'studio_created') {
      seededStudio = studioResp.studio;
      assert.strictEqual(seededStudio.name, 'Mission Control E2E Studio');
    }

    assert.ok(seededStudio, 'Seeded studio must be recorded');

    // Seed task 1: "Compile Rust agsd" with status "succeeded"
    const task1Resp = (await adminTransport.sendCommand({
      type: 'submit_task',
      studio_id: seededStudio.id,
      task_spec: {
        key: 'task-compile-01',
        title: 'Compile Rust agsd',
        description: 'Compile authoritative native binary with cargo',
        state: 'succeeded',
        dependency_keys: [],
        dependency_task_ids: [],
      } as unknown as { key: string; title: string; description: string; state: string },
    })) as RustIpcResponse;

    assert.strictEqual(task1Resp.type, 'tasks_submitted', 'Task 1 seeding must succeed');

    // Seed task 2: "Run E2E Suite" with status "running"
    const task2Resp = (await adminTransport.sendCommand({
      type: 'submit_task',
      studio_id: seededStudio.id,
      task_spec: {
        key: 'task-run-02',
        title: 'Run E2E Suite',
        description: 'Execute end-to-end integration test suite',
        state: 'running',
        dependency_keys: ['task-compile-01'],
        dependency_task_ids: ['task-compile-01'],
      } as unknown as { key: string; title: string; description: string; state: string },
    })) as RustIpcResponse;

    assert.strictEqual(task2Resp.type, 'tasks_submitted', 'Task 2 seeding must succeed');
  });

  it('2. Connect DaemonConnectedBridge and initialize MissionControlProjection refresh', async () => {
    assert.ok(bridge, 'Bridge must be initialized');
    assert.ok(projection, 'Projection must be initialized');
    assert.ok(seededStudio, 'Seeded studio must exist');

    await bridge.connect();
    assert.strictEqual(bridge.connectionState, 'connected');

    // Point bridge to seeded studio session if needed
    await bridge.switchSession(seededStudio.id);

    // Refresh projection to hydrate all read models from live daemon
    await projection.refresh();
    const state: MissionControlState = projection.getState();

    assert.strictEqual(state.connectionState, 'connected');
    assert.strictEqual(state.isPreview, false);
  });

  it('3. Studio appears in Mission Control (goalSummary and activeStudio match seeded state)', () => {
    assert.ok(projection, 'Projection must be initialized');
    const state = projection.getState();

    // Invariant: Studio goal summary matches authoritative seeded studio name
    assert.strictEqual(
      state.currentSession?.goalSummary,
      'Mission Control E2E Studio',
      'currentSession.goalSummary must match seeded studio name'
    );

    // Invariant: activeStudio read model populated
    assert.strictEqual(
      state.activeStudio?.name,
      'Mission Control E2E Studio',
      'activeStudio.name must match seeded studio name'
    );
    assert.strictEqual(
      state.activeStudio?.id,
      seededStudio?.id,
      'activeStudio.id must match seeded studio ID'
    );
  });

  it('4. Real task ID and description display accurately in state.tasks', () => {
    assert.ok(projection, 'Projection must be initialized');
    const state = projection.getState();

    assert.ok(Array.isArray(state.tasks), 'state.tasks must be an array');
    assert.ok(state.tasks.length >= 2, 'state.tasks must contain at least 2 seeded tasks');

    const compileTask = state.tasks.find((t) => t.title === 'Compile Rust agsd');
    assert.ok(compileTask, 'Task "Compile Rust agsd" must be present');
    assert.strictEqual(compileTask.id, 'task-compile-01');
    assert.strictEqual(
      compileTask.description,
      'Compile authoritative native binary with cargo'
    );

    const runTask = state.tasks.find((t) => t.title === 'Run E2E Suite');
    assert.ok(runTask, 'Task "Run E2E Suite" must be present');
    assert.strictEqual(runTask.id, 'task-run-02');
    assert.strictEqual(
      runTask.description,
      'Execute end-to-end integration test suite'
    );
  });

  it('5. Task status matches Rust state (succeeded, running)', () => {
    assert.ok(projection, 'Projection must be initialized');
    const state = projection.getState();

    const compileTask = state.tasks.find((t) => t.title === 'Compile Rust agsd');
    assert.ok(compileTask);
    assert.strictEqual(
      compileTask.state,
      'succeeded',
      'Task "Compile Rust agsd" state must be "succeeded"'
    );

    const runTask = state.tasks.find((t) => t.title === 'Run E2E Suite');
    assert.ok(runTask);
    assert.strictEqual(
      runTask.state,
      'running',
      'Task "Run E2E Suite" state must be "running"'
    );

    // Overview counters reflect truthful state
    assert.ok(state.overview.taskCounts.succeeded >= 1);
    assert.ok(state.overview.taskCounts.running >= 1);
  });

  it('6. Task progress is truthful: 100% for succeeded, undefined (indeterminate) for running', () => {
    assert.ok(projection, 'Projection must be initialized');
    const state = projection.getState();

    const compileTask = state.tasks.find((t) => t.title === 'Compile Rust agsd');
    assert.ok(compileTask);
    assert.strictEqual(
      compileTask.progress,
      100,
      'Succeeded task progress must be truthful 100%'
    );

    const runTask = state.tasks.find((t) => t.title === 'Run E2E Suite');
    assert.ok(runTask);
    assert.strictEqual(
      runTask.progress,
      undefined,
      'Running task progress must be undefined (indeterminate), never false 0% or hardcoded estimate'
    );
  });

  it('7. Real providers and models hydrated from daemon (Anthropic Claude, OpenAI GPT)', () => {
    assert.ok(projection, 'Projection must be initialized');
    const state = projection.getState();

    assert.ok(Array.isArray(state.providers), 'state.providers must be an array');
    assert.ok(state.providers.length >= 2, 'Must hydrate real providers from daemon');

    const claudeProvider = state.providers.find((p) =>
      p.displayName.includes('Claude') || p.id.includes('anthropic')
    );
    assert.ok(claudeProvider, 'Anthropic Claude provider must be present');
    assert.strictEqual(claudeProvider.displayName, 'Anthropic Claude');
    assert.strictEqual(claudeProvider.modelId, 'Claude 3.7 Sonnet');
    assert.strictEqual(claudeProvider.contextWindow, 200000);

    const gptProvider = state.providers.find((p) =>
      p.displayName.includes('GPT') || p.id.includes('openai')
    );
    assert.ok(gptProvider, 'OpenAI GPT provider must be present');
    assert.strictEqual(gptProvider.displayName, 'OpenAI GPT');
    assert.strictEqual(gptProvider.modelId, 'GPT-4o');
    assert.strictEqual(gptProvider.contextWindow, 128000);
  });

  it('8. Provider latency is unmeasured (undefined), NOT hardcoded 120ms', () => {
    assert.ok(projection, 'Projection must be initialized');
    const state = projection.getState();

    for (const prov of state.providers) {
      assert.strictEqual(
        prov.latencyMs,
        undefined,
        `Provider '${prov.displayName}' latencyMs must be undefined (unmeasured), not hardcoded 120ms`
      );
    }
  });

  it('9. Event subscription updates timeline and attention queue when events are published', async () => {
    assert.ok(projection, 'Projection must be initialized');
    assert.ok(adminTransport, 'Admin transport must be initialized');

    // Admin broadcasts a critical task_failed event across the socket
    const failSummary = 'Compilation linker failure: missing agsd.lib';
    await adminTransport.sendCommand({
      type: 'emit_test_event',
      event_type: 'task_failed',
      payload: {
        summary: failSummary,
        task_id: 'task-compile-01',
        error: 'LNK1181: cannot open input file agsd.lib',
      },
    } as unknown as RustIpcResponse as any);

    // Wait briefly for asynchronous wire frame delivery and projection state emission
    const deadline = Date.now() + 3000;
    let foundTimeline = false;
    let foundAttention = false;

    while (Date.now() < deadline) {
      const currentState = projection.getState();

      foundTimeline = currentState.timeline.some((e) => e.summary.includes(failSummary));
      foundAttention = currentState.overview.attentionItems.some(
        (a) => a.title.includes(failSummary) || a.description.includes('LNK1181')
      );

      if (foundTimeline && foundAttention) {
        break;
      }
      await new Promise((r) => setTimeout(r, 50));
    }

    assert.ok(foundTimeline, 'Published event must appear in state.timeline');
    assert.ok(foundAttention, 'Failed task event must produce alert in overview.attentionItems');
  });

  it('10. Query errors produce diagnostics alert in AttentionQueue, not false zeroes', async () => {
    assert.ok(projection, 'Projection must be initialized');
    assert.ok(bridge, 'Bridge must be initialized');

    // Simulate endpoint query failure on bridge.listTasks
    const originalListTasks = bridge.listTasks.bind(bridge);
    bridge.listTasks = async () => {
      throw new Error('Simulated daemon endpoint timeout / ECONNRESET');
    };

    try {
      await projection.refresh();
      const erroredState = projection.getState();

      // Query error recorded under diagnostics
      assert.ok(erroredState.queryErrors, 'queryErrors map must be present');
      assert.ok(
        erroredState.queryErrors.tasks?.includes('Simulated daemon endpoint timeout'),
        'Tasks query error must be captured'
      );

      // Attention queue contains diagnostic alert
      const queryAlert = erroredState.overview.attentionItems.find(
        (a) => a.id === 'att-query-errors'
      );
      assert.ok(queryAlert, 'AttentionQueue must contain diagnostic alert for query failure');
      assert.strictEqual(queryAlert.targetTab, 'diagnostics');
      assert.strictEqual(queryAlert.severity, 'warning');

      // Diagnostics record retains error information
      assert.ok(
        erroredState.diagnostics.lastError?.includes('Simulated daemon endpoint timeout'),
        'Diagnostics lastError must record the failure'
      );
    } finally {
      // Restore clean query function
      bridge.listTasks = originalListTasks;
      await projection.refresh();
    }

    const restoredState = projection.getState();
    assert.strictEqual(
      restoredState.overview.attentionItems.some((a) => a.id === 'att-query-errors'),
      false,
      'Query error alert must clear upon healthy refresh'
    );
  });

  it('11. Real compiled Rust daemon (agsd.exe) verification if binary present', async function () {
    const agsdBin = resolveAgsdBinaryPath();
    if (!agsdBin) {
      console.log('[LiveMC] Real agsd.exe binary not found, skipping real binary subtest.');
      return;
    }

    console.log(`[LiveMC] Found real agsd.exe at: ${agsdBin}`);
    const isWindows = process.platform === 'win32';
    const realTempDir = await fs.promises.mkdtemp(path.join(os.tmpdir(), 'ags-real-mc-'));
    const realRuntimePath = path.join(realTempDir, 'agsd.json');
    const realAdminRuntimePath = path.join(realTempDir, 'agsd-admin.json');
    const realPipeOrSocket = isWindows
      ? `\\\\.\\pipe\\ags-real-mc-${crypto.randomUUID()}`
      : path.join(realTempDir, `ags-real-mc-${crypto.randomUUID()}.sock`);
    if (!isWindows) {
      try {
        await fs.promises.chmod(realTempDir, 0o700);
      } catch {
        // Ignore
      }
    }

    const realDaemon = childProcess.spawn(
      agsdBin,
      [
        '--runtime-info-path',
        realRuntimePath,
        '--admin-runtime-info-path',
        realAdminRuntimePath,
        isWindows ? '--pipe-name' : '--socket-path',
        realPipeOrSocket,
      ],
      {
        stdio: ['pipe', 'pipe', 'pipe'],
        env: { ...process.env, RUST_LOG: 'info' },
      }
    );

    let realBridge: DaemonConnectedBridge | null = null;
    let realAdmin: NativeIpcTransport | null = null;
    let realProjection: MissionControlProjection | null = null;
    let realManager: BridgeManager | null = null;

    try {
      // Wait for bootstrap files to be written
      const deadline = Date.now() + 5000;
      while (
        (!fs.existsSync(realRuntimePath) || !fs.existsSync(realAdminRuntimePath)) &&
        Date.now() < deadline
      ) {
        await new Promise((r) => setTimeout(r, 50));
      }

      assert.ok(fs.existsSync(realRuntimePath), 'Real daemon agsd.json must be written');
      assert.ok(fs.existsSync(realAdminRuntimePath), 'Real daemon agsd-admin.json must be written');

      const adminContent = JSON.parse(await fs.promises.readFile(realAdminRuntimePath, 'utf8'));

      // Connect admin transport to real daemon
      realAdmin = new NativeIpcTransport({
        endpoint: adminContent.endpoint,
        keyId: adminContent.key_id,
        clientKey: adminContent.admin_key,
        commandTimeoutMs: 5000,
      });
      await realAdmin.connect();
      assert.strictEqual(realAdmin.getState(), 'ready');

      // Seed real studio in real agsd
      const studioResp = (await realAdmin.sendCommand({
        type: 'create_studio',
        name: 'Mission Control E2E Studio',
      })) as RustIpcResponse;

      assert.strictEqual(studioResp.type, 'studio_created');
      let realStudioId = '';
      if (studioResp.type === 'studio_created') {
        realStudioId = studioResp.studio.id;
      }

      // Connect real standard bridge
      realBridge = new DaemonConnectedBridge({
        bootstrapPath: realRuntimePath,
        autoSubscribeEvents: true,
      });
      await realBridge.connect();
      assert.strictEqual(realBridge.connectionState, 'connected');

      // Standard client mutations fail closed against real Rust daemon
      await assert.rejects(
        async () => {
          await realBridge!.createSession('Mutate Standard Fail');
        },
        (err: Error) => err.message.includes('disabled') || err.message.includes('read-only'),
        'Standard client mutations must fail closed on real Rust daemon'
      );

      // Wire real MissionControlProjection
      realManager = new BridgeManager(realBridge);
      realProjection = new MissionControlProjection(realManager);
      await realBridge.switchSession(realStudioId);
      await realProjection.refresh();

      const realState = realProjection.getState();
      assert.strictEqual(
        realState.currentSession?.goalSummary,
        'Mission Control E2E Studio',
        'Real Rust daemon studio name must match goalSummary'
      );
      assert.strictEqual(
        realState.activeStudio?.name,
        'Mission Control E2E Studio',
        'Real Rust daemon studio name must match activeStudio'
      );
      assert.strictEqual(realState.isPreview, false);
      assert.strictEqual(realState.connectionState, 'connected');
    } finally {
      if (realProjection) {
        realProjection.dispose();
      }
      if (realManager) {
        realManager.dispose();
      }
      if (realBridge) {
        try {
          await realBridge.disconnect();
          realBridge.dispose();
        } catch {
          // Safe ignore
        }
      }
      if (realAdmin) {
        try {
          await realAdmin.sendCommand({ type: 'shutdown' });
          await realAdmin.disconnect();
        } catch {
          // Safe ignore
        }
      }
      if (realDaemon && !realDaemon.killed) {
        try {
          realDaemon.kill();
        } catch {
          // Safe ignore
        }
      }
      try {
        await fs.promises.rm(realTempDir, { recursive: true, force: true });
      } catch {
        // Safe ignore
      }
    }
  });
});
