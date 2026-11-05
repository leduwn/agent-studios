import * as assert from 'assert';
import * as vscode from 'vscode';
import {
  DIFF_SCHEME,
  DiffDocumentProvider,
  applyPatch,
  extractAffectedPaths,
} from '../../bridge/DiffDocumentProvider';
import { FixtureBridge } from '../../bridge/FixtureBridge';
import { TimelineEvent } from '../../bridge/types';

describe('DiffDocumentProvider Tests', () => {
  describe('applyPatch utility', () => {
    it('applies added lines for new files (--- /dev/null)', () => {
      const diff = [
        '--- /dev/null',
        '+++ b/src/example.ts',
        '@@ -0,0 +1,3 @@',
        '+const a = 1;',
        '+const b = 2;',
        '+export default a + b;',
      ].join('\n');

      const result = applyPatch('', diff);
      assert.strictEqual(result, 'const a = 1;\nconst b = 2;\nexport default a + b;');
    });

    it('applies modification hunks to original text in memory', () => {
      const original = ['line 1', 'line 2', 'line 3'].join('\n');
      const diff = [
        '--- a/test.txt',
        '+++ b/test.txt',
        '@@ -2,1 +2,2 @@',
        '-line 2',
        '+line 2 modified',
        '+line 2.5 added',
      ].join('\n');

      const result = applyPatch(original, diff);
      assert.strictEqual(result, 'line 1\nline 2 modified\nline 2.5 added\nline 3');
    });

    it('safely returns diff text when not a unified patch', () => {
      const plainText = 'Just some non-diff content';
      const result = applyPatch('original', plainText);
      assert.strictEqual(result, plainText);
    });
  });

  describe('extractAffectedPaths utility', () => {
    it('extracts +++ b/ target paths from unified diff headers', () => {
      const diff = [
        '--- a/src/old.ts',
        '+++ b/src/new.ts',
        '@@ -1,1 +1,1 @@',
        '+// change',
      ].join('\n');

      const paths = extractAffectedPaths(diff);
      assert.deepStrictEqual(paths, ['src/new.ts', 'src/old.ts']);
    });

    it('uses fallback path when diff has no valid paths', () => {
      const paths = extractAffectedPaths('no diff', 'src/fallback.ts');
      assert.deepStrictEqual(paths, ['src/fallback.ts']);
    });
  });

  describe('DiffDocumentProvider class', () => {
    let bridge: FixtureBridge;
    let provider: DiffDocumentProvider;

    beforeEach(() => {
      bridge = new FixtureBridge();
      provider = new DiffDocumentProvider(bridge);
    });

    afterEach(() => {
      provider.dispose();
      bridge.dispose();
    });

    it('has scheme agent-studios-diff', () => {
      assert.strictEqual(DIFF_SCHEME, 'agent-studios-diff');
    });

    it('serves virtual document content set explicitly in-memory', async () => {
      const uri = vscode.Uri.parse('agent-studios-diff://custom/test-doc');
      provider.setDocumentContent(uri, 'virtual custom content');

      const content = await provider.provideTextDocumentContent(uri);
      assert.strictEqual(content, 'virtual custom content');
    });

    it('looks up diff content from bridge artifacts', async () => {
      const artifacts = await bridge.listArtifacts();
      assert.ok(artifacts.length > 0);
      const artifact = artifacts[0];

      const uri = vscode.Uri.parse(
        `agent-studios-diff://artifact/${artifact.id}/${artifact.name}?view=diff`
      );
      const content = await provider.provideTextDocumentContent(uri);
      assert.ok(content.includes('package.json'));
      assert.strictEqual(content, artifact.diffContent);
    });

    it('returns empty string for non-existent artifact', async () => {
      const uri = vscode.Uri.parse('agent-studios-diff://artifact/non-existent/test.patch');
      const content = await provider.provideTextDocumentContent(uri);
      assert.strictEqual(content, '');
    });

    it('fires onDidChange when setDocumentContent is called', () => {
      const uri = vscode.Uri.parse('agent-studios-diff://artifact/art-1/test.patch');
      let firedUri: vscode.Uri | undefined;
      const sub = provider.onDidChange((e) => {
        firedUri = e;
      });

      provider.setDocumentContent(uri, 'new content');
      assert.ok(firedUri !== undefined);
      assert.strictEqual(firedUri?.toString(), uri.toString());
      sub.dispose();
    });

    it('fires onDidChange when bridge artifact_created event occurs', () => {
      let firedCount = 0;
      const sub = provider.onDidChange(() => {
        firedCount++;
      });

      const event: TimelineEvent = {
        sequenceNumber: 20,
        id: 'evt-artifact-created',
        timestamp: Date.now(),
        type: 'artifact_created',
        summary: 'Artifact created',
        details: {
          artifact: {
            id: 'art-new-99',
            sessionId: 'session-1',
            name: 'patch-99.diff',
            path: 'patch-99.diff',
            diffContent: '+++ b/test.txt\n@@ -0,0 +1,1 @@\n+test',
            fileCount: 1,
            sha256: 'abc',
            version: 1,
            createdAt: Date.now(),
          },
        },
      };

      (bridge as unknown as { _onEvent: vscode.EventEmitter<TimelineEvent> })._onEvent.fire(event);

      assert.ok(firedCount >= 1);
      sub.dispose();
    });
  });
});
