/**
 * Agent Studios — Real Extension Host Mode Continuity Integration Tests
 * Milestone M15 UI-06: Extension Development Host Verification
 *
 * Verifies actual command registration and execution inside real VS Code Extension Host
 * without mock overrides.
 */

import * as assert from 'assert';
import * as vscode from 'vscode';

describe('Extension Host: Mode Continuity & Navigation Integration Tests (M15 UI-06)', () => {
  const extensionId = 'agent-studios.agent-studios-vscode';

  before(async () => {
    const ext = vscode.extensions.getExtension(extensionId);
    assert.ok(ext, `Extension ${extensionId} must be present`);
    if (!ext.isActive) {
      await ext.activate();
    }
    assert.strictEqual(ext.isActive, true, 'Extension must be active');
  });

  it('registers openInIde in real VS Code commands registry', async () => {
    const commands = await vscode.commands.getCommands(true);
    assert.ok(
      commands.includes('agent-studios.openInIde'),
      'agent-studios.openInIde must be registered in real VS Code commands registry'
    );
  });

  it('registers switchToAgentMode in real VS Code commands registry', async () => {
    const commands = await vscode.commands.getCommands(true);
    assert.ok(
      commands.includes('agent-studios.switchToAgentMode'),
      'agent-studios.switchToAgentMode must be registered in real VS Code commands registry'
    );
  });

  it('registers continueSession in real VS Code commands registry', async () => {
    const commands = await vscode.commands.getCommands(true);
    assert.ok(
      commands.includes('agent-studios.continueSession'),
      'agent-studios.continueSession must be registered in real VS Code commands registry'
    );
  });

  it('executes agent-studios.openInIde in real Extension Host without error', async () => {
    try {
      await vscode.commands.executeCommand('agent-studios.openInIde');
      assert.ok(true, 'openInIde executed successfully');
    } catch (err: unknown) {
      assert.fail(`openInIde threw an unexpected error: ${err}`);
    }
  });

  it('executes agent-studios.switchToAgentMode in real Extension Host without error', async () => {
    try {
      await vscode.commands.executeCommand('agent-studios.switchToAgentMode');
      assert.ok(true, 'switchToAgentMode executed successfully');
    } catch (err: unknown) {
      assert.fail(`switchToAgentMode threw an unexpected error: ${err}`);
    }
  });

  it('executes agent-studios.continueSession without crashing', async () => {
    try {
      const result = await vscode.commands.executeCommand('agent-studios.continueSession');
      // When executed, restores session object or safely returns null without crashing
      assert.ok(
        result === null || (result !== null && typeof result === 'object'),
        'continueSession must return restored session or null'
      );
    } catch (err: unknown) {
      assert.fail(`continueSession threw an unexpected error: ${err}`);
    }
  });
});
