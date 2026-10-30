/**
 * Agent Studios — Task Management View Component
 * Milestone M14 Mission Control Workbench
 */

import { MissionControlState, TaskLifecycleModel } from '../models/types';
import { renderKanbanBoard } from '../components/KanbanBoard';
import { renderDagGraph } from '../components/DagGraphRenderer';

export function renderTasksView(state: MissionControlState): string {
  const { tasks, selectedTaskId } = state;
  const selectedTask = tasks.find((t) => t.id === selectedTaskId);

  const selectedTaskPanel = selectedTask
    ? renderSelectedTaskDrawer(selectedTask)
    : '';

  const tableRowsHtml =
    tasks.length > 0
      ? tasks
          .map((task) => {
            const isSelected = task.id === selectedTaskId;
            const statusClass = `status-${task.state}`;

            return `
            <tr class="table-row ${isSelected ? 'selected' : ''}" onclick="selectTask('${task.id}')">
              <td><strong>${escapeHtml(task.title)}</strong></td>
              <td><code>${escapeHtml(task.id)}</code></td>
              <td><span class="status-pill ${statusClass}">${task.state.toUpperCase()}</span></td>
              <td>
                <div class="progress-bar-container" style="width: 100px;">
                  <div class="progress-bar-fill" style="width: ${Math.min(100, Math.max(0, task.progress))}%"></div>
                </div>
                <span class="text-xs text-secondary">${task.progress}%</span>
              </td>
              <td>
                ${
                  task.assignedAgentName
                    ? `<span class="agent-tag"><span class="codicon codicon-hubot"></span> ${escapeHtml(task.assignedAgentName)}</span>`
                    : '<span class="text-secondary">—</span>'
                }
              </td>
              <td>
                ${
                  task.dependencies.length > 0
                    ? `<code>${task.dependencies.map((d) => escapeHtml(d)).join(', ')}</code>`
                    : '<span class="text-secondary">None</span>'
                }
              </td>
              <td>
                <button class="btn btn-secondary btn-sm" onclick="selectTask('${task.id}'); event.stopPropagation();">
                  Details
                </button>
              </td>
            </tr>
          `;
          })
          .join('')
      : `<tr><td colspan="7" class="text-center text-secondary">No tasks recorded.</td></tr>`;

  return `
    <div class="view-content tasks-view">
      <div class="view-header">
        <div>
          <h2>Task Management & Execution DAG</h2>
          <div class="view-subtitle text-secondary">Observability across all task lifecycles (${tasks.length} tasks total)</div>
        </div>

        <!-- Task Sub-View Mode Switcher -->
        <div class="sub-nav-tabs">
          <button id="task-subtab-kanban" class="sub-tab-btn active" onclick="switchTaskSubView('kanban')">
            <span class="codicon codicon-layout"></span> Kanban Board
          </button>
          <button id="task-subtab-dag" class="sub-tab-btn" onclick="switchTaskSubView('dag')">
            <span class="codicon codicon-git-branch"></span> Dependency DAG
          </button>
          <button id="task-subtab-list" class="sub-tab-btn" onclick="switchTaskSubView('list')">
            <span class="codicon codicon-list-flat"></span> Searchable List
          </button>
        </div>
      </div>

      <div class="tasks-layout-split">
        <div class="tasks-content-area">
          <!-- Subview 1: Kanban Board -->
          <div id="task-subview-kanban" class="task-subview active">
            ${renderKanbanBoard(tasks, selectedTaskId)}
          </div>

          <!-- Subview 2: DAG Graph -->
          <div id="task-subview-dag" class="task-subview" style="display: none;">
            ${renderDagGraph(tasks, selectedTaskId)}
          </div>

          <!-- Subview 3: Dense List -->
          <div id="task-subview-list" class="task-subview" style="display: none;">
            <div class="task-list-filter-bar">
              <input
                type="text"
                id="task-search-input"
                class="input-search"
                placeholder="Search tasks by title, ID, or agent..."
                oninput="filterTaskList(this.value)"
              />
            </div>
            <div class="tasks-table-container">
              <table class="data-table" id="task-list-table">
                <thead>
                  <tr>
                    <th>Title</th>
                    <th>Task ID</th>
                    <th>State</th>
                    <th>Progress</th>
                    <th>Assigned Agent</th>
                    <th>Dependencies</th>
                    <th>Actions</th>
                  </tr>
                </thead>
                <tbody>
                  ${tableRowsHtml}
                </tbody>
              </table>
            </div>
          </div>
        </div>

        <!-- Selected Task Details Drawer -->
        ${selectedTaskPanel}
      </div>
    </div>
  `;
}

function renderSelectedTaskDrawer(task: TaskLifecycleModel): string {
  const statusClass = `status-${task.state}`;

  return `
    <div class="task-drawer-panel">
      <div class="inspector-header">
        <div class="inspector-title-group">
          <span class="codicon codicon-tasklist"></span>
          <h3>${escapeHtml(task.title)}</h3>
        </div>
        <button class="btn btn-icon" onclick="selectTask(null)" title="Close Details" aria-label="Close Details">
          <span class="codicon codicon-close"></span>
        </button>
      </div>

      <div class="inspector-meta-grid">
        <div class="meta-item">
          <span class="meta-label">Lifecycle State</span>
          <span class="meta-value"><span class="status-pill ${statusClass}">${task.state.toUpperCase()}</span></span>
        </div>
        <div class="meta-item">
          <span class="meta-label">Task ID</span>
          <span class="meta-value"><code>${escapeHtml(task.id)}</code></span>
        </div>
        <div class="meta-item">
          <span class="meta-label">Progress</span>
          <span class="meta-value">${task.progress}%</span>
        </div>
        <div class="meta-item">
          <span class="meta-label">Assigned Agent</span>
          <span class="meta-value">
            ${
              task.assignedAgentId
                ? `<a href="#" onclick="selectAgent('${task.assignedAgentId}'); switchTab('agents'); return false;">${escapeHtml(task.assignedAgentName ?? task.assignedAgentId)}</a>`
                : '<span class="text-secondary">Unassigned</span>'
            }
          </span>
        </div>
        <div class="meta-item">
          <span class="meta-label">Dependencies</span>
          <span class="meta-value">
            ${
              task.dependencies.length > 0
                ? task.dependencies.map((d) => `<code>${escapeHtml(d)}</code>`).join(' ')
                : '<span class="text-secondary">None</span>'
            }
          </span>
        </div>
        <div class="meta-item">
          <span class="meta-label">Worktree</span>
          <span class="meta-value">
            ${task.worktreePath ? `<code>${escapeHtml(task.worktreePath)}</code>` : '<span class="text-secondary">Default</span>'}
          </span>
        </div>
      </div>

      <div class="inspector-section">
        <h4>Description</h4>
        <div class="task-desc-box">${escapeHtml(task.description || 'No description provided.')}</div>
      </div>

      ${
        task.error
          ? `
        <div class="inspector-error-box">
          <span class="codicon codicon-error"></span>
          <span>${escapeHtml(task.error)}</span>
        </div>
      `
          : ''
      }
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
