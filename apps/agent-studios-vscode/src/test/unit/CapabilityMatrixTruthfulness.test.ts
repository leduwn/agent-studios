/**
 * Agent Studios — Capability Matrix Truthfulness Unit Tests
 * Milestone M14 Final Freeze & Capability Synchronization
 *
 * Asserts strict truthfulness between CapabilityMatrix, BridgeManager, and DaemonConnectedBridge:
 * 1. task.submit is gated (enabled: false)
 * 2. approval.resolve is disabled in production read-only slice (enabled: false)
 * 3. task.cancel is disabled in production read-only slice (enabled: false)
 * 4. artifact.review is gated/unimplemented (enabled: false)
 * 5. DaemonConnectedBridge fails closed and does not fabricate approval or cancellation success
 * 6. BridgeManager reflects truthful capability enablement per active bridge mode
 */

import * as assert from 'assert';
import {
  CapabilityMatrix,
  CAPABILITY_REGISTRY,
} from '../../bridge/ipc/CapabilityMatrix';
import { BridgeManager } from '../../bridge/BridgeManager';
import { FixtureBridge } from '../../bridge/FixtureBridge';
import { DisconnectedBridge } from '../../bridge/DisconnectedBridge';
import { DaemonConnectedBridge } from '../../bridge/DaemonConnectedBridge';

describe('Capability Matrix Truthfulness & Synchronization', () => {
  describe('Authoritative Capability Declarations in CapabilityMatrix', () => {
    it('declares task.submit as TASK_SUBMISSION_READY and enabled: false in production', () => {
      const cap = CAPABILITY_REGISTRY['task.submit'];
      assert.ok(cap, 'task.submit capability descriptor must exist');
      assert.strictEqual(cap.tier, 'TASK_SUBMISSION_READY');
      assert.strictEqual(cap.enabled, false);
      assert.ok(cap.limitation?.includes('defaults'), 'Must document daemon-level default model limitation');
    });

    it('declares approval.resolve as APPROVAL_READY and enabled: false in production read-only slice', () => {
      const cap = CAPABILITY_REGISTRY['approval.resolve'];
      assert.ok(cap, 'approval.resolve capability descriptor must exist');
      assert.strictEqual(cap.tier, 'APPROVAL_READY');
      assert.strictEqual(cap.enabled, false);
      assert.ok(cap.limitation?.includes('disabled in M14'), 'Must document read-only vertical slice limitation');
    });

    it('declares task.cancel as CANCEL_READY and enabled: false in production read-only slice', () => {
      const cap = CAPABILITY_REGISTRY['task.cancel'];
      assert.ok(cap, 'task.cancel capability descriptor must exist');
      assert.strictEqual(cap.tier, 'CANCEL_READY');
      assert.strictEqual(cap.enabled, false);
      assert.ok(cap.limitation?.includes('disabled in M14'), 'Must document read-only vertical slice limitation');
    });

    it('declares artifact.review as ARTIFACT_REVIEW_READY and enabled: false', () => {
      const cap = CAPABILITY_REGISTRY['artifact.review'];
      assert.ok(cap, 'artifact.review capability descriptor must exist');
      assert.strictEqual(cap.tier, 'ARTIFACT_REVIEW_READY');
      assert.strictEqual(cap.enabled, false);
      assert.ok(cap.limitation?.includes('capture_changes'), 'Must document backend PR-M15-03 requirement');
    });

    it('declares read-only query capabilities as enabled: true', () => {
      assert.strictEqual(CapabilityMatrix.isCapabilityEnabled('daemon.health'), true);
      assert.strictEqual(CapabilityMatrix.isCapabilityEnabled('studio.list'), true);
      assert.strictEqual(CapabilityMatrix.isCapabilityEnabled('provider.list'), true);
      assert.strictEqual(CapabilityMatrix.isCapabilityEnabled('event.stream'), true);
    });
  });

  describe('BridgeManager Truthful Capability Integration', () => {
    it('reports all mutating capabilities as disabled when bridge is disconnected', () => {
      const disconnectedBridge = new DisconnectedBridge();
      const manager = new BridgeManager(disconnectedBridge);

      assert.strictEqual(manager.isCapabilityEnabled('approval.resolve'), false);
      assert.strictEqual(manager.isCapabilityEnabled('task.cancel'), false);
      assert.strictEqual(manager.isCapabilityEnabled('task.submit'), false);
      assert.strictEqual(manager.isCapabilityEnabled('artifact.review'), false);

      manager.dispose();
    });

    it('reports simulated capabilities when in developer FixtureBridge preview mode', () => {
      const fixtureBridge = new FixtureBridge();
      const manager = new BridgeManager(fixtureBridge);

      assert.strictEqual(manager.isPreviewMode, true);
      assert.strictEqual(manager.isCapabilityEnabled('approval.resolve'), true);
      assert.strictEqual(manager.isCapabilityEnabled('task.cancel'), true);
      assert.strictEqual(manager.isCapabilityEnabled('artifact.review'), false);

      const approvalDesc = manager.getCapabilityDescriptor('approval.resolve');
      assert.ok(approvalDesc);
      assert.strictEqual(approvalDesc.enabled, true);

      manager.dispose();
    });

    it('reports strict production capabilities when active bridge is DaemonConnectedBridge', () => {
      const daemonBridge = new DaemonConnectedBridge();
      const manager = new BridgeManager(daemonBridge);

      assert.strictEqual(manager.isDaemonMode, true);
      // Even if connected, mutating operations are disabled in read-only slice
      assert.strictEqual(manager.isCapabilityEnabled('approval.resolve'), false);
      assert.strictEqual(manager.isCapabilityEnabled('task.cancel'), false);
      assert.strictEqual(manager.isCapabilityEnabled('task.submit'), false);
      assert.strictEqual(manager.isCapabilityEnabled('artifact.review'), false);

      manager.dispose();
    });
  });

  describe('DaemonConnectedBridge Truthful Fail-Closed Behavior', () => {
    it('requestApprovalDecision fails closed and returns applied: false without mutating daemon', async () => {
      const bridge = new DaemonConnectedBridge();
      const result = await bridge.requestApprovalDecision('appr-01', 'approved');

      assert.strictEqual(result.applied, false);
      assert.strictEqual(result.decision, 'approved');
      assert.ok(result.message?.includes('disabled in M14-IPC-03'));
      bridge.dispose();
    });

    it('cancelCurrentRun rejects with disabled error instead of fabricating cancellation', async () => {
      const bridge = new DaemonConnectedBridge();
      await assert.rejects(
        async () => {
          await bridge.cancelCurrentRun();
        },
        (err: Error) => {
          return err.message.includes('disabled in M14-IPC-03');
        }
      );
      bridge.dispose();
    });
  });
});
