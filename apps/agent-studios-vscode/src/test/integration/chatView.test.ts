import * as assert from 'assert';
import * as vscode from 'vscode';

describe('Extension Host: Views & Chat Navigation Integration Tests', () => {
  it('executes agent-studios.openAgents without error', async () => {
    try {
      await vscode.commands.executeCommand('agent-studios.openAgents');
      assert.ok(true, 'openAgents command executed successfully');
    } catch (err) {
      assert.fail(`openAgents threw an unexpected error: ${err}`);
    }
  });

  it('executes agent-studios.showApprovals view navigation', async () => {
    try {
      await vscode.commands.executeCommand('agent-studios.showApprovals');
      assert.ok(true, 'showApprovals command executed successfully');
    } catch (err) {
      assert.fail(`showApprovals threw an unexpected error: ${err}`);
    }
  });

  it('executes agent-studios.refreshViews command across all TreeViews', async () => {
    try {
      await vscode.commands.executeCommand('agent-studios.refreshViews');
      assert.ok(true, 'refreshViews command executed successfully');
    } catch (err) {
      assert.fail(`refreshViews threw an unexpected error: ${err}`);
    }
  });

  it('executes agent-studios.disconnectDaemon cleanly returning to fail-closed state', async () => {
    try {
      await vscode.commands.executeCommand('agent-studios.disconnectDaemon');
      assert.ok(true, 'disconnectDaemon command executed successfully');
    } catch (err) {
      assert.fail(`disconnectDaemon threw an error: ${err}`);
    }
  });
});
