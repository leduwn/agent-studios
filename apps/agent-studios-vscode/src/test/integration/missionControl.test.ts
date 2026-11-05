import * as assert from 'assert';
import * as vscode from 'vscode';
import { MissionControlPanel } from '../../mission-control/MissionControlPanel';

describe('Extension Host: Mission Control Integration Tests', () => {
  const extensionId = 'agent-studios.agent-studios-vscode';

  before(async () => {
    const ext = vscode.extensions.getExtension(extensionId);
    assert.ok(ext, `Extension ${extensionId} must be present`);
    if (!ext.isActive) {
      await ext.activate();
    }
  });

  afterEach(() => {
    if (MissionControlPanel.currentPanel) {
      MissionControlPanel.currentPanel.dispose();
    }
  });

  it('registers agent-studios.openMissionControl in VS Code command registry', async () => {
    const commands = await vscode.commands.getCommands(true);
    assert.ok(
      commands.includes('agent-studios.openMissionControl'),
      'agent-studios.openMissionControl command must be registered'
    );
  });

  it('executes agent-studios.openMissionControl and creates MissionControlPanel singleton', async () => {
    assert.strictEqual(MissionControlPanel.currentPanel, undefined);

    await vscode.commands.executeCommand('agent-studios.openMissionControl');

    assert.ok(MissionControlPanel.currentPanel, 'MissionControlPanel singleton must be created');
    assert.strictEqual(
      MissionControlPanel.viewType,
      'agent-studios.missionControl',
      'viewType must match agent-studios.missionControl'
    );
  });

  it('reveals existing MissionControlPanel on subsequent open commands', async () => {
    await vscode.commands.executeCommand('agent-studios.openMissionControl');
    const firstInstance = MissionControlPanel.currentPanel;
    assert.ok(firstInstance);

    // Call openMissionControl again
    await vscode.commands.executeCommand('agent-studios.openMissionControl');
    const secondInstance = MissionControlPanel.currentPanel;

    assert.strictEqual(
      firstInstance,
      secondInstance,
      'Subsequent openMissionControl command must reuse existing panel singleton'
    );
  });

  it('resets MissionControlPanel singleton on disposal', async () => {
    await vscode.commands.executeCommand('agent-studios.openMissionControl');
    assert.ok(MissionControlPanel.currentPanel);

    MissionControlPanel.currentPanel.dispose();
    assert.strictEqual(
      MissionControlPanel.currentPanel,
      undefined,
      'MissionControlPanel.currentPanel must be undefined after dispose()'
    );
  });
});
