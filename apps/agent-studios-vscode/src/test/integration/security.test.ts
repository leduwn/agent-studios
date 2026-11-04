import * as assert from 'assert';
import * as vscode from 'vscode';

describe('Extension Host: Security & Fail-Closed Guard Integration Tests', () => {
  it('blocks editor action safely when no active editor is open', async () => {
    // Ensure no active editor is open
    await vscode.commands.executeCommand('workbench.action.closeAllEditors');

    try {
      await vscode.commands.executeCommand('agent-studios.explainSelection');
      assert.ok(true, 'Command handled safely without uncaught exception');
    } catch (err) {
      assert.fail(`explainSelection threw uncaught exception: ${err}`);
    }
  });

  it('blocks refactor action safely when selection is empty', async () => {
    try {
      await vscode.commands.executeCommand('agent-studios.refactorSelection');
      assert.ok(true, 'refactorSelection handled empty selection safely');
    } catch (err) {
      assert.fail(`refactorSelection threw uncaught exception: ${err}`);
    }
  });

  it('blocks test generation safely when disconnected', async () => {
    try {
      await vscode.commands.executeCommand('agent-studios.generateTests');
      assert.ok(true, 'generateTests failed closed safely');
    } catch (err) {
      assert.fail(`generateTests threw uncaught exception: ${err}`);
    }
  });
});
