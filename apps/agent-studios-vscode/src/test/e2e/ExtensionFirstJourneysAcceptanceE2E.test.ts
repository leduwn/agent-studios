/**
 * Agent Studios — Extension-First End-to-End Acceptance Suite (Journeys A-G)
 *
 * Verifies all 7 operator journeys end-to-end against real `agsd.exe` spawned by
 * `EmbeddedDaemonSupervisor` + local `$0.00` zero-billable HTTP `/v1/models` mock server:
 *   - Journey A: Embedded agsd supervisor startup, health, 12-provider upsert & dynamic discovery
 *   - Journey B: ContextEngine incremental symbols + provenance & HunkReviewManager checkpoint/revert
 *   - Journey C: Multi-agent DAG viewport (#dag-viewport-group), minimap, filters, follow & 9 tabs
 *   - Journey D: Studio fork lineage & SQLite WAL recovery across daemon restart
 *   - Journey E: Real IPC `list_extensibility` (Skills, Redacted MCP, AGENTS.md)
 *   - Journey F: Multi-provider + SecretStorage / SecretReference isolation
 *   - Journey G: Debounced inline completion epoch cancellation & diagnostic Quick Fix
 */

import * as assert from 'assert';
import * as crypto from 'crypto';
import * as fs from 'fs';
import * as http from 'http';
import * as os from 'os';
import * as path from 'path';
import * as vscode from 'vscode';

import { BridgeManager } from '../../bridge/BridgeManager';
import { DaemonConnectedBridge } from '../../bridge/DaemonConnectedBridge';
import { DisconnectedBridge } from '../../bridge/DisconnectedBridge';
import { EmbeddedDaemonSupervisor } from '../../bridge/EmbeddedDaemonSupervisor';
import { FixtureBridge } from '../../bridge/FixtureBridge';
import { NativeIpcTransport } from '../../bridge/ipc/NativeIpcTransport';
import { HunkReviewManager } from '../../cards/HunkReviewManager';
import {
  DEFAULT_BASE_URLS,
  looksLikePlaintextSecret,
  ProviderProfileManager,
} from '../../commands/ProviderManagementCommands';
import { ContextEngine } from '../../context/ContextEngine';
import {
  AgentStudiosDiagnosticsCodeActionProvider,
  AgentStudiosInlineCompletionProvider,
} from '../../editor/EditorIntelligenceProvider';
import { MissionControlHtmlBuilder } from '../../mission-control/MissionControlHtmlBuilder';
import { MissionControlState } from '../../mission-control/models/types';
import { MockDiagnostic, MockRange, MockUri } from '../unit/vscodeMock';

function createMemoryMemento(): vscode.Memento {
  const map = new Map<string, unknown>();
  return {
    keys: () => Array.from(map.keys()),
    get: <T>(key: string, defaultValue?: T): T =>
      (map.has(key) ? map.get(key) : defaultValue) as T,
    update: async (key: string, value: unknown): Promise<void> => {
      map.set(key, value);
    },
  };
}

function createSecretStorage(): vscode.SecretStorage {
  const secrets = new Map<string, string>();
  const emitter = new vscode.EventEmitter<vscode.SecretStorageChangeEvent>();
  return {
    onDidChange: emitter.event,
    get: async (key: string) => secrets.get(key),
    store: async (key: string, value: string) => {
      secrets.set(key, value);
      emitter.fire({ key });
    },
    delete: async (key: string) => {
      secrets.delete(key);
      emitter.fire({ key });
    },
  } as unknown as vscode.SecretStorage;
}

describe('Extension-First Acceptance Suite (Journeys A-G)', function () {
  this.timeout(45000);

  let tempRoot: string;
  let workspaceDir: string;
  let stateDir: string;
  let bootstrapPath: string;
  let adminBootstrapPath: string;
  let pipeOrSocket: string;
  let daemonPid: number | undefined;
  let mockServer: http.Server;
  let mockPort = 0;

  before(async () => {
    tempRoot = await fs.promises.mkdtemp(path.join(os.tmpdir(), 'ags-ext-first-'));
    workspaceDir = path.join(tempRoot, 'workspace');
    stateDir = path.join(tempRoot, 'state');
    bootstrapPath = path.join(tempRoot, 'agsd.json');
    adminBootstrapPath = path.join(tempRoot, 'agsd-admin.json');
    await fs.promises.mkdir(workspaceDir, { recursive: true });
    await fs.promises.mkdir(stateDir, { recursive: true });

    const skillDir = path.join(workspaceDir, '.agent-studios', 'skills', 'audit');
    await fs.promises.mkdir(skillDir, { recursive: true });
    await fs.promises.writeFile(path.join(workspaceDir, 'AGENTS.md'), '# Repo Rules\nRun cargo test cleanly.\n');
    await fs.promises.writeFile(
      path.join(skillDir, 'SKILL.md'),
      '---\nname: audit\ndescription: Audit repository readiness\n---\nCheck invariants.\n'
    );
    await fs.promises.writeFile(
      path.join(workspaceDir, '.mcp.json'),
      JSON.stringify({
        mcpServers: {
          github: { url: 'https://mcp.example.com/sse?api_key=topsecret999' },
        },
      })
    );

    const isWindows = process.platform === 'win32';
    const id = crypto.randomUUID();
    pipeOrSocket = isWindows
      ? `\\\\.\\pipe\\ags-ext-first-${id}`
      : path.join(tempRoot, `ags-ext-first-${id}.sock`);

    // $0.00 local mock OpenAI-compatible /v1/models endpoint
    mockServer = http.createServer((_req, res) => {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(
        JSON.stringify({
          object: 'list',
          data: [
            { id: 'mock-gpt-4.1', owned_by: 'openai' },
            { id: 'mock-vllm-qwen', owned_by: 'vllm' },
          ],
        })
      );
    });
    await new Promise<void>((resolve) => mockServer.listen(0, '127.0.0.1', () => resolve()));
    const addr = mockServer.address();
    mockPort = typeof addr === 'object' && addr ? addr.port : 0;
  });

  after(async () => {
    if (daemonPid && EmbeddedDaemonSupervisor.isProcessAlive(daemonPid)) {
      try {
        process.kill(daemonPid);
      } catch {
        // Ignore already-exited daemon
      }
    }
    if (mockServer) {
      await new Promise<void>((resolve) => mockServer.close(() => resolve()));
    }
    await fs.promises.rm(tempRoot, { recursive: true, force: true }).catch(() => undefined);
  });

  it('Journey A & E:EmbeddedDaemonSupervisor boots real agsd, discovers dynamic models & redacted extensibility', async () => {
    const isWindows = process.platform === 'win32';
    const ensured = await EmbeddedDaemonSupervisor.ensureDaemonRunning({
      bootstrapPath,
      adminBootstrapPath,
      pipeName: isWindows ? pipeOrSocket : undefined,
      socketPath: isWindows ? undefined : pipeOrSocket,
      stateDir,
      workspacePath: workspaceDir,
      startupTimeoutMs: 12000,
    });

    assert.strictEqual(ensured.running, true, 'EmbeddedDaemonSupervisor must find and boot real agsd.exe');
    assert.ok(ensured.binaryPath && fs.existsSync(ensured.binaryPath), 'Resolved agsd binary path must exist');
    assert.ok(fs.existsSync(bootstrapPath), 'Bootstrap agsd.json must be emitted by agsd');
    daemonPid = ensured.pid;

    const bridge = new DaemonConnectedBridge({ bootstrapPath });
    await bridge.connect();
    try {
      const health = await bridge.getHealth();
      assert.strictEqual(health.status, 'healthy');

      // Journey A: Upsert LiteLLM instance, run live HTTP /v1/models discovery via agsd & verify connection
      const upserted = await bridge.upsertProviderInstance({
        providerType: 'litellm',
        displayName: 'Journey A LiteLLM',
        baseUrl: `http://127.0.0.1:${mockPort}/v1`,
      });
      assert.ok(upserted.instanceId.length > 0);

      const models = await bridge.discoverProviderModels(upserted.instanceId);
      assert.strictEqual(models.length, 2);
      assert.strictEqual(models[0].id, 'mock-gpt-4.1');

      const connReport = await bridge.testProviderConnection(upserted.instanceId);
      assert.strictEqual(connReport.ok, true);

      // Journey E: List workspace skills, redacted MCP servers, and instructions over real IPC
      const ext = await bridge.listExtensibility(workspaceDir);
      assert.ok(
        ext.skills.some((s) => (s as { name?: string }).name === 'audit'),
        'listExtensibility must discover workspace audit skill'
      );
      assert.ok(ext.instructions.length >= 1, 'listExtensibility must discover AGENTS.md');
      const serializedExt = JSON.stringify(ext);
      assert.strictEqual(
        serializedExt.includes('topsecret999'),
        false,
        'Plaintext MCP token must never leak across IPC'
      );

      const removed = await bridge.removeProviderInstance(upserted.instanceId);
      assert.strictEqual(removed, true);
    } finally {
      bridge.dispose();
    }
  });

  it('Journey B: ContextEngine incremental symbols + provenance & HunkReviewManager checkpoint/revert', () => {
    const engine = new ContextEngine();
    engine.indexSourceFile(
      'src/audit.ts',
      'export function runAudit(target: string): boolean { return Boolean(target); }\nexport class AuditRunner {}\n'
    );
    const hits = engine.searchSymbols('runAudit');
    assert.strictEqual(hits.length, 1);
    assert.strictEqual(hits[0].kind, 'function');

    const fileItem = engine.makeItem('file', 'src/audit.ts', 'export function runAudit() {}');
    const symItem = engine.makeItem('symbol', 'src/audit.ts', 'fn runAudit', 'runAudit');
    const packed = engine.packWithinBudget([fileItem, symItem], 500, '- src/audit.ts [function:runAudit]');
    assert.strictEqual(packed.attachments.length, 2);
    assert.ok(packed.provenance.every((p) => p.sha256.length === 64 && p.includedInPrompt));

    const mgr = new HunkReviewManager();
    const diff = [
      'diff --git a/src/audit.ts b/src/audit.ts',
      '--- a/src/audit.ts',
      '+++ b/src/audit.ts',
      '@@ -1,2 +1,2 @@',
      ' export function runAudit() {',
      '-  return false;',
      '+  return true;',
    ].join('\n');
    const parsed = mgr.parseDiff(diff);
    const original = 'export function runAudit() {\n  return false;';
    const applied = mgr.applyHunkToContent('art-j-b', 'src/audit.ts', 0, original, parsed[0].hunks[0]);
    assert.strictEqual(applied.applied, true);
    assert.ok(applied.checkpointId);
    const reverted = mgr.revertToCheckpoint(applied.checkpointId!);
    assert.strictEqual(reverted?.originalContent, original);
    mgr.dispose();
  });

  it('Journey C: Mission Control 9 tabs, #dag-viewport-group alignment, minimap, filter & keyboard nav', () => {
    const now = Date.now();
    const nowIso = new Date(now).toISOString();
    const state: MissionControlState = {
      version: '1.0.0',
      timestamp: now,
      activeTab: 'tasks',
      isPreview: false,
      connectionState: 'connected',
      activeStudio: {
        id: 'sess-mc',
        name: 'Multi-Agent Verification',
        workspacePath: workspaceDir,
        createdAt: nowIso,
        runStatus: 'running',
      },
      currentSession: {
        id: 'sess-mc',
        workspacePath: workspaceDir,
        goalSummary: 'Multi-Agent Verification',
        status: 'running',
        activeRunId: 'run-mc',
        activeAgentCount: 2,
        totalAgentCount: 2,
        completedTasksCount: 1,
        totalTasksCount: 2,
        startedAt: now - 2000,
        elapsedMs: 2000,
      },
      overview: {
        activeAgentCount: 1,
        totalAgentsCount: 2,
        taskCounts: {
          pending: 0,
          ready: 0,
          running: 1,
          blocked: 0,
          review: 0,
          succeeded: 1,
          failed: 0,
          cancelled: 0,
        },
        attentionItems: [],
        recentEvents: [],
        activeRunCount: 1,
      },
      fleet: [
        {
          agentId: 'ag-1',
          name: 'Architect',
          role: 'planner',
          status: 'completed',
          runtime: 'codex',
          providerModel: 'openai/gpt-4.1',
          lastActivityAt: now,
          toolCallCount: 3,
        },
        {
          agentId: 'ag-2',
          name: 'Verifier',
          role: 'tester',
          status: 'running',
          assignedTaskId: 't-2',
          runtime: 'codex',
          providerModel: 'openai/gpt-4.1',
          lastActivityAt: now,
          toolCallCount: 1,
        },
      ],
      tasks: [
        {
          id: 't-1',
          studioId: 'sess-mc',
          title: 'Plan',
          description: 'Plan work',
          state: 'succeeded',
          dependencies: [],
          createdAt: nowIso,
          updatedAt: nowIso,
        },
        {
          id: 't-2',
          studioId: 'sess-mc',
          title: 'Verify',
          description: 'Verify tests',
          state: 'running',
          dependencies: ['t-1'],
          createdAt: nowIso,
          updatedAt: nowIso,
        },
      ],
      runs: [
        {
          runId: 'run-mc',
          taskId: 't-2',
          agentId: 'ag-2',
          runtime: 'codex',
          status: 'running',
          startedAt: now - 2000,
          toolCalls: [],
        },
      ],
      artifacts: [],
      approvals: [],
      timeline: [],
      providers: [],
      diagnostics: {
        endpoint: bootstrapPath,
        sessionId: 'sess-mc',
        protocolVersion: '1.0',
        agreedCapabilities: ['event_streaming'],
        watermark: 1,
        activeClients: 1,
        uptimeSecs: 5,
        lastError: null,
      },
    };

    const fakeWebview = { cspSource: 'https://example.invalid' } as unknown as vscode.Webview;
    const html = MissionControlHtmlBuilder.buildHtml(fakeWebview, MockUri.file(tempRoot) as unknown as vscode.Uri, state);

    assert.ok(html.includes('id="dag-viewport-group"'), 'DAG SVG must include #dag-viewport-group');
    assert.ok(html.includes('id="dag-minimap"'), 'DAG canvas must include minimap');
    assert.ok(html.includes('window.fitDagToScreen ='), 'Mission Control must define fitDagToScreen');
    assert.ok(html.includes('window.filterDagState ='), 'Mission Control must define filterDagState');
    assert.ok(html.includes('window.toggleFollowExecution ='), 'Mission Control must define toggleFollowExecution');
    assert.ok(html.includes('ArrowRight'), 'Mission Control must support keyboard arrow DAG navigation');
    for (const tab of ['overview', 'agents', 'tasks', 'runs', 'timeline', 'review', 'approvals', 'providers', 'diagnostics']) {
      assert.ok(html.includes(`data-tab="${tab}"`), `Mission Control missing tab ${tab}`);
    }
  });

  it('Journey D: Studio fork lineage & SQLite WAL durability across real agsd restart', async () => {
    const adminInfo = JSON.parse(await fs.promises.readFile(adminBootstrapPath, 'utf8')) as {
      endpoint: string;
      key_id: string;
      admin_key: string;
    };
    const admin = new NativeIpcTransport({
      endpoint: adminInfo.endpoint,
      keyId: adminInfo.key_id,
      clientKey: adminInfo.admin_key,
    });
    await admin.connect();
    let createdStudioId = '';
    try {
      const createdResp = await admin.sendCommand({
        type: 'create_studio',
        name: 'Durable Root Studio',
      });
      assert.strictEqual(createdResp.type, 'studio_created');
      if (createdResp.type === 'studio_created') {
        createdStudioId = createdResp.studio.id;
      }
    } finally {
      admin.disconnect();
    }

    const bridge = new DaemonConnectedBridge({ bootstrapPath });
    await bridge.connect();
    let forkedId = '';
    try {
      const forked = await bridge.forkSession(createdStudioId, 'Durable Fork Branch');
      forkedId = forked.id;
      assert.notStrictEqual(forkedId, createdStudioId);
      assert.strictEqual(forked.goalSummary, 'Durable Fork Branch');
    } finally {
      bridge.dispose();
    }

    // Simulate daemon crash/restart on the same SQLite WAL stateDir
    if (daemonPid && EmbeddedDaemonSupervisor.isProcessAlive(daemonPid)) {
      process.kill(daemonPid);
      await new Promise((r) => setTimeout(r, 350));
    }
    await fs.promises.rm(bootstrapPath, { force: true }).catch(() => undefined);

    const isWindows = process.platform === 'win32';
    const restarted = await EmbeddedDaemonSupervisor.ensureDaemonRunning({
      bootstrapPath,
      adminBootstrapPath,
      pipeName: isWindows ? pipeOrSocket : undefined,
      socketPath: isWindows ? undefined : pipeOrSocket,
      stateDir,
      workspacePath: workspaceDir,
      startupTimeoutMs: 12000,
    });
    assert.strictEqual(restarted.running, true);
    daemonPid = restarted.pid;

    const reconnected = new DaemonConnectedBridge({ bootstrapPath });
    await reconnected.connect();
    try {
      const sessions = await reconnected.listSessions();
      assert.ok(sessions.some((s) => s.id === createdStudioId), 'Root studio must survive agsd restart');
      assert.ok(sessions.some((s) => s.id === forkedId), 'Forked studio must survive agsd restart');
    } finally {
      reconnected.dispose();
    }
  });

  it('Journey F & G: 12-provider registry + SecretStorage isolation & inline completion epoch cancellation', async () => {
    assert.strictEqual(Object.keys(DEFAULT_BASE_URLS).length, 12);
    assert.strictEqual(looksLikePlaintextSecret('sk-vllm-vault-only-value-1234'), true);

    const ctx = {
      subscriptions: [],
      workspaceState: createMemoryMemento(),
      globalState: Object.assign(createMemoryMemento(), { setKeysForSync: () => {} }),
      secrets: createSecretStorage(),
    } as unknown as vscode.ExtensionContext;

    const mgr = new ProviderProfileManager(ctx);
    const handleKey = 'agentStudios.secret.vllm-e2e';
    await mgr.storeSecretInExtensionVault(handleKey, 'sk-vllm-vault-only-value-1234');
    const saved = await mgr.upsertProfile({
      providerType: 'vllm',
      displayName: 'Vault vLLM',
      secretRefKey: handleKey,
      secretSource: 'secret_storage',
    });
    assert.strictEqual(saved.secretRef.source, 'secret_storage');
    assert.strictEqual(await mgr.getSecretFromExtensionVault(handleKey), 'sk-vllm-vault-only-value-1234');
    assert.strictEqual(
      JSON.stringify(mgr.listProfiles()).includes('sk-vllm-vault-only-value-1234'),
      false,
      'SecretStorage token must never appear in persisted globalState profile metadata'
    );

    const bm = new BridgeManager(new DisconnectedBridge());
    const offlineProvider = new AgentStudiosInlineCompletionProvider(() => bm.bridge);
    const offline = await offlineProvider.suggestCompletion({
      languageId: 'typescript',
      prefix: 'function test() {',
      suffix: '}',
      relativePath: 'src/test.ts',
    });
    assert.strictEqual(offline.length, 0, 'Offline inline completion must fail closed');

    const fixture = new FixtureBridge();
    await fixture.connect();
    bm.setBridge(fixture);
    const liveProvider = new AgentStudiosInlineCompletionProvider(() => bm.bridge);
    const [stale, latest] = await Promise.all([
      liveProvider.suggestCompletion({
        languageId: 'typescript',
        prefix: 'const x =',
        suffix: '',
        relativePath: 'src/test.ts',
        debounceMs: 35,
      }),
      liveProvider.suggestCompletion({
        languageId: 'typescript',
        prefix: 'const y =',
        suffix: '',
        relativePath: 'src/test.ts',
        debounceMs: 5,
      }),
    ]);
    assert.strictEqual(stale.length, 0);
    assert.strictEqual(latest.length, 1);

    const codeActions = new AgentStudiosDiagnosticsCodeActionProvider();
    const doc = { uri: MockUri.file('/workspace/src/test.ts'), languageId: 'typescript' } as unknown as vscode.TextDocument;
    const range = new MockRange(0, 0, 0, 5) as unknown as vscode.Range;
    const actions = codeActions.provideCodeActions(doc, range, {
      diagnostics: [new MockDiagnostic(range as unknown as MockRange, 'Type error') as unknown as vscode.Diagnostic],
    });
    assert.strictEqual(actions[0].command.command, 'agent-studios.fixDiagnostics');
    bm.dispose();
  });
});
