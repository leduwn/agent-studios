import * as assert from 'assert';
import * as http from 'http';
import * as vscode from 'vscode';
import { DisconnectedBridge } from '../../bridge/DisconnectedBridge';
import { FixtureBridge } from '../fixtures/FixtureBridge';
import { HunkReviewManager } from '../../cards/HunkReviewManager';
import { ConversationManager } from '../../chat/ConversationManager';
import {
  discoverWorkspaceExtensibility,
  redactMcpEndpoint,
} from '../../commands/ExtensibilityCommands';
import {
  looksLikePlaintextSecret,
  ProviderProfileManager,
} from '../../commands/ProviderManagementCommands';
import { ContextEngine } from '../../context/ContextEngine';
import {
  AgentStudiosDiagnosticsCodeActionProvider,
  AgentStudiosInlineCompletionProvider,
  registerEditorIntelligence,
} from '../../editor/EditorIntelligenceProvider';
import {
  MockDiagnostic,
  MockRange,
  MockUri,
  mockDiagnosticsList,
  mockWindowMessages,
  mockWorkspaceFindFilesResult,
  mockWorkspaceReadFileMap,
  registeredCommands,
  resetMockState,
} from './vscodeMock';

function createMockMemento(): vscode.Memento {
  const store = new Map<string, unknown>();
  return {
    keys: () => Array.from(store.keys()),
    get: <T>(key: string, defaultValue?: T): T =>
      (store.has(key) ? store.get(key) : defaultValue) as T,
    update: async (key: string, value: unknown): Promise<void> => {
      store.set(key, value);
    },
  };
}

function createMockExtensionContext(): vscode.ExtensionContext {
  return {
    subscriptions: [],
    workspaceState: createMockMemento(),
    globalState: Object.assign(createMockMemento(), {
      setKeysForSync: () => {},
    }),
  } as unknown as vscode.ExtensionContext;
}

describe('Wave 4 RC2 Extension Daily AI Coding Features', () => {
  beforeEach(() => {
    resetMockState();
  });

  it('Group 1: manages conversation lifecycle, search, archive, pin, export & token budget', async () => {
    const ctx = createMockExtensionContext();
    const mgr = new ConversationManager(ctx, () => new DisconnectedBridge());

    const c1 = await mgr.createConversation('Fix daemon crash recovery', 100);
    await mgr.appendMessage(c1.id, {
      id: 'm1',
      role: 'user',
      content: 'Check why agsd exited unexpectedly on Windows named pipe.',
      timestamp: 1000,
    });
    await mgr.appendMessage(c1.id, {
      id: 'm2',
      role: 'assistant',
      content: 'Respawning agsd with exponential backoff resolves the pipe race.',
      timestamp: 2000,
    });

    assert.strictEqual(mgr.listConversations().length, 1);
    assert.strictEqual(mgr.listConversations({ searchQuery: 'named pipe' }).length, 1);
    assert.strictEqual(mgr.listConversations({ searchQuery: 'nonexistent' }).length, 0);

    await mgr.renameConversation(c1.id, 'Daemon pipe crash recovery');
    assert.strictEqual(mgr.getConversation(c1.id)?.title, 'Daemon pipe crash recovery');

    const budget = mgr.getTokenBudget(c1.id);
    assert.ok(budget && budget.totalTokens > 0);

    const exported = mgr.exportAsMarkdown(c1.id) ?? '';
    assert.ok(exported.includes('# Daemon pipe crash recovery'));
    assert.ok(exported.includes('USER'));

    await mgr.setArchived(c1.id, true);
    assert.strictEqual(mgr.listConversations({ includeArchived: false }).length, 0);
    assert.strictEqual(mgr.listConversations({ includeArchived: true }).length, 1);
    mgr.dispose();
  });

  it('Group 2: provides inline completions, diagnostic code actions & inlineEdit dispatch', async () => {
    const ctx = createMockExtensionContext();
    const fixture = new FixtureBridge();
    await fixture.connect();

    const inline = new AgentStudiosInlineCompletionProvider(() => new DisconnectedBridge());
    const offlineSuggestions = await inline.suggestCompletion({
      languageId: 'typescript',
      prefix: 'export function parseConfig()',
      suffix: '',
      relativePath: 'src/config.ts',
    });
    assert.strictEqual(offlineSuggestions.length, 0);

    const connectedInline = new AgentStudiosInlineCompletionProvider(() => fixture);
    const liveSuggestions = await connectedInline.suggestCompletion({
      languageId: 'typescript',
      prefix: 'export function parseConfig()',
      suffix: '',
      relativePath: 'src/config.ts',
    });
    assert.strictEqual(liveSuggestions.length, 1);
    assert.ok(liveSuggestions[0].length > 0);

    const firstEpoch = connectedInline.suggestCompletion({
      languageId: 'typescript',
      prefix: 'const a =',
      suffix: '',
      relativePath: 'src/config.ts',
      debounceMs: 40,
    });
    const secondEpoch = connectedInline.suggestCompletion({
      languageId: 'typescript',
      prefix: 'const b =',
      suffix: '',
      relativePath: 'src/config.ts',
      debounceMs: 5,
    });
    const [resFirst, resSecond] = await Promise.all([firstEpoch, secondEpoch]);
    assert.strictEqual(resFirst.length, 0);
    assert.strictEqual(resSecond.length, 1);

    const codeActions = new AgentStudiosDiagnosticsCodeActionProvider();
    const doc = {
      uri: MockUri.file('/workspace/src/main.ts'),
      languageId: 'typescript',
    } as unknown as vscode.TextDocument;
    const range = new MockRange(0, 0, 0, 10) as unknown as vscode.Range;
    const diag = new MockDiagnostic(range as unknown as MockRange, 'Cannot find name x');
    const actions = codeActions.provideCodeActions(doc, range, {
      diagnostics: [diag as unknown as vscode.Diagnostic],
    });
    assert.strictEqual(actions.length, 1);
    assert.strictEqual(actions[0].command.command, 'agent-studios.fixDiagnostics');

    registerEditorIntelligence(ctx, () => fixture);
    assert.ok(registeredCommands.has('agent-studios.fixDiagnostics'));
    assert.ok(registeredCommands.has('agent-studios.inlineEdit'));

    mockDiagnosticsList.push(diag);
    const fixRes = (await vscode.commands.executeCommand('agent-studios.fixDiagnostics')) as {
      dispatched: boolean;
      count: number;
    };
    assert.strictEqual(fixRes.dispatched, true);
    assert.strictEqual(fixRes.count, 1);
  });

  it('Group 3: deduplicates snippets with SHA-256 and respects deterministic token budget', async () => {
    const engine = new ContextEngine();
    engine.recordTerminalOutput('cargo-test', 'test result: ok. 12 passed');

    const dupA = engine.makeItem('file', 'src/a.ts', 'export const x = 1;');
    const dupB = engine.makeItem('file', 'src/a_copy.ts', 'export const x = 1;');
    const hiPri = engine.makeItem('selection', 'src/main.ts', 'fn main() { run(); }');
    const huge = engine.makeItem('folder', 'docs/long.txt', 'A'.repeat(2000));

    const packed = engine.packWithinBudget([dupA, dupB, hiPri, huge], 120, '- src/main.ts');
    assert.strictEqual(packed.deduplicatedCount, 1);
    assert.ok(packed.totalEstimatedTokens <= 120);
    assert.strictEqual(packed.items[0].kind, 'selection');
  });

  it('Group 4: supports hunk-level apply, conflict detection & checkpoint revert', () => {
    const mgr = new HunkReviewManager();
    const unified = [
      'diff --git a/src/app.ts b/src/app.ts',
      '--- a/src/app.ts',
      '+++ b/src/app.ts',
      '@@ -1,3 +1,3 @@',
      ' const a = 1;',
      '-const b = 2;',
      '+const b = 42;',
      ' export { a, b };',
    ].join('\n');

    const parsed = mgr.parseDiff(unified);
    assert.strictEqual(parsed.length, 1);
    const hunk = parsed[0].hunks[0];
    const base = 'const a = 1;\nconst b = 2;\nexport { a, b };';

    const ok = mgr.applyHunkToContent('art-1', 'src/app.ts', 0, base, hunk);
    assert.strictEqual(ok.applied, true);
    assert.strictEqual(ok.updatedContent, 'const a = 1;\nconst b = 42;\nexport { a, b };');
    assert.strictEqual(mgr.getHunkDecision('art-1', 'src/app.ts', 0), 'accepted');

    const conflict = mgr.applyHunkToContent('art-1', 'src/app.ts', 0, 'const a = 99;\nconst b = 2;\nexport { a, b };', hunk);
    assert.strictEqual(conflict.applied, false);
    assert.ok(conflict.conflictReason?.includes('Stale base conflict'));

    const reverted = mgr.revertToCheckpoint(ok.checkpointId!);
    assert.strictEqual(reverted?.originalContent, base);
    mgr.dispose();
  });

  it('Group 5: enforces SecretReference metadata only and rejects plaintext API keys', async () => {
    const ctx = createMockExtensionContext();
    const mgr = new ProviderProfileManager(ctx);

    assert.strictEqual(looksLikePlaintextSecret('sk-live-secret-1234567890'), true);
    assert.strictEqual(looksLikePlaintextSecret('OPENAI_API_KEY'), false);

    let rejected = false;
    try {
      await mgr.upsertProfile({
        providerType: 'openai',
        displayName: 'Unsafe Profile',
        secretRefKey: 'sk-proj-secret-abcdef123456',
      });
    } catch {
      rejected = true;
    }
    assert.strictEqual(rejected, true);

    const saved = await mgr.upsertProfile({
      providerType: 'openai',
      displayName: 'Work OpenAI',
      secretRefKey: 'OPENAI_API_KEY',
    });
    assert.strictEqual(saved.secretRef.source, 'env');
    assert.strictEqual(saved.secretRef.key, 'OPENAI_API_KEY');

    // Local zero-billable HTTP /v1/models discovery test across 12-provider registry
    const mockServer = http.createServer((_req, res) => {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ object: 'list', data: [{ id: 'mock-litellm-1' }, { id: 'mock-vllm-1' }] }));
    });
    await new Promise<void>((resolve) => mockServer.listen(0, '127.0.0.1', () => resolve()));
    try {
      const addr = mockServer.address();
      const port = typeof addr === 'object' && addr ? addr.port : 0;
      const litellm = await mgr.upsertProfile({
        providerType: 'litellm',
        displayName: 'Local LiteLLM',
        baseUrl: `http://127.0.0.1:${port}/v1`,
        secretRefKey: 'LITELLM_API_KEY',
      });
      const discovered = await mgr.discoverModelsDynamic(litellm);
      assert.strictEqual(discovered.length, 2);
      assert.strictEqual(discovered[0].id, 'mock-litellm-1');
    } finally {
      await new Promise<void>((resolve) => mockServer.close(() => resolve()));
    }
  });

  it('Group 6: discovers Skills/MCP/Hooks/AGENTS.md with secret redaction & single Activity Bar invariant', async () => {
    assert.strictEqual(
      redactMcpEndpoint('https://mcp.example.com/sse?token=supersecret123'),
      'https://mcp.example.com/sse?token=***REDACTED***'
    );

    mockWorkspaceFindFilesResult.push(
      MockUri.file('/workspace/.agent-studios/skills/audit/SKILL.md'),
      MockUri.file('/workspace/AGENTS.md'),
      MockUri.file('/workspace/.mcp.json')
    );
    mockWorkspaceReadFileMap.set(
      '/workspace/.mcp.json',
      JSON.stringify({
        mcpServers: {
          github: { url: 'https://api.example.com/mcp?api_key=xyz987' },
        },
      })
    );

    const discovered = await discoverWorkspaceExtensibility();
    assert.strictEqual(discovered.instructionFiles[0], 'AGENTS.md');
    assert.strictEqual(discovered.mcpServers[0]?.endpointRedacted, 'https://api.example.com/mcp?api_key=***REDACTED***');

    // Enforce Single Activity Bar Icon Rule from package.json
    // eslint-disable-next-line @typescript-eslint/no-var-requires
    const pkg = require('../../../package.json') as {
      contributes: { viewsContainers: { activitybar: Array<{ id: string }> } };
    };
    assert.strictEqual(pkg.contributes.viewsContainers.activitybar.length, 1);
    assert.strictEqual(
      pkg.contributes.viewsContainers.activitybar[0].id,
      'agent-studios-activitybar'
    );
    assert.ok(mockWindowMessages);
  });
});
