/**
 * Agent Studios — Mission Control Commands
 * Milestone M14 Mission Control Workbench
 *
 * Registers commands:
 * - agent-studios.openMissionControl: opens or reveals Mission Control WebviewPanel
 */

import * as vscode from 'vscode';
import { DiffReviewManager } from '../cards/DiffReviewManager';
import { MissionControlPanel } from './MissionControlPanel';
import { MissionControlProjection } from './state/MissionControlProjection';

export function registerMissionControlCommands(
  context: vscode.ExtensionContext,
  projection: MissionControlProjection,
  diffReviewManager?: DiffReviewManager
): void {
  // Command to open Mission Control WebviewPanel
  context.subscriptions.push(
    vscode.commands.registerCommand('agent-studios.openMissionControl', () => {
      MissionControlPanel.createOrShow(
        context.extensionUri,
        projection,
        diffReviewManager
      );
    })
  );

  // Register WebviewPanel serializer for restoration across VS Code restarts
  context.subscriptions.push(
    vscode.window.registerWebviewPanelSerializer(MissionControlPanel.viewType, {
      async deserializeWebviewPanel(
        webviewPanel: vscode.WebviewPanel,
        _state: unknown
      ): Promise<void> {
        MissionControlPanel.revive(
          webviewPanel,
          context.extensionUri,
          projection,
          diffReviewManager
        );
      },
    })
  );
}
