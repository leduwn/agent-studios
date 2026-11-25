/**
 * MASTER MISSION 05 — Real End-to-End Codex Execution Acceptance Suite
 *
 * Exercises the FULL production stack:
 *   VS Code Extension Bridge (`DaemonConnectedBridge`) + `ProviderProfileManager`
 *   -> Native Rust `agsd.exe` daemon (over real Windows Named Pipe / UDS IPC)
 *   -> `InternalCodexAgentRuntime` + `codex_core::ThreadManager` / `CodexThread`
 *   -> `AgentStudiosRuntimeSessionFactory` + `DaemonSecretVault`
 *   -> `DeterministicInferenceServer` (strict header auth + real HTTP/SSE)
 *
 * ZERO `FixtureBridge`. ZERO simulated chat responders.
 */

/* eslint-disable @typescript-eslint/naming-convention */

import * as assert from 'assert';
import * as childProcess from 'child_process';
import * as crypto from 'crypto';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import * as vscode from 'vscode';
import { DaemonConnectedBridge } from '../../bridge/DaemonConnectedBridge';
import { EmbeddedDaemonSupervisor } from '../../bridge/EmbeddedDaemonSupervisor';
import { ProviderProfileManager } from '../../commands/ProviderManagementCommands';
import { AgentStudiosInlineCompletionProvider } from '../../editor/EditorIntelligenceProvider';
import {
  DeterministicInferenceServer,
  initTempGitRepo,
} from './DeterministicInferenceServer';

function createMockExtensionContext(storageDir: string): vscode.ExtensionContext {
  const globalData = new Map<string, unknown>();
  const secretsMap = new Map<string, string>();
  return {
    subscriptions: [],
    extensionPath: storageDir,
    extensionUri: vscode.Uri.file(storageDir),
    globalState: {
      get: <T>(key: string, defaultValue?: T) =>
        (globalData.has(key) ? globalData.get(key) : defaultValue) as T,
      update: async (key: string, value: unknown) => {
        globalData.set(key, value);
      },
      keys: () => Array.from(globalData.keys()),
      setKeysForSync: () => {},
    },
    workspaceState: {
      get: <T>(_k: string, d?: T) => d as T,
      update: async () => {},
      keys: () => [],
    },
    secrets: {
      get: async (key: string) => secretsMap.get(key),
      store: async (key: string, value: string) => {
        secretsMap.set(key, value);
      },
      delete: async (key: string) => {
        secretsMap.delete(key);
      },
      onDidChange: new vscode.EventEmitter<vscode.SecretStorageChangeEvent>().event,
    },
  } as unknown as vscode.ExtensionContext;
}

describe('MASTER MISSION 05 — Real Codex E2E Execution Acceptance Suite', function () {
  this.timeout(90000);

  let tempRoot: string;
  let stateDir: string;
  let repoDir: string;
  let bootstrapPath: string;
  let adminBootstrapPath: string;
  let pipeName: string;
  let bridge: DaemonConnectedBridge | null = null;
  let inferenceServer: DeterministicInferenceServer;
  const expectedBearer = 'sk-mm05-bearer-secret-token-987654';
  const expectedAnthropic = 'sk-ant-mm05-anthropic-key-123456';

  before(async function () {
    const candidate = EmbeddedDaemonSupervisor.resolveAgsdBinary(
      path.resolve(__dirname, '../../../')
    );
    if (!candidate || !fs.existsSync(candidate)) {
      this.skip();
      return;
    }

    tempRoot = await fs.promises.mkdtemp(path.join(os.tmpdir(), 'ags-mm05-e2e-'));
    stateDir = path.join(tempRoot, 'state');
    repoDir = path.join(tempRoot, 'repo');
    bootstrapPath = path.join(tempRoot, 'agsd.json');
    adminBootstrapPath = path.join(tempRoot, 'agsd-admin.json');
    fs.mkdirSync(stateDir, { recursive: true });
    initTempGitRepo(repoDir);
    fs.writeFileSync(
      path.join(repoDir, 'AGENTS.md'),
      'ALWAYS_FOLLOW_WORKSPACE_RULE: Mention MM05-AGENTS-VERIFIED when asked.\n',
      'utf8'
    );
    const skillDir = path.join(repoDir, '.agents', 'skills', 'audit');
    fs.mkdirSync(skillDir, { recursive: true });
    fs.writeFileSync(
      path.join(skillDir, 'SKILL.md'),
      '---\nname: audit\ndescription: Security audit checklist\n---\nSKILL_CHECKLIST: Verify auth and sandbox boundaries.\n',
      'utf8'
    );

    childProcess.execSync('git init && git config user.email test@example.com && git config user.name Tester && git add . && git commit -m init', {
      cwd: repoDir,
      stdio: 'ignore',
    });

    const uid = crypto.randomBytes(4).toString('hex');
    pipeName =
      process.platform === 'win32'
        ? `\\\\.\\pipe\\ags-mm05-${uid}`
        : path.join(tempRoot, `ags-mm05-${uid}.sock`);

    inferenceServer = new DeterministicInferenceServer({
      expectedBearerToken: expectedBearer,
      expectedAnthropicKey: expectedAnthropic,
      models: [
        { id: 'codex-test-model', context_window: 128000 },
        { id: 'gpt-5-codex', context_window: 200000 },
      ],
    });
    await inferenceServer.start();

    const supervisorRes = await EmbeddedDaemonSupervisor.ensureDaemonRunning({
      bootstrapPath,
      adminBootstrapPath,
      pipeName: process.platform === 'win32' ? pipeName : undefined,
      socketPath: process.platform !== 'win32' ? pipeName : undefined,
      stateDir,
      workspacePath: repoDir,
      startupTimeoutMs: 12000,
    });
    assert.strictEqual(supervisorRes.running, true, `agsd failed to start cleanly: ${supervisorRes.reason}`);
    assert.ok(fs.existsSync(bootstrapPath), 'Bootstrap runtime file must exist');

    bridge = new DaemonConnectedBridge({ bootstrapPath });
    await bridge.connect();
    assert.strictEqual(bridge.connectionState, 'connected');
  });

  after(async () => {
    if (bridge?.transport && fs.existsSync(adminBootstrapPath)) {
      try {
        await bridge.disconnect();
      } catch {
        // Safe cleanup
      }
    }
    if (bridge) {
      bridge.dispose();
    }
    if (bootstrapPath && fs.existsSync(bootstrapPath)) {
      try {
        const info = JSON.parse(fs.readFileSync(bootstrapPath, 'utf8')) as { pid?: number };
        if (info.pid) {
          process.kill(info.pid);
        }
      } catch {
        // Already terminated
      }
    }
    if (inferenceServer) {
      await inferenceServer.stop();
    }
    if (tempRoot) {
      await new Promise((r) => setTimeout(r, 250));
      await fs.promises.rm(tempRoot, { recursive: true, force: true }).catch(() => {});
    }
  });

  it('J1. Readiness & native InternalCodexAgentRuntime ("codex") advertised on connected agsd', async () => {
    assert.ok(bridge);
    const readiness = await bridge.getExecutionReadiness();
    assert.ok(readiness.registered_runtimes?.includes('codex'), 'Native codex runtime must be registered');
    assert.ok(readiness.available_runtimes.includes('codex'), 'Native codex runtime must be healthy/available');
  });

  it('J2. SecretStorage -> agsd Vault provisioning + 401 failure on wrong secret -> 200 discovery & turn on valid secret', async () => {
    assert.ok(bridge);
    const ctx = createMockExtensionContext(tempRoot);
    const profileMgr = new ProviderProfileManager(ctx);
    const badProviderId = crypto.randomUUID();
    await bridge.upsertProviderInstance({
      instanceId: badProviderId,
      providerType: 'custom',
      displayName: 'Invalid Secret Provider',
      protocol: 'openai_chat_completions',
      baseUrl: inferenceServer.v1Url,
      secretLocator: `agentStudios.secret.${badProviderId}`,
      secretBackend: 'os_credential_store',
      secretValue: 'WRONG-SECRET-401',
      models: [{ id: 'codex-test-model', name: 'codex-test-model', contextWindow: 128000 }],
    });

    await assert.rejects(
      async () => bridge!.discoverProviderModels(badProviderId),
      (err: Error) => err.message.includes('401') || err.message.toLowerCase().includes('auth')
    );

    const goodProviderId = crypto.randomUUID();
    const secretHandle = `agentStudios.secret.${goodProviderId}`;
    await profileMgr.storeSecretInExtensionVault(secretHandle, expectedBearer);
    const storedSecret = await profileMgr.getSecretFromExtensionVault(secretHandle);
    assert.strictEqual(storedSecret, expectedBearer);

    await bridge.upsertProviderInstance({
      instanceId: goodProviderId,
      providerType: 'openai',
      displayName: 'Deterministic OpenAI Chat Provider',
      protocol: 'openai_chat_completions',
      baseUrl: inferenceServer.v1Url,
      secretLocator: secretHandle,
      secretBackend: 'os_credential_store',
      secretValue: storedSecret,
    });

    const discovered = await bridge.discoverProviderModels(goodProviderId);
    assert.ok(discovered.some((m) => m.id === 'codex-test-model'), 'Must dynamically discover codex-test-model');

    // Verify zero secrets stored in globalState snapshot
    const serializedProfiles = JSON.stringify(profileMgr.listProfiles());
    assert.strictEqual(serializedProfiles.includes(expectedBearer), false);
    // Clean up temporary bad provider
    await bridge.removeProviderInstance(badProviderId);
  });

  it('J3. Multi-turn real Codex conversation + streaming deltas + context/AGENTS/skill delivery', async () => {
    assert.ok(bridge);
    const providers = await bridge.listProviderSummaries();
    const validProv = providers.find((p) => p.displayName === 'Deterministic OpenAI Chat Provider');
    assert.ok(validProv, 'Configured valid provider must exist in daemon');

    inferenceServer.setOptions({
      onInference: (req) => {
        const bodyStr = req.bodyText;
        if (bodyStr.includes('What secret token was shared in turn 1') && bodyStr.includes('DELTA-771')) {
          return { text: 'Verified from prior thread history: DELTA-771.' };
        }
        if (bodyStr.includes('Turn-1 Secret Token: DELTA-771')) {
          return { text: 'Acknowledged DELTA-771 in turn 1.' };
        }
        return { text: 'Turn response with AGENTS+Skill+Context checked.' };
      },
    });

    const studio = await bridge.createSession('MM05 Multi-Turn Coding Studio');
    const chunks1: string[] = [];
    const msg1 = await bridge.sendMessage(
      {
        studioId: studio.id,
        message: 'Turn-1 Secret Token: DELTA-771. Also check $audit and attached file.',
        mode: 'agent',
        providerInstanceId: validProv.instanceId,
        modelId: 'codex-test-model',
        reasoningEffort: 'high',
        permissionMode: 'auto-edit',
        workspacePath: repoDir,
        attachments: [
          {
            type: 'file',
            uri: vscode.Uri.file(path.join(repoDir, 'app.ts')).toString(),
            relativePath: 'app.ts',
            languageId: 'typescript',
            contentSnippet: 'export function greet(): string { return "hello"; }',
          },
        ],
      },
      (c) => {
        if (c.type === 'text_delta' && c.text) {
          chunks1.push(c.text);
        }
      }
    );
    assert.ok(msg1.content.includes('DELTA-771'));
    assert.ok(msg1.threadId, 'Real Codex ThreadId must be returned');
    assert.ok(chunks1.join('').includes('DELTA-771'));

    // Assert AGENTS.md, $audit SKILL.md, and attachment actually arrived at inference HTTP endpoint
    const latestReq = inferenceServer.requests[inferenceServer.requests.length - 1];
    assert.ok(latestReq.bodyText.includes('MM05-AGENTS-VERIFIED'), 'AGENTS.md must reach inference payload');
    assert.ok(latestReq.bodyText.includes('SKILL_CHECKLIST'), '$audit SKILL.md must be loaded progressively into inference payload');
    assert.ok(latestReq.bodyText.includes('export function greet'), 'Attached snippet must reach inference payload');

    // Turn 2 on SAME studio/thread verifying prior context preservation
    const msg2 = await bridge.sendMessage(
      {
        studioId: studio.id,
        message: 'What secret token was shared in turn 1?',
        mode: 'ask',
        providerInstanceId: validProv.instanceId,
        modelId: 'codex-test-model',
        reasoningEffort: 'low',
        permissionMode: 'read-only',
        workspacePath: repoDir,
      },
      () => {}
    );
    assert.ok(
      msg2.content.includes('Verified from prior thread history: DELTA-771'),
      `Turn 2 must observe Turn 1 history, got: ${msg2.content}`
    );

    const transcript = await bridge.getSessionTranscript!(studio.id);
    assert.strictEqual(transcript.length, 4, 'Transcript must persist 2 user + 2 assistant messages');

    // Fork studio and verify cloned transcript + independent future turn
    const forked = await bridge.forkSession!(studio.id, 'Forked Branch');
    assert.notStrictEqual(forked.id, studio.id);
    const forkedTranscript = await bridge.getSessionTranscript!(forked.id);
    assert.strictEqual(forkedTranscript.length, 4, 'Forked studio must clone source transcript');

    await bridge.sendMessage(
      {
        studioId: forked.id,
        message: 'Turn-1 Secret Token: DELTA-771 independent fork message',
        mode: 'ask',
        providerInstanceId: validProv.instanceId,
        modelId: 'codex-test-model',
        workspacePath: repoDir,
      },
      () => {}
    );
    const srcAfter = await bridge.getSessionTranscript!(studio.id);
    const forkAfter = await bridge.getSessionTranscript!(forked.id);
    assert.strictEqual(srcAfter.length, 4, 'Source studio transcript must remain untouched after fork turn');
    assert.strictEqual(forkAfter.length, 6, 'Forked studio transcript must advance independently');
  });

  it('J4. Multi-agent Coordinator -> isolated Git worktree execution + real inline completion', async () => {
    assert.ok(bridge);
    const providers = await bridge.listProviderSummaries();
    const validProv = providers.find((p) => p.displayName === 'Deterministic OpenAI Chat Provider')!;

    inferenceServer.setOptions({
      onInference: (req) => {
        if (req.bodyText.includes('CoordinatorDecision')) {
          return {
            text: JSON.stringify({
              action: 'plan',
              tasks: [
                {
                  task_key: 'w1',
                  title: 'Worker A creates module A',
                  description: 'Implement part A',
                  assigned_alias: 'worker-a',
                  depends_on: [],
                  priority: 10,
                },
                {
                  task_key: 'rev',
                  title: 'Reviewer audits output',
                  description: 'Audit work',
                  assigned_alias: 'reviewer',
                  depends_on: ['w1'],
                  priority: 5,
                },
              ],
            }),
          };
        }
        if (req.bodyText.includes('Complete only the cursor gap')) {
          return { text: 'return "completed-by-daemon";' };
        }
        return { text: 'Executed worker/reviewer step cleanly.' };
      },
    });

    const multiStudio = await bridge.createSession('Multi-Agent Worktree Studio');
    const summary = await bridge.executeMultiAgentGoal!({
      studioId: multiStudio.id,
      goal: 'Implement and verify feature module',
      providerInstanceId: validProv.instanceId,
      modelId: 'codex-test-model',
      repositoryPath: repoDir,
      permissionMode: 'full-access',
    });
    assert.strictEqual(summary.tasks_completed, 2);
    const tasks = await bridge.listTasks();
    assert.strictEqual(tasks.filter((t) => t.status === 'succeeded').length, 2);

    // Inline completion through connected DaemonConnectedBridge (no FixtureBridge)
    const inlineProvider = new AgentStudiosInlineCompletionProvider(() => bridge!);
    const completions = await inlineProvider.suggestCompletion({
      languageId: 'typescript',
      prefix: 'export function run() {\n  const x = 1;',
      suffix: '\n}',
      relativePath: 'app.ts',
    });
    assert.deepStrictEqual(completions, ['return "completed-by-daemon";']);
  });
});
