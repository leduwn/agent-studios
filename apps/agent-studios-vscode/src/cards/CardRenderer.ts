/**
 * Agent Studios — Structured Card Component HTML Renderer
 * Milestone M14-UI-01: In-Conversation Activities & Diff Review
 */

import * as vscode from 'vscode';
import { AgentStudiosBridge } from '../bridge/AgentStudiosBridge';
import { ApprovalDecisionResult, ApprovalRequest } from '../bridge/types';
import { escapeHtml } from '../webviews/markdownRenderer';
import {
  ApprovalCardData,
  CardRiskLevel,
  ChangedFilesCardData,
  SupportedTool,
  TaskProgressCardData,
  ToolCallCardData,
} from './types';

/**
 * Returns human-readable label and icon representation for supported tools.
 */
function getToolMetadata(tool: SupportedTool): { label: string; icon: string } {
  switch (tool) {
    case 'read_file':
      return { label: 'Read File', icon: 'file-text' };
    case 'edit_file':
      return { label: 'Edit File', icon: 'edit' };
    case 'search_codebase':
      return { label: 'Search Codebase', icon: 'search' };
    case 'exec_command':
      return { label: 'Execute Command', icon: 'terminal' };
    case 'run_tests':
      return { label: 'Run Tests', icon: 'beaker' };
    case 'apply_patch':
      return { label: 'Apply Patch', icon: 'diff' };
    case 'review_code':
      return { label: 'Review Code', icon: 'eye' };
    case 'delegate_agent':
      return { label: 'Delegate Agent', icon: 'hubot' };
    default:
      return { label: String(tool), icon: 'tools' };
  }
}

function isBridgeDisconnected(bridge: AgentStudiosBridge): boolean {
  return (bridge.connectionState as string) === 'disconnected';
}

/**
 * Normalizes risk level into upper-case standardized token.
 */
export function normalizeRiskLevel(risk: CardRiskLevel): 'LOW' | 'MEDIUM' | 'HIGH' | 'CRITICAL' {
  const norm = String(risk).toUpperCase();
  if (norm === 'CRITICAL') {
    return 'CRITICAL';
  }
  if (norm === 'HIGH') {
    return 'HIGH';
  }
  if (norm === 'MEDIUM') {
    return 'MEDIUM';
  }
  return 'LOW';
}

/**
 * Core Card Renderer providing monochrome VS Code native styling and accessible HTML.
 */
export class CardRenderer {
  /**
   * Generates monochrome VS Code CSS styling rules for all card types.
   */
  public static getStyles(): string {
    return `
      /* Agent Studios In-Conversation Cards — Native VS Code monochrome style */
      .agent-studios-card {
        box-sizing: border-box;
        font-family: var(--vscode-font-family, -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif);
        font-size: var(--vscode-font-size, 13px);
        color: var(--vscode-editorWidget-foreground, var(--vscode-editor-foreground, #cccccc));
        background-color: var(--vscode-editorWidget-background, #252526);
        border: 1px solid var(--vscode-editorWidget-border, var(--vscode-widget-border, #3c3c3c));
        border-radius: 4px;
        margin: 8px 0 12px 0;
        overflow: hidden;
        line-height: 1.4;
      }

      .agent-studios-card .card-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 8px 12px;
        background-color: var(--vscode-sideBar-background, rgba(255, 255, 255, 0.03));
        border-bottom: 1px solid var(--vscode-editorWidget-border, var(--vscode-widget-border, #3c3c3c));
        gap: 8px;
        flex-wrap: wrap;
      }

      .agent-studios-card .card-body {
        padding: 10px 12px;
      }

      .agent-studios-card .card-footer {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 8px 12px;
        border-top: 1px solid var(--vscode-editorWidget-border, var(--vscode-widget-border, #3c3c3c));
        background-color: var(--vscode-sideBar-background, rgba(255, 255, 255, 0.02));
        gap: 8px;
      }

      /* Tool Call Card */
      .tool-call-card .tool-info {
        display: flex;
        align-items: center;
        gap: 8px;
        flex: 1;
        min-width: 0;
      }

      .tool-call-card .tool-icon {
        font-family: var(--vscode-editor-font-family, monospace);
        font-size: 11px;
        padding: 2px 6px;
        background-color: var(--vscode-badge-background, #4d4d4d);
        color: var(--vscode-badge-foreground, #ffffff);
        border-radius: 3px;
        text-transform: uppercase;
        font-weight: 600;
        flex-shrink: 0;
      }

      .tool-call-card .tool-name {
        font-weight: 600;
        font-size: 12px;
        color: var(--vscode-editorWidget-foreground, #ffffff);
      }

      .tool-call-card .tool-target {
        font-family: var(--vscode-editor-font-family, monospace);
        font-size: 12px;
        color: var(--vscode-textLink-foreground, #3794ff);
        background-color: var(--vscode-textCodeBlock-background, rgba(0, 0, 0, 0.2));
        padding: 1px 5px;
        border-radius: 3px;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        max-width: 320px;
      }

      .tool-call-card .card-meta {
        display: flex;
        align-items: center;
        gap: 8px;
        flex-shrink: 0;
      }

      .tool-call-card .tool-duration {
        font-size: 11px;
        color: var(--vscode-descriptionForeground, #8c8c8c);
        font-family: var(--vscode-editor-font-family, monospace);
      }

      /* Status Badges */
      .status-badge {
        display: inline-flex;
        align-items: center;
        gap: 5px;
        padding: 2px 7px;
        border-radius: 3px;
        font-size: 11px;
        font-weight: 500;
        line-height: 16px;
      }

      .status-badge.status-running {
        background-color: var(--vscode-badge-background, #4d4d4d);
        color: var(--vscode-badge-foreground, #ffffff);
      }

      .status-badge.status-completed {
        background-color: rgba(137, 209, 133, 0.15);
        color: var(--vscode-testing-iconPassed, #89d185);
        border: 1px solid rgba(137, 209, 133, 0.35);
      }

      .status-badge.status-failed {
        background-color: rgba(241, 76, 76, 0.15);
        color: var(--vscode-testing-iconFailed, #f14c4c);
        border: 1px solid rgba(241, 76, 76, 0.35);
      }

      /* Animated Spinner */
      @keyframes agent-studios-spin {
        from { transform: rotate(0deg); }
        to { transform: rotate(360deg); }
      }

      .spinner {
        display: inline-block;
        width: 11px;
        height: 11px;
        border: 2px solid currentColor;
        border-top-color: transparent;
        border-radius: 50%;
        animation: agent-studios-spin 0.8s linear infinite;
        vertical-align: middle;
      }

      .status-icon {
        font-size: 12px;
        font-weight: bold;
      }

      /* Collapsible Output */
      .tool-output-details {
        margin-top: 6px;
      }

      .tool-output-summary {
        cursor: pointer;
        user-select: none;
        font-size: 12px;
        color: var(--vscode-descriptionForeground, #8c8c8c);
        outline: none;
        padding: 2px 0;
      }

      .tool-output-summary:hover {
        color: var(--vscode-editorWidget-foreground, #cccccc);
      }

      .tool-output-body {
        margin-top: 6px;
        max-height: 240px;
        overflow-y: auto;
        background-color: var(--vscode-textCodeBlock-background, rgba(0, 0, 0, 0.25));
        border: 1px solid var(--vscode-editorWidget-border, #3c3c3c);
        border-radius: 3px;
        padding: 8px 10px;
      }

      .tool-output-body pre {
        margin: 0;
        font-family: var(--vscode-editor-font-family, monospace);
        font-size: 12px;
        white-space: pre-wrap;
        word-break: break-all;
      }

      /* Approval Card & Risk Badges */
      .badge-risk {
        display: inline-flex;
        align-items: center;
        padding: 2px 7px;
        border-radius: 3px;
        font-size: 11px;
        font-weight: 700;
        text-transform: uppercase;
        letter-spacing: 0.5px;
      }

      .badge-risk-low {
        background-color: var(--vscode-badge-background, #4d4d4d);
        color: var(--vscode-badge-foreground, #ffffff);
      }

      .badge-risk-medium {
        background-color: rgba(204, 167, 0, 0.15);
        color: var(--vscode-editorWarning-foreground, #cca700);
        border: 1px solid var(--vscode-editorWarning-foreground, #cca700);
      }

      .badge-risk-high {
        background-color: rgba(255, 140, 0, 0.15);
        color: var(--vscode-editorOverviewRuler-warningAccent, #ff8c00);
        border: 1px solid var(--vscode-editorOverviewRuler-warningAccent, #ff8c00);
      }

      .badge-risk-critical {
        background-color: rgba(241, 76, 76, 0.2);
        color: var(--vscode-editorError-foreground, #f14c4c);
        border: 1px solid var(--vscode-editorError-foreground, #f14c4c);
      }

      .approval-card .approval-meta {
        display: flex;
        align-items: center;
        gap: 8px;
      }

      .approval-card .approval-action-type {
        font-size: 12px;
        color: var(--vscode-descriptionForeground, #8c8c8c);
        font-family: var(--vscode-editor-font-family, monospace);
      }

      .approval-card .approval-target-container {
        display: flex;
        flex-direction: column;
        gap: 4px;
      }

      .approval-card .approval-target-label {
        font-size: 11px;
        text-transform: uppercase;
        letter-spacing: 0.5px;
        color: var(--vscode-descriptionForeground, #8c8c8c);
      }

      .approval-card .approval-target {
        font-family: var(--vscode-editor-font-family, monospace);
        font-size: 12px;
        background-color: var(--vscode-textCodeBlock-background, rgba(0, 0, 0, 0.25));
        border: 1px solid var(--vscode-editorWidget-border, #3c3c3c);
        padding: 6px 8px;
        border-radius: 3px;
        white-space: pre-wrap;
        word-break: break-all;
      }

      .approval-card .approval-permissions {
        margin-top: 8px;
        font-size: 11px;
        color: var(--vscode-descriptionForeground, #8c8c8c);
      }

      /* Buttons */
      .btn {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        padding: 4px 12px;
        font-size: 12px;
        font-family: inherit;
        border-radius: 3px;
        cursor: pointer;
        border: 1px solid transparent;
        line-height: 18px;
        transition: background-color 0.15s ease;
      }

      .btn-approve {
        background-color: var(--vscode-button-background, #0e639c);
        color: var(--vscode-button-foreground, #ffffff);
      }

      .btn-approve:hover {
        background-color: var(--vscode-button-hoverBackground, #1177bb);
      }

      .btn-deny {
        background-color: var(--vscode-button-secondaryBackground, #3a3d41);
        color: var(--vscode-button-secondaryForeground, #ffffff);
        border: 1px solid var(--vscode-editorWidget-border, #3c3c3c);
      }

      .btn-deny:hover {
        background-color: var(--vscode-button-secondaryHoverBackground, #45494e);
      }

      .btn-task-graph, .btn-diff {
        background-color: var(--vscode-button-secondaryBackground, #3a3d41);
        color: var(--vscode-button-secondaryForeground, #ffffff);
        border: 1px solid var(--vscode-editorWidget-border, #3c3c3c);
        font-size: 11px;
        padding: 2px 8px;
      }

      .btn-task-graph:hover, .btn-diff:hover {
        background-color: var(--vscode-button-secondaryHoverBackground, #45494e);
      }

      .badge-resolution {
        display: inline-flex;
        align-items: center;
        padding: 3px 8px;
        border-radius: 3px;
        font-size: 11px;
        font-weight: 600;
        text-transform: uppercase;
      }

      .badge-resolution.badge-approved {
        background-color: rgba(137, 209, 133, 0.15);
        color: var(--vscode-testing-iconPassed, #89d185);
        border: 1px solid rgba(137, 209, 133, 0.4);
      }

      .badge-resolution.badge-denied {
        background-color: rgba(241, 76, 76, 0.15);
        color: var(--vscode-testing-iconFailed, #f14c4c);
        border: 1px solid rgba(241, 76, 76, 0.4);
      }

      .badge-resolution.badge-expired {
        background-color: rgba(140, 140, 140, 0.15);
        color: var(--vscode-descriptionForeground, #8c8c8c);
        border: 1px solid rgba(140, 140, 140, 0.4);
      }

      /* Task Progress Card */
      .task-progress-card .progress-agents {
        display: flex;
        align-items: center;
        gap: 12px;
        font-size: 12px;
      }

      .task-progress-card .task-count {
        font-size: 12px;
        font-weight: 600;
        color: var(--vscode-editorWidget-foreground, #ffffff);
      }

      .task-progress-card .progress-bar-container {
        display: flex;
        align-items: center;
        gap: 10px;
        width: 100%;
        margin-top: 4px;
      }

      .task-progress-card .progress-bar-track {
        flex: 1;
        height: 6px;
        background-color: var(--vscode-editorWidget-border, #3c3c3c);
        border-radius: 3px;
        overflow: hidden;
      }

      .task-progress-card .progress-bar-fill {
        height: 100%;
        background-color: var(--vscode-progressBar-background, #007acc);
        transition: width 0.3s ease;
      }

      .task-progress-card .progress-percent {
        font-size: 11px;
        font-family: var(--vscode-editor-font-family, monospace);
        color: var(--vscode-descriptionForeground, #8c8c8c);
        min-width: 35px;
        text-align: right;
      }

      /* Changed Files Card */
      .changed-files-card .changed-files-title {
        font-weight: 600;
        font-size: 12px;
      }

      .changed-files-card .changed-files-totals {
        display: flex;
        align-items: center;
        gap: 6px;
        font-family: var(--vscode-editor-font-family, monospace);
        font-size: 12px;
      }

      .changed-files-card .changed-files-list {
        list-style: none;
        margin: 0;
        padding: 0;
        display: flex;
        flex-direction: column;
        gap: 6px;
      }

      .changed-files-card .changed-file-row {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 4px 6px;
        background-color: var(--vscode-textCodeBlock-background, rgba(0, 0, 0, 0.15));
        border: 1px solid var(--vscode-editorWidget-border, #3c3c3c);
        border-radius: 3px;
        gap: 8px;
      }

      .changed-files-card .file-path {
        font-family: var(--vscode-editor-font-family, monospace);
        font-size: 12px;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        flex: 1;
        color: var(--vscode-editorWidget-foreground, #cccccc);
      }

      .changed-files-card .file-stats {
        display: flex;
        align-items: center;
        gap: 8px;
        flex-shrink: 0;
      }

      .diff-additions {
        color: var(--vscode-testing-iconPassed, #89d185);
        font-family: var(--vscode-editor-font-family, monospace);
        font-size: 12px;
        font-weight: 600;
      }

      .diff-deletions {
        color: var(--vscode-testing-iconFailed, #f14c4c);
        font-family: var(--vscode-editor-font-family, monospace);
        font-size: 12px;
        font-weight: 600;
      }
    `;
  }

  /**
   * Renders a structured ToolCallCard.
   */
  public static renderToolCallCard(data: ToolCallCardData): string {
    const meta = getToolMetadata(data.tool);
    const escapedTool = escapeHtml(data.tool);
    const escapedTarget = data.target !== undefined ? escapeHtml(data.target) : '';
    const durationText = data.durationMs !== undefined ? `${data.durationMs} ms` : '';

    let statusBadgeHtml = '';
    if (data.status === 'running') {
      statusBadgeHtml = `<span class="badge status-badge status-running"><span class="spinner" aria-hidden="true"></span> Running</span>`;
    } else if (data.status === 'completed') {
      statusBadgeHtml = `<span class="badge status-badge status-completed"><span class="status-icon" aria-hidden="true">&#10003;</span> Completed</span>`;
    } else if (data.status === 'failed') {
      statusBadgeHtml = `<span class="badge status-badge status-failed"><span class="status-icon" aria-hidden="true">&#10005;</span> Failed</span>`;
    }

    const outputContent = data.output || data.error;
    let collapsibleHtml = '';
    if (outputContent !== undefined && outputContent !== null && outputContent.length > 0) {
      const summaryLabel = data.summary
        ? escapeHtml(data.summary)
        : data.status === 'failed'
        ? 'Error Output'
        : 'Output';
      const sanitizedOutput = escapeHtml(outputContent);
      collapsibleHtml = `
      <details class="tool-output-details">
        <summary class="tool-output-summary">${summaryLabel}</summary>
        <div class="tool-output-body">
          <pre><code>${sanitizedOutput}</code></pre>
        </div>
      </details>`;
    } else if (data.summary) {
      collapsibleHtml = `
      <div class="tool-output-details">
        <span class="tool-output-summary">${escapeHtml(data.summary)}</span>
      </div>`;
    }

    return `
<div class="agent-studios-card tool-call-card" data-tool="${escapedTool}" data-status="${escapeHtml(data.status)}">
  <div class="card-header">
    <div class="tool-info">
      <span class="tool-icon">${escapeHtml(meta.label)}</span>
      <span class="tool-name">${escapedTool}</span>
      ${escapedTarget ? `<code class="tool-target" title="${escapedTarget}">${escapedTarget}</code>` : ''}
    </div>
    <div class="card-meta">
      ${durationText ? `<span class="tool-duration">${durationText}</span>` : ''}
      ${statusBadgeHtml}
    </div>
  </div>
  ${collapsibleHtml ? `<div class="card-body">${collapsibleHtml}</div>` : ''}
</div>`.trim();
  }

  /**
   * Renders an ApprovalCard with risk badges and fail-closed button hooks.
   */
  public static renderApprovalCard(data: ApprovalCardData): string {
    const normRisk = normalizeRiskLevel(data.riskLevel);
    const escapedId = escapeHtml(data.id);
    const escapedTarget = escapeHtml(data.commandOrTarget);
    const escapedActionType = data.actionType ? escapeHtml(data.actionType) : '';
    const escapedTaskTitle = data.taskTitle ? escapeHtml(data.taskTitle) : '';

    let riskBadgeClass = 'badge-risk-low';
    if (normRisk === 'MEDIUM') {
      riskBadgeClass = 'badge-risk-medium';
    } else if (normRisk === 'HIGH') {
      riskBadgeClass = 'badge-risk-high';
    } else if (normRisk === 'CRITICAL') {
      riskBadgeClass = 'badge-risk-critical';
    }

    const riskBadgeHtml = `<span class="badge badge-risk ${riskBadgeClass}">${normRisk}</span>`;

    let footerHtml = '';
    if (data.status === 'pending') {
      footerHtml = `
      <div class="card-footer approval-actions">
        <button class="btn btn-approve" data-action="approve" data-approval-id="${escapedId}">Approve</button>
        <button class="btn btn-deny" data-action="deny" data-approval-id="${escapedId}">Deny</button>
      </div>`;
    } else if (data.status === 'approved') {
      footerHtml = `
      <div class="card-footer">
        <span class="badge badge-resolution badge-approved">Approved</span>
      </div>`;
    } else if (data.status === 'denied') {
      footerHtml = `
      <div class="card-footer">
        <span class="badge badge-resolution badge-denied">Denied</span>
      </div>`;
    } else if (data.status === 'expired') {
      footerHtml = `
      <div class="card-footer">
        <span class="badge badge-resolution badge-expired">Expired</span>
      </div>`;
    }

    const permissionsHtml = data.requestedPermissions && data.requestedPermissions.length > 0
      ? `<div class="approval-permissions">Permissions: ${data.requestedPermissions.map(p => escapeHtml(p)).join(', ')}</div>`
      : '';

    return `
<div class="agent-studios-card approval-card" data-approval-id="${escapedId}" data-status="${escapeHtml(data.status)}" data-risk="${normRisk}">
  <div class="card-header">
    <div class="approval-meta">
      ${riskBadgeHtml}
      ${escapedActionType ? `<span class="approval-action-type">${escapedActionType}</span>` : ''}
    </div>
    ${escapedTaskTitle ? `<span class="approval-task-title">${escapedTaskTitle}</span>` : ''}
  </div>
  <div class="card-body">
    <div class="approval-target-container">
      <span class="approval-target-label">Target:</span>
      <code class="approval-target">${escapedTarget}</code>
    </div>
    ${permissionsHtml}
  </div>
  ${footerHtml}
</div>`.trim();
  }

  /**
   * Dispatches approval decisions with fail-closed checks and modal warnings for HIGH/CRITICAL risks.
   */
  public static async handleApprovalAction(
    approval: ApprovalCardData | ApprovalRequest,
    decision: 'approved' | 'denied',
    bridge: AgentStudiosBridge
  ): Promise<ApprovalDecisionResult> {
    // 1. Fail closed if bridge is disconnected
    if (isBridgeDisconnected(bridge)) {
      vscode.window.showErrorMessage(
        `Cannot ${decision} action: Agent Studios daemon is disconnected. Approvals fail closed.`
      );
      return {
        id: approval.id,
        decision,
        applied: false,
        message: 'Agent Studios daemon is disconnected. Approvals fail closed.',
      };
    }

    // 2. High or Critical risk modal confirmation before approving
    const normRisk = normalizeRiskLevel(approval.riskLevel);
    if (decision === 'approved' && (normRisk === 'HIGH' || normRisk === 'CRITICAL')) {
      const target = approval.commandOrTarget || approval.id;
      const confirmChoice = await vscode.window.showWarningMessage(
        `Approve ${normRisk}-risk action: ${target} ?`,
        { modal: true },
        'Confirm Approve',
        'Cancel'
      );

      if (confirmChoice !== 'Confirm Approve') {
        return {
          id: approval.id,
          decision,
          applied: false,
          message: 'Approval cancelled by user modal confirmation.',
        };
      }
    }

    // 3. Re-verify bridge connection after modal prompt
    if (isBridgeDisconnected(bridge)) {
      vscode.window.showErrorMessage(
        `Cannot ${decision} action: Agent Studios daemon is disconnected. Approvals fail closed.`
      );
      return {
        id: approval.id,
        decision,
        applied: false,
        message: 'Agent Studios daemon is disconnected. Approvals fail closed.',
      };
    }

    // 4. Request decision from authoritative bridge
    return await bridge.requestApprovalDecision(approval.id, decision);
  }

  /**
   * Renders a TaskProgressCard displaying agent progress, task completion metrics, and graph trigger.
   */
  public static renderTaskProgressCard(data: TaskProgressCardData): string {
    const total = Math.max(0, data.totalTasks);
    const completed = Math.max(0, data.completedTasks);
    const percentage = data.progressPercentage !== undefined
      ? Math.min(100, Math.max(0, data.progressPercentage))
      : total > 0
      ? Math.round((completed / total) * 100)
      : 0;

    const coordinatorText = data.coordinatorName ? escapeHtml(data.coordinatorName) : 'Agent Studios';
    const activeAgentText = data.activeAgentName ? escapeHtml(data.activeAgentName) : undefined;

    return `
<div class="agent-studios-card task-progress-card" data-completed="${completed}" data-total="${total}">
  <div class="card-header">
    <div class="progress-agents">
      <span class="coordinator-label">Coordinator: <strong>${coordinatorText}</strong></span>
      ${activeAgentText ? `<span class="active-agent-label">Active: <strong>${activeAgentText}</strong></span>` : ''}
    </div>
    <div class="task-count">${completed}/${total} complete</div>
  </div>
  <div class="card-body">
    <div class="progress-bar-container">
      <div class="progress-bar-track">
        <div class="progress-bar-fill" style="width: ${percentage}%;"></div>
      </div>
      <span class="progress-percent">${percentage}%</span>
    </div>
  </div>
  <div class="card-footer">
    <button class="btn btn-task-graph" data-command="agent-studios.showTaskGraph">Show Task Graph</button>
  </div>
</div>`.trim();
  }

  /**
   * Renders a ChangedFilesCard summarizing modified files and diff inspection buttons.
   */
  public static renderChangedFilesCard(data: ChangedFilesCardData): string {
    const totalAdditions = data.totalAdditions !== undefined
      ? data.totalAdditions
      : data.files.reduce((acc, f) => acc + (f.additions || 0), 0);
    const totalDeletions = data.totalDeletions !== undefined
      ? data.totalDeletions
      : data.files.reduce((acc, f) => acc + (f.deletions || 0), 0);

    const title = data.title || `Changed Files (${data.files.length})`;

    const rowsHtml = data.files.map((file) => {
      const escapedPath = escapeHtml(file.path);
      const escapedArtifactId = file.artifactId ? escapeHtml(file.artifactId) : '';

      return `
      <li class="changed-file-row">
        <span class="file-path" title="${escapedPath}">${escapedPath}</span>
        <div class="file-stats">
          <span class="diff-additions">+${file.additions}</span>
          <span class="diff-deletions">-${file.deletions}</span>
          <button class="btn btn-diff" data-command="agent-studios.inspectArtifact" data-path="${escapedPath}" data-artifact-id="${escapedArtifactId}">Diff</button>
        </div>
      </li>`;
    }).join('\n');

    return `
<div class="agent-studios-card changed-files-card">
  <div class="card-header">
    <span class="changed-files-title">${escapeHtml(title)}</span>
    <div class="changed-files-totals">
      <span class="diff-additions">+${totalAdditions}</span>
      <span class="diff-deletions">-${totalDeletions}</span>
    </div>
  </div>
  <div class="card-body">
    <ul class="changed-files-list">
      ${rowsHtml}
    </ul>
  </div>
</div>`.trim();
  }
}
