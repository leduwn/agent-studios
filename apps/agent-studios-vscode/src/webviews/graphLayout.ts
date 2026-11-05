/**
 * Agent Studios — Deterministic DAG Topological Layout & Cycle Detection
 * Milestone M14-UI-00: Code-OSS Native Agents Workbench
 */

import { TaskId, TaskNode, TaskStatus } from '../bridge/types';

export interface LayoutNode {
  id: TaskId;
  runId: string;
  title: string;
  description: string;
  status: TaskStatus;
  assignedAgentId?: string;
  assignedAgentName?: string;
  progress: number;
  error?: string;
  level: number;
  x: number;
  y: number;
  width: number;
  height: number;
  dependencies: TaskId[];
  createdAt: number;
  updatedAt: number;
}

export interface LayoutEdge {
  from: TaskId;
  to: TaskId;
  startX: number;
  startY: number;
  endX: number;
  endY: number;
  pathD: string;
}

export interface LayoutOptions {
  nodeWidth?: number;
  nodeHeight?: number;
  horizontalGap?: number;
  verticalGap?: number;
  paddingX?: number;
  paddingY?: number;
}

export interface CycleDetectionResult {
  hasCycle: boolean;
  cyclePath?: TaskId[];
}

export interface LayoutResult {
  hasCycle: boolean;
  cyclePath?: TaskId[];
  nodes: LayoutNode[];
  edges: LayoutEdge[];
  width: number;
  height: number;
  levelCount: number;
}

const DEFAULT_OPTIONS: Required<LayoutOptions> = {
  nodeWidth: 240,
  nodeHeight: 84,
  horizontalGap: 80,
  verticalGap: 28,
  paddingX: 40,
  paddingY: 40,
};

/**
 * Normalizes edges by combining explicit edges and node.dependencies,
 * deduplicating and removing dangling references.
 */
export function normalizeEdges(
  nodes: TaskNode[],
  rawEdges?: Array<{ from: TaskId; to: TaskId }>
): Array<{ from: TaskId; to: TaskId }> {
  const nodeIds = new Set(nodes.map((n) => n.id));
  const edgeSet = new Set<string>();
  const normalized: Array<{ from: TaskId; to: TaskId }> = [];

  const addEdge = (from: TaskId, to: TaskId) => {
    if (from === to) {
      return; // Self-loops handled separately in cycle detection if needed
    }
    if (!nodeIds.has(from) || !nodeIds.has(to)) {
      return;
    }
    const key = `${from}-->${to}`;
    if (!edgeSet.has(key)) {
      edgeSet.add(key);
      normalized.push({ from, to });
    }
  };

  // Add explicit edges
  if (rawEdges) {
    for (const e of rawEdges) {
      addEdge(e.from, e.to);
    }
  }

  // Add edges from node.dependencies (dependency -> task)
  for (const node of nodes) {
    if (node.dependencies && Array.isArray(node.dependencies)) {
      for (const depId of node.dependencies) {
        addEdge(depId, node.id);
      }
    }
  }

  return normalized;
}

/**
 * Detects cycles in directed graph using DFS 3-color coloring.
 * If cycle detected, extracts exact cycle path.
 */
export function detectCycle(
  nodes: TaskNode[],
  edges: Array<{ from: TaskId; to: TaskId }>
): CycleDetectionResult {
  const nodeMap = new Map<TaskId, TaskNode>(nodes.map((n) => [n.id, n]));
  const adj = new Map<TaskId, TaskId[]>();

  for (const n of nodes) {
    adj.set(n.id, []);
  }

  // Check for direct self-loops first
  for (const n of nodes) {
    if (n.dependencies && n.dependencies.includes(n.id)) {
      return { hasCycle: true, cyclePath: [n.id, n.id] };
    }
  }

  for (const edge of edges) {
    if (edge.from === edge.to) {
      return { hasCycle: true, cyclePath: [edge.from, edge.to] };
    }
    const list = adj.get(edge.from);
    if (list) {
      list.push(edge.to);
    }
  }

  // Sort node IDs deterministically
  const sortedIds = Array.from(nodeMap.keys()).sort();

  // 0 = unvisited, 1 = visiting (in recursion stack), 2 = visited
  const state = new Map<TaskId, number>();
  const parent = new Map<TaskId, TaskId>();

  for (const id of sortedIds) {
    state.set(id, 0);
  }

  let foundCycle: TaskId[] | undefined;

  function dfs(u: TaskId): boolean {
    state.set(u, 1);

    const neighbors = adj.get(u) || [];
    // Sort neighbors deterministically
    const sortedNeighbors = [...neighbors].sort();

    for (const v of sortedNeighbors) {
      const vState = state.get(v) ?? 0;
      if (vState === 1) {
        // Cycle detected: trace from u back to v
        const cycle: TaskId[] = [v];
        let curr = u;
        while (curr !== v && parent.has(curr)) {
          cycle.push(curr);
          curr = parent.get(curr)!;
        }
        cycle.push(v);
        cycle.reverse();
        foundCycle = cycle;
        return true;
      }

      if (vState === 0) {
        parent.set(v, u);
        if (dfs(v)) {
          return true;
        }
      }
    }

    state.set(u, 2);
    return false;
  }

  for (const id of sortedIds) {
    if (state.get(id) === 0) {
      if (dfs(id)) {
        return { hasCycle: true, cyclePath: foundCycle };
      }
    }
  }

  return { hasCycle: false };
}

/**
 * Computes deterministic topological levels and Cartesian coordinates for all nodes and edges.
 * Falls back gracefully to grid layout if a cycle is present.
 */
export function computeTopologicalLayout(
  nodes: TaskNode[],
  rawEdges?: Array<{ from: TaskId; to: TaskId }>,
  options?: LayoutOptions
): LayoutResult {
  const opts: Required<LayoutOptions> = { ...DEFAULT_OPTIONS, ...options };

  if (!nodes || nodes.length === 0) {
    return {
      hasCycle: false,
      nodes: [],
      edges: [],
      width: opts.paddingX * 2,
      height: opts.paddingY * 2,
      levelCount: 0,
    };
  }

  const normalizedEdges = normalizeEdges(nodes, rawEdges);
  const cycleResult = detectCycle(nodes, normalizedEdges);

  // If cycle exists, provide a fallback grid layout so user can still see nodes and cycle
  if (cycleResult.hasCycle) {
    return buildFallbackGridLayout(nodes, normalizedEdges, cycleResult.cyclePath, opts);
  }

  // 1. Calculate in-degrees and build adjacency lists
  const inDegree = new Map<TaskId, number>();
  const adj = new Map<TaskId, TaskId[]>();

  for (const n of nodes) {
    inDegree.set(n.id, 0);
    adj.set(n.id, []);
  }

  for (const edge of normalizedEdges) {
    const deg = inDegree.get(edge.to) || 0;
    inDegree.set(edge.to, deg + 1);
    const list = adj.get(edge.from);
    if (list) {
      list.push(edge.to);
    }
  }

  // 2. Longest-path level assignment via topological traversal
  const levels = new Map<TaskId, number>();
  for (const n of nodes) {
    levels.set(n.id, 0);
  }

  // Queue of nodes with in-degree 0
  const queue: TaskId[] = [];
  for (const [id, deg] of inDegree.entries()) {
    if (deg === 0) {
      queue.push(id);
    }
  }
  queue.sort();

  const workingInDegree = new Map<TaskId, number>(inDegree);

  while (queue.length > 0) {
    const u = queue.shift()!;
    const uLevel = levels.get(u) || 0;
    const neighbors = adj.get(u) || [];

    for (const v of neighbors) {
      const currentVLevel = levels.get(v) || 0;
      levels.set(v, Math.max(currentVLevel, uLevel + 1));

      const remainingDeg = (workingInDegree.get(v) || 1) - 1;
      workingInDegree.set(v, remainingDeg);
      if (remainingDeg === 0) {
        queue.push(v);
        queue.sort();
      }
    }
  }

  // 3. Group nodes into levels
  let maxLevel = 0;
  for (const lvl of levels.values()) {
    if (lvl > maxLevel) {
      maxLevel = lvl;
    }
  }

  const levelGroups: TaskNode[][] = Array.from({ length: maxLevel + 1 }, () => []);
  for (const node of nodes) {
    const lvl = levels.get(node.id) || 0;
    levelGroups[lvl].push(node);
  }

  // Status priority for deterministic ordering within level
  const statusPriority: Record<TaskStatus, number> = {
    running: 0,
    blocked: 1,
    ready: 2,
    pending: 3,
    succeeded: 4,
    failed: 5,
    cancelled: 6,
  };

  for (const group of levelGroups) {
    group.sort((a, b) => {
      const prioA = statusPriority[a.status] ?? 99;
      const prioB = statusPriority[b.status] ?? 99;
      if (prioA !== prioB) {
        return prioA - prioB;
      }
      return a.title.localeCompare(b.title) || a.id.localeCompare(b.id);
    });
  }

  // 4. Compute Cartesian coordinates
  const layoutNodes: LayoutNode[] = [];
  const nodePositionMap = new Map<TaskId, { x: number; y: number }>();
  let maxNodesInLevel = 0;

  for (let lvl = 0; lvl < levelGroups.length; lvl++) {
    const group = levelGroups[lvl];
    if (group.length > maxNodesInLevel) {
      maxNodesInLevel = group.length;
    }

    const x = opts.paddingX + lvl * (opts.nodeWidth + opts.horizontalGap);

    for (let i = 0; i < group.length; i++) {
      const node = group[i];
      const y = opts.paddingY + i * (opts.nodeHeight + opts.verticalGap);

      nodePositionMap.set(node.id, { x, y });

      layoutNodes.push({
        id: node.id,
        runId: node.runId,
        title: node.title,
        description: node.description,
        status: node.status,
        assignedAgentId: node.assignedAgentId,
        assignedAgentName: node.assignedAgentName,
        progress: node.progress || 0,
        error: node.error,
        level: lvl,
        x,
        y,
        width: opts.nodeWidth,
        height: opts.nodeHeight,
        dependencies: node.dependencies || [],
        createdAt: node.createdAt,
        updatedAt: node.updatedAt,
      });
    }
  }

  // 5. Generate Layout Edges with smooth cubic bezier paths
  const layoutEdges: LayoutEdge[] = [];
  for (const edge of normalizedEdges) {
    const fromPos = nodePositionMap.get(edge.from);
    const toPos = nodePositionMap.get(edge.to);

    if (fromPos && toPos) {
      const startX = fromPos.x + opts.nodeWidth;
      const startY = fromPos.y + opts.nodeHeight / 2;
      const endX = toPos.x;
      const endY = toPos.y + opts.nodeHeight / 2;

      const dx = Math.max(30, (endX - startX) * 0.45);
      const pathD = `M ${startX} ${startY} C ${startX + dx} ${startY}, ${endX - dx} ${endY}, ${endX} ${endY}`;

      layoutEdges.push({
        from: edge.from,
        to: edge.to,
        startX,
        startY,
        endX,
        endY,
        pathD,
      });
    }
  }

  const levelCount = levelGroups.length;
  const totalWidth =
    opts.paddingX * 2 + levelCount * opts.nodeWidth + Math.max(0, levelCount - 1) * opts.horizontalGap;
  const totalHeight =
    opts.paddingY * 2 +
    maxNodesInLevel * opts.nodeHeight +
    Math.max(0, maxNodesInLevel - 1) * opts.verticalGap;

  return {
    hasCycle: false,
    nodes: layoutNodes,
    edges: layoutEdges,
    width: Math.max(totalWidth, 600),
    height: Math.max(totalHeight, 400),
    levelCount,
  };
}

/**
 * Builds fallback grid layout when cycle is detected.
 */
function buildFallbackGridLayout(
  nodes: TaskNode[],
  edges: Array<{ from: TaskId; to: TaskId }>,
  cyclePath: TaskId[] | undefined,
  opts: Required<LayoutOptions>
): LayoutResult {
  const cols = Math.max(1, Math.ceil(Math.sqrt(nodes.length)));
  const layoutNodes: LayoutNode[] = [];
  const nodePositionMap = new Map<TaskId, { x: number; y: number }>();

  for (let i = 0; i < nodes.length; i++) {
    const node = nodes[i];
    const col = i % cols;
    const row = Math.floor(i / cols);
    const x = opts.paddingX + col * (opts.nodeWidth + opts.horizontalGap);
    const y = opts.paddingY + row * (opts.nodeHeight + opts.verticalGap);

    nodePositionMap.set(node.id, { x, y });

    layoutNodes.push({
      id: node.id,
      runId: node.runId,
      title: node.title,
      description: node.description,
      status: node.status,
      assignedAgentId: node.assignedAgentId,
      assignedAgentName: node.assignedAgentName,
      progress: node.progress || 0,
      error: node.error,
      level: 0,
      x,
      y,
      width: opts.nodeWidth,
      height: opts.nodeHeight,
      dependencies: node.dependencies || [],
      createdAt: node.createdAt,
      updatedAt: node.updatedAt,
    });
  }

  const layoutEdges: LayoutEdge[] = [];
  for (const edge of edges) {
    const fromPos = nodePositionMap.get(edge.from);
    const toPos = nodePositionMap.get(edge.to);
    if (fromPos && toPos) {
      const startX = fromPos.x + opts.nodeWidth / 2;
      const startY = fromPos.y + opts.nodeHeight / 2;
      const endX = toPos.x + opts.nodeWidth / 2;
      const endY = toPos.y + opts.nodeHeight / 2;
      const pathD = `M ${startX} ${startY} L ${endX} ${endY}`;
      layoutEdges.push({
        from: edge.from,
        to: edge.to,
        startX,
        startY,
        endX,
        endY,
        pathD,
      });
    }
  }

  const rows = Math.ceil(nodes.length / cols);
  const totalWidth =
    opts.paddingX * 2 + cols * opts.nodeWidth + Math.max(0, cols - 1) * opts.horizontalGap;
  const totalHeight =
    opts.paddingY * 2 + rows * opts.nodeHeight + Math.max(0, rows - 1) * opts.verticalGap;

  return {
    hasCycle: true,
    cyclePath,
    nodes: layoutNodes,
    edges: layoutEdges,
    width: Math.max(totalWidth, 600),
    height: Math.max(totalHeight, 400),
    levelCount: 1,
  };
}
