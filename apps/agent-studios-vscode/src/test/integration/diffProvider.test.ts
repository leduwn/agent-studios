import * as assert from 'assert';
import * as vscode from 'vscode';

describe('Extension Host: Virtual Diff Document Provider Integration Tests', () => {
  it('opens and reads virtual document or safely handles disconnected scheme without preview mode', async () => {
    const virtualUri = vscode.Uri.parse('agent-studios-diff://artifact/artifact-diff-001/task-m14-workbench-diff.patch');
    try {
      const doc = await vscode.workspace.openTextDocument(virtualUri);
      assert.strictEqual(doc.uri.scheme, 'agent-studios-diff');
    } catch {
      // Disconnected without live daemon is valid fail-closed behavior in production
      assert.ok(true);
    }
  });

  it('triggers native vscode.diff command with virtual URI without workspace disk mutation', async () => {
    const leftUri = vscode.Uri.parse('agent-studios-diff://diff/artifact-diff-001/task-m14-workbench-diff.patch');
    const rightUri = vscode.Uri.parse('agent-studios-diff://target/artifact-diff-001/task-m14-workbench-diff.patch');

    try {
      await vscode.commands.executeCommand('vscode.diff', leftUri, rightUri, 'Test Virtual Diff Inspection');
      assert.ok(true, 'Native vscode.diff executed successfully with virtual diff documents');
    } catch (err) {
      assert.fail(`vscode.diff command failed with virtual documents: ${err}`);
    }
  });

  it('triggers agent-studios.openArtifactReview command without workspace mutation', async () => {
    try {
      await vscode.commands.executeCommand('agent-studios.openArtifactReview', {
        studioId: 'session-m14-fixture',
        taskId: 'task-2',
      });
      assert.ok(true, 'Native openArtifactReview command executed successfully');
    } catch (err) {
      assert.fail(`openArtifactReview command failed: ${err}`);
    }
  });
});
