/**
 * Agent Studios — Artifact Commands
 * Milestone M14-UI-00: Code-OSS Native Agents Workbench
 */

import * as vscode from 'vscode';
import { AgentStudiosBridge } from '../bridge/AgentStudiosBridge';
import {
  DIFF_SCHEME,
  DiffDocumentProvider,
} from '../bridge/DiffDocumentProvider';
import { Artifact } from '../bridge/types';
import { ArtifactReviewPanel, ArtifactReviewOptions } from '../webviews/ArtifactReviewPanel';

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

      // 4. If the artifact has an associated task, open the unified patch review workbench
      if (artifact.taskId) {
        const session = await bridge.getCurrentSession();
        const studioId = session?.id || artifact.sessionId;
        if (studioId) {
          await vscode.commands.executeCommand('agent-studios.openArtifactReview', {
            studioId,
            taskId: artifact.taskId,
            taskTitle: artifact.taskTitle || artifact.name,
          });
          return;
        }
      }

      // 5. Standalone virtual patch viewer:
      // Open patch document directly for review (never mislabel unified patch text as a proposed source file in vscode.diff)
      const doc = await vscode.workspace.openTextDocument(virtualUri);
      await vscode.window.showTextDocument(doc, { preview: true });
    }
  );

  const reviewCommand = vscode.commands.registerCommand(
    'agent-studios.openArtifactReview',
    async (args?: unknown) => {
      let options: ArtifactReviewOptions;

      if (args && typeof args === 'object' && 'studioId' in args && 'taskId' in args) {
        options = args as ArtifactReviewOptions;
      } else {
        const session = await bridge.getCurrentSession();
        let studioId = session?.id;

        if (!studioId) {
          const sessions = await bridge.listSessions();
          if (sessions.length === 1) {
            studioId = sessions[0].id;
          } else if (sessions.length > 1) {
            const pick = await vscode.window.showQuickPick(
              sessions.map((s) => ({
                label: s.title || `Session ${s.id}`,
                description: `Status: ${s.status} | ID: ${s.id}`,
                session: s,
              })),
              { placeHolder: 'Select a studio/session for artifact & diff review' }
            );
            if (!pick) {
              return;
            }
            studioId = pick.session.id;
          }
        }

        if (!studioId) {
          vscode.window.showErrorMessage(
            'No active studio available. Please connect to a studio before reviewing artifacts.'
          );
          return;
        }

        const tasks = await bridge.listTasks();
        if (tasks.length === 0) {
          vscode.window.showInformationMessage('No tasks available to review.');
          return;
        }

        const items = tasks.map((t) => ({
          label: t.title,
          description: `Status: ${t.status} | ID: ${t.id}`,
          task: t,
        }));

        const pick = await vscode.window.showQuickPick(items, {
          placeHolder: 'Select a task to review artifacts and diff',
        });
        if (!pick) {
          return;
        }

        options = {
          studioId,
          taskId: pick.task.id,
          taskTitle: pick.task.title,
          taskStatus: pick.task.status,
        };
      }

      const loadDiffFn = bridge.getTaskDiff
        ? () => bridge.getTaskDiff!(options.studioId, options.taskId)
        : undefined;

      ArtifactReviewPanel.createOrShow(context.extensionUri, options, loadDiffFn);
    }
  );

  disposables.push(inspectCommand, reviewCommand);

  const compositeDisposable = vscode.Disposable.from(...disposables);
  context.subscriptions.push(compositeDisposable);
  return compositeDisposable;
}

// eslint-disable-next-line @typescript-eslint/naming-convention
export const ArtifactCommands = {
  register: registerArtifactCommands,
};
