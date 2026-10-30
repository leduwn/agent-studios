/**
 * Agent Studios — SubmitTaskAdapter & CapabilityMatrix Verification Tests
 * Milestone M14 Integration Freeze & M15 UI Execution Preparation
 *
 * Verifies effective runtime, model, and workspace configuration verification:
 * 1. Daemon repository root determination parity (connection.rs:1568-1574)
 * 2. Workspace folder matching and fail-closed rejection
 * 3. Model & Provider selection parity (connection.rs:1771-1827) and fail-closed rejection
 * 4. IPC queries: get_health, list_providers, list_models
 */

import * as assert from 'assert';
import {
  CapabilityMatrix,
  OperatorConfirmationManager,
  normalizeWorkspacePath,
  parseDefaultModelEnv,
  SubmitTaskAdapter,
  SubmitTaskRequest,
  TaskSubmissionError,
  RustIpcCommand,
  RustIpcResponse,
} from '../../bridge/ipc';

describe('SubmitTask & CapabilityMatrix Configuration Verification', () => {
  const validStudioId = 'a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d';
  const originalEnvRepo = process.env.AGENT_STUDIOS_REPO_PATH;
  const originalEnvModel = process.env.AGENT_STUDIOS_DEFAULT_MODEL;

  afterEach(() => {
    if (originalEnvRepo !== undefined) {
      process.env.AGENT_STUDIOS_REPO_PATH = originalEnvRepo;
    } else {
      delete process.env.AGENT_STUDIOS_REPO_PATH;
    }

    if (originalEnvModel !== undefined) {
      process.env.AGENT_STUDIOS_DEFAULT_MODEL = originalEnvModel;
    } else {
      delete process.env.AGENT_STUDIOS_DEFAULT_MODEL;
    }
  });

  describe('Utility: normalizeWorkspacePath & parseDefaultModelEnv', () => {
    it('normalizes workspace paths across separators and trailing slashes', () => {
      const p1 = normalizeWorkspacePath('/projects/agent-studios/');
      const p2 = normalizeWorkspacePath('/projects/agent-studios');
      assert.strictEqual(p1, p2);
    });

    it('parses AGENT_STUDIOS_DEFAULT_MODEL provider/model format', () => {
      const parsed = parseDefaultModelEnv('anthropic/claude-3-5-sonnet-20241022');
      assert.deepStrictEqual(parsed, {
        providerId: 'anthropic',
        modelId: 'claude-3-5-sonnet-20241022',
      });
    });

    it('parses AGENT_STUDIOS_DEFAULT_MODEL bare model format', () => {
      const parsed = parseDefaultModelEnv('gpt-4o');
      assert.deepStrictEqual(parsed, {
        providerId: '',
        modelId: 'gpt-4o',
      });
    });

    it('returns null for empty or invalid model env', () => {
      assert.strictEqual(parseDefaultModelEnv(''), null);
      assert.strictEqual(parseDefaultModelEnv(undefined), null);
    });
  });

  describe('Workspace Verification & Fail-Closed Guard (Requirement 2)', () => {
    it('fails closed when active VS Code workspace is missing and requireWorkspace is true', () => {
      const result = CapabilityMatrix.evaluateTaskSubmissionPreflight({
        studioId: validStudioId,
        prompt: 'Fix memory leak in buffer pool',
        confirmedByUser: true,
        requireWorkspace: true,
        workspacePath: '',
      });

      assert.strictEqual(result.allowed, false);
      assert.ok(result.error?.includes('Active VS Code workspace folder is required'));
    });

    it('fails closed when workspace directory does not match daemon repository root', () => {
      const result = CapabilityMatrix.evaluateTaskSubmissionPreflight({
        studioId: validStudioId,
        prompt: 'Refactor authentication service',
        confirmedByUser: true,
        workspacePath: '/home/user/workspace-alpha',
        daemonRepoRoot: '/home/user/workspace-beta',
      });

      assert.strictEqual(result.allowed, false);
      assert.ok(result.error?.includes('Workspace mismatch'));
      assert.ok(result.error?.includes('workspace-alpha'));
      assert.ok(result.error?.includes('workspace-beta'));
    });

    it('fails closed when workspace directory does not match AGENT_STUDIOS_REPO_PATH env', () => {
      process.env.AGENT_STUDIOS_REPO_PATH = '/opt/daemon/repo';

      const result = CapabilityMatrix.evaluateTaskSubmissionPreflight({
        studioId: validStudioId,
        prompt: 'Refactor authentication service',
        confirmedByUser: true,
        workspacePath: '/opt/user/different-repo',
      });

      assert.strictEqual(result.allowed, false);
      assert.ok(result.error?.includes('Workspace mismatch'));
      assert.ok(result.error?.includes('/opt/daemon/repo'));
    });

    it('fails closed on null byte injection in workspace path', () => {
      const result = CapabilityMatrix.evaluateTaskSubmissionPreflight({
        studioId: validStudioId,
        prompt: 'Task prompt',
        confirmedByUser: true,
        workspacePath: '/home/user/repo\0malicious',
        daemonRepoRoot: '/home/user/repo',
      });

      assert.strictEqual(result.allowed, false);
      assert.ok(result.error?.includes('Null byte injection'));
    });

    it('fails closed on UNC network path in workspace path', () => {
      const result = CapabilityMatrix.evaluateTaskSubmissionPreflight({
        studioId: validStudioId,
        prompt: 'Task prompt',
        confirmedByUser: true,
        workspacePath: '\\\\evil-server\\share\\repo',
        daemonRepoRoot: '\\\\evil-server\\share\\repo',
      });

      assert.strictEqual(result.allowed, false);
      assert.ok(result.error?.includes('UNC network paths rejected'));
    });

    it('fails closed on path traversal sequence in workspace path', () => {
      const result = CapabilityMatrix.evaluateTaskSubmissionPreflight({
        studioId: validStudioId,
        prompt: 'Task prompt',
        confirmedByUser: true,
        workspacePath: '/home/user/repo/../secret',
        daemonRepoRoot: '/home/user/secret',
      });

      assert.strictEqual(result.allowed, false);
      assert.ok(result.error?.includes('Path traversal sequences rejected'));
    });

    it('succeeds when workspace folder matches daemon repo root after normalization', () => {
      const result = CapabilityMatrix.evaluateTaskSubmissionPreflight({
        studioId: validStudioId,
        prompt: 'Task prompt',
        confirmedByUser: true,
        workspacePath: '/home/user/my-project/',
        daemonRepoRoot: '/home/user/my-project',
      });

      assert.strictEqual(result.allowed, true);
      assert.strictEqual(result.error, undefined);
    });
  });

  describe('Model & Provider Verification & Rejection (Requirement 3)', () => {
    it('REJECTS task submission when requestedModelId differs from verified daemon default model', () => {
      const result = CapabilityMatrix.evaluateTaskSubmissionPreflight({
        studioId: validStudioId,
        prompt: 'Synthesize gRPC client',
        confirmedByUser: true,
        requestedModelId: 'gpt-4o',
        daemonEffectiveModel: 'claude-3-5-sonnet-20241022',
      });

      assert.strictEqual(result.allowed, false);
      assert.ok(result.error?.includes('Model routing mismatch'));
      assert.ok(result.error?.includes('gpt-4o'));
      assert.ok(result.error?.includes('claude-3-5-sonnet-20241022'));
    });

    it('REJECTS task submission when requestedModelId is specified but daemon model is unverified', () => {
      delete process.env.AGENT_STUDIOS_DEFAULT_MODEL;

      const result = CapabilityMatrix.evaluateTaskSubmissionPreflight({
        studioId: validStudioId,
        prompt: 'Synthesize gRPC client',
        confirmedByUser: true,
        requestedModelId: 'claude-3-opus',
        // daemonEffectiveModel omitted
      });

      assert.strictEqual(result.allowed, false);
      assert.ok(result.error?.includes('Model routing rejected (fail-closed)'));
      assert.ok(result.error?.includes('daemon default model could not be verified'));
    });

    it('REJECTS task submission when requestedProviderId differs from daemon default provider', () => {
      const result = CapabilityMatrix.evaluateTaskSubmissionPreflight({
        studioId: validStudioId,
        prompt: 'Synthesize gRPC client',
        confirmedByUser: true,
        requestedProviderId: 'openai',
        daemonEffectiveProvider: 'anthropic',
      });

      assert.strictEqual(result.allowed, false);
      assert.ok(result.error?.includes('Provider routing mismatch'));
      assert.ok(result.error?.includes('openai'));
      assert.ok(result.error?.includes('anthropic'));
    });

    it('allows task submission when requestedModelId matches verified daemon default model', () => {
      const result = CapabilityMatrix.evaluateTaskSubmissionPreflight({
        studioId: validStudioId,
        prompt: 'Synthesize gRPC client',
        confirmedByUser: true,
        requestedModelId: 'claude-3-5-sonnet-20241022',
        daemonEffectiveModel: 'claude-3-5-sonnet-20241022',
      });

      assert.strictEqual(result.allowed, true);
    });

    it('allows task submission when requestedModelId is "default" or omitted', () => {
      const result = CapabilityMatrix.evaluateTaskSubmissionPreflight({
        studioId: validStudioId,
        prompt: 'Synthesize gRPC client',
        confirmedByUser: true,
        requestedModelId: 'default',
        daemonEffectiveModel: 'claude-3-5-sonnet-20241022',
      });

      assert.strictEqual(result.allowed, true);
    });
  });

  describe('SubmitTaskAdapter.verifyDaemonConfiguration over IPC (Requirement 4)', () => {
    it('fails closed when daemon reports unhealthy status', async () => {
      const mockTransport = {
        sendCommand: async (cmd: RustIpcCommand): Promise<RustIpcResponse> => {
          if (cmd.type === 'get_health') {
            return {
              type: 'health',
              status: 'degraded',
              active_clients: 1,
              uptime_secs: 10,
              memory_events_count: 0,
            };
          }
          throw new Error(`Unexpected command: ${cmd.type}`);
        },
      };

      await assert.rejects(
        async () => {
          await SubmitTaskAdapter.verifyDaemonConfiguration(mockTransport);
        },
        (err: Error) => {
          return (
            err instanceof TaskSubmissionError &&
            err.message.includes('Daemon reported unhealthy status')
          );
        }
      );
    });

    it('fails closed when catalog has multiple models without explicit AGENT_STUDIOS_DEFAULT_MODEL (connection.rs:1809)', async () => {
      delete process.env.AGENT_STUDIOS_DEFAULT_MODEL;

      const mockTransport = {
        sendCommand: async (cmd: RustIpcCommand): Promise<RustIpcResponse> => {
          if (cmd.type === 'get_health') {
            return {
              type: 'health',
              status: 'healthy',
              active_clients: 1,
              uptime_secs: 10,
              memory_events_count: 0,
            };
          }
          if (cmd.type === 'list_providers') {
            return {
              type: 'providers',
              providers: [
                { id: 'inst-1', provider_id: 'anthropic', display_name: 'Anthropic', enabled: true },
              ],
            };
          }
          if (cmd.type === 'list_models') {
            return {
              type: 'models',
              models: [
                {
                  provider_instance_id: 'inst-1',
                  id: 'claude-3-5-sonnet',
                  display_name: 'Claude 3.5 Sonnet',
                  limits: { context_window_tokens: 200000 },
                },
                {
                  provider_instance_id: 'inst-1',
                  id: 'claude-3-haiku',
                  display_name: 'Claude 3 Haiku',
                  limits: { context_window_tokens: 200000 },
                },
              ],
            };
          }
          throw new Error(`Unexpected command: ${cmd.type}`);
        },
      };

      await assert.rejects(
        async () => {
          await SubmitTaskAdapter.verifyDaemonConfiguration(mockTransport);
        },
        (err: Error) => {
          return (
            err instanceof TaskSubmissionError &&
            err.message.includes('Multiple models available (2) in daemon catalog') &&
            err.message.includes('AGENT_STUDIOS_DEFAULT_MODEL')
          );
        }
      );
    });

    it('resolves unambiguous single model catalog (connection.rs:1806)', async () => {
      delete process.env.AGENT_STUDIOS_DEFAULT_MODEL;

      const mockTransport = {
        sendCommand: async (cmd: RustIpcCommand): Promise<RustIpcResponse> => {
          if (cmd.type === 'get_health') {
            return {
              type: 'health',
              status: 'healthy',
              active_clients: 1,
              uptime_secs: 10,
              memory_events_count: 0,
            };
          }
          if (cmd.type === 'list_providers') {
            return {
              type: 'providers',
              providers: [
                { id: 'inst-single', provider_id: 'anthropic', display_name: 'Anthropic', enabled: true },
              ],
            };
          }
          if (cmd.type === 'list_models') {
            return {
              type: 'models',
              models: [
                {
                  provider_instance_id: 'inst-single',
                  id: 'claude-3-5-sonnet-20241022',
                  display_name: 'Claude 3.5 Sonnet',
                  limits: { context_window_tokens: 200000 },
                },
              ],
            };
          }
          throw new Error(`Unexpected command: ${cmd.type}`);
        },
      };

      const verified = await SubmitTaskAdapter.verifyDaemonConfiguration(mockTransport);
      assert.strictEqual(verified.healthy, true);
      assert.strictEqual(verified.defaultModelId, 'claude-3-5-sonnet-20241022');
      assert.strictEqual(verified.defaultProviderId, 'inst-single');
    });

    it('validates AGENT_STUDIOS_DEFAULT_MODEL exists in catalog (connection.rs:1771)', async () => {
      process.env.AGENT_STUDIOS_DEFAULT_MODEL = 'inst-1/claude-3-5-sonnet';

      const mockTransport = {
        sendCommand: async (cmd: RustIpcCommand): Promise<RustIpcResponse> => {
          if (cmd.type === 'get_health') {
            return {
              type: 'health',
              status: 'healthy',
              active_clients: 1,
              uptime_secs: 10,
              memory_events_count: 0,
            };
          }
          if (cmd.type === 'list_providers') {
            return {
              type: 'providers',
              providers: [
                { id: 'inst-1', provider_id: 'anthropic', display_name: 'Anthropic', enabled: true },
              ],
            };
          }
          if (cmd.type === 'list_models') {
            return {
              type: 'models',
              models: [
                {
                  provider_instance_id: 'inst-1',
                  id: 'claude-3-5-sonnet',
                  display_name: 'Claude 3.5 Sonnet',
                  limits: { context_window_tokens: 200000 },
                },
                {
                  provider_instance_id: 'inst-1',
                  id: 'claude-3-haiku',
                  display_name: 'Claude 3 Haiku',
                  limits: { context_window_tokens: 200000 },
                },
              ],
            };
          }
          throw new Error(`Unexpected command: ${cmd.type}`);
        },
      };

      const verified = await SubmitTaskAdapter.verifyDaemonConfiguration(mockTransport);
      assert.strictEqual(verified.healthy, true);
      assert.strictEqual(verified.defaultModelId, 'claude-3-5-sonnet');
      assert.strictEqual(verified.defaultProviderId, 'inst-1');
    });

    it('fails closed when AGENT_STUDIOS_DEFAULT_MODEL is not found in catalog (connection.rs:1788)', async () => {
      process.env.AGENT_STUDIOS_DEFAULT_MODEL = 'inst-1/nonexistent-model';

      const mockTransport = {
        sendCommand: async (cmd: RustIpcCommand): Promise<RustIpcResponse> => {
          if (cmd.type === 'get_health') {
            return {
              type: 'health',
              status: 'healthy',
              active_clients: 1,
              uptime_secs: 10,
              memory_events_count: 0,
            };
          }
          if (cmd.type === 'list_providers') {
            return { type: 'providers', providers: [] };
          }
          if (cmd.type === 'list_models') {
            return { type: 'models', models: [] };
          }
          throw new Error(`Unexpected command: ${cmd.type}`);
        },
      };

      await assert.rejects(
        async () => {
          await SubmitTaskAdapter.verifyDaemonConfiguration(mockTransport);
        },
        (err: Error) => {
          return (
            err instanceof TaskSubmissionError &&
            err.message.includes('not found in daemon provider catalog')
          );
        }
      );
    });
  });

  describe('SubmitTaskAdapter.submitTask End-to-End Fail-Closed Verification (Requirement 5)', () => {
    beforeEach(() => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);
    });

    beforeEach(() => {
      CapabilityMatrix.setCapabilityEnabled('task.submit', true);
    });

    afterEach(() => {
      CapabilityMatrix.resetCapabilities();
    });

    const defaultMockTransport = {
      sendCommand: async (cmd: RustIpcCommand): Promise<RustIpcResponse> => {
        if (cmd.type === 'get_health') {
          return {
            type: 'health',
            status: 'healthy',
            active_clients: 1,
            uptime_secs: 100,
            memory_events_count: 5,
          };
        }
        if (cmd.type === 'list_providers') {
          return {
            type: 'providers',
            providers: [
              { id: 'inst-verified', provider_id: 'anthropic', display_name: 'Anthropic', enabled: true },
            ],
          };
        }
        if (cmd.type === 'list_models') {
          return {
            type: 'models',
            models: [
              {
                provider_instance_id: 'inst-verified',
                id: 'claude-3-5-sonnet-20241022',
                display_name: 'Claude 3.5 Sonnet',
                limits: { context_window_tokens: 200000 },
              },
            ],
          };
        }
        if (cmd.type === 'submit_task') {
          return {
            type: 'tasks_submitted',
            task_ids: ['task-rust-uuid-999'],
          };
        }
        throw new Error(`Unexpected command: ${cmd.type}`);
      },
    };

    it('fails closed when active workspace is missing during submitTask', async () => {
      const prompt = 'Build auth module';
      const challenge = OperatorConfirmationManager.createChallenge(validStudioId, prompt);
      const req: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt,
        confirmedByUser: true,
        confirmationChallenge: challenge,
        workspacePath: '',
        requireWorkspace: true,
      };

      await assert.rejects(
        async () => {
          await SubmitTaskAdapter.submitTask(defaultMockTransport, req);
        },
        (err: Error) => {
          return (
            err instanceof TaskSubmissionError &&
            err.message.includes('Active VS Code workspace folder is required')
          );
        }
      );
    });

    it('fails closed when active workspace does not match daemon execution repository', async () => {
      const prompt = 'Build auth module';
      const challenge = OperatorConfirmationManager.createChallenge(validStudioId, prompt, {
        workspacePath: '/home/user/workspace-one',
      });
      const req: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt,
        confirmedByUser: true,
        confirmationChallenge: challenge,
        workspacePath: '/home/user/workspace-one',
        daemonRepoRoot: '/home/user/workspace-two',
      };

      await assert.rejects(
        async () => {
          await SubmitTaskAdapter.submitTask(defaultMockTransport, req);
        },
        (err: Error) => {
          return (
            err instanceof TaskSubmissionError &&
            err.message.includes('Workspace mismatch')
          );
        }
      );
    });

    it('REJECTS task submission when requestedModelId differs from verified daemon default model with verifyDaemon: true', async () => {
      const prompt = 'Build auth module';
      const challenge = OperatorConfirmationManager.createChallenge(validStudioId, prompt, {
        workspacePath: '/home/user/matching-repo',
        modelId: 'gpt-4o',
      });
      const req: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt,
        confirmedByUser: true,
        confirmationChallenge: challenge,
        workspacePath: '/home/user/matching-repo',
        daemonRepoRoot: '/home/user/matching-repo',
        requestedModelId: 'gpt-4o',
        verifyDaemon: true,
      };

      await assert.rejects(
        async () => {
          await SubmitTaskAdapter.submitTask(defaultMockTransport, req);
        },
        (err: Error) => {
          return (
            err instanceof TaskSubmissionError &&
            err.message.includes('Model routing mismatch') &&
            err.message.includes('gpt-4o')
          );
        }
      );
    });

    it('dispatches task successfully when workspace and model match verified daemon configuration', async () => {
      const prompt = 'Build auth module';
      const challenge = OperatorConfirmationManager.createChallenge(validStudioId, prompt, {
        workspacePath: '/home/user/matching-repo',
        modelId: 'claude-3-5-sonnet-20241022',
      });
      const req: SubmitTaskRequest = {
        studioId: validStudioId,
        prompt,
        confirmedByUser: true,
        confirmationChallenge: challenge,
        workspacePath: '/home/user/matching-repo',
        daemonRepoRoot: '/home/user/matching-repo',
        requestedModelId: 'claude-3-5-sonnet-20241022',
        verifyDaemon: true,
      };

      const result = await SubmitTaskAdapter.submitTask(defaultMockTransport, req);
      assert.strictEqual(result.success, true);
      assert.strictEqual(result.taskId, 'task-rust-uuid-999');
      assert.strictEqual(result.studioId, validStudioId);
    });
  });
});
