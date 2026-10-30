/**
 * Agent Studios — Activity Timeline View Component
 * Milestone M14 Mission Control Workbench
 */

import { MissionControlState } from '../models/types';

export function renderTimelineView(state: MissionControlState): string {
  const { timeline, diagnostics } = state;

  const eventsHtml =
    timeline.length > 0
      ? timeline
          .map((evt) => {
            let icon = 'circle-outline';
            let iconClass = 'text-secondary';

            if (evt.type.includes('task_completed') || evt.type.includes('succeeded')) {
              icon = 'pass';
              iconClass = 'text-success';
            } else if (evt.type.includes('failed') || evt.type.includes('error')) {
              icon = 'error';
              iconClass = 'text-error';
            } else if (evt.type.includes('started') || evt.type.includes('running')) {
              icon = 'sync~spin';
              iconClass = 'text-info';
            } else if (evt.type.includes('approval')) {
              icon = 'shield';
              iconClass = 'text-warning';
            }

            return `
            <div class="timeline-entry" data-type="${escapeHtml(evt.type)}">
              <div class="timeline-seq-col">
                <span class="seq-badge">#${evt.sequence}</span>
              </div>
              <div class="timeline-icon-col">
                <span class="codicon codicon-${icon} ${iconClass}"></span>
                <div class="timeline-line"></div>
              </div>
              <div class="timeline-content-col">
                <div class="timeline-header-row">
                  <span class="event-type-badge"><code>${escapeHtml(evt.type)}</code></span>
                  <span class="event-timestamp">${formatTime(evt.timestamp)}</span>
                  ${
                    evt.taskId
                      ? `<a href="#" onclick="selectTask('${evt.taskId}'); switchTab('tasks'); return false;" class="event-link"><span class="codicon codicon-tasklist"></span> ${escapeHtml(evt.taskId)}</a>`
                      : ''
                  }
                  ${
                    evt.agentId
                      ? `<a href="#" onclick="selectAgent('${evt.agentId}'); switchTab('agents'); return false;" class="event-link"><span class="codicon codicon-hubot"></span> ${escapeHtml(evt.agentId)}</a>`
                      : ''
                  }
                </div>
                <div class="timeline-summary">${escapeHtml(evt.summary)}</div>
                ${
                  evt.details && Object.keys(evt.details).length > 0
                    ? `
                  <details class="timeline-details-collapse">
                    <summary class="text-xs text-secondary">Event Details</summary>
                    <pre class="event-details-json">${escapeHtml(JSON.stringify(evt.details, null, 2))}</pre>
                  </details>
                `
                    : ''
                }
              </div>
            </div>
          `;
          })
          .join('')
      : `<div class="text-center text-secondary py-8">No activity events recorded in timeline.</div>`;

  return `
    <div class="view-content timeline-view">
      <div class="view-header">
        <div>
          <h2>Activity Event Stream & Audit Trail</h2>
          <div class="view-subtitle text-secondary">Chronological event replay and watermarking (${timeline.length} events, Watermark: seq#${diagnostics.watermark})</div>
        </div>

        <div class="timeline-controls">
          <input
            type="text"
            id="timeline-search-input"
            class="input-search"
            placeholder="Search events by summary, type, or ID..."
            oninput="filterTimelineEvents(this.value)"
          />
        </div>
      </div>

      <div class="timeline-filter-pills">
        <button class="pill-btn active" onclick="filterTimelineCategory('all')">All</button>
        <button class="pill-btn" onclick="filterTimelineCategory('task')">Tasks</button>
        <button class="pill-btn" onclick="filterTimelineCategory('agent')">Agents</button>
        <button class="pill-btn" onclick="filterTimelineCategory('approval')">Approvals</button>
        <button class="pill-btn" onclick="filterTimelineCategory('session')">Session</button>
      </div>

      <div class="timeline-stream-container" id="timeline-stream">
        ${eventsHtml}
      </div>
    </div>
  `;
}

function formatTime(isoString: string): string {
  try {
    const d = new Date(isoString);
    return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
  } catch {
    return isoString;
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
