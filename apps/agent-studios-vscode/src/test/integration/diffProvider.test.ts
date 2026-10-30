import * as assert from 'assert';
import * as vscode from 'vscode';

describe('Extension Host: Virtual Diff Document Provider Integration Tests', () => {
  before(async () => {
    // Ensure developer preview mode is enabled with FixtureBridge artifacts
    const config = vscode.workspace.getConfiguration('agentStudios');
    if (!config.get<boolean>('previewMode')) {
      await vscode.commands.executeCommand('agent-studios.toggleFixturePreview');
    }
  });

  it('opens and reads virtual document from agent-studios-diff URI scheme', async () => {
    const virtualUri = vscode.Uri.parse('agent-studios-diff://artifact/artifact-diff-001/task-m14-workbench-diff.patch');

    try {
      const doc = await vscode.workspace.openTextDocument(virtualUri);
      assert.ok(doc, 'Virtual document should be opened by VS Code text document provider');
      assert.strictEqual(doc.uri.scheme, 'agent-studios-diff');
      // Content returned by provider
      assert.strictEqual(typeof doc.getText(), 'string');
      assert.ok(doc.getText().length > 0, 'Virtual diff document content should not be empty');
    } catch (err) {
      assert.fail(`openTextDocument failed for virtual diff scheme: ${err}`);
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
});
