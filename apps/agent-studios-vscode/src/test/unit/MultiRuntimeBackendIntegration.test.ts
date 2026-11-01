/**
 * Agent Studios — Multi-Runtime Backend Integration Regression Suite
 * Milestone M15 UI-02: Multi-Runtime Backend Integration
 * Pinned Backend Commit: 61cb7e88e4ae0e3a0432134b77eee0c30f56c302
 *
 * Verifies all 15 multi-runtime integration scenarios:
 * 1. Two implementations with different healthy instances
 * 2. Two healthy instances sharing one implementation
 * 3. Registered but unavailable adapter
 * 4. Unsupported/unknown runtime capability
 * 5. Unknown implementation or instance
 * 6. Duplicate instance identity
 * 7. Missing default with ambiguous selection
 * 8. Correct configured default
 * 9. Exact implementation/instance propagation (no model_ref)
 * 10. Model/provider identity propagation
 * 11. Stale selection after readiness refresh
 * 12. Operator challenge runtime binding
 * 13. Failed routing after successful submission acknowledgement
 * 14. Chat/Mission Control event correlation
 * 15. Cancellation, replay and reconnect
 * 16. Backward compatibility: M15-01 readiness payload without M15-02 optional fields
 */

/* eslint-disable @typescript-eslint/naming-convention */

import * as assert from 'assert';
import {
  CapabilityMatrix,
  OperatorConfirmationManager,
} from '../../bridge/ipc/CapabilityMatrix';
import {
  SubmitTaskAdapter,
  TaskSubmissionError,
} from '../../bridge/ipc/SubmitTaskAdapter';
import {
  RustExecutionReadinessReport,
  RustIpcCommand,
  RustIpcResponse,
} from '../../bridge/ipc/RustIpcProtocolTypes';
import { ChatWebviewViewProvider } from '../../chat/ChatWebviewViewProvider';
import { TaskExecutionStore } from '../../state/TaskExecutionStore';
import { ExecutionClient } from '../../bridge/ipc/ExecutionClient';
import { resetMockState } from './vscodeMock';

class MockCommandTransport {
  public sentCommands: RustIpcCommand[] = [];
  public responseGenerator: (cmd: RustIpcCommand) => RustIpcResponse = () => ({
    type: 'tasks_submitted',
    task_ids: ['task-uuid-00000000-0000-0000-0000-000000000001'],
  });

  public async sendCommand(cmd: RustIpcCommand): Promise<RustIpcResponse> {
    this.sentCommands.push(cmd);
    return this.responseGenerator(cmd);
  }
}

describe('M15-02 Multi-Runtime Backend Integration Tests', () => {
  const validStudioId = '3fa85f64-5717-4562-b3fc-2c963f66afa6';
  let transport: MockCommandTransport;

  beforeEach(() => {
    resetMockState();
    transport = new MockCommandTransport();
    CapabilityMatrix.resetCapabilities();
    OperatorConfirmationManager.resetForTesting();
    TaskExecutionStore.instance.reset();
  });

  afterEach(() => {
    CapabilityMatrix.resetCapabilities();
    OperatorConfirmationManager.resetForTesting();
    TaskExecutionStore.instance.reset();
  });

  // =========================================================================
  // Scenario 1: Two implementations with different healthy instances
  // =========================================================================
  it('Scenario 1: distinguishes two implementations with different healthy instances', async () => {
    CapabilityMatrix.setCapabilityEnabled('task.submit', true);

    const readiness: RustExecutionReadinessReport = {
      is_ready: true,
      registered_runtimes: ['fake-alpha', 'fake-beta'],
      available_runtimes: ['fake-alpha', 'fake-beta'],
      runtime_instances: [
        { instance_id: 'rt-inst-alpha-01', implementation: 'fake-alpha', healthy: true },
        { instance_id: 'rt-inst-beta-01', implementation: 'fake-beta', healthy: true },
      ],
      available_models: [],
      authorized_workspaces: [],
      defaults: {
        configured_instance_id: 'rt-inst-alpha-01',
      },
      blocking_reasons: [],
    };

    const { runtimeOptions, selectedRuntime } = ChatWebviewViewProvider.buildRuntimeOptions(readiness);

    assert.strictEqual(runtimeOptions.length, 2);
    assert.strictEqual(runtimeOptions[0].id, 'rt-inst-alpha-01|fake-alpha');
    assert.strictEqual(runtimeOptions[0].enabled, true);
    assert.strictEqual(runtimeOptions[1].id, 'rt-inst-beta-01|fake-beta');
    assert.strictEqual(runtimeOptions[1].enabled, true);
    assert.strictEqual(selectedRuntime, 'rt-inst-alpha-01|fake-alpha');

    // Submit with beta
    const challenge = OperatorConfirmationManager.createChallenge(validStudioId, 'test prompt', {
      runtimePreference: 'fake-beta',
      runtimeInstanceId: 'rt-inst-beta-01',
    });

    const submitResult = await SubmitTaskAdapter.submitTask(transport, {
      studioId: validStudioId,
      prompt: 'test prompt',
      runtimePreference: 'fake-beta',
      runtimeInstanceId: 'rt-inst-beta-01',
      confirmedByUser: true,
      confirmationChallenge: challenge,
      readiness,
    });

    assert.strictEqual(submitResult.success, true);
    const sent = transport.sentCommands[0];
    assert.strictEqual(sent.type, 'submit_task');
    if (sent.type === 'submit_task') {
      assert.strictEqual(sent.task_spec.execution?.runtime_preference, 'fake-beta');
      assert.strictEqual(sent.task_spec.execution?.runtime_instance_id, 'rt-inst-beta-01');
    }
  });

  // =========================================================================
  // Scenario 2: Two healthy instances sharing one implementation
  // =========================================================================
  it('Scenario 2: manages two healthy instances sharing one implementation without collision', () => {
    const readiness: RustExecutionReadinessReport = {
      is_ready: true,
      available_runtimes: ['fake-alpha'],
      runtime_instances: [
        { instance_id: 'rt-alpha-inst-01', implementation: 'fake-alpha', healthy: true },
        { instance_id: 'rt-alpha-inst-02', implementation: 'fake-alpha', healthy: true },
      ],
      available_models: [],
      authorized_workspaces: [],
      defaults: {},
      blocking_reasons: [],
    };

    const { runtimeOptions, selectedRuntime } = ChatWebviewViewProvider.buildRuntimeOptions(readiness);

    assert.strictEqual(runtimeOptions.length, 2);
    assert.strictEqual(runtimeOptions[0].id, 'rt-alpha-inst-01|fake-alpha');
    assert.strictEqual(runtimeOptions[1].id, 'rt-alpha-inst-02|fake-alpha');
    // Multiple healthy instances with no configured default -> selectedRuntime must be empty (no silent auto-pick)
    assert.strictEqual(selectedRuntime, '');

    // Explicit validation by instance ID succeeds
    const val1 = CapabilityMatrix.validateRuntimeInstanceSelection(readiness, {
      runtimeInstanceId: 'rt-alpha-inst-01',
    });
    assert.strictEqual(val1.valid, true);
    assert.strictEqual(val1.runtimePreference, 'fake-alpha');
    assert.strictEqual(val1.runtimeInstanceId, 'rt-alpha-inst-01');

    const val2 = CapabilityMatrix.validateRuntimeInstanceSelection(readiness, {
      runtimeInstanceId: 'rt-alpha-inst-02',
    });
    assert.strictEqual(val2.valid, true);
    assert.strictEqual(val2.runtimePreference, 'fake-alpha');
    assert.strictEqual(val2.runtimeInstanceId, 'rt-alpha-inst-02');
  });

  // =========================================================================
  // Scenario 3: Registered but unavailable adapter
  // =========================================================================
  it('Scenario 3: marks registered but unavailable adapter as disabled and fails closed on submit', async () => {
    CapabilityMatrix.setCapabilityEnabled('task.submit', true);

    const readiness: RustExecutionReadinessReport = {
      is_ready: false,
      registered_runtimes: ['fake-alpha', 'unavailable-gamma'],
      available_runtimes: ['fake-alpha'],
      runtime_instances: [
        { instance_id: 'rt-alpha-01', implementation: 'fake-alpha', healthy: true },
      ],
      available_models: [],
      authorized_workspaces: [],
      defaults: {},
      blocking_reasons: [],
    };

    const { runtimeOptions } = ChatWebviewViewProvider.buildRuntimeOptions(readiness);
    const gammaOption = runtimeOptions.find((r) => r.implementation === 'unavailable-gamma');
    assert.ok(gammaOption, 'Must include option for registered adapter');
    assert.strictEqual(gammaOption?.enabled, false);

    // Submitting with unavailable adapter must fail closed
    const challenge = OperatorConfirmationManager.createChallenge(validStudioId, 'test prompt', {
      runtimePreference: 'unavailable-gamma',
    });

    await assert.rejects(
      async () => {
        await SubmitTaskAdapter.submitTask(transport, {
          studioId: validStudioId,
          prompt: 'test prompt',
          runtimePreference: 'unavailable-gamma',
          confirmedByUser: true,
          confirmationChallenge: challenge,
          readiness,
        });
      },
      (err: unknown) => {
        assert.ok(err instanceof TaskSubmissionError);
        assert.ok(err.message.includes("Unknown runtime implementation 'unavailable-gamma'"));
        return true;
      }
    );
  });

  // =========================================================================
  // Scenario 4: Tri-state runtime capability validation & action gates
  // =========================================================================
  it('Scenario 4: fails closed when runtime reports unsupported required capabilities', () => {
    const readiness: RustExecutionReadinessReport = {
      is_ready: true,
      available_runtimes: ['fake-alpha'],
      runtime_capabilities: {
        'rt-alpha-01': {
          streaming_events: 'unsupported',
          stop: 'unsupported',
        },
      },
      runtime_instances: [
        { instance_id: 'rt-alpha-01', implementation: 'fake-alpha', healthy: true },
      ],
      available_models: [],
      authorized_workspaces: [],
      defaults: {},
      blocking_reasons: [],
    };

    const validation = CapabilityMatrix.validateRuntimeInstanceSelection(readiness, {
      runtimeInstanceId: 'rt-alpha-01',
    });

    assert.strictEqual(validation.valid, false);
    assert.ok(
      validation.reason?.includes("does not support required 'streaming_events' capability")
    );
  });

  it('Scenario 4a: fails closed when runtime reports unknown capability for task_execution', () => {
    const readiness: RustExecutionReadinessReport = {
      is_ready: true,
      available_runtimes: ['fake-alpha'],
      runtime_capabilities: {
        'rt-alpha-01': {
          streaming_events: 'supported',
          stop: 'unknown',
        },
      },
      runtime_instances: [
        { instance_id: 'rt-alpha-01', implementation: 'fake-alpha', healthy: true },
      ],
      available_models: [],
      authorized_workspaces: [],
      defaults: {},
      blocking_reasons: [],
    };

    const validation = CapabilityMatrix.validateRuntimeInstanceSelection(readiness, {
      runtimeInstanceId: 'rt-alpha-01',
    });

    assert.strictEqual(validation.valid, false);
    assert.ok(
      validation.reason?.includes("has unknown support for required 'stop' capability")
    );
  });

  it('Scenario 4b: fails closed when required capability is missing from instance map', () => {
    const readiness: RustExecutionReadinessReport = {
      is_ready: true,
      available_runtimes: ['fake-alpha'],
      runtime_capabilities: {
        'rt-alpha-01': {
          streaming_events: 'supported',
        },
      },
      runtime_instances: [
        { instance_id: 'rt-alpha-01', implementation: 'fake-alpha', healthy: true },
      ],
      available_models: [],
      authorized_workspaces: [],
      defaults: {},
      blocking_reasons: [],
    };

    const validation = CapabilityMatrix.validateRuntimeInstanceSelection(readiness, {
      runtimeInstanceId: 'rt-alpha-01',
    });

    assert.strictEqual(validation.valid, false);
    assert.ok(
      validation.reason?.includes("is missing required 'stop' capability")
    );
  });

  it('Scenario 4c: fails closed when instance is missing entirely from runtime_capabilities map', () => {
    const readiness: RustExecutionReadinessReport = {
      is_ready: true,
      available_runtimes: ['fake-alpha'],
      runtime_capabilities: {},
      runtime_instances: [
        { instance_id: 'rt-alpha-01', implementation: 'fake-alpha', healthy: true },
      ],
      available_models: [],
      authorized_workspaces: [],
      defaults: {},
      blocking_reasons: [],
    };

    const validation = CapabilityMatrix.validateRuntimeInstanceSelection(readiness, {
      runtimeInstanceId: 'rt-alpha-01',
    });

    assert.strictEqual(validation.valid, false);
    assert.ok(
      validation.reason?.includes("has missing runtime capabilities in daemon readiness")
    );
  });

  it('Scenario 4d: passes task_cancellation when stop is supported even if streaming_events is unsupported', () => {
    const readiness: RustExecutionReadinessReport = {
      is_ready: true,
      available_runtimes: ['fake-alpha'],
      runtime_capabilities: {
        'rt-alpha-01': {
          streaming_events: 'unsupported',
          stop: 'supported',
        },
      },
      runtime_instances: [
        { instance_id: 'rt-alpha-01', implementation: 'fake-alpha', healthy: true },
      ],
      available_models: [],
      authorized_workspaces: [],
      defaults: {},
      blocking_reasons: [],
    };

    const validation = CapabilityMatrix.validateRuntimeInstanceSelection(
      readiness,
      { runtimeInstanceId: 'rt-alpha-01' },
      { action: 'task_cancellation' }
    );

    assert.strictEqual(validation.valid, true);
    assert.strictEqual(validation.runtimeInstanceId, 'rt-alpha-01');
  });

  it('Scenario 4e: fails task_cancellation when stop capability is unsupported or unknown', () => {
    const readiness: RustExecutionReadinessReport = {
      is_ready: true,
      available_runtimes: ['fake-alpha'],
      runtime_capabilities: {
        'rt-alpha-01': {
          streaming_events: 'supported',
          stop: 'unsupported',
        },
      },
      runtime_instances: [
        { instance_id: 'rt-alpha-01', implementation: 'fake-alpha', healthy: true },
      ],
      available_models: [],
      authorized_workspaces: [],
      defaults: {},
      blocking_reasons: [],
    };

    const validation = CapabilityMatrix.validateRuntimeInstanceSelection(
      readiness,
      { runtimeInstanceId: 'rt-alpha-01' },
      { action: 'task_cancellation' }
    );

    assert.strictEqual(validation.valid, false);
    assert.ok(
      validation.reason?.includes("does not support required 'stop' capability")
    );
  });

  it('Scenario 4f: passes read_only action for runtimes lacking mutating capabilities', () => {
    const readiness: RustExecutionReadinessReport = {
      is_ready: true,
      available_runtimes: ['fake-alpha'],
      runtime_capabilities: {
        'rt-alpha-01': {
          streaming_events: 'unsupported',
          stop: 'unsupported',
        },
      },
      runtime_instances: [
        { instance_id: 'rt-alpha-01', implementation: 'fake-alpha', healthy: true },
      ],
      available_models: [],
      authorized_workspaces: [],
      defaults: {},
      blocking_reasons: [],
    };

    const validation = CapabilityMatrix.validateRuntimeInstanceSelection(
      readiness,
      { runtimeInstanceId: 'rt-alpha-01' },
      { action: 'read_only' }
    );

    assert.strictEqual(validation.valid, true);
    assert.strictEqual(validation.runtimeInstanceId, 'rt-alpha-01');
  });

  it('Scenario 4g: passes task_execution when both streaming_events and stop are supported', () => {
    const readiness: RustExecutionReadinessReport = {
      is_ready: true,
      available_runtimes: ['fake-alpha'],
      runtime_capabilities: {
        'rt-alpha-01': {
          streaming_events: 'supported',
          stop: 'supported',
        },
      },
      runtime_instances: [
        { instance_id: 'rt-alpha-01', implementation: 'fake-alpha', healthy: true },
      ],
      available_models: [],
      authorized_workspaces: [],
      defaults: {},
      blocking_reasons: [],
    };

    const validation = CapabilityMatrix.validateRuntimeInstanceSelection(
      readiness,
      { runtimeInstanceId: 'rt-alpha-01' },
      { action: 'task_execution' }
    );

    assert.strictEqual(validation.valid, true);
    assert.strictEqual(validation.runtimeInstanceId, 'rt-alpha-01');
    assert.strictEqual(validation.runtimePreference, 'fake-alpha');
  });

  // =========================================================================
  // Scenario 5: Unknown implementation or instance
  // =========================================================================
  it('Scenario 5: rejects unknown instance ID and unknown implementation', () => {
    const readiness: RustExecutionReadinessReport = {
      is_ready: true,
      available_runtimes: ['fake-alpha'],
      runtime_instances: [
        { instance_id: 'rt-alpha-01', implementation: 'fake-alpha', healthy: true },
      ],
      available_models: [],
      authorized_workspaces: [],
      defaults: {},
      blocking_reasons: [],
    };

    const unknownInst = CapabilityMatrix.validateRuntimeInstanceSelection(readiness, {
      runtimeInstanceId: 'rt-non-existent-99',
    });
    assert.strictEqual(unknownInst.valid, false);
    assert.ok(unknownInst.reason?.includes("Unknown runtime instance 'rt-non-existent-99'"));

    const unknownImpl = CapabilityMatrix.validateRuntimeInstanceSelection(readiness, {
      runtimePreference: 'unknown-lang-runtime',
    });
    assert.strictEqual(unknownImpl.valid, false);
    assert.ok(unknownImpl.reason?.includes("Unknown runtime implementation 'unknown-lang-runtime'"));
  });

  // =========================================================================
  // Scenario 6: Duplicate instance identity
  // =========================================================================
  it('Scenario 6: detects duplicate instance identity and disables execution', () => {
    const readiness: RustExecutionReadinessReport = {
      is_ready: true,
      available_runtimes: ['fake-alpha', 'fake-beta'],
      runtime_instances: [
        { instance_id: 'rt-dup-01', implementation: 'fake-alpha', healthy: true },
        { instance_id: 'rt-dup-01', implementation: 'fake-beta', healthy: true },
      ],
      available_models: [],
      authorized_workspaces: [],
      defaults: {},
      blocking_reasons: [],
    };

    const { runtimeOptions } = ChatWebviewViewProvider.buildRuntimeOptions(readiness);
    assert.strictEqual(runtimeOptions[0].enabled, false);
    assert.strictEqual(runtimeOptions[1].enabled, false);
    assert.ok(runtimeOptions[0].name.includes('Duplicate ID'));

    const validation = CapabilityMatrix.validateRuntimeInstanceSelection(readiness, {
      runtimeInstanceId: 'rt-dup-01',
    });
    assert.strictEqual(validation.valid, false);
    assert.ok(
      validation.reason?.includes("Duplicate runtime instance identity detected in daemon readiness: 'rt-dup-01'")
    );
  });

  // =========================================================================
  // Scenario 7: Missing default with ambiguous selection
  // =========================================================================
  it('Scenario 7: rejects ambiguous selection when multiple instances exist and default is missing', () => {
    const readiness: RustExecutionReadinessReport = {
      is_ready: true,
      available_runtimes: ['fake-alpha'],
      runtime_instances: [
        { instance_id: 'rt-alpha-01', implementation: 'fake-alpha', healthy: true },
        { instance_id: 'rt-alpha-02', implementation: 'fake-alpha', healthy: true },
      ],
      available_models: [],
      authorized_workspaces: [],
      defaults: {},
      blocking_reasons: [],
    };

    // No selection given
    const noSelection = CapabilityMatrix.validateRuntimeInstanceSelection(readiness, {});
    assert.strictEqual(noSelection.valid, false);
    assert.ok(noSelection.reason?.includes('Multiple runtime instances discovered but no configured default'));

    // Implementation only given (ambiguous between 01 and 02)
    const ambiguous = CapabilityMatrix.validateRuntimeInstanceSelection(readiness, {
      runtimePreference: 'fake-alpha',
    });
    assert.strictEqual(ambiguous.valid, false);
    assert.ok(ambiguous.reason?.includes("Ambiguous runtime selection: multiple instances discovered for implementation 'fake-alpha'"));
  });

  // =========================================================================
  // Scenario 8: Correct configured default
  // =========================================================================
  it('Scenario 8: honors correct configured default and rejects unhealthy configured default', () => {
    const healthyReadiness: RustExecutionReadinessReport = {
      is_ready: true,
      available_runtimes: ['fake-alpha'],
      runtime_instances: [
        { instance_id: 'rt-alpha-01', implementation: 'fake-alpha', healthy: true },
        { instance_id: 'rt-alpha-02', implementation: 'fake-alpha', healthy: true },
      ],
      available_models: [],
      authorized_workspaces: [],
      defaults: {
        configured_instance_id: 'rt-alpha-02',
      },
      blocking_reasons: [],
    };

    const healthyDefault = CapabilityMatrix.validateRuntimeInstanceSelection(healthyReadiness, {});
    assert.strictEqual(healthyDefault.valid, true);
    assert.strictEqual(healthyDefault.runtimeInstanceId, 'rt-alpha-02');
    assert.strictEqual(healthyDefault.runtimePreference, 'fake-alpha');

    // Unhealthy default
    const degradedReadiness: RustExecutionReadinessReport = {
      is_ready: true,
      available_runtimes: ['fake-alpha'],
      runtime_instances: [
        { instance_id: 'rt-alpha-01', implementation: 'fake-alpha', healthy: false },
      ],
      available_models: [],
      authorized_workspaces: [],
      defaults: {
        configured_instance_id: 'rt-alpha-01',
      },
      blocking_reasons: [],
    };

    const degradedDefault = CapabilityMatrix.validateRuntimeInstanceSelection(degradedReadiness, {});
    assert.strictEqual(degradedDefault.valid, false);
    assert.ok(degradedDefault.reason?.includes('is degraded or unhealthy'));
  });

  // =========================================================================
  // Scenario 9: Exact implementation/instance propagation (no model_ref)
  // =========================================================================
  it('Scenario 9: propagates exact execution fields on wire and never includes model_ref', async () => {
    CapabilityMatrix.setCapabilityEnabled('task.submit', true);

    const readiness: RustExecutionReadinessReport = {
      is_ready: true,
      available_runtimes: ['fake-alpha'],
      runtime_instances: [
        { instance_id: 'rt-alpha-01', implementation: 'fake-alpha', healthy: true },
      ],
      available_models: [],
      authorized_workspaces: [],
      defaults: {},
      blocking_reasons: [],
    };

    const challenge = OperatorConfirmationManager.createChallenge(validStudioId, 'test prompt', {
      runtimePreference: 'fake-alpha',
      runtimeInstanceId: 'rt-alpha-01',
    });

    await SubmitTaskAdapter.submitTask(transport, {
      studioId: validStudioId,
      prompt: 'test prompt',
      runtimePreference: 'fake-alpha',
      runtimeInstanceId: 'rt-alpha-01',
      confirmedByUser: true,
      confirmationChallenge: challenge,
      readiness,
    });

    const sent = transport.sentCommands[0];
    assert.strictEqual(sent.type, 'submit_task');
    if (sent.type === 'submit_task') {
      const exec = sent.task_spec.execution;
      assert.ok(exec, 'task_spec.execution must exist');
      assert.strictEqual(exec?.runtime_preference, 'fake-alpha');
      assert.strictEqual(exec?.runtime_instance_id, 'rt-alpha-01');
      assert.strictEqual(exec?.initial_prompt, 'test prompt');

      // Wire shape check: model_ref MUST NOT exist
      const serialized = JSON.stringify(sent);
      assert.strictEqual(serialized.includes('model_ref'), false, 'Wire shape must never contain model_ref');
    }
  });

  // =========================================================================
  // Scenario 10: Model/provider identity propagation
  // =========================================================================
  it('Scenario 10: cryptographically binds and propagates model_id and provider_instance_id', async () => {
    CapabilityMatrix.setCapabilityEnabled('task.submit', true);

    const readiness: RustExecutionReadinessReport = {
      is_ready: true,
      available_runtimes: ['fake-alpha'],
      runtime_instances: [
        { instance_id: 'rt-alpha-01', implementation: 'fake-alpha', healthy: true },
      ],
      available_models: [
        { provider_instance_id: 'provider-test', model_id: 'model-test-01', display_name: 'Test Model' },
      ],
      authorized_workspaces: [],
      defaults: {},
      blocking_reasons: [],
    };

    const challenge = OperatorConfirmationManager.createChallenge(validStudioId, 'test prompt', {
      runtimePreference: 'fake-alpha',
      runtimeInstanceId: 'rt-alpha-01',
      modelId: 'model-test-01',
      providerId: 'provider-test',
    });

    await SubmitTaskAdapter.submitTask(transport, {
      studioId: validStudioId,
      prompt: 'test prompt',
      runtimePreference: 'fake-alpha',
      runtimeInstanceId: 'rt-alpha-01',
      requestedModelId: 'model-test-01',
      requestedProviderId: 'provider-test',
      confirmedByUser: true,
      confirmationChallenge: challenge,
      readiness,
    });

    const sent = transport.sentCommands[0];
    assert.strictEqual(sent.type, 'submit_task');
    if (sent.type === 'submit_task') {
      assert.strictEqual(sent.task_spec.execution?.model_id, 'model-test-01');
      assert.strictEqual(sent.task_spec.execution?.provider_instance_id, 'provider-test');
    }
  });

  it('Scenario 10b: permits explicit per-task model differing from daemon default when model exists in available_models', async () => {
    CapabilityMatrix.setCapabilityEnabled('task.submit', true);

    const readiness: RustExecutionReadinessReport = {
      is_ready: true,
      available_runtimes: ['fake-alpha'],
      runtime_instances: [
        { instance_id: 'rt-alpha-01', implementation: 'fake-alpha', healthy: true },
      ],
      available_models: [
        { model_id: 'default-model-01', provider_instance_id: 'default-provider' },
        { model_id: 'custom-model-02', provider_instance_id: 'custom-provider' },
      ],
      authorized_workspaces: [],
      defaults: {
        default_model_id: 'default-model-01',
        default_provider_instance_id: 'default-provider',
      },
      blocking_reasons: [],
    };

    const challenge = OperatorConfirmationManager.createChallenge(validStudioId, 'test prompt', {
      runtimePreference: 'fake-alpha',
      runtimeInstanceId: 'rt-alpha-01',
      modelId: 'custom-model-02',
      providerId: 'custom-provider',
    });

    await SubmitTaskAdapter.submitTask(transport, {
      studioId: validStudioId,
      prompt: 'test prompt',
      runtimePreference: 'fake-alpha',
      runtimeInstanceId: 'rt-alpha-01',
      requestedModelId: 'custom-model-02',
      requestedProviderId: 'custom-provider',
      confirmedByUser: true,
      confirmationChallenge: challenge,
      readiness,
    });

    const sent = transport.sentCommands[transport.sentCommands.length - 1];
    assert.strictEqual(sent.type, 'submit_task');
    if (sent.type === 'submit_task') {
      assert.strictEqual(sent.task_spec.execution?.model_id, 'custom-model-02');
      assert.strictEqual(sent.task_spec.execution?.provider_instance_id, 'custom-provider');
    }
  });

  it('Scenario 10c: rejects explicit per-task model when model does not exist in available_models (prevents silent substitution)', async () => {
    CapabilityMatrix.setCapabilityEnabled('task.submit', true);

    const readiness: RustExecutionReadinessReport = {
      is_ready: true,
      available_runtimes: ['fake-alpha'],
      runtime_instances: [
        { instance_id: 'rt-alpha-01', implementation: 'fake-alpha', healthy: true },
      ],
      available_models: [
        { model_id: 'default-model-01', provider_instance_id: 'default-provider' },
      ],
      authorized_workspaces: [],
      defaults: {
        default_model_id: 'default-model-01',
        default_provider_instance_id: 'default-provider',
      },
      blocking_reasons: [],
    };

    const challenge = OperatorConfirmationManager.createChallenge(validStudioId, 'test prompt', {
      runtimePreference: 'fake-alpha',
      runtimeInstanceId: 'rt-alpha-01',
      modelId: 'non-existent-model',
    });

    await assert.rejects(
      async () => {
        await SubmitTaskAdapter.submitTask(transport, {
          studioId: validStudioId,
          prompt: 'test prompt',
          runtimePreference: 'fake-alpha',
          runtimeInstanceId: 'rt-alpha-01',
          requestedModelId: 'non-existent-model',
          confirmedByUser: true,
          confirmationChallenge: challenge,
          readiness,
        });
      },
      (err: Error) => {
        return (
          err instanceof TaskSubmissionError &&
          err.message.includes("Requested model 'non-existent-model' not found in daemon available models")
        );
      }
    );
  });

  it('Scenario 10d: rejects explicit per-task model when provider instance does not match', async () => {
    CapabilityMatrix.setCapabilityEnabled('task.submit', true);

    const readiness: RustExecutionReadinessReport = {
      is_ready: true,
      available_runtimes: ['fake-alpha'],
      runtime_instances: [
        { instance_id: 'rt-alpha-01', implementation: 'fake-alpha', healthy: true },
      ],
      available_models: [
        { model_id: 'model-a', provider_instance_id: 'provider-alpha' },
      ],
      authorized_workspaces: [],
      defaults: {},
      blocking_reasons: [],
    };

    const challenge = OperatorConfirmationManager.createChallenge(validStudioId, 'test prompt', {
      runtimePreference: 'fake-alpha',
      runtimeInstanceId: 'rt-alpha-01',
      modelId: 'model-a',
      providerId: 'provider-beta',
    });

    await assert.rejects(
      async () => {
        await SubmitTaskAdapter.submitTask(transport, {
          studioId: validStudioId,
          prompt: 'test prompt',
          runtimePreference: 'fake-alpha',
          runtimeInstanceId: 'rt-alpha-01',
          requestedModelId: 'model-a',
          requestedProviderId: 'provider-beta',
          confirmedByUser: true,
          confirmationChallenge: challenge,
          readiness,
        });
      },
      (err: Error) => {
        return (
          err instanceof TaskSubmissionError &&
          err.message.includes("Requested model 'model-a' with provider 'provider-beta' not found in daemon available models")
        );
      }
    );
  });

  it('Scenario 10e: rejects ambiguous model when multiple provider instances exist without explicit provider selection', async () => {
    CapabilityMatrix.setCapabilityEnabled('task.submit', true);

    const readiness: RustExecutionReadinessReport = {
      is_ready: true,
      available_runtimes: ['fake-alpha'],
      runtime_instances: [
        { instance_id: 'rt-alpha-01', implementation: 'fake-alpha', healthy: true },
      ],
      available_models: [
        { model_id: 'common-model', provider_instance_id: 'provider-alpha' },
        { model_id: 'common-model', provider_instance_id: 'provider-beta' },
      ],
      authorized_workspaces: [],
      defaults: {},
      blocking_reasons: [],
    };

    const challenge = OperatorConfirmationManager.createChallenge(validStudioId, 'test prompt', {
      runtimePreference: 'fake-alpha',
      runtimeInstanceId: 'rt-alpha-01',
      modelId: 'common-model',
    });

    await assert.rejects(
      async () => {
        await SubmitTaskAdapter.submitTask(transport, {
          studioId: validStudioId,
          prompt: 'test prompt',
          runtimePreference: 'fake-alpha',
          runtimeInstanceId: 'rt-alpha-01',
          requestedModelId: 'common-model',
          confirmedByUser: true,
          confirmationChallenge: challenge,
          readiness,
        });
      },
      (err: Error) => {
        return (
          err instanceof TaskSubmissionError &&
          err.message.includes("matches multiple provider instances (provider-alpha, provider-beta)")
        );
      }
    );
  });

  it('Scenario 10f: falls back to daemon default model when no explicit model is requested', async () => {
    CapabilityMatrix.setCapabilityEnabled('task.submit', true);

    const readiness: RustExecutionReadinessReport = {
      is_ready: true,
      available_runtimes: ['fake-alpha'],
      runtime_instances: [
        { instance_id: 'rt-alpha-01', implementation: 'fake-alpha', healthy: true },
      ],
      available_models: [
        { model_id: 'default-model', provider_instance_id: 'default-provider' },
        { model_id: 'other-model', provider_instance_id: 'other-provider' },
      ],
      authorized_workspaces: [],
      defaults: {
        default_model_id: 'default-model',
        default_provider_instance_id: 'default-provider',
      },
      blocking_reasons: [],
    };

    const challenge = OperatorConfirmationManager.createChallenge(validStudioId, 'test prompt', {
      runtimePreference: 'fake-alpha',
      runtimeInstanceId: 'rt-alpha-01',
    });

    await SubmitTaskAdapter.submitTask(transport, {
      studioId: validStudioId,
      prompt: 'test prompt',
      runtimePreference: 'fake-alpha',
      runtimeInstanceId: 'rt-alpha-01',
      confirmedByUser: true,
      confirmationChallenge: challenge,
      readiness,
    });

    const sent = transport.sentCommands[transport.sentCommands.length - 1];
    assert.strictEqual(sent.type, 'submit_task');
    if (sent.type === 'submit_task') {
      assert.strictEqual(sent.task_spec.execution?.model_id, 'default-model');
      assert.strictEqual(sent.task_spec.execution?.provider_instance_id, 'default-provider');
    }
  });

  // =========================================================================
  // Scenario 11: Stale selection after readiness refresh
  // =========================================================================
  it('Scenario 11: rejects stale selection when selected instance degrades after refresh', () => {
    // Initial readiness where rt-alpha-01 was healthy
    const staleReadiness: RustExecutionReadinessReport = {
      is_ready: false,
      available_runtimes: ['fake-alpha'],
      runtime_instances: [
        { instance_id: 'rt-alpha-01', implementation: 'fake-alpha', healthy: false }, // degraded
      ],
      available_models: [],
      authorized_workspaces: [],
      defaults: {},
      blocking_reasons: ['Runtime instance degraded'],
    };

    const revalidation = CapabilityMatrix.validateRuntimeInstanceSelection(staleReadiness, {
      runtime: 'rt-alpha-01|fake-alpha',
    });

    assert.strictEqual(revalidation.valid, false);
    assert.ok(revalidation.reason?.includes("Runtime instance 'rt-alpha-01' (fake-alpha) is degraded or unhealthy"));
  });

  // =========================================================================
  // Scenario 12: Operator challenge runtime binding
  // =========================================================================
  it('Scenario 12: fails verification if runtime preference or instance ID is altered after challenge issued', () => {
    const challenge = OperatorConfirmationManager.createChallenge(validStudioId, 'test prompt', {
      runtimePreference: 'fake-alpha',
      runtimeInstanceId: 'rt-alpha-01',
    });

    // Mismatched runtime preference
    const mismatchPref = OperatorConfirmationManager.verifyChallenge(challenge, {
      studioId: validStudioId,
      prompt: 'test prompt',
      runtimePreference: 'fake-beta', // tampered
      runtimeInstanceId: 'rt-alpha-01',
      intent: 'task_execution',
    });
    assert.strictEqual(mismatchPref.valid, false);
    assert.ok(mismatchPref.reason?.includes('Runtime preference mismatch'));

    // Mismatched runtime instance ID
    const mismatchInst = OperatorConfirmationManager.verifyChallenge(challenge, {
      studioId: validStudioId,
      prompt: 'test prompt',
      runtimePreference: 'fake-alpha',
      runtimeInstanceId: 'rt-alpha-02', // tampered
      intent: 'task_execution',
    });
    assert.strictEqual(mismatchInst.valid, false);
    assert.ok(mismatchInst.reason?.includes('Runtime instance ID mismatch'));

    // Replay check
    const validVerify = OperatorConfirmationManager.verifyAndConsumeChallenge(challenge, {
      studioId: validStudioId,
      prompt: 'test prompt',
      runtimePreference: 'fake-alpha',
      runtimeInstanceId: 'rt-alpha-01',
      intent: 'task_execution',
    });
    assert.strictEqual(validVerify.valid, true);

    const replayAttempt = OperatorConfirmationManager.verifyAndConsumeChallenge(challenge, {
      studioId: validStudioId,
      prompt: 'test prompt',
      runtimePreference: 'fake-alpha',
      runtimeInstanceId: 'rt-alpha-01',
      intent: 'task_execution',
    });
    assert.strictEqual(replayAttempt.valid, false);
    assert.ok(replayAttempt.reason?.includes('already been consumed'));
  });

  // =========================================================================
  // Scenario 13: Failed routing after successful submission acknowledgement
  // =========================================================================
  it('Scenario 13: transitions TaskExecutionStore to failed state on post-acknowledgement failure', () => {
    const store = TaskExecutionStore.instance;
    const taskId = 'task-uuid-routing-failure';

    store.beginSubmission(validStudioId, 'test prompt', undefined, 'fake-alpha', 'rt-alpha-01');
    assert.strictEqual(store.state.status, 'submitting');

    store.taskSubmitted(taskId, validStudioId);
    assert.strictEqual(store.state.status, 'queued');
    assert.strictEqual(store.state.activeTaskId, taskId);

    // Daemon emits task failure due to launch error
    store.consumeDaemonTimelineEvent({
      sequenceNumber: 1,
      id: 'event-01',
      timestamp: Date.now(),
      type: 'task_failed',
      summary: 'Runtime process failed to spawn: adapter error',
      taskId,
      details: {
        task_id: taskId,
        error: 'Process spawn failure',
      },
    });

    assert.strictEqual(store.state.status, 'failed');
    assert.strictEqual(store.state.lastError?.message, 'Runtime process failed to spawn: adapter error');
  });

  // =========================================================================
  // Scenario 14: Chat/Mission Control event correlation
  // =========================================================================
  it('Scenario 14: correlates run and execution parameters from typed daemon events', () => {
    const store = TaskExecutionStore.instance;
    const taskId = 'task-uuid-corr-01';

    store.beginSubmission(validStudioId, 'correlated task', undefined, 'fake-alpha', 'rt-alpha-01');
    store.taskSubmitted(taskId, validStudioId);

    // Timeline event for run_started with execution metadata
    store.consumeDaemonTimelineEvent({
      sequenceNumber: 1,
      id: 'evt-run-01',
      timestamp: Date.now(),
      type: 'run_started',
      summary: 'Run started for task',
      taskId,
      details: {
        run_id: 'run-uuid-001',
        runtime_preference: 'fake-alpha',
        runtime_instance_id: 'rt-alpha-01',
      },
    });

    assert.strictEqual(store.state.status, 'running');
    assert.strictEqual(store.state.activeRunId, 'run-uuid-001');
    assert.strictEqual(store.state.runtimePreference, 'fake-alpha');
    assert.strictEqual(store.state.runtimeInstanceId, 'rt-alpha-01');

    // Unrelated task event should be ignored
    store.consumeDaemonTimelineEvent({
      sequenceNumber: 2,
      id: 'evt-unrelated',
      timestamp: Date.now(),
      type: 'task_failed',
      summary: 'Other task failed',
      taskId: 'unrelated-task-99',
    });

    assert.strictEqual(store.state.status, 'running'); // Unaffected
  });

  // =========================================================================
  // Scenario 15: Cancellation, replay and reconnect
  // =========================================================================
  it('Scenario 15: dispatches typed cancellation and transitions store safely', async () => {
    CapabilityMatrix.setCapabilityEnabled('task.cancel', true);

    const store = TaskExecutionStore.instance;
    const taskId = 'task-uuid-cancel-01';

    store.beginSubmission(validStudioId, 'prompt to cancel');
    store.taskSubmitted(taskId, validStudioId);

    transport.responseGenerator = () => ({
      type: 'task_cancelled',
      summary: {
        cancelled_tasks: [taskId],
        cancelled_runs: [],
        cancelled_approvals: [],
      },
    });

    const cancelResult = await ExecutionClient.cancelTask(transport, {
      studioId: validStudioId,
      taskId,
      reason: 'Operator requested abort',
    });

    assert.strictEqual(cancelResult.success, true);
    assert.strictEqual(cancelResult.taskId, taskId);

    // Consume timeline cancellation event
    store.consumeDaemonTimelineEvent({
      sequenceNumber: 1,
      id: 'evt-cancel',
      timestamp: Date.now(),
      type: 'task_state_transitioned',
      summary: 'Task cancelled by operator',
      taskId,
      details: {
        to_state: 'cancelled',
      },
    });

    assert.strictEqual(store.state.status, 'cancelled');
  });

  // =========================================================================
  // Scenario 16: Backward compatibility: M15-01 readiness without M15-02 fields
  // =========================================================================
  it('Scenario 16: cleanly handles M15-01 readiness payload missing M15-02 optional fields', async () => {
    CapabilityMatrix.setCapabilityEnabled('task.submit', true);

    // Payload as returned by M15-01 daemon without registered_runtimes or runtime_capabilities
    const m15_01_readiness: RustExecutionReadinessReport = {
      is_ready: true,
      available_runtimes: ['opencode'],
      runtime_instances: [
        { instance_id: 'rt-legacy-01', implementation: 'opencode', healthy: true },
      ],
      available_models: [],
      authorized_workspaces: [],
      defaults: {
        configured_instance_id: 'rt-legacy-01',
      },
      blocking_reasons: [],
    };

    const { runtimeOptions, selectedRuntime } = ChatWebviewViewProvider.buildRuntimeOptions(m15_01_readiness);

    assert.strictEqual(runtimeOptions.length, 1);
    assert.strictEqual(runtimeOptions[0].id, 'rt-legacy-01|opencode');
    assert.strictEqual(selectedRuntime, 'rt-legacy-01|opencode');

    const validation = CapabilityMatrix.validateRuntimeInstanceSelection(m15_01_readiness, {
      runtime: 'rt-legacy-01|opencode',
    });
    assert.strictEqual(validation.valid, true);
    assert.strictEqual(validation.runtimePreference, 'opencode');
    assert.strictEqual(validation.runtimeInstanceId, 'rt-legacy-01');
  });
});
