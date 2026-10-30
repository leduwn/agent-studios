import * as assert from 'assert';
import { TaskNode } from '../../bridge/types';
import {
  computeTopologicalLayout,
  detectCycle,
  normalizeEdges,
} from '../../webviews/graphLayout';

describe('TaskGraphLayout Pure Unit Tests', () => {
  const baseTask = (id: string, dependencies: string[] = []): TaskNode => ({
    id,
    runId: 'run-1',
    title: `Task ${id}`,
    description: `Description for ${id}`,
    status: 'running',
    assignedAgentId: 'agent-1',
    assignedAgentName: 'Agent One',
    dependencies,
    progress: 50,
    createdAt: 1000,
    updatedAt: 2000,
  });

  it('DAG layout places nodes without dependencies at level 0', () => {
    const nodes: TaskNode[] = [
      baseTask('task-a', []),
      baseTask('task-b', []),
      baseTask('task-c', []),
    ];

    const layout = computeTopologicalLayout(nodes);

    assert.strictEqual(layout.hasCycle, false);
    assert.strictEqual(layout.levelCount, 1);

    for (const node of layout.nodes) {
      assert.strictEqual(node.level, 0, `Node ${node.id} should be placed at level 0`);
      assert.strictEqual(node.x, 40, `Node ${node.id} x should match initial padding`);
    }
  });

  it('DAG layout places dependent nodes at deeper levels', () => {
    // Pipeline: root -> step1 -> step2 -> step3
    const nodes: TaskNode[] = [
      baseTask('root', []),
      baseTask('step1', ['root']),
      baseTask('step2', ['step1']),
      baseTask('step3', ['step2']),
    ];

    const layout = computeTopologicalLayout(nodes);

    assert.strictEqual(layout.hasCycle, false);
    assert.strictEqual(layout.levelCount, 4);

    const rootNode = layout.nodes.find((n) => n.id === 'root');
    const step1Node = layout.nodes.find((n) => n.id === 'step1');
    const step2Node = layout.nodes.find((n) => n.id === 'step2');
    const step3Node = layout.nodes.find((n) => n.id === 'step3');

    assert.ok(rootNode && step1Node && step2Node && step3Node);
    assert.strictEqual(rootNode.level, 0);
    assert.strictEqual(step1Node.level, 1);
    assert.strictEqual(step2Node.level, 2);
    assert.strictEqual(step3Node.level, 3);

    assert.ok(step1Node.x > rootNode.x);
    assert.ok(step2Node.x > step1Node.x);
    assert.ok(step3Node.x > step2Node.x);
  });

  it('Cycle detection detects cyclic graph and reports cycle', () => {
    // Cyclic dependency: A -> B -> C -> A
    const cyclicNodes: TaskNode[] = [
      baseTask('A', ['C']),
      baseTask('B', ['A']),
      baseTask('C', ['B']),
    ];

    const edges = normalizeEdges(cyclicNodes);
    const detection = detectCycle(cyclicNodes, edges);

    assert.strictEqual(detection.hasCycle, true);
    assert.ok(detection.cyclePath !== undefined);
    assert.ok(detection.cyclePath.length >= 3);
    // First and last node in cycle path should form the loop
    assert.strictEqual(detection.cyclePath[0], detection.cyclePath[detection.cyclePath.length - 1]);

    // Layout should also catch it and fall back gracefully
    const layout = computeTopologicalLayout(cyclicNodes);
    assert.strictEqual(layout.hasCycle, true);
    assert.ok(layout.cyclePath !== undefined);
  });

  it('Acyclic graph passes validation', () => {
    // Diamond DAG: root -> branchA, branchB -> merge
    const diamondNodes: TaskNode[] = [
      baseTask('root', []),
      baseTask('branchA', ['root']),
      baseTask('branchB', ['root']),
      baseTask('merge', ['branchA', 'branchB']),
    ];

    const edges = normalizeEdges(diamondNodes);
    const detection = detectCycle(diamondNodes, edges);

    assert.strictEqual(detection.hasCycle, false);
    assert.strictEqual(detection.cyclePath, undefined);

    const layout = computeTopologicalLayout(diamondNodes);
    assert.strictEqual(layout.hasCycle, false);
    assert.strictEqual(layout.levelCount, 3);

    const rootNode = layout.nodes.find((n) => n.id === 'root');
    const branchANode = layout.nodes.find((n) => n.id === 'branchA');
    const branchBNode = layout.nodes.find((n) => n.id === 'branchB');
    const mergeNode = layout.nodes.find((n) => n.id === 'merge');

    assert.strictEqual(rootNode?.level, 0);
    assert.strictEqual(branchANode?.level, 1);
    assert.strictEqual(branchBNode?.level, 1);
    assert.strictEqual(mergeNode?.level, 2);
  });
});
