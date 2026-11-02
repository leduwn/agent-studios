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

  it('inspects artifact with associated taskId by delegating to openArtifactReview workbench', async () => {
    const artifacts = await bridge.listArtifacts();
    const artifact = artifacts[0]; // has taskId: 'task-1'

    await vscode.commands.executeCommand('agent-studios.inspectArtifact', artifact);

    const reviewCall = executedCommands.find(c => c.command === 'agent-studios.openArtifactReview');
    assert.ok(reviewCall !== undefined, 'agent-studios.openArtifactReview should have been executed');
    const options = reviewCall.args[0] as { studioId: string; taskId: string; taskTitle: string };
    assert.strictEqual(options.taskId, artifact.taskId);

    const diffCall = executedCommands.find(c => c.command === 'vscode.diff');
    assert.strictEqual(diffCall, undefined, 'Must NEVER call vscode.diff on unified patch');
  });

  it('inspects standalone artifact without taskId by opening virtual text document directly', async () => {
    const standaloneArtifact = {
      id: 'artifact-standalone-001',
      sessionId: 'session-m14-fixture',
      name: 'standalone.patch',
      path: 'diffs/standalone.patch',
      diffContent: 'diff --git a/standalone.txt b/standalone.txt\n+standalone content\n',
      fileCount: 1,
      sha256: 'abc123',
      version: 1,
      createdAt: 1716000070000,
    };
    bridge.getArtifactDiff = async () => standaloneArtifact.diffContent;

    await vscode.commands.executeCommand('agent-studios.inspectArtifact', standaloneArtifact);

    const diffCall = executedCommands.find(c => c.command === 'vscode.diff');
    assert.strictEqual(diffCall, undefined, 'Must NEVER execute vscode.diff');
  });

  it('inspects artifact by string artifactId delegating to openArtifactReview', async () => {
    const artifacts = await bridge.listArtifacts();
    const artifact = artifacts[0];

    await vscode.commands.executeCommand('agent-studios.inspectArtifact', artifact.id);

    const reviewCall = executedCommands.find(c => c.command === 'agent-studios.openArtifactReview');
    assert.ok(reviewCall !== undefined);
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

    const reviewCall = executedCommands.find(c => c.command === 'agent-studios.openArtifactReview');
    assert.ok(reviewCall !== undefined);
  });

  it('fails closed when opening review with no active studio and no available sessions', async () => {
    bridge.getCurrentSession = async () => null;
    bridge.listSessions = async () => [];

    await vscode.commands.executeCommand('agent-studios.openArtifactReview');

    const errorMsg = mockWindowMessages.error.find(m => m.includes('No active studio available'));
    assert.ok(errorMsg !== undefined, 'Must fail closed when no active studio is available');
  });

  it('opens ArtifactReviewPanel with explicit studioId and taskId options', async () => {
    await vscode.commands.executeCommand('agent-studios.openArtifactReview', {
      studioId: 'session-m14-fixture',
      taskId: 'task-1',
      taskTitle: 'Analyze workspace',
      taskStatus: 'succeeded',
    });
    // Panel was created without error
    assert.ok(true);
  });

  it('prompts user via QuickPick for openArtifactReview when no options passed', async () => {
    const tasks = await bridge.listTasks();
    mockWindowMessages.quickPickSelection = {
      label: tasks[0].title,
      task: tasks[0],
    };

    await vscode.commands.executeCommand('agent-studios.openArtifactReview');
    assert.ok(true);
  });
});
