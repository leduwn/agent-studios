/**
 * Agent Studios — Approvals Center View Component
 * Milestone M14 Mission Control Workbench
 */

import { MissionControlState } from '../models/types';

export function renderApprovalsView(state: MissionControlState): string {
  const { approvals } = state;
  const pendingApprovals = approvals.filter((a) => a.status === 'pending');

  const rowsHtml =
    pendingApprovals.length > 0
      ? pendingApprovals
          .map((appr) => {
            let riskBadgeClass = 'badge-secondary';
            if (appr.riskLevel === 'critical' || appr.riskLevel === 'high') {
              riskBadgeClass = 'badge-error';
            } else if (appr.riskLevel === 'medium') {
              riskBadgeClass = 'badge-warning';
            }

            return `
            <tr class="table-row">
              <td><code>${escapeHtml(appr.id)}</code></td>
              <td><span class="badge ${riskBadgeClass}">${appr.riskLevel.toUpperCase()}</span></td>
              <td><code>${escapeHtml(appr.actionType)}</code></td>
              <td><code class="code-target">${escapeHtml(appr.commandOrTarget)}</code></td>
              <td>${appr.requestedPermissions.map((p) => `<span class="badge badge-secondary">${escapeHtml(p)}</span>`).join(' ')}</td>
              <td>${formatDate(appr.createdAt)}</td>
              <td class="approval-actions-cell">
                <button class="btn btn-primary btn-sm" onclick="approveAction('${appr.id}')" title="Approve Action" disabled>
                  <span class="codicon codicon-check"></span> Approve
                </button>
                <button class="btn btn-secondary btn-sm" onclick="denyAction('${appr.id}')" title="Deny Action" disabled>
                  <span class="codicon codicon-close"></span> Deny
                </button>
              </td>
            </tr>
          `;
          })
          .join('')
      : `<tr><td colspan="7" class="text-center text-secondary">No pending human approval requests.</td></tr>`;

  return `
    <div class="view-content approvals-view">
      <div class="view-header">
        <div>
          <h2>Approval Center & Security Governance</h2>
          <div class="view-subtitle text-secondary">Human-in-the-loop review for elevated permissions and destructive operations (${pendingApprovals.length} pending)</div>
        </div>
      </div>

      <div class="approval-security-notice" role="alert">
        <span class="codicon codicon-shield text-info"></span>
        <div>
          <strong>Fail-Closed Security Boundary:</strong> Mutating approvals are disabled in the read-only vertical slice. All actions remain safely paused until interactive execution gates are authorized.
        </div>
      </div>

      <div class="approvals-table-container">
        <table class="data-table">
          <thead>
            <tr>
              <th>ID</th>
              <th>Risk Level</th>
              <th>Action Type</th>
              <th>Command / Target</th>
              <th>Permissions</th>
              <th>Requested</th>
              <th>Decision</th>
            </tr>
          </thead>
          <tbody>
            ${rowsHtml}
          </tbody>
        </table>
      </div>
    </div>
  `;
}

function formatDate(timestamp: number): string {
  try {
    return new Date(timestamp).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
  } catch {
    return String(timestamp);
  }
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}
