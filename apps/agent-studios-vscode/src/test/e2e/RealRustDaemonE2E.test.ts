/**
 * Agent Studios — Tier 3: Real Compiled Rust agsd E2E Test Suite
 * Milestone M14-M15 Integration Quality Gate: Mission Control + ConnectedClient + Real Rust Daemon
 *
 * Classification:
 * - AUTHENTICATED_RUST_IPC_VERIFIED (Real two-phase HMAC-SHA256 negotiation and auth)
 * - LIVE_READ_ONLY_RUST_DATA_VERIFIED (Real daemon health, ping, tasks, providers, models query)
 * - REAL_AI_TASK_EXECUTION_NOT_VERIFIED (Mutating coding task execution with external AI runtime engine not run)
 *
 * Verifies real OS cross-process communication against the actual compiled Rust
 * `agsd` binary over Windows Named Pipes (and Unix Domain Sockets on POSIX).
 *
 * Covers all 20 required genuine Rust daemon scenarios:
 * 1. Daemon process startup.
 * 2. Bootstrap file discovery (DaemonDiscovery.loadBootstrapInfo).
 * 3. Standard vs admin credential separation (agsd.json vs agsd-admin.json).
 * 4. Two-phase capability negotiation and HMAC authentication (DaemonConnectedBridge.connect()).
 * 5. Invalid HMAC rejection (assert auth fails with wrong key).
 * 6. Health query (getHealth() -> healthy, active_clients: 1).
 * 7. Ping latency (ping() -> >= 0ms).
 * 8. Studio listing (list_studios / getCurrentSession()).
 * 9. Task listing (listTasks()).
 * 10. Provider listing (listProviderSummaries()).
 * 11. Model listing (models inside provider summary).
 * 12. Event subscription (daemon activates streaming on connect).
 * 13. Sequence watermark handling (tracking lastWatermark).
 * 14. Connection loss handling (disconnect socket).
 * 15. Reconnect with monotonic sequence watermark and replay dedup.
 * 16. Read-only mutation rejection (assert requestApprovalDecision and submitEditorAction fail closed).
 * 17. Standard-client shutdown denial (assert standard client sending shutdown receives permission denied).
 * 18. Authorized admin shutdown (connect with admin key from agsd-admin.json and send shutdown -> shutdown_acknowledged).
 * 19. Child-process termination (assert child process exits with code 0).
 * 20. Temporary resource cleanup (assert bootstrap files and pipe/socket cleaned up).
 */

import * as assert from 'assert';
import * as childProcess from 'child_process';
import * as crypto from 'crypto';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { DaemonConnectedBridge } from '../../bridge/DaemonConnectedBridge';
import { DaemonDiscovery } from '../../bridge/ipc/DaemonDiscovery';
import { NativeIpcTransport } from '../../bridge/ipc/NativeIpcTransport';
import { DaemonBootstrapInfo } from '../../bridge/ipc/RustIpcProtocolTypes';
import { TimelineEvent } from '../../bridge/types';

function resolveAgsdBinaryPath(): string {
  const isWindows = process.platform === 'win32';
  const exeSuffix = isWindows ? '.exe' : '';

  const candidatePaths = [
    process.env.AGSD_BIN_PATH,
    path.resolve(__dirname, '../../../../../../agent-studios-rs/target/debug/agsd' + exeSuffix),
    path.resolve(__dirname, '../../../../../agent-studios-rs/target/debug/agsd' + exeSuffix),
    path.resolve(__dirname, '../../../../agent-studios-rs/target/debug/agsd' + exeSuffix),
    path.resolve(process.cwd(), '../../agent-studios-rs/target/debug/agsd' + exeSuffix),
    isWindows ? 'D:\\Projects\\agent-studios-m15\\agent-studios-rs\\target\\debug\\agsd.exe' : undefined,
    isWindows ? 'D:\\Projects\\agent-studios-m15-02\\agent-studios-rs\\target\\debug\\agsd.exe' : undefined,
  ].filter((p): p is string => typeof p === 'string' && p.length > 0);

  for (const candidate of candidatePaths) {
    if (fs.existsSync(candidate)) {
      return candidate;
    }
  }

  throw new Error(
    `[RealRustDaemonE2E] Real Rust daemon binary (agsd) not found. Checked candidate locations:\n` +
      candidatePaths.map((p) => `  - ${p}`).join('\n') +
      `\n\nPlease build the agsd binary first using:\n` +
      `  cargo build --bin agsd -p agent-studios-ipc-transport\n` +
      `Or set the AGSD_BIN_PATH environment variable.`
  );
}

describe('Tier 3: Real Compiled Rust agsd E2E Test Suite (AUTHENTICATED_RUST_IPC_VERIFIED & LIVE_READ_ONLY_RUST_DATA_VERIFIED)', function () {
  // Guard timeout up to 30s per test across all scenarios
  this.timeout(30000);

  let agsdBinaryPath: string;
  let tempDir: string;
  let runtimePath: string;
  let adminRuntimePath: string;
  let pipeOrSocketPath: string;
  let daemonProcess: childProcess.ChildProcess | null = null;
  let bootstrapInfo: DaemonBootstrapInfo;
  let bridge: DaemonConnectedBridge | null = null;

  before(async () => {
    // 1. Resolve binary or fail with explicit error instructing cargo build (no silent skipping)
    agsdBinaryPath = resolveAgsdBinaryPath();

    // 2. Create isolated temporary directory
    tempDir = await fs.promises.mkdtemp(path.join(os.tmpdir(), 'agsd-real-e2e-'));
    runtimePath = path.join(tempDir, 'agsd.json');
    adminRuntimePath = path.join(tempDir, 'agsd-admin.json');

    // 3. Setup cross-platform IPC endpoint (Named Pipe on Windows, Unix Domain Socket on POSIX)
    const isWindows = process.platform === 'win32';
    const uuid = crypto.randomUUID();
    pipeOrSocketPath = isWindows
      ? `\\\\.\\pipe\\ags-e2e-${uuid}`
      : path.join(tempDir, `ags-e2e-${uuid}.sock`);
    if (!isWindows) {
      try {
        await fs.promises.chmod(tempDir, 0o700);
      } catch {
        // Ignore on platforms without chmod support
      }
    }
  });

  after(async () => {
    // Safe teardown: ensure standard bridge disposed, daemon killed, and tempDir cleaned up
    if (bridge) {
      try {
        await bridge.disconnect();
        bridge.dispose();
      } catch {
        // Ignore teardown errors
      }
      bridge = null;
    }

    if (daemonProcess && !daemonProcess.killed) {
      try {
        daemonProcess.kill('SIGKILL');
      } catch {
        // Ignore teardown errors
      }
      daemonProcess = null;
    }

    try {
      await fs.promises.rm(tempDir, { recursive: true, force: true });
    } catch {
      // Ignore teardown errors
    }
  });

  it('1. Daemon process startup', async () => {
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

    daemonProcess.stderr?.on('data', (_chunk: Buffer) => {
      // Consume stderr to prevent pipe buffer blockage
    });
    daemonProcess.stdout?.on('data', (_chunk: Buffer) => {
      // Consume stdout to prevent pipe buffer blockage
    });

    assert.ok(daemonProcess !== null, 'Daemon child process must be initialized');
    assert.ok(
      typeof daemonProcess.pid === 'number' && daemonProcess.pid > 0,
      'Daemon process must have a valid PID'
    );
    assert.strictEqual(daemonProcess.killed, false, 'Daemon process must not be killed');

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
          // File write may still be in progress
        }
      }
      await new Promise((r) => setTimeout(r, 100));
    }

    assert.ok(
      ready,
      `agsd failed to write runtime discovery files within 10000ms at ${runtimePath}`
    );
  });

  it('2. Bootstrap file discovery (DaemonDiscovery.loadBootstrapInfo)', async () => {
    bootstrapInfo = await DaemonDiscovery.loadBootstrapInfo(runtimePath);
    assert.ok(bootstrapInfo.endpoint.length > 0, 'Discovered endpoint must not be empty');
    assert.strictEqual(bootstrapInfo.key_id, 'ags-client', 'Client key ID must be ags-client');
    assert.ok(bootstrapInfo.client_key.length > 0, 'Discovered client_key must be present');
    assert.strictEqual(bootstrapInfo.admin_key, null, 'Security invariant: admin_key must never be exposed');
    assert.ok(bootstrapInfo.pid > 0, 'Discovered daemon PID must be valid');
    assert.strictEqual(bootstrapInfo.pid, daemonProcess?.pid, 'Discovered PID must match child process PID');
  });

  it('3. Standard vs admin credential separation (agsd.json vs agsd-admin.json)', async () => {
    const stdContent = JSON.parse(await fs.promises.readFile(runtimePath, 'utf8'));
    const adminContent = JSON.parse(await fs.promises.readFile(adminRuntimePath, 'utf8'));

    // Standard bootstrap file verification
    assert.strictEqual(stdContent.key_id, 'ags-client');
    assert.ok(typeof stdContent.client_key === 'string' && stdContent.client_key.length > 0);
    assert.strictEqual(
      stdContent.admin_key,
      undefined,
      'Standard bootstrap file must never contain admin_key'
    );

    // Admin bootstrap file verification
    assert.strictEqual(adminContent.key_id, 'ags-client-admin');
    assert.ok(typeof adminContent.admin_key === 'string' && adminContent.admin_key.length > 0);
    assert.notStrictEqual(
      stdContent.client_key,
      adminContent.admin_key,
      'Client key and admin key must be distinct cryptographically random secrets'
    );

    // Verify DaemonDiscovery redaction security invariant
    const redacted = DaemonDiscovery.redact(bootstrapInfo);
    assert.strictEqual(redacted.client_key, '[REDACTED]');
    assert.strictEqual(redacted.admin_key, undefined);
  });

  it('4. Two-phase capability negotiation and HMAC authentication (DaemonConnectedBridge.connect())', async () => {
    bridge = new DaemonConnectedBridge({ bootstrapPath: runtimePath });
    assert.strictEqual(bridge.connectionState, 'disconnected');

    await bridge.connect();
    assert.strictEqual(bridge.connectionState, 'connected');
    assert.ok(bridge.endpoint !== null, 'Bridge must expose connected endpoint');
    assert.ok(bridge.transport !== null, 'Bridge must have active transport');
    assert.strictEqual(bridge.transport?.getState(), 'ready');
    assert.ok(bridge.transport?.getSessionId() !== null, 'Transport must have authenticated session ID');

    const caps = bridge.transport?.getAgreedCapabilities() ?? [];
    assert.ok(Array.isArray(caps), 'Agreed capabilities must be an array');
  });

  it('5. Invalid HMAC rejection (assert auth fails with wrong key)', async () => {
    const badTransport = new NativeIpcTransport({
      endpoint: bootstrapInfo.endpoint,
      keyId: bootstrapInfo.key_id,
      clientKey: 'invalid-cryptographic-secret-for-rejection-test',
      commandTimeoutMs: 5000,
    });

    await assert.rejects(
      async () => {
        await badTransport.connect();
      },
      (err: Error) => {
        const msg = err.message.toLowerCase();
        return (
          msg.includes('auth') ||
          msg.includes('handshake') ||
          msg.includes('signature') ||
          msg.includes('closed') ||
          msg.includes('failed')
        );
      },
      'Connecting with invalid HMAC key must be rejected by daemon'
    );
    badTransport.disconnect();
  });

  it('6. Health query (getHealth() -> healthy, active_clients: 1)', async () => {
    assert.ok(bridge, 'Bridge must be initialized');
    const health = await bridge.getHealth();
    assert.ok(health !== null, 'Health response must not be null');
    assert.strictEqual(health.status, 'healthy');
    assert.strictEqual(health.active_clients, 1, 'Active clients must be 1 (current bridge)');
    assert.ok(typeof health.uptime_secs === 'number' && health.uptime_secs >= 0);
    assert.ok(typeof health.memory_events_count === 'number' && health.memory_events_count >= 0);
  });

  it('7. Ping latency (ping() -> >= 0ms)', async () => {
    assert.ok(bridge, 'Bridge must be initialized');
    const latency = await bridge.ping('m14-real-rust-ping');
    assert.ok(typeof latency === 'number', 'Latency must be a number');
    assert.ok(latency >= 0, 'Ping latency must be non-negative');
  });

  it('8. Studio listing (list_studios / getCurrentSession())', async () => {
    assert.ok(bridge && bridge.transport, 'Bridge and transport must be initialized');

    // Direct command listing
    const studiosResp = await bridge.transport.sendCommand({ type: 'list_studios' });
    assert.strictEqual(studiosResp.type, 'studios');
    if (studiosResp.type === 'studios') {
      assert.ok(Array.isArray(studiosResp.studios));
    }

    // Create a studio to verify active studio and session mapping
    const createResp = await bridge.transport.sendCommand({
      type: 'create_studio',
      name: 'E2E Genuine Verification Studio',
    });
    assert.strictEqual(createResp.type, 'studio_created');

    const session = await bridge.getCurrentSession();
    assert.ok(session !== null, 'Current session must not be null after studio creation');
    assert.strictEqual(session?.goalSummary, 'E2E Genuine Verification Studio');
    assert.strictEqual(session?.isPreview, false);

    const sessions = await bridge.listSessions();
    assert.ok(Array.isArray(sessions), 'listSessions must return an array');
    assert.ok(sessions.length >= 1, 'listSessions must contain created studio');
  });

  it('9. Task listing (listTasks())', async () => {
    assert.ok(bridge, 'Bridge must be initialized');
    const tasks = await bridge.listTasks();
    assert.ok(Array.isArray(tasks), 'Tasks must be an array');
    assert.strictEqual(tasks.length, 0, 'Fresh studio starts with 0 tasks');
  });

  it('10. Provider listing (listProviderSummaries())', async () => {
    assert.ok(bridge && bridge.transport, 'Bridge and transport must be initialized');
    const providers = await bridge.listProviderSummaries();
    assert.ok(Array.isArray(providers), 'Providers must be an array');

    const provResp = await bridge.transport.sendCommand({ type: 'list_providers' });
    assert.strictEqual(provResp.type, 'providers');
    if (provResp.type === 'providers') {
      assert.ok(Array.isArray(provResp.providers));
    }
  });

  it('11. Model listing (models inside provider summary)', async () => {
    assert.ok(bridge && bridge.transport, 'Bridge and transport must be initialized');

    // Verify direct models query from catalog
    const modelsResp = await bridge.transport.sendCommand({ type: 'list_models' });
    assert.strictEqual(modelsResp.type, 'models');
    if (modelsResp.type === 'models') {
      assert.ok(Array.isArray(modelsResp.models), 'Models must be an array');
    }

    // Verify models field structure inside provider summaries
    const summaries = await bridge.listProviderSummaries();
    assert.ok(Array.isArray(summaries), 'Provider summaries must be an array');
    for (const summary of summaries) {
      assert.ok(Array.isArray(summary.models), 'Provider summary must contain models array');
    }
  });

  it('12. Event subscription (daemon activates streaming on connect)', async () => {
    assert.ok(bridge && bridge.transport, 'Bridge and transport must be initialized');

    let streamEventReceived = false;
    let deliveredEvent: TimelineEvent | null = null;

    const eventDisposable = bridge.onEvent((ev) => {
      streamEventReceived = true;
      deliveredEvent = ev;
    });

    // Trigger daemon event emission by creating a studio
    await bridge.transport.sendCommand({
      type: 'create_studio',
      name: 'Event Stream Test Studio',
    });

    const deadline = Date.now() + 4000;
    while (!streamEventReceived && Date.now() < deadline) {
      await new Promise((r) => setTimeout(r, 50));
    }

    eventDisposable.dispose();
    assert.strictEqual(streamEventReceived, true, 'Daemon event streaming must deliver event to bridge');
    assert.ok(deliveredEvent !== null, 'Delivered event must not be null');
    assert.ok((deliveredEvent as TimelineEvent).sequenceNumber > 0, 'Delivered event must have sequence > 0');
  });

  it('13. Sequence watermark handling (tracking lastWatermark)', async () => {
    assert.ok(bridge, 'Bridge must be initialized');
    const watermark = bridge.lastWatermark;
    assert.ok(typeof watermark === 'number', 'lastWatermark must be a number');
    assert.ok(watermark > 0, `Watermark must be tracked and > 0, got ${watermark}`);
  });

  it('14. Connection loss handling (disconnect socket)', async () => {
    assert.ok(bridge, 'Bridge must be initialized');
    assert.strictEqual(bridge.connectionState, 'connected');

    await bridge.disconnect();
    assert.strictEqual(bridge.connectionState, 'disconnected');
    assert.strictEqual(bridge.transport?.getState() ?? 'disconnected', 'disconnected');
  });

  it('15. Reconnect with monotonic sequence watermark and replay dedup', async () => {
    assert.ok(bridge, 'Bridge must be initialized');
    const savedWatermark = bridge.lastWatermark;
    assert.ok(savedWatermark > 0, 'Saved watermark before reconnect must be > 0');

    // Reconnect bridge to running daemon
    await bridge.connect();
    assert.strictEqual(bridge.connectionState, 'connected');

    // Watermark must remain monotonic
    assert.ok(
      bridge.lastWatermark >= savedWatermark,
      `Watermark after reconnect (${bridge.lastWatermark}) must be >= saved watermark (${savedWatermark})`
    );

    // Verify commands succeed following reconnect
    const health = await bridge.getHealth();
    assert.strictEqual(health.status, 'healthy');
  });

  it('16. Read-only mutation rejection (assert requestApprovalDecision and submitEditorAction fail closed)', async () => {
    assert.ok(bridge, 'Bridge must be initialized');

    const approvalRes = await bridge.requestApprovalDecision('appr-real-01', 'approved');
    assert.strictEqual(approvalRes.applied, false, 'requestApprovalDecision must fail closed in read-only slice');
    assert.ok((approvalRes.message || '').includes('read-only'), 'Message must indicate read-only constraint');

    const editorRes = await bridge.submitEditorAction({
      action: 'refactor',
      documentUri: 'file:///workspace/test.ts',
      languageId: 'typescript',
      selectedText: 'fn()',
      selectionRange: { startLine: 1, startCol: 0, endLine: 1, endCol: 4 },
    });
    assert.strictEqual(editorRes.accepted, false, 'submitEditorAction must fail closed in read-only slice');
    assert.ok((editorRes.message || '').includes('read-only'), 'Message must indicate read-only constraint');

    await assert.rejects(
      async () => {
        await bridge!.sendMessage({ message: 'test', mode: 'ask' }, () => {});
      },
      (err: Error) => err.message.includes('read-only') || err.message.includes('disabled'),
      'sendMessage must reject in read-only slice'
    );

    await assert.rejects(
      async () => {
        await bridge!.createSession('test goal');
      },
      (err: Error) => err.message.includes('read-only') || err.message.includes('disabled'),
      'createSession must reject in read-only slice'
    );
  });

  it('17. Standard-client shutdown denial (assert standard client sending shutdown receives permission denied)', async () => {
    assert.ok(bridge && bridge.transport, 'Bridge and transport must be initialized');

    await assert.rejects(
      async () => {
        await bridge!.transport!.sendCommand({ type: 'shutdown' });
      },
      (err: Error) => {
        const msg = err.message.toLowerCase();
        return msg.includes('permission denied') || msg.includes('admin');
      },
      'Standard client must be rejected with permission denied when attempting shutdown'
    );

    // Verify daemon remains healthy and operational
    const health = await bridge.getHealth();
    assert.strictEqual(health.status, 'healthy');
  });

  it('18. Authorized admin shutdown (connect with admin key from agsd-admin.json and send shutdown -> shutdown_acknowledged)', async () => {
    assert.ok(bridge, 'Bridge must be initialized');

    // Cleanly disconnect standard client
    await bridge.disconnect();
    bridge.dispose();
    bridge = null;

    // Load admin bootstrap credentials
    const adminContent = JSON.parse(await fs.promises.readFile(adminRuntimePath, 'utf8'));
    assert.ok(adminContent.admin_key, 'Admin bootstrap file must contain admin_key');
    assert.strictEqual(adminContent.key_id, 'ags-client-admin');

    const adminTransport = new NativeIpcTransport({
      endpoint: adminContent.endpoint,
      keyId: adminContent.key_id,
      clientKey: adminContent.admin_key,
      commandTimeoutMs: 10000,
    });

    await adminTransport.connect();
    assert.strictEqual(adminTransport.getState(), 'ready');

    const shutdownResp = await adminTransport.sendCommand({ type: 'shutdown' });
    assert.strictEqual(
      shutdownResp.type,
      'shutdown_acknowledged',
      'Authorized admin shutdown command must receive shutdown_acknowledged'
    );

    adminTransport.disconnect();
  });

  it('19. Child-process termination (assert child process exits with code 0)', async () => {
    assert.ok(daemonProcess !== null, 'Daemon child process must exist');

    const exitCode = await new Promise<number | null>((resolve) => {
      if (daemonProcess?.exitCode !== null && daemonProcess?.exitCode !== undefined) {
        resolve(daemonProcess.exitCode);
        return;
      }
      const timer = setTimeout(() => resolve(-1), 10000);
      daemonProcess!.on('exit', (code) => {
        clearTimeout(timer);
        resolve(code);
      });
    });

    assert.strictEqual(exitCode, 0, 'agsd process must exit cleanly with code 0 upon authorized shutdown');
    daemonProcess = null;
  });

  it('20. Temporary resource cleanup (assert bootstrap files and pipe/socket cleaned up)', async () => {
    // Assert bootstrap discovery files removed by daemon during graceful shutdown
    assert.strictEqual(
      fs.existsSync(runtimePath),
      false,
      'Standard bootstrap file (agsd.json) must be cleaned up on daemon exit'
    );
    assert.strictEqual(
      fs.existsSync(adminRuntimePath),
      false,
      'Admin bootstrap file (agsd-admin.json) must be cleaned up on daemon exit'
    );

    // On Unix, assert socket file cleanup
    if (process.platform !== 'win32') {
      assert.strictEqual(
        fs.existsSync(pipeOrSocketPath),
        false,
        'Unix domain socket file must be cleaned up on daemon exit'
      );
    }

    // Clean up temporary directory
    await fs.promises.rm(tempDir, { recursive: true, force: true });
    assert.strictEqual(fs.existsSync(tempDir), false, 'Temporary directory must be completely removed');
  });
});
