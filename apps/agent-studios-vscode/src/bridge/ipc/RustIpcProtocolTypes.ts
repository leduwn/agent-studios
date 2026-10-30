/**
 * Agent Studios — Rust IPC Wire Protocol Types (Approach A)
 * Milestone M14-UI-02.1 & M14-IPC-03: Authoritative Wire Compatibility
 *
 * Defines the exact wire serialization types for the authoritative Rust daemon
 * (agent-studios-rs / crates ipc-protocol, ipc-transport, protocol, provider).
 *
 * Rust Serde Format:
 * - Commands: internally tagged enum with `{"type": "snake_case"}`
 * - Responses: internally tagged enum with `{"type": "snake_case"}`
 * - Framing: 4-byte big-endian length prefix, max 4 MiB (DEFAULT_MAX_FRAME_SIZE)
 */

/* eslint-disable @typescript-eslint/naming-convention */

export interface RustProtocolVersion {
  major: number;
  minor: number;
  patch: number;
}

export const RUST_PROTOCOL_VERSION_1_0_0: RustProtocolVersion = {
  major: 1,
  minor: 0,
  patch: 0,
};

export interface RustHmacAuthHeader {
  protocol_version: RustProtocolVersion;
  key_id: string;
  session_id: string;
  nonce: string;
  timestamp_millis: number;
  signature: string;
}

export type RustRiskLevel = 'low' | 'medium' | 'high' | 'critical';

export type RustApprovalAction =
  | {
      action_type: 'command_execution';
      command: string;
      cwd: string;
    }
  | {
      action_type: 'file_modification';
      path: string;
      diff: string;
    }
  | {
      action_type: 'network_request';
      url: string;
      method: string;
    }
  | {
      action_type: 'workspace_operation';
      operation: string;
      target: string;
    }
  | {
      action_type: 'custom';
      name: string;
      details: unknown;
    };

export type RustApprovalStatus =
  | 'requested'
  | 'authorized'
  | 'delivery_pending'
  | 'delivered'
  | 'rejected'
  | 'timed_out'
  | 'delivery_failed'
  | 'execution_completed'
  | 'execution_failed'
  | 'cancelled';

export interface RustApprovalContract {
  approval_id: string;
  session_id: string;
  task_id: string;
  run_id: string;
  tool_call_id?: string;
  risk_level: RustRiskLevel;
  action: RustApprovalAction;
  description: string;
  created_at: string;
  expires_at: string;
  status: RustApprovalStatus;
}

export type RustApprovalDecision =
  | {
      decision: 'approved';
      modified_details?: unknown;
      user_note?: string;
    }
  | {
      decision: 'rejected';
      reason: string;
    };

export interface RustStudio {
  id: string;
  name: string;
  created_at: string;
}

export type RustTaskState =
  | 'pending'
  | 'blocked'
  | 'ready'
  | 'running'
  | 'retrying'
  | 'paused'
  | 'succeeded'
  | 'failed'
  | 'cancelled';

export type RustDependencyOutputPolicy = 'task_success' | 'reconciled_output';

export interface RustTaskRecord {
  id: string;
  studio_id: string;
  parent_task_id?: string | null;
  assigned_agent_id?: string | null;
  title: string;
  description: string;
  state: RustTaskState;
  dependencies: string[];
  created_at: string;
  updated_at: string;
  retry_count: number;
  dependency_output_policy?: RustDependencyOutputPolicy;
}

export interface RustBatchTaskSpec {
  key: string;
  title: string;
  description: string;
  assigned_agent_id?: string | null;
  parent_task_key?: string | null;
  parent_task_id?: string | null;
  dependency_keys?: string[];
  dependency_task_ids?: string[];
  dependency_output_policy?: RustDependencyOutputPolicy;
}

export type RustRunState =
  | 'queued'
  | 'starting'
  | 'running'
  | 'paused'
  | 'succeeded'
  | 'failed'
  | 'cancelled';

export interface RustRunRecord {
  id: string;
  task_id: string;
  agent_id: string;
  attempt: number;
  state: RustRunState;
  started_at?: string | null;
  finished_at?: string | null;
}

export interface RustCancellationSummary {
  cancelled_tasks: string[];
  cancelled_runs: string[];
  cancelled_approvals: string[];
}

export interface RustProviderInstance {
  id: string;
  provider_id: string;
  display_name: string;
  protocol?: unknown;
  endpoint?: unknown;
  authentication?: unknown;
  enabled: boolean;
}

export interface RustModelLimits {
  context_window_tokens?: number | null;
  max_output_tokens?: number | null;
}

export interface RustModelDescriptor {
  provider_instance_id: string;
  id: string;
  display_name: string;
  capabilities?: unknown;
  limits: RustModelLimits;
  metadata_source?: 'manual' | 'static_catalog' | 'provider_discovery';
}

export interface RustIpcEvent {
  sequence: number;
  timestamp: string;
  event_type: string;
  payload: unknown;
}

export interface DaemonBootstrapInfo {
  endpoint: string;
  key_id: string;
  client_key: string;
  admin_key?: string | null;
  pid: number;
  created_at: number;
}

export type RustIpcCommand =
  | {
      type: 'negotiate_capabilities';
      protocol_version: RustProtocolVersion;
      capabilities: string[];
    }
  | {
      type: 'authenticate';
      header: RustHmacAuthHeader;
      payload?: number[] | null;
    }
  | {
      type: 'ping';
      payload: string;
      timestamp: number;
    }
  | {
      type: 'get_health';
    }
  | {
      type: 'subscribe_events';
      from_sequence?: number;
    }
  | {
      type: 'request_approval';
      contract: RustApprovalContract;
    }
  | {
      type: 'resolve_approval';
      approval_id: string;
      decision: RustApprovalDecision;
    }
  | {
      type: 'create_studio';
      name: string;
    }
  | {
      type: 'list_studios';
    }
  | {
      type: 'get_studio';
      studio_id: string;
    }
  | {
      type: 'submit_task';
      studio_id: string;
      task_spec: RustBatchTaskSpec;
    }
  | {
      type: 'cancel_task';
      studio_id: string;
      task_id: string;
      reason?: string | null;
    }
  | {
      type: 'list_tasks';
      studio_id: string;
    }
  | {
      type: 'get_task';
      studio_id: string;
      task_id: string;
    }
  | {
      type: 'list_runs';
      studio_id: string;
    }
  | {
      type: 'get_run';
      studio_id: string;
      run_id: string;
    }
  | {
      type: 'list_providers';
    }
  | {
      type: 'get_provider';
      provider_id: string;
    }
  | {
      type: 'list_models';
      instance_id?: string;
    }
  | {
      type: 'shutdown';
    };

export type RustIpcResponse =
  | {
      type: 'capabilities_negotiated';
      selected_version: RustProtocolVersion;
      agreed_capabilities: string[];
    }
  | {
      type: 'authenticated';
      session_id: string;
    }
  | {
      type: 'pong';
      payload: string;
      client_timestamp: number;
      server_timestamp: number;
    }
  | {
      type: 'health';
      status: string;
      active_clients: number;
      uptime_secs: number;
      memory_events_count: number;
    }
  | {
      type: 'event';
      event: RustIpcEvent;
    }
  | {
      type: 'approval_requested';
      approval_id: string;
    }
  | {
      type: 'approval_resolved';
      approval_id: string;
      status: RustApprovalStatus;
    }
  | {
      type: 'studio_created';
      studio: RustStudio;
    }
  | {
      type: 'studios';
      studios: RustStudio[];
    }
  | {
      type: 'studio';
      studio: RustStudio | null;
    }
  | {
      type: 'tasks_submitted';
      task_ids: string[];
    }
  | {
      type: 'task_cancelled';
      summary: RustCancellationSummary;
    }
  | {
      type: 'tasks';
      tasks: RustTaskRecord[];
    }
  | {
      type: 'task';
      task: RustTaskRecord | null;
    }
  | {
      type: 'runs';
      runs: RustRunRecord[];
    }
  | {
      type: 'run';
      run: RustRunRecord | null;
    }
  | {
      type: 'providers';
      providers: RustProviderInstance[];
    }
  | {
      type: 'provider';
      provider: RustProviderInstance | null;
    }
  | {
      type: 'models';
      models: RustModelDescriptor[];
    }
  | {
      type: 'shutdown_acknowledged';
    }
  | {
      type: 'error';
      message: string;
    };

export type RustHealthResponse = Extract<RustIpcResponse, { type: 'health' }>;

/* eslint-enable @typescript-eslint/naming-convention */
