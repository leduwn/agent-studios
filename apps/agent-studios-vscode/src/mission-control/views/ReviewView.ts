/**
 * Agent Studios — Review & Artifacts View Component
 * Milestone M14 Mission Control Workbench
 */

import { MissionControlState } from '../models/types';

export function renderReviewView(state: MissionControlState): string {
  const { artifacts } = state;

  const rowsHtml =
    artifacts.length > 0
      ? artifacts
          .map((art) => {
            const badgeClass =
              art.changeType === 'added'
                ? 'badge-success'
                : art.changeType === 'deleted'
                ? 'badge-error'
                : 'badge-secondary';

            return `
            <tr class="table-row">
              <td>
                <span class="badge ${badgeClass}">${art.changeType.toUpperCase()}</span>
                <code>${escapeHtml(art.path)}</code>
              </td>
              <td>
                ${
                  art.taskId
                    ? `<a href="#" onclick="selectTask('${art.taskId}'); switchTab('tasks'); return false;">${escapeHtml(art.taskId)}</a>`
                    : '<span class="text-secondary">—</span>'
                }
              </td>
              <td><span class="text-sm text-secondary">${escapeHtml(art.diffSummary ?? 'Diff available')}</span></td>
              <td>
                <button class="btn btn-secondary btn-sm" onclick="openDiff('${art.id}')">
                  <span class="codicon codicon-diff"></span> Open Diff
                </button>
              </td>
            </tr>
          `;
          })
          .join('')
      : `<tr><td colspan="4" class="text-center text-secondary">No modified artifacts or patch files recorded for review.</td></tr>`;

  return `
    <div class="view-content review-view">
      <div class="view-header">
        <div>
          <h2>Code Review & Artifacts</h2>
          <div class="view-subtitle text-secondary">Review generated file changes and compare diffs via native VS Code diff editor (${artifacts.length} modified files)</div>
        </div>
      </div>

      <div class="review-meta-banner">
        <div class="meta-item">
          <span class="meta-label">Total Changes</span>
          <span class="meta-value"><strong>${artifacts.length}</strong> files</span>
        </div>
        <div class="meta-item">
          <span class="meta-label">Diff Tool</span>
          <span class="meta-value">VS Code Native Editor (Side-by-Side)</span>
        </div>
        <div class="meta-item">
          <span class="meta-label">Merge Policy</span>
          <span class="meta-value"><span class="badge badge-secondary">Human Verification Required</span></span>
        </div>
      </div>

      <div class="review-table-container">
        <table class="data-table">
          <thead>
            <tr>
              <th>File Path</th>
              <th>Task</th>
              <th>Change Summary</th>
              <th>Actions</th>
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

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}
