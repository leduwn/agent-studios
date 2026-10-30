/**
 * Agent Studios — Composer & Context Attachment Pure Unit Tests
 * Milestone M14-UI-01: Professional AI Coding Assistant Experience
 */

import * as assert from 'assert';
import * as vscode from 'vscode';
import { FixtureBridge } from '../../bridge/FixtureBridge';
import { DisconnectedBridge } from '../../bridge/DisconnectedBridge';
import { ContextAttachment, SlashCommandContext } from '../../bridge/types';
import { SlashCommandRegistry } from '../../composer/SlashCommandRegistry';
import {
  ContextAttachmentManager,
  MAX_ATTACHMENT_SIZE_BYTES,
  MAX_ATTACHMENTS_PER_PROMPT,
  MAX_SNIPPET_LINES,
  boundContentSnippet,
  truncateToByteLimit,
} from '../../composer/ContextAttachmentManager';
import {
  mockDiagnosticsList,
  mockWorkspaceFindFilesResult,
  resetMockState,
} from './vscodeMock';

describe('Composer & Context Unit Tests', () => {
  let fixtureBridge: FixtureBridge;
  let disconnectedBridge: DisconnectedBridge;

  beforeEach(() => {
    resetMockState();
    fixtureBridge = new FixtureBridge();
    disconnectedBridge = new DisconnectedBridge();
  });

  afterEach(() => {
    fixtureBridge.dispose();
    disconnectedBridge.dispose();
    resetMockState();
  });

  describe('SlashCommandRegistry', () => {
    let registry: SlashCommandRegistry;

    beforeEach(() => {
      registry = new SlashCommandRegistry();
    });

    afterEach(() => {
      registry.dispose();
    });

    it('initializes with all 8 canonical slash commands', () => {
      const commands = registry.listCommands();
      const names = commands.map((c) => registry.normalizeName(c.name));

      assert.strictEqual(commands.length, 8);
      assert.ok(names.includes('help'));
      assert.ok(names.includes('clear'));
      assert.ok(names.includes('plan'));
      assert.ok(names.includes('review'));
      assert.ok(names.includes('explain'));
      assert.ok(names.includes('tests'));
      assert.ok(names.includes('providers'));
      assert.ok(names.includes('sessions'));
    });

    it('normalizes command names with leading slashes, whitespace, and case', () => {
      assert.strictEqual(registry.normalizeName('/Help'), 'help');
      assert.strictEqual(registry.normalizeName('  /PLAN  '), 'plan');
      assert.strictEqual(registry.normalizeName('review'), 'review');

      assert.ok(registry.getCommand('help'));
      assert.ok(registry.getCommand('/help'));
      assert.ok(registry.getCommand('  /HELP '));
    });

    it('allows dynamic registration and unregistration of custom commands', () => {
      const customDisposable = registry.register({
        name: '/custom',
        description: 'Custom test command',
        execute: async () => ({ success: true, message: 'Custom executed' }),
      });

      assert.ok(registry.getCommand('/custom'));
      assert.strictEqual(registry.listCommands().length, 9);

      customDisposable.dispose();
      assert.strictEqual(registry.getCommand('/custom'), undefined);
      assert.strictEqual(registry.listCommands().length, 8);
    });

    it('parses slash commands and arguments correctly', () => {
      assert.strictEqual(registry.isSlashCommand('/plan refactor code'), true);
      assert.strictEqual(registry.isSlashCommand('hello world'), false);

      const parsedWithArgs = registry.parseCommand('/plan implement auth service');
      assert.deepStrictEqual(parsedWithArgs, {
        name: 'plan',
        args: 'implement auth service',
      });

      const parsedNoArgs = registry.parseCommand('/clear');
      assert.deepStrictEqual(parsedNoArgs, {
        name: 'clear',
        args: '',
      });

      assert.strictEqual(registry.parseCommand('not a command'), null);
    });

    it('visibly marks unrecognized slash commands as unsupported', async () => {
      const context: SlashCommandContext = { bridge: fixtureBridge };
      const result = await registry.executeCommand('/unknownCommand arg1', context);

      assert.strictEqual(result.success, false);
      assert.strictEqual(result.unsupported, true);
      assert.ok(result.message?.includes("Command '/unknownCommand' is not recognized"));
    });

    it('/help returns formatted markdown listing commands and keyboard shortcuts', async () => {
      const context: SlashCommandContext = { bridge: fixtureBridge };
      const result = await registry.executeCommand('/help', context);

      assert.strictEqual(result.success, true);
      assert.strictEqual(result.actionTaken, 'help');
      assert.ok(result.markdown);
      assert.ok(result.markdown.includes('/help'));
      assert.ok(result.markdown.includes('/plan'));
      assert.ok(result.markdown.includes('/review'));
      assert.ok(result.markdown.includes('Keyboard Shortcuts'));
      assert.ok(result.markdown.includes('Ctrl+Enter'));
      assert.ok(result.markdown.includes('Ctrl+L'));
      assert.ok(result.markdown.includes('Escape'));
    });

    it('/clear invokes clearHistory callback on context', async () => {
      let cleared = false;
      const context: SlashCommandContext = {
        bridge: fixtureBridge,
        clearHistory: async () => {
          cleared = true;
        },
      };

      const result = await registry.executeCommand('/clear', context);
      assert.strictEqual(result.success, true);
      assert.strictEqual(result.actionTaken, 'clear');
      assert.strictEqual(cleared, true);
    });

    it('/plan sets mode to plan and formats prompt for architectural planning', async () => {
      let selectedMode = '';
      const context: SlashCommandContext = {
        bridge: fixtureBridge,
        setMode: (mode) => {
          selectedMode = mode;
        },
      };

      // With specific goal
      const result = await registry.executeCommand('/plan design multi-agent coordination', context);
      assert.strictEqual(result.success, true);
      assert.strictEqual(result.mode, 'plan');
      assert.strictEqual(result.actionTaken, 'plan');
      assert.strictEqual(selectedMode, 'plan');
      assert.ok(result.formattedPrompt?.includes('Goal: design multi-agent coordination'));
      assert.ok(result.formattedPrompt?.includes('[Architectural Plan Request]'));

      // Without goal args (default formulation)
      const defaultResult = await registry.executeCommand('/plan', context);
      assert.strictEqual(defaultResult.success, true);
      assert.strictEqual(defaultResult.mode, 'plan');
      assert.ok(defaultResult.formattedPrompt?.includes('formulate an execution plan'));
    });

    describe('/review command & fail-closed/unsupported handling', () => {
      it('marks unsupported when bridge is disconnected', async () => {
        const context: SlashCommandContext = { bridge: disconnectedBridge };
        const result = await registry.executeCommand('/review', context);

        assert.strictEqual(result.success, false);
        assert.strictEqual(result.unsupported, true);
        assert.ok(result.message?.includes('disconnected'));
      });

      it('triggers code review on explicit target arguments', async () => {
        const context: SlashCommandContext = { bridge: fixtureBridge };
        const result = await registry.executeCommand('/review src/bridge/types.ts', context);

        assert.strictEqual(result.success, true);
        assert.strictEqual(result.actionTaken, 'review');
        assert.ok(result.formattedPrompt?.includes('src/bridge/types.ts'));
      });

      it('triggers code review on active artifact patch when available', async () => {
        const context: SlashCommandContext = { bridge: fixtureBridge };
        const result = await registry.executeCommand('/review', context);

        assert.strictEqual(result.success, true);
        assert.strictEqual(result.actionTaken, 'review');
        assert.ok(result.formattedPrompt?.includes('task-m14-bridge-contracts.patch'));
        assert.ok(result.formattedPrompt?.includes('diff'));
      });

      it('visibly marks unsupported when no artifacts/diff exist and no args given', async () => {
        // Create bridge with no artifacts
        const emptyBridge = new FixtureBridge();
        // Override listArtifacts to return empty
        emptyBridge.listArtifacts = async () => [];

        const context: SlashCommandContext = { bridge: emptyBridge };
        const result = await registry.executeCommand('/review', context);

        assert.strictEqual(result.success, false);
        assert.strictEqual(result.unsupported, true);
        assert.ok(result.message?.includes('no active diff, patch artifacts'));
        emptyBridge.dispose();
      });
    });

    describe('/explain command', () => {
      it('marks unsupported when disconnected from daemon', async () => {
        const context: SlashCommandContext = { bridge: disconnectedBridge };
        const result = await registry.executeCommand('/explain', context);

        assert.strictEqual(result.success, false);
        assert.strictEqual(result.unsupported, true);
        assert.ok(result.message?.includes('disconnected'));
      });

      it('marks unsupported when no active editor or selection is found', async () => {
        const context: SlashCommandContext = {
          bridge: fixtureBridge,
          activeEditorContext: null,
          captureActiveEditorContext: async () => null,
        };
        const result = await registry.executeCommand('/explain', context);

        assert.strictEqual(result.success, false);
        assert.strictEqual(result.unsupported, true);
        assert.ok(result.message?.includes('no active text editor or selection'));
      });

      it('formats explanation prompt with code snippet and user query', async () => {
        const context: SlashCommandContext = {
          bridge: fixtureBridge,
          activeEditorContext: {
            type: 'selection',
            uri: 'file:///workspace/src/auth.ts',
            relativePath: 'src/auth.ts',
            selectionRange: { startLine: 10, endLine: 25 },
            contentSnippet: 'function authenticate(token: string) { return true; }',
            languageId: 'typescript',
          },
        };

        const result = await registry.executeCommand(
          '/explain how does token validation work?',
          context
        );

        assert.strictEqual(result.success, true);
        assert.strictEqual(result.actionTaken, 'explain');
        assert.ok(result.formattedPrompt?.includes('src/auth.ts'));
        assert.ok(result.formattedPrompt?.includes('(lines 10-25)'));
        assert.ok(result.formattedPrompt?.includes('function authenticate'));
        assert.ok(result.formattedPrompt?.includes('how does token validation work?'));
      });
    });

    describe('/tests command', () => {
      it('marks unsupported when disconnected from daemon', async () => {
        const context: SlashCommandContext = { bridge: disconnectedBridge };
        const result = await registry.executeCommand('/tests', context);

        assert.strictEqual(result.success, false);
        assert.strictEqual(result.unsupported, true);
      });

      it('marks unsupported when no active file is open', async () => {
        const context: SlashCommandContext = {
          bridge: fixtureBridge,
          activeEditorContext: null,
          captureActiveEditorContext: async () => null,
        };
        const result = await registry.executeCommand('/tests', context);

        assert.strictEqual(result.success, false);
        assert.strictEqual(result.unsupported, true);
        assert.ok(result.message?.includes('no active editor file found'));
      });

      it('generates unit tests prompt for active file context', async () => {
        const context: SlashCommandContext = {
          bridge: fixtureBridge,
          activeEditorContext: {
            type: 'file',
            uri: 'file:///workspace/src/math.ts',
            relativePath: 'src/math.ts',
            contentSnippet: 'export function add(a: number, b: number): number { return a + b; }',
            languageId: 'typescript',
          },
        };

        const result = await registry.executeCommand(
          '/tests cover edge cases and negative numbers',
          context
        );

        assert.strictEqual(result.success, true);
        assert.strictEqual(result.actionTaken, 'tests');
        assert.ok(result.formattedPrompt?.includes('src/math.ts'));
        assert.ok(result.formattedPrompt?.includes('typescript'));
        assert.ok(result.formattedPrompt?.includes('export function add'));
        assert.ok(result.formattedPrompt?.includes('cover edge cases and negative numbers'));
      });
    });

    describe('/providers command', () => {
      it('marks unsupported when disconnected from daemon', async () => {
        const context: SlashCommandContext = { bridge: disconnectedBridge };
        const result = await registry.executeCommand('/providers', context);

        assert.strictEqual(result.success, false);
        assert.strictEqual(result.unsupported, true);
      });

      it('renders configured providers and model summaries in markdown table', async () => {
        const context: SlashCommandContext = { bridge: fixtureBridge };
        const result = await registry.executeCommand('/providers', context);

        assert.strictEqual(result.success, true);
        assert.strictEqual(result.actionTaken, 'providers');
        assert.ok(result.markdown?.includes('Anthropic Claude 3.5 Sonnet'));
        assert.ok(result.markdown?.includes('OpenAI GPT-4o'));
        assert.ok(result.markdown?.includes('DeepSeek Coder 6.7B'));
        assert.ok(result.markdown?.includes('200k'));
      });

      it('handles empty providers list gracefully', async () => {
        const emptyBridge = new FixtureBridge();
        emptyBridge.listProviderSummaries = async () => [];

        const context: SlashCommandContext = { bridge: emptyBridge };
        const result = await registry.executeCommand('/providers', context);

        assert.strictEqual(result.success, true);
        assert.ok(result.markdown?.includes('No providers currently configured'));
        emptyBridge.dispose();
      });
    });

    describe('/sessions command & switch affordance', () => {
      it('marks unsupported when disconnected from daemon', async () => {
        const context: SlashCommandContext = { bridge: disconnectedBridge };
        const result = await registry.executeCommand('/sessions', context);

        assert.strictEqual(result.success, false);
        assert.strictEqual(result.unsupported, true);
      });

      it('lists sessions with current badge and switch commands', async () => {
        const context: SlashCommandContext = { bridge: fixtureBridge };
        const result = await registry.executeCommand('/sessions', context);

        assert.strictEqual(result.success, true);
        assert.strictEqual(result.actionTaken, 'sessions');
        assert.ok(result.markdown?.includes('session-m14-fixture'));
        assert.ok(result.markdown?.includes('(Current)'));
        assert.ok(result.markdown?.includes('/sessions switch session-m13-refactor'));
      });

      it('switches to target session via /sessions switch <id>', async () => {
        let switchedId = '';
        const context: SlashCommandContext = {
          bridge: fixtureBridge,
          switchSession: async (id) => {
            switchedId = id;
          },
        };

        const result = await registry.executeCommand(
          '/sessions switch session-m13-refactor',
          context
        );

        assert.strictEqual(result.success, true);
        assert.strictEqual(result.actionTaken, 'switch_session');
        assert.strictEqual(switchedId, 'session-m13-refactor');
        assert.ok(result.message?.includes('session-m13-refactor'));
      });

      it('reports error when switching to invalid session', async () => {
        const context: SlashCommandContext = { bridge: fixtureBridge };
        const result = await registry.executeCommand(
          '/sessions switch session-does-not-exist',
          context
        );

        assert.strictEqual(result.success, false);
        assert.ok(result.message?.includes('not found'));
      });
    });
  });

  describe('ContextAttachmentManager', () => {
    let manager: ContextAttachmentManager;

    beforeEach(() => {
      manager = new ContextAttachmentManager();
    });

    describe('captureActiveEditorContext', () => {
      it('returns null when no active editor is open', async () => {
        (vscode.window as any).activeTextEditor = undefined;
        const result = await manager.captureActiveEditorContext();
        assert.strictEqual(result, null);
      });

      it('captures full file context when selection is empty', async () => {
        const dummyUri = vscode.Uri.file('/workspace/src/index.ts');
        (vscode.window as any).activeTextEditor = {
          document: {
            uri: dummyUri,
            languageId: 'typescript',
            getText: () => 'export const version = "1.0.0";\nconsole.log(version);',
          },
          selection: {
            isEmpty: true,
            start: { line: 0, character: 0 },
            end: { line: 0, character: 0 },
          },
        };

        const result = await manager.captureActiveEditorContext();
        assert.ok(result);
        assert.strictEqual(result?.type, 'file');
        assert.strictEqual(result?.languageId, 'typescript');
        assert.strictEqual(result?.relativePath, 'src/index.ts');
        assert.strictEqual(result?.selectionRange, undefined);
        assert.ok(result?.contentSnippet?.includes('version = "1.0.0"'));
      });

      it('captures selection context with 1-based line ranges when selection is non-empty', async () => {
        const dummyUri = vscode.Uri.file('/workspace/src/service.ts');
        (vscode.window as any).activeTextEditor = {
          document: {
            uri: dummyUri,
            languageId: 'typescript',
            getText: (selection?: any) => {
              if (selection) {
                return 'class Service {\n  run() {}\n}';
              }
              return 'entire document';
            },
          },
          selection: {
            isEmpty: false,
            start: { line: 4, character: 0 },
            end: { line: 6, character: 1 },
          },
        };

        const result = await manager.captureActiveEditorContext();
        assert.ok(result);
        assert.strictEqual(result?.type, 'selection');
        assert.strictEqual(result?.relativePath, 'src/service.ts');
        assert.deepStrictEqual(result?.selectionRange, {
          startLine: 5,
          endLine: 7,
        });
        assert.ok(result?.contentSnippet?.includes('class Service'));
      });

      it('bounds snippet to 200 lines to avoid overflow on large selections', async () => {
        const longText = Array.from({ length: 500 }, (_, i) => `// Line ${i + 1}`).join('\n');
        const dummyUri = vscode.Uri.file('/workspace/src/large.ts');

        (vscode.window as any).activeTextEditor = {
          document: {
            uri: dummyUri,
            languageId: 'typescript',
            getText: () => longText,
          },
          selection: {
            isEmpty: false,
            start: { line: 0, character: 0 },
            end: { line: 499, character: 10 },
          },
        };

        const result = await manager.captureActiveEditorContext();
        assert.ok(result);
        const snippetLines = (result?.contentSnippet || '').split('\n');
        assert.strictEqual(snippetLines.length, MAX_SNIPPET_LINES);
        assert.strictEqual(snippetLines[0], '// Line 1');
        assert.strictEqual(snippetLines[199], '// Line 200');
      });
    });

    describe('searchWorkspaceFiles', () => {
      it('searches workspace files with glob query and returns relative paths', async () => {
        mockWorkspaceFindFilesResult.push(
          vscode.Uri.file('/workspace/src/composer/SlashCommandRegistry.ts'),
          vscode.Uri.file('/workspace/src/composer/ContextAttachmentManager.ts'),
          vscode.Uri.file('/workspace/src/bridge/types.ts')
        );

        const results = await manager.searchWorkspaceFiles('SlashCommand', 10);
        assert.strictEqual(results.length, 3);
        assert.strictEqual(results[0].label, 'SlashCommandRegistry.ts');
        assert.strictEqual(results[0].relativePath, 'src/composer/SlashCommandRegistry.ts');
      });

      it('respects maxResults bound when search returns many files', async () => {
        for (let i = 0; i < 20; i++) {
          mockWorkspaceFindFilesResult.push(
            vscode.Uri.file(`/workspace/src/file${i}.ts`)
          );
        }

        const results = await manager.searchWorkspaceFiles('file', 5);
        assert.strictEqual(results.length, 5);
      });
    });

    describe('captureDiagnostics', () => {
      it('extracts and formats errors and warnings with 1-based line numbers', () => {
        const testUri = vscode.Uri.file('/workspace/src/badCode.ts');

        mockDiagnosticsList.push(
          new (vscode as any).Diagnostic(
            new (vscode as any).Range(9, 4, 9, 20),
            'Cannot find name "unknownVar"',
            (vscode as any).DiagnosticSeverity.Error,
            'ts'
          ),
          new (vscode as any).Diagnostic(
            new (vscode as any).Range(14, 2, 14, 10),
            'Variable "unused" is declared but never read',
            (vscode as any).DiagnosticSeverity.Warning,
            'ts'
          ),
          new (vscode as any).Diagnostic(
            new (vscode as any).Range(0, 0, 0, 0),
            'Hint message should be ignored',
            (vscode as any).DiagnosticSeverity.Hint
          )
        );

        const formatted = manager.captureDiagnostics(testUri);
        assert.strictEqual(formatted.length, 2);
        assert.ok(formatted[0].includes('[Error] Line 10:5 - Cannot find name "unknownVar" [ts]'));
        assert.ok(formatted[1].includes('[Warning] Line 15:3 - Variable "unused" is declared but never read [ts]'));
      });

      it('returns empty array when document has no errors or warnings', () => {
        const testUri = vscode.Uri.file('/workspace/src/clean.ts');
        const formatted = manager.captureDiagnostics(testUri);
        assert.deepStrictEqual(formatted, []);
      });

      it('creates diagnostic attachment when errors exist', () => {
        const testUri = vscode.Uri.file('/workspace/src/diagnostics.ts');
        mockDiagnosticsList.push(
          new (vscode as any).Diagnostic(
            new (vscode as any).Range(2, 0, 2, 5),
            'Syntax error: unexpected token',
            (vscode as any).DiagnosticSeverity.Error
          )
        );

        const attachment = manager.createDiagnosticAttachment(testUri);
        assert.ok(attachment);
        assert.strictEqual(attachment?.type, 'diagnostic');
        assert.strictEqual(attachment?.relativePath, 'src/diagnostics.ts');
        assert.ok(attachment?.contentSnippet?.includes('Syntax error'));
      });

      it('returns null from createDiagnosticAttachment when no diagnostics exist', () => {
        const testUri = vscode.Uri.file('/workspace/src/clean.ts');
        const attachment = manager.createDiagnosticAttachment(testUri);
        assert.strictEqual(attachment, null);
      });
    });

    describe('Bounded Sizes & Constraints (20KB limit, 5 attachments limit)', () => {
      it('enforces 20KB max content per attachment via truncateToByteLimit', () => {
        // Create 30KB text
        const largeChunk = 'A'.repeat(30 * 1024);
        const truncated = truncateToByteLimit(largeChunk, MAX_ATTACHMENT_SIZE_BYTES);

        assert.ok(Buffer.byteLength(truncated, 'utf8') <= MAX_ATTACHMENT_SIZE_BYTES);
        assert.ok(truncated.includes('[truncated to 20KB limit]'));
      });

      it('keeps content unchanged when under 20KB', () => {
        const smallChunk = 'Small test snippet';
        const result = truncateToByteLimit(smallChunk, MAX_ATTACHMENT_SIZE_BYTES);
        assert.strictEqual(result, smallChunk);
      });

      it('boundContentSnippet enforces both line count (200 lines) and byte limit (20KB)', () => {
        const manyLines = Array.from({ length: 400 }, () => 'short line').join('\n');
        const bounded = boundContentSnippet(manyLines, MAX_SNIPPET_LINES, MAX_ATTACHMENT_SIZE_BYTES);
        const lines = bounded.split('\n');
        assert.strictEqual(lines.length, 200);
      });

      it('enforceAttachmentBounds caps attachments to 5 max per prompt', () => {
        const attachments: ContextAttachment[] = Array.from({ length: 8 }, (_, i) => ({
          type: 'file',
          uri: `file:///workspace/file${i}.ts`,
          relativePath: `file${i}.ts`,
          contentSnippet: `File content ${i}`,
        }));

        const bounded = manager.enforceAttachmentBounds(attachments);
        assert.strictEqual(bounded.length, MAX_ATTACHMENTS_PER_PROMPT);
        assert.strictEqual(bounded[0].relativePath, 'file0.ts');
        assert.strictEqual(bounded[4].relativePath, 'file4.ts');
      });

      it('enforceAttachmentBounds truncates each oversized attachment to 20KB', () => {
        const oversized: ContextAttachment[] = [
          {
            type: 'file',
            uri: 'file:///workspace/big.ts',
            relativePath: 'big.ts',
            contentSnippet: 'X'.repeat(25 * 1024),
          },
        ];

        const bounded = manager.enforceAttachmentBounds(oversized);
        assert.strictEqual(bounded.length, 1);
        assert.ok(
          Buffer.byteLength(bounded[0].contentSnippet || '', 'utf8') <=
            MAX_ATTACHMENT_SIZE_BYTES
        );
      });

      it('addAttachment appends and bounds to max 5 attachments and 20KB size', () => {
        let list: ContextAttachment[] = [];
        for (let i = 0; i < 7; i++) {
          list = manager.addAttachment(list, {
            type: 'file',
            uri: `file:///workspace/f${i}.ts`,
            relativePath: `f${i}.ts`,
            contentSnippet: `Content ${i}`,
          });
        }

        assert.strictEqual(list.length, 5);
        assert.strictEqual(list[0].relativePath, 'f0.ts');
        assert.strictEqual(list[4].relativePath, 'f4.ts');
      });
    });
  });
});
