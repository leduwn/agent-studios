import * as assert from 'assert';
import * as vscode from 'vscode';
import { FixtureBridge } from '../../bridge/FixtureBridge';
import { registerArtifactCommands } from '../../commands/ArtifactCommands';
import {
  executedCommands,
  mockWindowMessages,
  resetMockState,
} from './vscodeMock';

describe('ArtifactCommands Tests', () => {
  let bridge: FixtureBridge;
  let context: vscode.ExtensionContext;
  let disposable: vscode.Disposable;

  beforeEach(() => {
    resetMockState();
    bridge = new FixtureBridge();
    const subscriptions: vscode.Disposable[] = [];
    context = {
      subscriptions,
    } as unknown as vscode.ExtensionContext;

    disposable = registerArtifactCommands(context, bridge);
  });

  afterEach(() => {
    disposable.dispose();
    bridge.dispose();
    resetMockState();
  });

  it('registers agent-studios.inspectArtifact command and pushes to context.subscriptions', () => {
    assert.strictEqual(context.subscriptions.length, 1);
  });

  it('inspects artifact by direct Artifact object argument and executes vscode.diff when file found', async () => {
    const artifacts = await bridge.listArtifacts();
    const artifact = artifacts[0];

    // Mock a workspace folder containing the target file
    const mockFolderUri = vscode.Uri.file('d:/workspace');
    (vscode.workspace as unknown as { workspaceFolders: Array<{ uri: vscode.Uri; name: string }> }).workspaceFolders = [
      { uri: mockFolderUri, name: 'workspace' },
    ];

    await vscode.commands.executeCommand('agent-studios.inspectArtifact', artifact);

    const diffCall = executedCommands.find(c => c.command === 'vscode.diff');
    assert.ok(diffCall !== undefined, 'vscode.diff should have been executed');
    const [originalUri, virtualUri, title] = diffCall.args as [vscode.Uri, vscode.Uri, string];
    assert.ok(originalUri !== undefined);
    assert.ok(virtualUri.toString().startsWith('agent-studios-diff://artifact/'));
    assert.ok(virtualUri.toString().includes(artifact.id));
    assert.ok(typeof title === 'string' && title.includes(artifact.name));
  });

  it('inspects artifact by string artifactId', async () => {
    const artifacts = await bridge.listArtifacts();
    const artifact = artifacts[0];

    await vscode.commands.executeCommand('agent-studios.inspectArtifact', artifact.id);

    const diffCall = executedCommands.find(c => c.command === 'vscode.diff');
    assert.ok(diffCall !== undefined);
  });

  it('shows error message when artifact id is not found', async () => {
    await vscode.commands.executeCommand('agent-studios.inspectArtifact', 'non-existent-artifact');

    assert.ok(mockWindowMessages.error.length > 0);
    assert.ok(mockWindowMessages.error[0].includes('non-existent-artifact'));
  });

  it('prompts user via QuickPick when invoked with no arguments', async () => {
    const artifacts = await bridge.listArtifacts();
    mockWindowMessages.quickPickSelection = {
      label: artifacts[1].name,
      artifact: artifacts[1],
    };

    await vscode.commands.executeCommand('agent-studios.inspectArtifact');

    const diffCall = executedCommands.find(c => c.command === 'vscode.diff');
    assert.ok(diffCall !== undefined);
  });
});
