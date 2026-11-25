/**
 * Agent Studios — Interactive DAG Graph Renderer
 * Milestone M14 Mission Control Workbench
 *
 * Implements high-performance, theme-aware SVG rendering for task dependency graphs.
 * Reuses computeLayout and detectCycles from graphLayout.ts.
 */

import { TaskLifecycleModel } from '../models/types';
import {
  computeTopologicalLayout,
  detectCycle,
  LayoutEdge,
  LayoutNode,
  normalizeEdges,
} from '../../webviews/graphLayout';
import { TaskNode, TaskStatus } from '../../bridge/types';

export function renderDagGraph(
  tasks: TaskLifecycleModel[],
  selectedTaskId?: string
): string {
  if (tasks.length === 0) {
    return `
      <div class="dag-empty-state">
        <span class="codicon codicon-git-branch text-secondary"></span>
        <span>No tasks available to construct dependency graph.</span>
      </div>
    `;
  }

  // Map TaskLifecycleModel to TaskNode for graphLayout
  const taskNodes: TaskNode[] = tasks.map((t) => {
    let status: TaskStatus = 'pending';
    if (t.state === 'running') {
      status = 'running';
    } else if (t.state === 'succeeded') {
      status = 'succeeded';
    } else if (t.state === 'failed') {
      status = 'failed';
    } else if (t.state === 'blocked') {
      status = 'blocked';
    }

    return {
      id: t.id,
      runId: 'default-run',
      title: t.title,
      description: t.description,
      status,
      assignedAgentId: t.assignedAgentId,
      assignedAgentName: t.assignedAgentName,
      progress: t.progress,
      dependencies: t.dependencies,
      createdAt: Date.now(),
      updatedAt: Date.now(),
      error: t.error,
    };
  });

  const normalizedEdges = normalizeEdges(taskNodes);
  const cycleCheck = detectCycle(taskNodes, normalizedEdges);
  const layout = computeTopologicalLayout(taskNodes, normalizedEdges, {
    nodeWidth: 260,
    nodeHeight: 90,
    horizontalGap: 70,
    verticalGap: 30,
    paddingX: 40,
    paddingY: 40,
  });

  const edgesSvg = layout.edges
    .map(
      (e: LayoutEdge) => `
    <path class="dag-edge" d="${e.pathD}" marker-end="url(#arrowhead)" />
  `
    )
    .join('');

  const nodesSvg = layout.nodes
    .map((n: LayoutNode) => {
      const isSelected = n.id === selectedTaskId;
      const statusClass = `status-${n.status}`;
      return renderSvgNode(n, isSelected, statusClass);
    })
    .join('');

  const cycleBanner = cycleCheck.hasCycle
    ? `
    <div class="dag-cycle-alert" role="alert">
      <span class="codicon codicon-warning text-warning"></span>
      <span>Cyclic dependency detected: <code>${cycleCheck.cyclePath?.join(' &rarr; ')}</code></span>
    </div>
  `
    : '';

  const minimapScale = 0.14;
  const minimapNodes = layout.nodes
    .map(
      (n: LayoutNode) =>
        `<rect class="dag-minimap-node status-${n.status}" x="${Math.round(n.x * minimapScale)}" y="${Math.round(n.y * minimapScale)}" width="${Math.max(8, Math.round(n.width * minimapScale))}" height="${Math.max(4, Math.round(n.height * minimapScale))}" rx="1" />`
    )
    .join('');

  return `
    <div class="dag-graph-wrapper">
      <div class="dag-toolbar">
        <button class="btn btn-secondary btn-sm" onclick="zoomDag(0.1)" title="Zoom In">
          <span class="codicon codicon-zoom-in"></span>
        </button>
        <button class="btn btn-secondary btn-sm" onclick="zoomDag(-0.1)" title="Zoom Out">
          <span class="codicon codicon-zoom-out"></span>
        </button>
        <button class="btn btn-secondary btn-sm" onclick="fitDagToScreen()" title="Fit to Screen">
          <span class="codicon codicon-screen-full"></span> Fit
        </button>
        <button class="btn btn-secondary btn-sm" onclick="resetDagZoom()" title="Reset View">Reset</button>
        <button id="dag-follow-btn" class="btn btn-secondary btn-sm" onclick="toggleFollowExecution()" title="Auto-focus running tasks">
          Follow Run
        </button>
        <div class="dag-state-filters" role="group" aria-label="Filter DAG Nodes">
          <button class="pill-btn active" data-dag-filter="all" onclick="filterDagState('all')">All</button>
          <button class="pill-btn" data-dag-filter="running" onclick="filterDagState('running')">Running</button>
          <button class="pill-btn" data-dag-filter="failed" onclick="filterDagState('failed')">Failed</button>
          <button class="pill-btn" data-dag-filter="succeeded" onclick="filterDagState('succeeded')">Succeeded</button>
          <button class="pill-btn" data-dag-filter="blocked" onclick="filterDagState('blocked')">Blocked</button>
        </div>
        <span class="dag-node-count text-secondary text-sm">${layout.nodes.length} Nodes &bull; ${layout.edges.length} Edges</span>
      </div>

      ${cycleBanner}

      <div class="dag-svg-container" id="dag-viewport" style="position: relative;">
        <svg
          id="dag-svg"
          width="100%"
          height="100%"
          data-layout-width="${Math.max(800, layout.width)}"
          data-layout-height="${Math.max(500, layout.height)}"
          viewBox="0 0 ${Math.max(800, layout.width)} ${Math.max(500, layout.height)}"
          xmlns="http://www.w3.org/2000/svg"
        >
          <defs>
            <marker
              id="arrowhead"
              markerWidth="8"
              markerHeight="6"
              refX="7"
              refY="3"
              orient="auto"
            >
              <polygon points="0 0, 8 3, 0 6" class="dag-arrow-head" />
            </marker>
          </defs>
          <g id="dag-viewport-group">
            <g id="dag-zoom-group">
              <g class="dag-edges-layer">${edgesSvg}</g>
              <g class="dag-nodes-layer">${nodesSvg}</g>
            </g>
          </g>
        </svg>
        <div class="dag-minimap" id="dag-minimap" aria-label="DAG Minimap" style="position: absolute; right: 12px; bottom: 12px; background: rgba(20,20,24,0.82); border: 1px solid var(--border-subtle); border-radius: 4px; padding: 4px;">
          <svg width="${Math.max(110, Math.round(layout.width * minimapScale))}" height="${Math.max(70, Math.round(layout.height * minimapScale))}">
            ${minimapNodes}
          </svg>
        </div>
      </div>
    </div>
  `;
}

function renderSvgNode(n: LayoutNode, isSelected: boolean, statusClass: string): string {
  const selectedClass = isSelected ? 'selected' : '';
  const progressWidth = Math.min(n.width - 24, Math.max(0, ((n.width - 24) * n.progress) / 100));

  return `
    <g class="dag-node ${statusClass} ${selectedClass}" data-task-id="${escapeXml(n.id)}" data-state="${n.status}" tabindex="0" transform="translate(${n.x}, ${n.y})" onclick="selectTask('${n.id}')">
      <rect
        class="dag-node-bg"
        width="${n.width}"
        height="${n.height}"
        rx="6"
      />
      <text class="dag-node-title" x="12" y="24">
        ${escapeXml(truncateText(n.title, 28))}
      </text>
      <text class="dag-node-meta" x="12" y="44">
        ${escapeXml(n.id)} ${n.assignedAgentName ? `&bull; ${escapeXml(n.assignedAgentName)}` : ''}
      </text>

      <!-- Progress Bar Background -->
      <rect class="dag-node-prog-bg" x="12" y="58" width="${n.width - 24}" height="4" rx="2" />
      <!-- Progress Bar Fill -->
      <rect class="dag-node-prog-fill" x="12" y="58" width="${progressWidth}" height="4" rx="2" />

      <text class="dag-node-status" x="${n.width - 12}" y="76" text-anchor="end">
        ${n.status.toUpperCase()} (${n.progress}%)
      </text>
    </g>
  `;
}

function truncateText(text: string, maxLen: number): string {
  if (text.length <= maxLen) {
    return text;
  }
  return text.substring(0, maxLen - 1) + '…';
}

function escapeXml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&apos;');
}
