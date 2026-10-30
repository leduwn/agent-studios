/**
 * Agent Studios — Rust IPC Wire Protocol Types (Approach A)
 * Milestone M14-UI-02.1: Protocol Correctness & Wire Compatibility
 *
 * Defines the exact wire serialization types for the authoritative Rust daemon
 * (agent-studios-rs / crates ipc-protocol & ipc-transport).
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

export type RustIpcCommand =
  | {
      type: 'negotiate_capabilities';
      protocol_version: RustProtocolVersion;
      capabilities: string[];
    }
  | {
      type: 'authenticate';
      header: RustHmacAuthHeader;
      payload: unknown | null;
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
      contract: unknown;
    }
  | {
      type: 'resolve_approval';
      approval_id: string;
      decision: unknown;
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
      task_spec: unknown;
    }
  | {
      type: 'cancel_task';
      studio_id?: string;
      task_id: string;
      reason?: string;
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
    }
  | {
      type: 'custom';
      action: string;
      payload: unknown;
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
      event: unknown;
    }
  | {
      type: 'approval_requested';
      approval_id: string;
    }
  | {
      type: 'approval_resolved';
      approval_id: string;
      status: string;
    }
  | {
      type: 'studio_created';
      studio: unknown;
    }
  | {
      type: 'studios';
      studios: unknown[];
    }
  | {
      type: 'studio';
      studio: unknown;
    }
  | {
      type: 'tasks_submitted';
      task_ids: string[];
    }
  | {
      type: 'task_cancelled';
      summary: {
        cancelled_task_ids: string[];
        cancelled_run_ids: string[];
      };
    }
  | {
      type: 'tasks';
      tasks: unknown[];
    }
  | {
      type: 'task';
      task: unknown;
    }
  | {
      type: 'runs';
      runs: unknown[];
    }
  | {
      type: 'run';
      run: unknown;
    }
  | {
      type: 'providers';
      providers: unknown[];
    }
  | {
      type: 'provider';
      provider: unknown;
    }
  | {
      type: 'models';
      models: unknown[];
    }
  | {
      type: 'shutdown_acknowledged';
    }
  | {
      type: 'error';
      message: string;
      details?: unknown;
      code?: string;
    };

/* eslint-enable @typescript-eslint/naming-convention */
