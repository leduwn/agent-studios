/**
 * Agent Studios — IPC-03 Integration Readiness Matrix
 * Milestone M14-UI-02.1: Protocol Correctness & Release Review
 *
 * Tracks the integration readiness and contract status between VS Code Extension
 * and the Rust IPC Control Plane (M13 IPC-00, IPC-01, IPC-02).
 *
 * Strict separation of states:
 * - VERIFIED_RUST_IMPLEMENTATION: Verified end-to-end against live Rust daemon
 * - TYPESCRIPT_CONTRACT_ONLY: TypeScript types exist, but backend wire method not yet built
 * - INCOMPATIBLE_WIRE_FORMAT: Wire format differs (e.g. JSON-RPC vs Serde tagged enum)
 * - MOCK_ONLY: Implemented only against in-memory fixture bridge
 * - UNIMPLEMENTED: Not implemented on either side
 * - NOT_TESTED: Code exists but lacks automated integration tests
 */

export type ReadinessState =
  | 'VERIFIED_RUST_IMPLEMENTATION'
  | 'TYPESCRIPT_CONTRACT_ONLY'
  | 'INCOMPATIBLE_WIRE_FORMAT'
  | 'MOCK_ONLY'
  | 'UNIMPLEMENTED'
  | 'NOT_TESTED';

export interface IpcCapabilityStatus {
  id: string;
  name: string;
  method: string;
  rustEquivalent: string;
  state: ReadinessState;
  rustCrate: string;
  prReference?: string;
  securityVerdict: 'VERIFIED' | 'PENDING_M13_REVIEW' | 'FAIL_CLOSED';
  notes: string;
}

export const IPC_03_READINESS_MATRIX: IpcCapabilityStatus[] = [
  {
    id: 'daemon.handshake',
    name: 'Daemon Connection Handshake & Authentication',
    method: 'daemon.handshake',
    rustEquivalent: 'negotiate_capabilities + authenticate (IpcCommand)',
    state: 'INCOMPATIBLE_WIRE_FORMAT',
    rustCrate: 'ipc-protocol / ipc-transport',
    prReference: 'PR #19',
    securityVerdict: 'FAIL_CLOSED',
    notes: 'Rust daemon requires two-step handshake: negotiate_capabilities followed by authenticate with HMAC-SHA256 signature. TypeScript JSON-RPC daemon.handshake will be rejected by Rust Serde deserializer.',
  },
  {
    id: 'daemon.getStatus',
    name: 'Daemon Health & State Inspection',
    method: 'daemon.getStatus',
    rustEquivalent: 'get_health (IpcCommand)',
    state: 'INCOMPATIBLE_WIRE_FORMAT',
    rustCrate: 'ipc-transport',
    prReference: 'PR #19',
    securityVerdict: 'PENDING_M13_REVIEW',
    notes: 'Rust returns { type: "health", status, active_clients, uptime_secs, memory_events_count }. TypeScript assumed JSON-RPC getStatus.',
  },
  {
    id: 'session.list',
    name: 'Studio / Session Discovery',
    method: 'session.list',
    rustEquivalent: 'list_studios (IpcCommand)',
    state: 'INCOMPATIBLE_WIRE_FORMAT',
    rustCrate: 'ipc-transport / agent-studios-control-plane',
    prReference: 'PR #19',
    securityVerdict: 'PENDING_M13_REVIEW',
    notes: 'Rust daemon models Studio workspaces (list_studios), whereas VS Code UI models Sessions. Wire schema requires translation adapter.',
  },
  {
    id: 'session.create',
    name: 'Studio / Session Workspace Creation',
    method: 'session.create',
    rustEquivalent: 'create_studio (IpcCommand)',
    state: 'INCOMPATIBLE_WIRE_FORMAT',
    rustCrate: 'ipc-transport / agent-studios-control-plane',
    prReference: 'PR #19',
    securityVerdict: 'PENDING_M13_REVIEW',
    notes: 'Rust creates a Studio workspace via { type: "create_studio", name }. TS assumed JSON-RPC session.create with goal.',
  },
  {
    id: 'session.switch',
    name: 'Session Re-attach & Event Subscription',
    method: 'session.switch',
    rustEquivalent: 'subscribe_events (IpcCommand)',
    state: 'UNIMPLEMENTED',
    rustCrate: 'ipc-transport',
    prReference: 'PR #19',
    securityVerdict: 'PENDING_M13_REVIEW',
    notes: 'No discrete session.switch command in Rust daemon. Handled client-side by re-subscribing stream from sequence number.',
  },
  {
    id: 'session.sendMessage',
    name: 'Task Submission & Message Execution',
    method: 'session.sendMessage',
    rustEquivalent: 'submit_task (IpcCommand)',
    state: 'INCOMPATIBLE_WIRE_FORMAT',
    rustCrate: 'ipc-transport / runtime-transport',
    prReference: 'PR #19',
    securityVerdict: 'PENDING_M13_REVIEW',
    notes: 'Rust daemon accepts submit_task { studio_id, task_spec }. Text deltas stream via event subscription.',
  },
  {
    id: 'session.cancelRun',
    name: 'Task Execution Cancellation',
    method: 'session.cancelRun',
    rustEquivalent: 'cancel_task (IpcCommand)',
    state: 'INCOMPATIBLE_WIRE_FORMAT',
    rustCrate: 'ipc-transport / agent-studios-control-plane',
    prReference: 'PR #19',
    securityVerdict: 'PENDING_M13_REVIEW',
    notes: 'Rust uses cancel_task { studio_id, task_id, reason }. Wire schema requires studio_id and task_id parameters.',
  },
  {
    id: 'approval.list',
    name: 'Pending Approvals Query',
    method: 'approval.list',
    rustEquivalent: 'event stream (approval_requested event)',
    state: 'UNIMPLEMENTED',
    rustCrate: 'ipc-transport',
    prReference: 'PR #19',
    securityVerdict: 'FAIL_CLOSED',
    notes: 'Rust pushes approvals reactively over the event stream. A poll-based approval.list command does not exist in Rust daemon.',
  },
  {
    id: 'approval.decide',
    name: 'Privileged Approval Resolution',
    method: 'approval.decide',
    rustEquivalent: 'resolve_approval (IpcCommand)',
    state: 'INCOMPATIBLE_WIRE_FORMAT',
    rustCrate: 'ipc-protocol / ipc-transport',
    prReference: 'PR #19',
    securityVerdict: 'FAIL_CLOSED',
    notes: 'Rust uses resolve_approval { approval_id, decision: { approved: { ... } } | { denied: { ... } } }. Strict origin binding required.',
  },
  {
    id: 'artifact.list',
    name: 'Artifact Discovery Query',
    method: 'artifact.list',
    rustEquivalent: 'event stream / task graph',
    state: 'UNIMPLEMENTED',
    rustCrate: 'ipc-transport',
    prReference: 'PR #19',
    securityVerdict: 'VERIFIED',
    notes: 'No discrete artifact.list wire command. Artifacts are emitted within task step completion events.',
  },
  {
    id: 'artifact.getDiff',
    name: 'Virtual Patch & Diff Inspection',
    method: 'artifact.getDiff',
    rustEquivalent: 'agent-studios-diff:// virtual URI provider',
    state: 'TYPESCRIPT_CONTRACT_ONLY',
    rustCrate: 'apps/agent-studios-vscode',
    prReference: 'PR #22',
    securityVerdict: 'VERIFIED',
    notes: 'Read-only diff document serving is implemented in VS Code extension host via virtual URI provider. Direct wire RPC not yet exposed.',
  },
  {
    id: 'provider.list',
    name: 'Provider & Model Discovery',
    method: 'provider.list',
    rustEquivalent: 'list_providers + list_models (IpcCommand)',
    state: 'INCOMPATIBLE_WIRE_FORMAT',
    rustCrate: 'ipc-transport / provider-management',
    prReference: 'PR #20',
    securityVerdict: 'VERIFIED',
    notes: 'Rust implements list_providers and list_models commands returning Serde tagged responses. Zero API secrets exposed.',
  },
];

export function getReadinessSummary(): {
  total: number;
  verifiedRust: number;
  typescriptContractOnly: number;
  incompatibleWireFormat: number;
  mockOnly: number;
  unimplemented: number;
  notTested: number;
} {
  let verifiedRust = 0;
  let typescriptContractOnly = 0;
  let incompatibleWireFormat = 0;
  let mockOnly = 0;
  let unimplemented = 0;
  let notTested = 0;

  for (const cap of IPC_03_READINESS_MATRIX) {
    switch (cap.state) {
      case 'VERIFIED_RUST_IMPLEMENTATION': {
        verifiedRust++;
        break;
      }
      case 'TYPESCRIPT_CONTRACT_ONLY': {
        typescriptContractOnly++;
        break;
      }
      case 'INCOMPATIBLE_WIRE_FORMAT': {
        incompatibleWireFormat++;
        break;
      }
      case 'MOCK_ONLY': {
        mockOnly++;
        break;
      }
      case 'UNIMPLEMENTED': {
        unimplemented++;
        break;
      }
      case 'NOT_TESTED': {
        notTested++;
        break;
      }
    }
  }

  return {
    total: IPC_03_READINESS_MATRIX.length,
    verifiedRust,
    typescriptContractOnly,
    incompatibleWireFormat,
    mockOnly,
    unimplemented,
    notTested,
  };
}
