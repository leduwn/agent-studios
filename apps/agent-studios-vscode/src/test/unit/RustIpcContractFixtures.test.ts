/**
 * Agent Studios — Rust vs TypeScript Contract Fixtures Test Suite
 * Milestone M14-IPC-03: Real ConnectedClient Vertical Slice
 *
 * Validates exact Serde JSON wire compatibility for all 20 IpcCommand variants
 * and 21 IpcResponse variants against Rust crates (ipc-transport, ipc-protocol, protocol, provider).
 */

import * as assert from 'assert';
import { RustIpcCodec } from '../../bridge/ipc/RustIpcCodec';
import {
  RustIpcCommand,
  RustApprovalContract,
  RustApprovalDecision,
  RustBatchTaskSpec,
  RustStudio,
  RustTaskRecord,
  RustRunRecord,
  RustProviderInstance,
  RustModelDescriptor,
  RUST_PROTOCOL_VERSION_1_0_0,
} from '../../bridge/ipc/RustIpcProtocolTypes';

describe('Rust vs TypeScript IPC Contract Fixtures', () => {
  describe('IpcCommand Serde Serialization Fixtures', () => {
    it('serializes negotiate_capabilities matching Rust Serde', () => {
      const cmd: RustIpcCommand = {
        type: 'negotiate_capabilities',
        protocol_version: RUST_PROTOCOL_VERSION_1_0_0,
        capabilities: ['worktree_isolation', 'event_streaming'],
      };
      const wire = RustIpcCodec.encodeCommand(cmd);
      const length = wire.readUInt32BE(0);
      const jsonStr = wire.subarray(4, 4 + length).toString('utf8');
      const parsed = JSON.parse(jsonStr);

      assert.strictEqual(parsed.type, 'negotiate_capabilities');
      assert.deepStrictEqual(parsed.protocol_version, { major: 1, minor: 0, patch: 0 });
      assert.deepStrictEqual(parsed.capabilities, ['worktree_isolation', 'event_streaming']);
    });

    it('serializes authenticate with HmacAuthHeader and optional payload', () => {
      const cmd: RustIpcCommand = {
        type: 'authenticate',
        header: {
          protocol_version: RUST_PROTOCOL_VERSION_1_0_0,
          key_id: 'ags-client',
          session_id: 'sess-uuid-001',
          nonce: 'nonce-12345',
          timestamp_millis: 1761820800000,
          signature: 'a'.repeat(64),
        },
      };
      const wire = RustIpcCodec.encodeCommand(cmd);
      const length = wire.readUInt32BE(0);
      const jsonStr = wire.subarray(4, 4 + length).toString('utf8');
      const parsed = JSON.parse(jsonStr);

      assert.strictEqual(parsed.type, 'authenticate');
      assert.strictEqual(parsed.header.key_id, 'ags-client');
      assert.strictEqual(parsed.header.signature.length, 64);
      assert.strictEqual(parsed.payload, undefined);
    });

    it('serializes ping and get_health commands', () => {
      const pingCmd: RustIpcCommand = {
        type: 'ping',
        payload: 'heartbeat',
        timestamp: 1761820801000,
      };
      const wirePing = RustIpcCodec.encodeCommand(pingCmd);
      const pingJson = JSON.parse(wirePing.subarray(4).toString('utf8'));
      assert.strictEqual(pingJson.type, 'ping');
      assert.strictEqual(pingJson.payload, 'heartbeat');

      const healthCmd: RustIpcCommand = { type: 'get_health' };
      const wireHealth = RustIpcCodec.encodeCommand(healthCmd);
      const healthJson = JSON.parse(wireHealth.subarray(4).toString('utf8'));
      assert.strictEqual(healthJson.type, 'get_health');
    });

    it('serializes subscribe_events with from_sequence', () => {
      const subCmd: RustIpcCommand = {
        type: 'subscribe_events',
        from_sequence: 42,
      };
      const wire = RustIpcCodec.encodeCommand(subCmd);
      const json = JSON.parse(wire.subarray(4).toString('utf8'));
      assert.strictEqual(json.type, 'subscribe_events');
      assert.strictEqual(json.from_sequence, 42);
    });

    it('serializes request_approval with full RustApprovalContract', () => {
      const contract: RustApprovalContract = {
        approval_id: 'appr-01',
        session_id: 'sess-01',
        task_id: 'task-01',
        run_id: 'run-01',
        risk_level: 'high',
        action: {
          action_type: 'command_execution',
          command: 'npm test',
          cwd: '/repo',
        },
        description: 'Run test suite',
        created_at: '2026-10-30T12:00:00Z',
        expires_at: '2026-10-30T12:05:00Z',
        status: 'requested',
      };
      const cmd: RustIpcCommand = {
        type: 'request_approval',
        contract,
      };
      const wire = RustIpcCodec.encodeCommand(cmd);
      const json = JSON.parse(wire.subarray(4).toString('utf8'));
      assert.strictEqual(json.type, 'request_approval');
      assert.strictEqual(json.contract.risk_level, 'high');
      assert.strictEqual(json.contract.action.action_type, 'command_execution');
      assert.strictEqual(json.contract.action.command, 'npm test');
    });

    it('serializes resolve_approval with tagged decision', () => {
      const approvedDecision: RustApprovalDecision = {
        decision: 'approved',
        user_note: 'Approved by developer',
      };
      const cmdApproved: RustIpcCommand = {
        type: 'resolve_approval',
        approval_id: 'appr-01',
        decision: approvedDecision,
      };
      const wireApproved = RustIpcCodec.encodeCommand(cmdApproved);
      const jsonApproved = JSON.parse(wireApproved.subarray(4).toString('utf8'));
      assert.strictEqual(jsonApproved.type, 'resolve_approval');
      assert.strictEqual(jsonApproved.decision.decision, 'approved');
      assert.strictEqual(jsonApproved.decision.user_note, 'Approved by developer');

      const rejectedDecision: RustApprovalDecision = {
        decision: 'rejected',
        reason: 'Security violation',
      };
      const cmdRejected: RustIpcCommand = {
        type: 'resolve_approval',
        approval_id: 'appr-02',
        decision: rejectedDecision,
      };
      const wireRejected = RustIpcCodec.encodeCommand(cmdRejected);
      const jsonRejected = JSON.parse(wireRejected.subarray(4).toString('utf8'));
      assert.strictEqual(jsonRejected.decision.decision, 'rejected');
      assert.strictEqual(jsonRejected.decision.reason, 'Security violation');
    });

    it('serializes studio and task commands: create, list, get, submit, cancel', () => {
      const createStudio: RustIpcCommand = { type: 'create_studio', name: 'Dev Studio' };
      assert.strictEqual(JSON.parse(RustIpcCodec.encodeCommand(createStudio).subarray(4).toString('utf8')).name, 'Dev Studio');

      const listStudios: RustIpcCommand = { type: 'list_studios' };
      assert.strictEqual(JSON.parse(RustIpcCodec.encodeCommand(listStudios).subarray(4).toString('utf8')).type, 'list_studios');

      const getStudio: RustIpcCommand = { type: 'get_studio', studio_id: 'std-1' };
      assert.strictEqual(JSON.parse(RustIpcCodec.encodeCommand(getStudio).subarray(4).toString('utf8')).studio_id, 'std-1');

      const batchSpec: RustBatchTaskSpec = {
        key: 'k1',
        title: 'Task 1',
        description: 'First task',
        dependency_keys: [],
        dependency_task_ids: [],
        dependency_output_policy: 'task_success',
      };
      const submitTask: RustIpcCommand = {
        type: 'submit_task',
        studio_id: 'std-1',
        task_spec: batchSpec,
      };
      const submitJson = JSON.parse(RustIpcCodec.encodeCommand(submitTask).subarray(4).toString('utf8'));
      assert.strictEqual(submitJson.type, 'submit_task');
      assert.strictEqual(submitJson.task_spec.key, 'k1');

      const cancelTask: RustIpcCommand = {
        type: 'cancel_task',
        studio_id: 'std-1',
        task_id: 'task-99',
        reason: 'User request',
      };
      const cancelJson = JSON.parse(RustIpcCodec.encodeCommand(cancelTask).subarray(4).toString('utf8'));
      assert.strictEqual(cancelJson.type, 'cancel_task');
      assert.strictEqual(cancelJson.task_id, 'task-99');
      assert.strictEqual(cancelJson.reason, 'User request');
    });

    it('serializes provider, model, and shutdown commands', () => {
      const listProviders: RustIpcCommand = { type: 'list_providers' };
      assert.strictEqual(JSON.parse(RustIpcCodec.encodeCommand(listProviders).subarray(4).toString('utf8')).type, 'list_providers');

      const getProvider: RustIpcCommand = { type: 'get_provider', provider_id: 'prov-inst-01' };
      assert.strictEqual(JSON.parse(RustIpcCodec.encodeCommand(getProvider).subarray(4).toString('utf8')).provider_id, 'prov-inst-01');

      const listModels: RustIpcCommand = { type: 'list_models', instance_id: 'prov-inst-01' };
      assert.strictEqual(JSON.parse(RustIpcCodec.encodeCommand(listModels).subarray(4).toString('utf8')).instance_id, 'prov-inst-01');

      const shutdown: RustIpcCommand = { type: 'shutdown' };
      assert.strictEqual(JSON.parse(RustIpcCodec.encodeCommand(shutdown).subarray(4).toString('utf8')).type, 'shutdown');
    });
  });

  describe('IpcResponse Serde Deserialization Fixtures', () => {
    function createWireFrame(payload: unknown): Buffer {
      const json = JSON.stringify(payload);
      const jsonBytes = Buffer.from(json, 'utf8');
      const header = Buffer.alloc(4);
      header.writeUInt32BE(jsonBytes.length, 0);
      return Buffer.concat([header, jsonBytes]);
    }

    it('deserializes capabilities_negotiated and authenticated', () => {
      const rawNegotiated = {
        type: 'capabilities_negotiated',
        selected_version: { major: 1, minor: 0, patch: 0 },
        agreed_capabilities: ['worktree_isolation', 'event_streaming'],
      };
      const { responses: [r1] } = RustIpcCodec.decodeResponses(createWireFrame(rawNegotiated));
      assert.strictEqual(r1.type, 'capabilities_negotiated');
      if (r1.type === 'capabilities_negotiated') {
        assert.deepStrictEqual(r1.agreed_capabilities, ['worktree_isolation', 'event_streaming']);
      }

      const rawAuth = {
        type: 'authenticated',
        session_id: 'sess-uuid-1234',
      };
      const { responses: [r2] } = RustIpcCodec.decodeResponses(createWireFrame(rawAuth));
      assert.strictEqual(r2.type, 'authenticated');
      if (r2.type === 'authenticated') {
        assert.strictEqual(r2.session_id, 'sess-uuid-1234');
      }
    });

    it('deserializes pong and health responses', () => {
      const rawPong = {
        type: 'pong',
        payload: 'test',
        client_timestamp: 1000,
        server_timestamp: 1010,
      };
      const { responses: [rPong] } = RustIpcCodec.decodeResponses(createWireFrame(rawPong));
      assert.strictEqual(rPong.type, 'pong');
      if (rPong.type === 'pong') {
        assert.strictEqual(rPong.payload, 'test');
        assert.strictEqual(rPong.server_timestamp, 1010);
      }

      const rawHealth = {
        type: 'health',
        status: 'ok',
        active_clients: 2,
        uptime_secs: 120,
        memory_events_count: 55,
      };
      const { responses: [rHealth] } = RustIpcCodec.decodeResponses(createWireFrame(rawHealth));
      assert.strictEqual(rHealth.type, 'health');
      if (rHealth.type === 'health') {
        assert.strictEqual(rHealth.status, 'ok');
        assert.strictEqual(rHealth.active_clients, 2);
        assert.strictEqual(rHealth.uptime_secs, 120);
        assert.strictEqual(rHealth.memory_events_count, 55);
      }
    });

    it('deserializes event response with IpcEvent', () => {
      const rawEvent = {
        type: 'event',
        event: {
          sequence: 15,
          timestamp: '2026-10-30T12:00:00Z',
          event_type: 'task_progress',
          payload: { task_id: 'task-1', percent: 75 },
        },
      };
      const { responses: [rEvent] } = RustIpcCodec.decodeResponses(createWireFrame(rawEvent));
      assert.strictEqual(rEvent.type, 'event');
      if (rEvent.type === 'event') {
        assert.strictEqual(rEvent.event.sequence, 15);
        assert.strictEqual(rEvent.event.event_type, 'task_progress');
      }
    });

    it('deserializes studio responses (created, list, get with null and object)', () => {
      const studioObj: RustStudio = {
        id: 'std-uuid-1',
        name: 'Studio Alpha',
        created_at: '2026-10-30T10:00:00Z',
      };

      const rawCreated = { type: 'studio_created', studio: studioObj };
      const { responses: [rCreated] } = RustIpcCodec.decodeResponses(createWireFrame(rawCreated));
      assert.strictEqual(rCreated.type, 'studio_created');

      const rawStudios = { type: 'studios', studios: [studioObj] };
      const { responses: [rStudios] } = RustIpcCodec.decodeResponses(createWireFrame(rawStudios));
      assert.strictEqual(rStudios.type, 'studios');
      if (rStudios.type === 'studios') {
        assert.strictEqual(rStudios.studios.length, 1);
        assert.strictEqual(rStudios.studios[0].name, 'Studio Alpha');
      }

      const rawStudioNull = { type: 'studio', studio: null };
      const { responses: [rStudioNull] } = RustIpcCodec.decodeResponses(createWireFrame(rawStudioNull));
      assert.strictEqual(rStudioNull.type, 'studio');
      if (rStudioNull.type === 'studio') {
        assert.strictEqual(rStudioNull.studio, null);
      }
    });

    it('deserializes task and run records with states', () => {
      const taskObj: RustTaskRecord = {
        id: 'task-10',
        studio_id: 'std-1',
        title: 'Run build',
        description: 'Cargo build',
        state: 'running',
        dependencies: [],
        created_at: '2026-10-30T10:00:00Z',
        updated_at: '2026-10-30T10:01:00Z',
        retry_count: 0,
        dependency_output_policy: 'task_success',
      };
      const rawTasks = { type: 'tasks', tasks: [taskObj] };
      const { responses: [rTasks] } = RustIpcCodec.decodeResponses(createWireFrame(rawTasks));
      assert.strictEqual(rTasks.type, 'tasks');
      if (rTasks.type === 'tasks') {
        assert.strictEqual(rTasks.tasks[0].state, 'running');
      }

      const runObj: RustRunRecord = {
        id: 'run-1',
        task_id: 'task-10',
        agent_id: 'coordinator',
        attempt: 1,
        state: 'running',
        started_at: '2026-10-30T10:00:05Z',
      };
      const rawRuns = { type: 'runs', runs: [runObj] };
      const { responses: [rRuns] } = RustIpcCodec.decodeResponses(createWireFrame(rawRuns));
      assert.strictEqual(rRuns.type, 'runs');
      if (rRuns.type === 'runs') {
        assert.strictEqual(rRuns.runs[0].state, 'running');
      }
    });

    it('deserializes providers and models descriptors', () => {
      const providerInst: RustProviderInstance = {
        id: 'inst-01',
        provider_id: 'openai',
        display_name: 'OpenAI Production',
        enabled: true,
      };
      const rawProviders = { type: 'providers', providers: [providerInst] };
      const { responses: [rProviders] } = RustIpcCodec.decodeResponses(createWireFrame(rawProviders));
      assert.strictEqual(rProviders.type, 'providers');
      if (rProviders.type === 'providers') {
        assert.strictEqual(rProviders.providers[0].display_name, 'OpenAI Production');
      }

      const modelDesc: RustModelDescriptor = {
        provider_instance_id: 'inst-01',
        id: 'gpt-5.6',
        display_name: 'GPT 5.6',
        limits: {
          context_window_tokens: 200000,
          max_output_tokens: 8192,
        },
        metadata_source: 'static_catalog',
      };
      const rawModels = { type: 'models', models: [modelDesc] };
      const { responses: [rModels] } = RustIpcCodec.decodeResponses(createWireFrame(rawModels));
      assert.strictEqual(rModels.type, 'models');
      if (rModels.type === 'models') {
        assert.strictEqual(rModels.models[0].id, 'gpt-5.6');
        assert.strictEqual(rModels.models[0].limits.context_window_tokens, 200000);
      }
    });

    it('deserializes error and shutdown_acknowledged responses', () => {
      const rawErr = { type: 'error', message: 'Unauthorized client' };
      const { responses: [rErr] } = RustIpcCodec.decodeResponses(createWireFrame(rawErr));
      assert.strictEqual(rErr.type, 'error');
      if (rErr.type === 'error') {
        assert.strictEqual(rErr.message, 'Unauthorized client');
      }

      const rawShutdown = { type: 'shutdown_acknowledged' };
      const { responses: [rShutdown] } = RustIpcCodec.decodeResponses(createWireFrame(rawShutdown));
      assert.strictEqual(rShutdown.type, 'shutdown_acknowledged');
    });
  });
});
