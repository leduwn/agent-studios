/**
 * Agent Studios — Task Kanban Board Component
 * Milestone M14 Mission Control Workbench
 *
 * Implements a 7-column Kanban board based strictly on verified task lifecycle states.
 */

import { TaskLifecycleModel, TaskLifecycleState } from '../models/types';

interface KanbanColumnConfig {
  id: TaskLifecycleState | 'failed_cancelled';
  title: string;
  states: TaskLifecycleState[];
  icon: string;
}

const KANBAN_COLUMNS: KanbanColumnConfig[] = [
  { id: 'pending', title: 'Pending', states: ['pending'], icon: 'history' },
  { id: 'ready', title: 'Ready', states: ['ready'], icon: 'play' },
  { id: 'running', title: 'Running', states: ['running'], icon: 'sync~spin' },
  { id: 'blocked', title: 'Blocked', states: ['blocked'], icon: 'lock' },
  { id: 'review', title: 'Review', states: ['review'], icon: 'eye' },
  { id: 'succeeded', title: 'Succeeded', states: ['succeeded'], icon: 'pass' },
  { id: 'failed_cancelled', title: 'Failed / Cancelled', states: ['failed', 'cancelled'], icon: 'error' },
];

export function renderKanbanBoard(
  tasks: TaskLifecycleModel[],
  selectedTaskId?: string
): string {
  const columnsHtml = KANBAN_COLUMNS.map((col) => {
    const colTasks = tasks.filter((t) => col.states.includes(t.state));

    const taskCardsHtml =
      colTasks.length > 0
        ? colTasks
            .map((task) => {
              const isSelected = task.id === selectedTaskId;
              return `
              <div class="kanban-card ${isSelected ? 'selected' : ''}" onclick="selectTask('${task.id}')" tabindex="0" role="button" aria-pressed="${isSelected}">
                <div class="kanban-card-title">${escapeHtml(task.title)}</div>
                <div class="kanban-card-meta">
                  <span class="task-id"><code>${escapeHtml(task.id)}</code></span>
                  ${
                    task.assignedAgentName
                      ? `<span class="agent-tag"><span class="codicon codicon-hubot"></span>${escapeHtml(task.assignedAgentName)}</span>`
                      : ''
                  }
                </div>
                ${
                  task.progress !== undefined && task.progress > 0
                    ? `
                  <div class="progress-bar-container">
                    <div class="progress-bar-fill" style="width: ${Math.min(100, Math.max(0, task.progress))}%"></div>
                  </div>
                `
                    : ''
                }
                ${
                  task.dependencies && task.dependencies.length > 0
                    ? `<div class="deps-tag"><span class="codicon codicon-git-branch"></span> ${task.dependencies.length} deps</div>`
                    : ''
                }
              </div>
            `;
            })
            .join('')
        : `<div class="kanban-empty-col">No tasks in ${escapeHtml(col.title)}</div>`;

    return `
      <div class="kanban-column" data-col="${col.id}">
        <div class="kanban-column-header">
          <span class="codicon codicon-${col.icon}"></span>
          <span class="col-title">${escapeHtml(col.title)}</span>
          <span class="col-count">${colTasks.length}</span>
        </div>
        <div class="kanban-column-body">
          ${taskCardsHtml}
        </div>
      </div>
    `;
  }).join('');

  return `
    <div class="kanban-board-container" role="region" aria-label="Tasks Kanban Board">
      ${columnsHtml}
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
