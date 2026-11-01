/**
 * Agent Studios — DiffParser Unit Tests
 * Milestone M15 UI-03: Artifact & Diff Review Workbench
 */

import * as assert from 'assert';
import { DiffParser } from '../../cards/DiffParser';

describe('DiffParser Unit Tests', () => {
  it('1. parses empty diff string into empty parsed diff', () => {
    const parsed = DiffParser.parseUnifiedDiff('');
    assert.strictEqual(parsed.files.length, 0);
    assert.strictEqual(parsed.totalFiles, 0);
    assert.strictEqual(parsed.totalAdditions, 0);
    assert.strictEqual(parsed.totalDeletions, 0);
  });

  it('2. parses added file diff with hunks, additions and line counts', () => {
    const raw = `diff --git a/new-file.txt b/new-file.txt
new file mode 100644
index 0000000..d00491f
--- /dev/null
+++ b/new-file.txt
@@ -0,0 +1,3 @@
+line 1
+line 2
+line 3
`;
    const parsed = DiffParser.parseUnifiedDiff(raw);
    assert.strictEqual(parsed.files.length, 1);
    assert.strictEqual(parsed.files[0].displayPath, 'new-file.txt');
    assert.strictEqual(parsed.files[0].changeKind, 'added');
    assert.strictEqual(parsed.files[0].isBinary, false);
    assert.strictEqual(parsed.files[0].additions, 3);
    assert.strictEqual(parsed.files[0].deletions, 0);
    assert.strictEqual(parsed.files[0].hunks.length, 1);
    assert.strictEqual(parsed.totalAdditions, 3);
    assert.strictEqual(parsed.totalDeletions, 0);
  });

  it('3. parses modified file with context, additions, deletions', () => {
    const raw = `diff --git a/src/app.ts b/src/app.ts
index 1111111..2222222 100644
--- a/src/app.ts
+++ b/src/app.ts
@@ -10,4 +10,4 @@ function main() {
   const x = 1;
-  const y = 2;
+  const y = 3;
   return x + y;
 }
`;
    const parsed = DiffParser.parseUnifiedDiff(raw);
    assert.strictEqual(parsed.files.length, 1);
    assert.strictEqual(parsed.files[0].displayPath, 'src/app.ts');
    assert.strictEqual(parsed.files[0].changeKind, 'modified');
    assert.strictEqual(parsed.files[0].additions, 1);
    assert.strictEqual(parsed.files[0].deletions, 1);
    assert.strictEqual(parsed.files[0].hunks[0].lines.length >= 5, true);
  });

  it('4. detects deleted, renamed, and binary files accurately', () => {
    const raw = `diff --git a/deleted.txt b/deleted.txt
deleted file mode 100644
index 1234567..0000000
--- a/deleted.txt
+++ /dev/null
@@ -1,1 +0,0 @@
-goodbye
diff --git a/old-name.txt b/new-name.txt
similarity index 100%
rename from old-name.txt
rename to new-name.txt
diff --git a/logo.png b/logo.png
new file mode 100644
index 0000000..abcdef1
GIT binary patch
literal 16
zc$@)Z0000100001
`;
    const parsed = DiffParser.parseUnifiedDiff(raw);
    assert.strictEqual(parsed.files.length, 3);

    assert.strictEqual(parsed.files[0].displayPath, 'deleted.txt');
    assert.strictEqual(parsed.files[0].changeKind, 'deleted');

    assert.strictEqual(parsed.files[1].displayPath, 'new-name.txt');
    assert.strictEqual(parsed.files[1].changeKind, 'renamed');

    assert.strictEqual(parsed.files[2].displayPath, 'logo.png');
    assert.strictEqual(parsed.files[2].changeKind, 'binary');
    assert.strictEqual(parsed.files[2].isBinary, true);
  });
});
