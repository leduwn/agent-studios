/**
 * Agent Studios — Artifact Commands
 * Milestone M14-UI-00: Code-OSS Native Agents Workbench
 */

import * as path from 'path';
import * as vscode from 'vscode';
import { AgentStudiosBridge } from '../bridge/AgentStudiosBridge';
import {
  DIFF_SCHEME,
  DiffDocumentProvider,
  findWorkspaceFileForArtifact,
} from '../bridge/DiffDocumentProvider';
import { Artifact } from '../bridge/types';

/**
 * Registers commands for inspecting and visualizing Agent Studios artifacts.
 */
export function registerArtifactCommands(
  context: vscode.ExtensionContext,
  bridge: AgentStudiosBridge
): vscode.Disposable {
  const disposables: vscode.Disposable[] = [];

  // Register the DiffDocumentProvider for the agent-studios-diff scheme
  const diffProvider = new DiffDocumentProvider(bridge);
  disposables.push(
    vscode.workspace.registerTextDocumentContentProvider(DIFF_SCHEME, diffProvider),
    diffProvider
  );

  const inspectCommand = vscode.commands.registerCommand(
    'agent-studios.inspectArtifact',
    async (arg?: unknown) => {
      let artifact: Artifact | undefined;

      // 1. Resolve artifact from argument or QuickPick
      if (typeof arg === 'string') {
        const artifacts = await bridge.listArtifacts();
        artifact = artifacts.find(a => a.id === arg || a.name === arg);
        if (!artifact) {
          vscode.window.showErrorMessage(`Artifact '${arg}' not found.`);
          return;
        }
      } else if (arg && typeof arg === 'object') {
        if ('id' in arg && 'name' in arg) {
          artifact = arg as Artifact;
        } else if ('artifact' in arg && typeof (arg as { artifact: unknown }).artifact === 'object') {
          artifact = (arg as { artifact: Artifact }).artifact;
        } else if ('id' in arg && typeof (arg as { id: unknown }).id === 'string') {
          const artifactId = (arg as { id: string }).id;
          const artifacts = await bridge.listArtifacts();
          artifact = artifacts.find(a => a.id === artifactId || a.name === artifactId);
          if (!artifact) {
            vscode.window.showErrorMessage(`Artifact '${artifactId}' not found.`);
            return;
          }
        }
      }

      if (!artifact) {
        const artifacts = await bridge.listArtifacts();
        if (artifacts.length === 0) {
          vscode.window.showInformationMessage('No artifacts available to inspect.');
          return;
        }

        const pick = await vscode.window.showQuickPick(
          artifacts.map(a => ({
            label: a.name,
            description: a.path || a.id,
            detail: a.taskTitle ? `Task: ${a.taskTitle} (v${a.version})` : `v${a.version}`,
            artifact: a,
          })),
          {
            placeHolder: 'Select an artifact to inspect diff',
          }
        );

        if (!pick) {
          return;
        }
        artifact = pick.artifact;
      }

      // 2. Retrieve diff content from bridge
      let diffContent = await bridge.getArtifactDiff(artifact.id);
      if (!diffContent && artifact.diffContent) {
        diffContent = artifact.diffContent;
      }

      if (!diffContent) {
        vscode.window.showInformationMessage(`No diff content available for artifact '${artifact.name}'.`);
        return;
      }

      // 3. Create virtual document URI
      const virtualUri = vscode.Uri.parse(
        'agent-studios-diff://artifact/' + artifact.id + '/' + artifact.name
      );

      // 4. Check if the artifact affects an existing file in the workspace
      const originalUri = await findWorkspaceFileForArtifact(artifact, diffContent);

      if (originalUri) {
        const fileName = path.basename(originalUri.fsPath);
        const title = `${artifact.name} (${fileName}: Working Tree ↔ Proposed Diff)`;
        await vscode.commands.executeCommand('vscode.diff', originalUri, virtualUri, title);
      } else {
        // Standalone diff: opens text document with virtual URI
        const doc = await vscode.workspace.openTextDocument(virtualUri);
        await vscode.window.showTextDocument(doc, { preview: true });
      }
    }
  );

  disposables.push(inspectCommand);

  const compositeDisposable = vscode.Disposable.from(...disposables);
  context.subscriptions.push(compositeDisposable);
  return compositeDisposable;
}

// eslint-disable-next-line @typescript-eslint/naming-convention
export const ArtifactCommands = {
  register: registerArtifactCommands,
};
