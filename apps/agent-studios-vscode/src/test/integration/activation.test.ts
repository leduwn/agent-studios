import * as assert from 'assert';
import * as vscode from 'vscode';

describe('Extension Host: Activation & Commands Integration Tests', () => {
  const extensionId = 'agent-studios.agent-studios-vscode';

  it('verifies extension is present in VS Code extensions catalog', () => {
    const ext = vscode.extensions.getExtension(extensionId);
    assert.ok(ext, `Extension ${extensionId} should be found in vscode.extensions`);
  });

  it('activates extension cleanly in real extension host', async () => {
    const ext = vscode.extensions.getExtension(extensionId);
    assert.ok(ext);
    if (!ext.isActive) {
      await ext.activate();
    }
    assert.strictEqual(ext.isActive, true, 'Extension should be active after calling activate()');
  });

  it('registers all workbench and editor commands in vscode.commands', async () => {
    const commands = await vscode.commands.getCommands(true);

    const requiredCommands = [
      'agent-studios.openAgents',
      'agent-studios.openSession',
      'agent-studios.showTaskGraph',
      'agent-studios.showApprovals',
      'agent-studios.inspectArtifact',
      'agent-studios.approveAction',
      'agent-studios.denyAction',
      'agent-studios.connectDaemon',
      'agent-studios.disconnectDaemon',
      'agent-studios.toggleFixturePreview',
      'agent-studios.refreshViews',
      'agent-studios.explainSelection',
      'agent-studios.refactorSelection',
      'agent-studios.generateTests',
      'agent-studios.openMissionControl',
    ];

    for (const cmd of requiredCommands) {
      assert.ok(
        commands.includes(cmd),
        `Command ${cmd} should be registered in real VS Code commands registry`
      );
    }
  });

  it('verifies default extension settings schema in vscode.workspace', async () => {
    // Reset any persisted previewMode setting to default
    const resetConfig = vscode.workspace.getConfiguration('agentStudios');
    await resetConfig.update('previewMode', undefined, vscode.ConfigurationTarget.Global);

    const config = vscode.workspace.getConfiguration('agentStudios');
    assert.strictEqual(config.get('daemonEndpoint'), 'ipc://agent-studios.sock');
    assert.strictEqual(config.get('previewMode'), false);
    assert.strictEqual(config.get('logLevel'), 'info');
  });

  it('executes agent-studios.toggleFixturePreview command and toggles preview mode', async () => {
    const initialConfig = vscode.workspace.getConfiguration('agentStudios');
    const initialMode = initialConfig.get<boolean>('previewMode', false);

    // Execute toggle command
    await vscode.commands.executeCommand('agent-studios.toggleFixturePreview');

    const updatedConfig = vscode.workspace.getConfiguration('agentStudios');
    const toggledMode = updatedConfig.get<boolean>('previewMode');
    assert.strictEqual(toggledMode, !initialMode, 'Preview mode setting should be inverted');

    // Revert to initial state
    await vscode.commands.executeCommand('agent-studios.toggleFixturePreview');
  });
});
