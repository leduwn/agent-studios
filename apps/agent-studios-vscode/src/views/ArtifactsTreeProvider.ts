/**
 * Agent Studios — Artifacts TreeDataProvider
 * Milestone M14-UI-00: Code-OSS Native Agents Workbench
 */

import * as vscode from 'vscode';
import { AgentStudiosBridge } from '../bridge/AgentStudiosBridge';
import { BridgeManager } from '../bridge/BridgeManager';
import { Artifact } from '../bridge/types';
import { SessionStateProjection } from '../state/SessionStateProjection';

export class ArtifactTreeItem extends vscode.TreeItem {
  public readonly artifact?: Artifact;

  constructor(
    artifactOrLabel: Artifact | string,
    collapsibleState: vscode.TreeItemCollapsibleState = vscode.TreeItemCollapsibleState.None
  ) {
    if (typeof artifactOrLabel === 'string') {
      super(artifactOrLabel, collapsibleState);
      return;
    }

    const artifact = artifactOrLabel;
    super(artifact.name, collapsibleState);
    this.artifact = artifact;
    this.id = artifact.id;

    // contextValue = 'artifactItem'
    this.contextValue = 'artifactItem';

    // Description: v${version} (${fileCount} files) - ${sha256.slice(0, 8)}...
    this.description = `v${artifact.version} (${artifact.fileCount} files) - ${artifact.sha256.slice(0, 8)}...`;

    // Tooltip: Full path, SHA-256 hash, creation timestamp
    const tooltipParts = [
      `Artifact: ${artifact.name}`,
      `Path: ${artifact.path}`,
      `SHA-256: ${artifact.sha256}`,
      `Version: v${artifact.version}`,
      `Files Changed: ${artifact.fileCount}`,
      `Created: ${new Date(artifact.createdAt).toISOString()}`,
      artifact.taskTitle
        ? `Task: ${artifact.taskTitle}`
        : artifact.taskId
          ? `Task ID: ${artifact.taskId}`
          : null,
    ].filter(Boolean);

    this.tooltip = tooltipParts.join('\n');

    // Icon: new vscode.ThemeIcon('diff')
    this.iconPath = new vscode.ThemeIcon('diff');

    // Command: executes agent-studios.inspectArtifact passing artifact ID
    this.command = {
      command: 'agent-studios.inspectArtifact',
      title: 'Inspect Artifact Diff',
      arguments: [artifact.id],
    };
  }
}

export class ArtifactsTreeProvider
  implements vscode.TreeDataProvider<ArtifactTreeItem>, vscode.Disposable
{
  private _projection?: SessionStateProjection;
  private readonly _disposables: vscode.Disposable[] = [];

  private readonly _onDidChangeTreeData = new vscode.EventEmitter<
    ArtifactTreeItem | undefined | void
  >();
  public readonly onDidChangeTreeData: vscode.Event<ArtifactTreeItem | undefined | void> =
    this._onDidChangeTreeData.event;

  constructor(
    bridgeOrManager: AgentStudiosBridge | BridgeManager | SessionStateProjection,
    projection?: SessionStateProjection
  ) {
    this._disposables.push(this._onDidChangeTreeData);

    if ('stateProjection' in bridgeOrManager) {
      const manager = bridgeOrManager as BridgeManager;
      this._projection = manager.stateProjection;
      this._disposables.push(
        manager.onBridgeChanged(() => {
          this._projection = manager.stateProjection;
          this.refresh();
        })
      );
    } else if ('handleEvent' in bridgeOrManager) {
      this._projection = bridgeOrManager as SessionStateProjection;
    } else if (projection) {
      this._projection = projection;
    }

    if (this._projection) {
      this._disposables.push(
        this._projection.onArtifactsChanged(() => this.refresh())
      );
    }
  }

  public refresh(): void {
    this._onDidChangeTreeData.fire();
  }

  public getTreeItem(element: ArtifactTreeItem): vscode.TreeItem {
    return element;
  }

  public getChildren(element?: ArtifactTreeItem): vscode.ProviderResult<ArtifactTreeItem[]> {
    if (element) {
      return [];
    }

    const artifacts = this._projection?.artifacts ?? [];

    if (artifacts.length === 0) {
      const placeholder = new ArtifactTreeItem(
        'No artifacts generated',
        vscode.TreeItemCollapsibleState.None
      );
      placeholder.iconPath = new vscode.ThemeIcon('diff');
      placeholder.description = 'Patches will appear here';
      placeholder.tooltip = 'No patch artifacts or deliverables generated for current session.';
      placeholder.contextValue = 'noArtifacts';
      return [placeholder];
    }

    return artifacts.map((artifact) => new ArtifactTreeItem(artifact));
  }

  public dispose(): void {
    while (this._disposables.length > 0) {
      const d = this._disposables.pop();
      d?.dispose();
    }
  }
}
