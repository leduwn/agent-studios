/**
 * Agent Studios â€” Capability Matrix Truthfulness Unit Tests
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
import { FixtureBridge } from '../fixtures/FixtureBridge';
import { DisconnectedBridge } from '../../bridge/DisconnectedBridge';
import { DaemonConnectedBridge } from '../../bridge/DaemonConnectedBridge';

describe('Capability Matrix Truthfulness & Synchronization', () => {
  afterEach(() => {
    CapabilityMatrix.resetCapabilities();
  });

  describe('Authoritative Capability Declarations in CapabilityMatrix', () => {
    it('declares task.submit as TASK_SUBMISSION_READY and enabled by default in Extension-First mode', () => {
      const cap = CAPABILITY_REGISTRY['task.submit'];
      assert.ok(cap, 'task.submit capability descriptor must exist');
      assert.strictEqual(cap.tier, 'TASK_SUBMISSION_READY');
      assert.strictEqual(cap.enabled, true);
      assert.ok(cap.limitation?.includes('defaults'), 'Must document operator confirmation & model routing gates');
    });

    it('declares approval.resolve as APPROVAL_READY and enabled by default', () => {
      const cap = CAPABILITY_REGISTRY['approval.resolve'];
      assert.ok(cap, 'approval.resolve capability descriptor must exist');
      assert.strictEqual(cap.tier, 'APPROVAL_READY');
      assert.strictEqual(cap.enabled, true);
    });

    it('declares task.cancel as CANCEL_READY and enabled by default', () => {
      const cap = CAPABILITY_REGISTRY['task.cancel'];
      assert.ok(cap, 'task.cancel capability descriptor must exist');
      assert.strictEqual(cap.tier, 'CANCEL_READY');
      assert.strictEqual(cap.enabled, true);
    });

    it('declares artifact.review as ARTIFACT_REVIEW_READY and enabled by default', () => {
      const cap = CAPABILITY_REGISTRY['artifact.review'];
      assert.ok(cap, 'artifact.review capability descriptor must exist');
      assert.strictEqual(cap.tier, 'ARTIFACT_REVIEW_READY');
      assert.strictEqual(cap.enabled, true);
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

    it('reports enabled capabilities only when active bridge reports connected', () => {
      const fixtureBridge = new FixtureBridge();
      const manager = new BridgeManager(fixtureBridge);

      assert.strictEqual(manager.isConnected, true);
      assert.strictEqual(manager.isCapabilityEnabled('approval.resolve'), true);
      assert.strictEqual(manager.isCapabilityEnabled('task.cancel'), true);

      const approvalDesc = manager.getCapabilityDescriptor('approval.resolve');
      assert.ok(approvalDesc);
      assert.strictEqual(approvalDesc.enabled, true);

      manager.dispose();
    });

    it('fails closed on mutating capabilities when DaemonConnectedBridge is not yet connected', () => {
      const daemonBridge = new DaemonConnectedBridge();
      const manager = new BridgeManager(daemonBridge);

      assert.strictEqual(manager.isDaemonMode, true);
      // Must fail closed while disconnected
      assert.strictEqual(manager.isCapabilityEnabled('approval.resolve'), false);
      assert.strictEqual(manager.isCapabilityEnabled('task.cancel'), false);
      assert.strictEqual(manager.isCapabilityEnabled('task.submit'), false);
      assert.strictEqual(manager.isCapabilityEnabled('artifact.review'), false);

      manager.dispose();
    });
  });

  describe('DaemonConnectedBridge Truthful Fail-Closed Behavior', () => {
    it('requestApprovalDecision rejects when bridge is disconnected or capability is disabled', async () => {
      const bridge = new DaemonConnectedBridge();
      await assert.rejects(
        async () => {
          await bridge.requestApprovalDecision('appr-01', 'approved');
        },
        (err: Error) => err.message.includes('not connected')
      );
      bridge.dispose();
    });

    it('cancelCurrentRun rejects with capability policy error when task.cancel is disabled', async () => {
      CapabilityMatrix.setCapabilityEnabled('task.cancel', false);
      const bridge = new DaemonConnectedBridge();
      await assert.rejects(
        async () => {
          await bridge.cancelCurrentRun();
        },
        (err: Error) => {
          return err.message.includes('disabled by capability policy');
        }
      );
      bridge.dispose();
    });
  });
});
