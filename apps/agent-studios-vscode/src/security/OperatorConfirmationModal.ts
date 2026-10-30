/**
 * Agent Studios — Operator Confirmation Native Modal Dialog
 * Milestone M15 UI-01: Interactive Agent Execution Workbench
 *
 * Implements trusted, host-owned operator confirmation flow using native VS Code modal dialogs:
 * `vscode.window.showWarningMessage` with `{ modal: true }`.
 *
 * Security & Architectural Invariants:
 * 1. Webview-rendered confirmation is strictly forbidden as untrusted.
 * 2. Extension Host owns challenge token generation and cryptographic binding.
 * 3. Human-readable summary displays all bound execution parameters.
 * 4. Tokens are atomically bound to studioId, prompt hash, workspace, model, provider, and intent.
 * 5. Cancellation purges pending tokens to prevent lingering valid challenges.
 * 6. Test injection is strictly forbidden in production mode (NODE_ENV === 'production').
 */

import * as vscode from 'vscode';
import {
  OperatorConfirmationChallenge,
  OperatorConfirmationManager,
} from '../bridge/ipc/CapabilityMatrix';

export interface OperatorConfirmationDialogRequest {
  studioId: string;
  studioName?: string;
  runtime: string;
  providerId?: string;
  modelId?: string;
  workspacePath?: string;
  prompt: string;
  actionDescription?: string;
}

export interface OperatorConfirmationDialogResult {
  confirmed: boolean;
  challenge?: OperatorConfirmationChallenge;
  cancelledByUser?: boolean;
  reason?: string;
}

export type OperatorConfirmationDialogHandler = (
  request: OperatorConfirmationDialogRequest,
  formattedDetail: string
) => Promise<boolean>;

export class OperatorConfirmationModal {
  private static customDialogHandler?: OperatorConfirmationDialogHandler;

  /**
   * Sets custom confirmation dialog handler for automated tests.
   * Prohibited in production mode.
   */
  public static setDialogHandlerForTesting(handler?: OperatorConfirmationDialogHandler): void {
    if (process.env.NODE_ENV === 'production' && handler !== undefined) {
      throw new Error('Test dialog handler cannot be injected in production mode.');
    }
    this.customDialogHandler = handler;
  }

  /**
   * Clears custom confirmation dialog handler.
   */
  public static resetDialogHandlerForTesting(): void {
    this.customDialogHandler = undefined;
  }

  /**
   * Requests operator confirmation for a mutating coding agent task.
   * Generates a cryptographically bound challenge and displays a native VS Code modal dialog.
   */
  public static async requestConfirmation(
    request: OperatorConfirmationDialogRequest
  ): Promise<OperatorConfirmationDialogResult> {
    if (!request.studioId || request.studioId.trim().length === 0) {
      return {
        confirmed: false,
        reason: 'Studio ID is required for operator confirmation.',
      };
    }

    if (!request.prompt || request.prompt.trim().length === 0) {
      return {
        confirmed: false,
        reason: 'Prompt text is required for operator confirmation.',
      };
    }

    // 1. Format human-readable preview & configuration summary
    const cleanPrompt = request.prompt.trim().replace(/\r?\n+/g, ' ');
    const promptPreview = cleanPrompt.length > 200
      ? `${cleanPrompt.substring(0, 200)}...`
      : cleanPrompt;

    const detailLines: string[] = [
      `Action: ${request.actionDescription || 'Mutating Agent Task Execution'}`,
      `Studio: ${request.studioName || request.studioId}`,
      `Runtime: ${request.runtime}`,
      `Provider / Model: ${request.providerId || 'default'} / ${request.modelId || 'default'}`,
      `Workspace: ${request.workspacePath || 'None configured'}`,
      '',
      `Prompt Preview: "${promptPreview}"`,
      '',
      'WARNING: This task may modify workspace files or run commands. Ensure you trust the requested action.',
    ];
    const detail = detailLines.join('\n');

    // 2. Prompt user via native modal dialog BEFORE issuing challenge
    let confirmed = false;

    if (this.customDialogHandler) {
      confirmed = await this.customDialogHandler(request, detail);
    } else {
      const choice = await vscode.window.showWarningMessage(
        'Agent Studios: Confirm Mutating Agent Task Execution',
        { modal: true, detail },
        'Confirm & Execute',
        'Cancel'
      );
      confirmed = choice === 'Confirm & Execute';
    }

    // 3. Handle cancellation: purge pending challenges so token cannot be used
    if (!confirmed) {
      OperatorConfirmationManager.clearPendingChallenges();
      return {
        confirmed: false,
        cancelledByUser: true,
        reason: 'Operator cancelled confirmation dialog.',
      };
    }

    // 4. Issue cryptographically bound challenge only AFTER operator confirmed
    const challenge = OperatorConfirmationManager.createChallenge(
      request.studioId,
      request.prompt,
      {
        workspacePath: request.workspacePath,
        modelId: request.modelId,
        providerId: request.providerId,
        intent: 'task_execution',
      }
    );

    return {
      confirmed: true,
      challenge,
    };
  }
}
