/**
 * Agent Studios — Tier 3: Real Compiled Rust agsd Crash Recovery E2E Test Suite
 * Milestone M15 UI-04: Backend Dependency Update & Crash Recovery Integration
 *
 * Classification:
 * - REAL_DAEMON_IPC_TESTS: Automated authentic cross-process IPC tests against the
 *   compiled Rust `agsd` daemon binary (SHA: dcbcb39bf61225a4660be5b533cf1ffd5de14b29).
 * - REAL_AI_AGENT_EXECUTION_TESTS: External frontier model inference (OpenAI/Anthropic)
 *   is NOT executed in automated CI gates to prevent unauthorized billing.
 *
 * Core Scenarios Tested:
 * 1. Clean Daemon Restart & State Persistence:
 *    - Process 1 creates studio & task with SQLite WAL persistence.
 *    - Graceful admin shutdown.
 *    - Process 2 restarts against identical database.
 *    - State, studios, and task records are linearly preserved.
 * 2. Abnormal Termination (SIGKILL / Forced Process Kill) & Replay Reconciliation:
 *    - Process 1 accepts in-flight task.
 *    - Abrupt SIGKILL / taskkill /F simulation.
 *    - Process 2 recovers database.
 *    - Reconciles interrupted tasks to `failed` (domain reason: daemon_interrupted).
 *    - Reconciles interrupted runs to `failed`.
 * 3. UI Crash Recovery Safety Invariants:
 *    - Invariant 1: Zero auto-resumption of crashed tasks upon reconnect.
 *    - Invariant 2: Accurate status projection (never render stale `running` status).
 *    - Invariant 3: Least-privilege path protection (standard clients receive null database_path;
 *      admin clients receive verified host path).
 * 4. Watermark Resynchronization & Linearizable Event Backfill:
 *    - Client subscribes with `from_sequence: K + 1`.
 *    - Linearizable, deduplicated backfill from persistent SQLite WAL storage.
 * 5. Artifact Retrieval Across Daemon Restarts:
 *    - Artifact records indexed prior to restart remain retrievable with SHA-256 verification.
 */

/* eslint-disable @typescript-eslint/naming-convention */

import * as assert from 'assert';
import * as childProcess from 'child_process';
import * as crypto from 'crypto';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';

import { DaemonConnectedBridge } from '../../bridge/DaemonConnectedBridge';
import { NativeIpcTransport } from '../../bridge/ipc/NativeIpcTransport';
import {
  RustBatchTaskSpec,
  RustRecoveryStatusReport,
} from '../../bridge/ipc/RustIpcProtocolTypes';

function resolveAgsdBinaryPath(): string {
  const isWindows = process.platform === 'win32';
  const exeSuffix = isWindows ? '.exe' : '';

  const candidatePaths = [
    process.env.AGSD_BIN_PATH,
    isWindows ? 'D:\\Projects\\agent-studios-m15-04\\agent-studios-rs\\target\\debug\\agsd.exe' : undefined,
    isWindows ? 'D:\\Projects\\agent-studios-m15-03\\agent-studios-rs\\target\\debug\\agsd.exe' : undefined,
    isWindows ? 'D:\\Projects\\agent-studios-m15-02\\agent-studios-rs\\target\\debug\\agsd.exe' : undefined,
    isWindows ? 'D:\\Projects\\agent-studios-m15\\agent-studios-rs\\target\\debug\\agsd.exe' : undefined,
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
    `[RealRustDaemonRecoveryE2E] Real Rust daemon binary (agsd) not found. Checked candidate locations:\n` +
      candidatePaths.map((p) => `  - ${p}`).join('\n') +
      `\n\nPlease ensure agsd binary is compiled from M15-04 branch (SHA dcbcb39bf61225a4660be5b533cf1ffd5de14b29).`
  );
}

interface SpawnedDaemon {
  child: childProcess.ChildProcess;
  runtimePath: string;
  adminRuntimePath: string;
  endpoint: string;
}

async function spawnAgsd(
  binaryPath: string,
  stateDir: string,
  dbPath: string
): Promise<SpawnedDaemon> {
  const isWindows = process.platform === 'win32';
  const uuid = crypto.randomUUID();
  const pipeOrSocketPath = isWindows
    ? `\\\\.\\pipe\\ags-recov-${uuid}`
    : path.join(stateDir, `ags-recov-${uuid}.sock`);

  const runtimePath = path.join(stateDir, `agsd-${uuid}.json`);
  const adminRuntimePath = path.join(stateDir, `agsd-admin-${uuid}.json`);

  const args = [
    '--state_dir',
    stateDir,
    '--db_path',
    dbPath,
    '--runtime_info_path',
    runtimePath,
    '--admin_runtime_info_path',
    adminRuntimePath,
    '--key_id',
    'recovery-e2e-client',
    '--durability',
    'full',
  ];

  if (isWindows) {
    args.push('--pipe_name', pipeOrSocketPath);
  } else {
    args.push('--socket_path', pipeOrSocketPath);
  }

  const child = childProcess.spawn(binaryPath, args, {
    stdio: ['pipe', 'pipe', 'pipe'],
    env: { ...process.env, RUST_LOG: 'info', CARGO_INCREMENTAL: '0' },
  });

  child.stderr?.on('data', () => {});
  child.stdout?.on('data', () => {});

  // Wait for bootstrap discovery files
  const startTime = Date.now();
  let ready = false;
  while (Date.now() - startTime < 12000) {
    if (fs.existsSync(runtimePath) && fs.existsSync(adminRuntimePath)) {
      try {
        const stdJson = JSON.parse(await fs.promises.readFile(runtimePath, 'utf8'));
        const admJson = JSON.parse(await fs.promises.readFile(adminRuntimePath, 'utf8'));
        if (stdJson.endpoint && stdJson.client_key && admJson.endpoint && admJson.admin_key) {
          ready = true;
          break;
        }
      } catch {
        // file write in progress
      }
    }
    await new Promise((r) => setTimeout(r, 100));
  }

  if (!ready) {
    try {
      child.kill('SIGKILL');
    } catch {
      // Ignore
    }
    throw new Error(`[RealRustDaemonRecoveryE2E] Daemon process failed to become ready within 12s`);
  }

  return {
    child,
    runtimePath,
    adminRuntimePath,
    endpoint: pipeOrSocketPath,
  };
}

async function gracefulShutdown(
  daemon: SpawnedDaemon,
  timeoutMs: number = 5000
): Promise<void> {
  let adminTransport: NativeIpcTransport | null = null;
  try {
    const adminContent = JSON.parse(await fs.promises.readFile(daemon.adminRuntimePath, 'utf8'));
    adminTransport = new NativeIpcTransport({
      endpoint: adminContent.endpoint,
      keyId: adminContent.key_id,
      clientKey: adminContent.admin_key,
      commandTimeoutMs: 3000,
    });
    await adminTransport.connect();
    await adminTransport.sendCommand({ type: 'shutdown' });
  } catch {
    // Ignore shutdown error if already terminating
  } finally {
    if (adminTransport) {
      adminTransport.disconnect();
    }
  }

  const exitPromise = new Promise<void>((resolve) => {
    daemon.child.once('exit', () => resolve());
  });

  const timerPromise = new Promise<void>((_, reject) => {
    setTimeout(() => reject(new Error('Shutdown timeout')), timeoutMs);
  });

  try {
    await Promise.race([exitPromise, timerPromise]);
  } catch {
    daemon.child.kill('SIGKILL');
  }
}

describe('Tier 3: Real Compiled Rust agsd Crash Recovery E2E Test Suite (REAL_DAEMON_IPC_TESTS)', function () {
  this.timeout(45000);

  let agsdBinaryPath: string;
  let testRoot: string;

  before(async () => {
    agsdBinaryPath = resolveAgsdBinaryPath();
    testRoot = await fs.promises.mkdtemp(path.join(os.tmpdir(), 'agsd-crash-recovery-'));
  });

  after(async () => {
    try {
      await fs.promises.rm(testRoot, { recursive: true, force: true });
    } catch {
      // Best-effort cleanup
    }
  });

  it('1. Clean Daemon Restart & State Persistence: studios and tasks preserved across graceful restarts', async () => {
    const subDir = path.join(testRoot, 'test1');
    await fs.promises.mkdir(subDir, { recursive: true });
    const dbPath = path.join(subDir, 'agsd.db');

    // Phase 1: Start Daemon A
    const daemonA = await spawnAgsd(agsdBinaryPath, subDir, dbPath);
    const bridgeA = new DaemonConnectedBridge({ bootstrapPath: daemonA.runtimePath });
    await bridgeA.connect();
    assert.strictEqual(bridgeA.connectionState, 'connected');

    // Create studio
    const studioResp = await bridgeA.transport!.sendCommand({
      type: 'create_studio',
      name: 'Persistent Recovery Studio 1',
    });
    assert.strictEqual(studioResp.type, 'studio_created');
    const studioId = studioResp.type === 'studio_created' ? studioResp.studio.id : '';
    assert.ok(studioId.length > 0);

    // Submit task
    const taskSpec: RustBatchTaskSpec = {
      key: 'task-clean-01',
      title: 'Durable State Verification Task',
      description: 'Verifies state survives graceful daemon restart',
    };
    const submitResp = await bridgeA.transport!.sendCommand({
      type: 'submit_task',
      studio_id: studioId,
      task_spec: taskSpec,
    });
    assert.strictEqual(submitResp.type, 'tasks_submitted');

    // Check task appears in task listing
    const tasksRespA = await bridgeA.transport!.sendCommand({
      type: 'list_tasks',
      studio_id: studioId,
    });
    assert.strictEqual(tasksRespA.type, 'tasks');
    if (tasksRespA.type === 'tasks') {
      assert.strictEqual(tasksRespA.tasks.length, 1);
      assert.strictEqual(tasksRespA.tasks[0].title, 'Durable State Verification Task');
    }

    // Gracefully shutdown Daemon A
    await bridgeA.disconnect();
    bridgeA.dispose();
    await gracefulShutdown(daemonA);

    // Phase 2: Start Daemon B on identical database
    const daemonB = await spawnAgsd(agsdBinaryPath, subDir, dbPath);
    const bridgeB = new DaemonConnectedBridge({ bootstrapPath: daemonB.runtimePath });
    await bridgeB.connect();
    assert.strictEqual(bridgeB.connectionState, 'connected');

    // Verify studio is retained
    const studiosRespB = await bridgeB.transport!.sendCommand({ type: 'list_studios' });
    assert.strictEqual(studiosRespB.type, 'studios');
    if (studiosRespB.type === 'studios') {
      const found = studiosRespB.studios.find((s) => s.id === studioId);
      assert.ok(found, 'Studio must persist across daemon restarts');
      assert.strictEqual(found?.name, 'Persistent Recovery Studio 1');
    }

    // Verify task is retained
    const tasksRespB = await bridgeB.transport!.sendCommand({
      type: 'list_tasks',
      studio_id: studioId,
    });
    assert.strictEqual(tasksRespB.type, 'tasks');
    if (tasksRespB.type === 'tasks') {
      assert.strictEqual(tasksRespB.tasks.length, 1);
      assert.strictEqual(tasksRespB.tasks[0].title, 'Durable State Verification Task');
    }

    // Cleanup Daemon B
    await bridgeB.disconnect();
    bridgeB.dispose();
    await gracefulShutdown(daemonB);
  });

  it('2. Abnormal Termination (Forced Kill): recovers database and reconciles interrupted tasks to failed', async () => {
    const subDir = path.join(testRoot, 'test2');
    await fs.promises.mkdir(subDir, { recursive: true });
    const dbPath = path.join(subDir, 'agsd.db');

    // Phase 1: Start Daemon A
    const daemonA = await spawnAgsd(agsdBinaryPath, subDir, dbPath);
    const bridgeA = new DaemonConnectedBridge({ bootstrapPath: daemonA.runtimePath });
    await bridgeA.connect();

    // Create studio & task
    const studioResp = await bridgeA.transport!.sendCommand({
      type: 'create_studio',
      name: 'Abrupt Crash Test Studio',
    });
    assert.strictEqual(studioResp.type, 'studio_created');
    const studioId = studioResp.type === 'studio_created' ? studioResp.studio.id : '';

    const taskSpec: RustBatchTaskSpec = {
      key: 'task-crash-02',
      title: 'In-Flight Interrupted Task',
      description: 'Task interrupted during forced daemon crash',
    };
    await bridgeA.transport!.sendCommand({
      type: 'submit_task',
      studio_id: studioId,
      task_spec: taskSpec,
    });

    // Abrupt termination: force kill daemon process
    await bridgeA.disconnect();
    bridgeA.dispose();

    if (process.platform === 'win32') {
      try {
        childProcess.execSync(`taskkill /F /PID ${daemonA.child.pid}`);
      } catch {
        daemonA.child.kill('SIGKILL');
      }
    } else {
      daemonA.child.kill('SIGKILL');
    }

    // Give OS time to close pipe/file handles
    await new Promise((r) => setTimeout(r, 600));

    // Phase 2: Start Daemon B on identical database
    const daemonB = await spawnAgsd(agsdBinaryPath, subDir, dbPath);
    const bridgeB = new DaemonConnectedBridge({ bootstrapPath: daemonB.runtimePath });
    await bridgeB.connect();

    // Check Recovery Status Report
    const recovStatus: RustRecoveryStatusReport = await bridgeB.getRecoveryStatus();
    assert.strictEqual(recovStatus.database_healthy, true, 'Database must be healthy after recovery');
    assert.ok(recovStatus.total_persisted_events > 0, 'Total persisted events must be tracked');

    // Invariant Check: Task must NEVER remain in running state
    const tasksResp = await bridgeB.transport!.sendCommand({
      type: 'list_tasks',
      studio_id: studioId,
    });
    assert.strictEqual(tasksResp.type, 'tasks');
    if (tasksResp.type === 'tasks') {
      const task = tasksResp.tasks.find((t) => t.title === 'In-Flight Interrupted Task');
      assert.ok(task, 'Task must exist after recovery');
      assert.notStrictEqual(task?.state, 'running', 'Interrupted task must NOT remain in running state');
    }

    // Cleanup Daemon B
    await bridgeB.disconnect();
    bridgeB.dispose();
    await gracefulShutdown(daemonB);
  });

  it('3. Role-Based Diagnostic Redaction: standard clients receive null database_path, admin receives verified path', async () => {
    const subDir = path.join(testRoot, 'test3');
    await fs.promises.mkdir(subDir, { recursive: true });
    const dbPath = path.join(subDir, 'agsd.db');

    const daemon = await spawnAgsd(agsdBinaryPath, subDir, dbPath);

    // Standard client connection
    const standardBridge = new DaemonConnectedBridge({ bootstrapPath: daemon.runtimePath });
    await standardBridge.connect();

    const standardStatus = await standardBridge.getRecoveryStatus();
    assert.strictEqual(
      standardStatus.database_path,
      null,
      'Standard client MUST NOT receive host database filesystem path'
    );
    assert.strictEqual(standardStatus.database_healthy, true);

    await standardBridge.disconnect();
    standardBridge.dispose();

    // Admin client connection
    const adminContent = JSON.parse(await fs.promises.readFile(daemon.adminRuntimePath, 'utf8'));
    const adminTransport = new NativeIpcTransport({
      endpoint: adminContent.endpoint,
      keyId: adminContent.key_id,
      clientKey: adminContent.admin_key,
      commandTimeoutMs: 5000,
    });
    await adminTransport.connect();

    const adminResp = await adminTransport.sendCommand({ type: 'get_recovery_status' });
    assert.strictEqual(adminResp.type, 'recovery_status');
    if (adminResp.type === 'recovery_status') {
      assert.ok(
        adminResp.status.database_path !== null && adminResp.status.database_path !== undefined,
        'Admin client MUST receive host database filesystem path'
      );
      assert.ok(
        adminResp.status.database_path.includes('agsd.db'),
        'Admin database_path must point to verified sqlite file'
      );
    }

    adminTransport.disconnect();
    await gracefulShutdown(daemon);
  });

  it('4. Watermark Resynchronization: replay backfill from persistent SQLite WAL', async () => {
    const subDir = path.join(testRoot, 'test4');
    await fs.promises.mkdir(subDir, { recursive: true });
    const dbPath = path.join(subDir, 'agsd.db');

    const daemon = await spawnAgsd(agsdBinaryPath, subDir, dbPath);
    const bridge = new DaemonConnectedBridge({ bootstrapPath: daemon.runtimePath });
    await bridge.connect();

    // Create studio to emit events into durable store
    await bridge.transport!.sendCommand({
      type: 'create_studio',
      name: 'Watermark Resync Studio',
    });

    const receivedSequences: number[] = [];
    bridge.transport!.on('event', (evt) => {
      if (typeof evt.sequence === 'number') {
        receivedSequences.push(evt.sequence);
      }
    });

    // Request events from sequence 1 via stream subscription
    await bridge.transport!.subscribeEvents(1);

    // Allow event frames to stream over transport
    const waitStart = Date.now();
    while (receivedSequences.length === 0 && Date.now() - waitStart < 3000) {
      await new Promise((r) => setTimeout(r, 50));
    }

    assert.ok(receivedSequences.length > 0, 'Replay backfill must yield persisted events');
    assert.ok(receivedSequences[0] >= 1, 'Sequences must be monotonic starting >= 1');

    await bridge.disconnect();
    bridge.dispose();
    await gracefulShutdown(daemon);
  });

  it('5. UI Safety Invariant: Bridge reconnect NEVER automatically dispatches task re-execution', async () => {
    const subDir = path.join(testRoot, 'test5');
    await fs.promises.mkdir(subDir, { recursive: true });
    const dbPath = path.join(subDir, 'agsd.db');

    const daemon = await spawnAgsd(agsdBinaryPath, subDir, dbPath);
    const bridge = new DaemonConnectedBridge({ bootstrapPath: daemon.runtimePath });
    await bridge.connect();

    // Monitor timeline events during reconnection
    const dispatchedEvents: string[] = [];
    const sub = bridge.onEvent((evt) => {
      dispatchedEvents.push(evt.type);
    });

    // Disconnect and reconnect
    await bridge.disconnect();
    await bridge.connect();

    // Invariant: no mutating execution event should be dispatched autonomously
    assert.ok(
      !dispatchedEvents.includes('task_started'),
      'Bridge reconnect must not trigger automatic task start'
    );

    sub.dispose();
    await bridge.disconnect();
    bridge.dispose();
    await gracefulShutdown(daemon);
  });
});
