/**
 * Agent Studios — Attention Queue Component
 * Milestone M14 Mission Control Workbench
 */

import { AttentionItem } from '../models/types';

export function renderAttentionQueue(items: AttentionItem[]): string {
  if (items.length === 0) {
    return `
      <div class="attention-queue-empty">
        <span class="codicon codicon-check-all text-success"></span>
        <span>All systems normal — no pending approvals or active blockers.</span>
      </div>
    `;
  }

  const cardsHtml = items
    .map((item) => {
      let icon = 'info';
      let iconClass = 'text-info';
      if (item.severity === 'critical' || item.severity === 'error') {
        icon = 'error';
        iconClass = 'text-error';
      } else if (item.severity === 'warning') {
        icon = 'warning';
        iconClass = 'text-warning';
      }

      return `
        <div class="attention-card severity-${item.severity}" role="alert">
          <div class="attention-card-header">
            <span class="codicon codicon-${icon} ${iconClass}"></span>
            <span class="attention-title">${escapeHtml(item.title)}</span>
            <button class="btn btn-secondary btn-sm" onclick="switchTab('${item.targetTab}', '${item.targetId ?? ''}')">
              Inspect
            </button>
          </div>
          <div class="attention-desc">${escapeHtml(item.description)}</div>
        </div>
      `;
    })
    .join('');

  return `
    <div class="attention-queue-container">
      <div class="attention-header">
        <span class="codicon codicon-bell"></span>
        <span class="section-title">Attention Queue (${items.length})</span>
      </div>
      <div class="attention-grid">
        ${cardsHtml}
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
