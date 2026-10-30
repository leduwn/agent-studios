/**
 * Agent Studios — IPC-03 Integration Readiness Matrix
 * Milestone M14-UI-02: Extension Hardening & IPC-03 Integration Preparation
 *
 * Tracks the integration readiness and contract status between VS Code Extension
 * and the Rust IPC Control Plane (M13 IPC-00, IPC-01, IPC-02).
 */

export type ReadinessState =
  | 'IMPLEMENTED_IN_RUST'
  | 'IMPLEMENTED_UNVERIFIED'
  | 'MOCK_ONLY'
  | 'MISSING_IN_RUST'
  | 'READY_FOR_INTEGRATION';

export interface IpcCapabilityStatus {
  id: string;
  name: string;
  method: string;
  state: ReadinessState;
  rustCrate: string;
  prReference?: string;
  securityVerdict: 'VERIFIED' | 'PENDING_M13_REVIEW' | 'FAIL_CLOSED';
  notes: string;
}

export const IPC_03_READINESS_MATRIX: IpcCapabilityStatus[] = [
  {
    id: 'daemon.handshake',
    name: 'Daemon Connection Handshake',
    method: 'daemon.handshake',
    state: 'IMPLEMENTED_IN_RUST',
    rustCrate: 'agent-studios-ipc',
    prReference: 'PR #17 / PR #18',
    securityVerdict: 'PENDING_M13_REVIEW',
    notes: 'Framing, protocol version negotiation, client identification supported via UDS / Named Pipes.',
  },
  {
    id: 'session.list',
    name: 'Session Discovery and List',
    method: 'session.list',
    state: 'IMPLEMENTED_IN_RUST',
    rustCrate: 'agent-studios-control-plane',
    prReference: 'PR #19',
    securityVerdict: 'PENDING_M13_REVIEW',
    notes: 'Multi-session discovery implemented in PR #19 branch.',
  },
  {
    id: 'session.create',
    name: 'Session Creation with Goal',
    method: 'session.create',
    state: 'IMPLEMENTED_IN_RUST',
    rustCrate: 'agent-studios-control-plane',
    prReference: 'PR #19',
    securityVerdict: 'PENDING_M13_REVIEW',
    notes: 'Creates isolated session workspace in daemon.',
  },
  {
    id: 'session.switch',
    name: 'Session Switch / Re-attach',
    method: 'session.switch',
    state: 'IMPLEMENTED_IN_RUST',
    rustCrate: 'agent-studios-control-plane',
    prReference: 'PR #19',
    securityVerdict: 'PENDING_M13_REVIEW',
    notes: 'Re-attaches event stream subscription to target session.',
  },
  {
    id: 'session.sendMessage',
    name: 'Chat Message Dispatch & Streaming',
    method: 'session.sendMessage',
    state: 'IMPLEMENTED_UNVERIFIED',
    rustCrate: 'runtime-transport',
    prReference: 'PR #18 / PR #19',
    securityVerdict: 'PENDING_M13_REVIEW',
    notes: 'Streaming text deltas and tool calls over native transport.',
  },
  {
    id: 'session.cancelRun',
    name: 'Active Run Cancellation',
    method: 'session.cancelRun',
    state: 'IMPLEMENTED_IN_RUST',
    rustCrate: 'agent-studios-control-plane',
    prReference: 'PR #19',
    securityVerdict: 'PENDING_M13_REVIEW',
    notes: 'SIGINT signal propagation to active coordinator.',
  },
  {
    id: 'approval.decide',
    name: 'Privileged Approval Decision Dispatch',
    method: 'approval.decide',
    state: 'IMPLEMENTED_UNVERIFIED',
    rustCrate: 'agent-studios-ipc',
    prReference: 'PR #18',
    securityVerdict: 'FAIL_CLOSED',
    notes: 'Origin-bound HMAC authorization required before un-gating high-risk approvals in production.',
  },
  {
    id: 'artifact.getDiff',
    name: 'Virtual Patch / Diff Retrieval',
    method: 'artifact.getDiff',
    state: 'READY_FOR_INTEGRATION',
    rustCrate: 'agent-studios-control-plane',
    prReference: 'PR #19',
    securityVerdict: 'VERIFIED',
    notes: 'Read-only diff document serving via agent-studios-diff:// virtual URI provider.',
  },
  {
    id: 'provider.list',
    name: 'Sanitized Provider & Model Summaries',
    method: 'provider.list',
    state: 'IMPLEMENTED_IN_RUST',
    rustCrate: 'provider-management',
    prReference: 'PR #20',
    securityVerdict: 'VERIFIED',
    notes: 'Strictly zero secrets returned to client; context window and model metadata only.',
  },
];

export function getReadinessSummary(): {
  total: number;
  ready: number;
  implemented: number;
  unverified: number;
  missing: number;
} {
  let ready = 0;
  let implemented = 0;
  let unverified = 0;
  let missing = 0;

  for (const cap of IPC_03_READINESS_MATRIX) {
    if (cap.state === 'READY_FOR_INTEGRATION') {
      ready++;
    } else if (cap.state === 'IMPLEMENTED_IN_RUST') {
      implemented++;
    } else if (cap.state === 'IMPLEMENTED_UNVERIFIED') {
      unverified++;
    } else if (cap.state === 'MISSING_IN_RUST') {
      missing++;
    }
  }

  return {
    total: IPC_03_READINESS_MATRIX.length,
    ready,
    implemented,
    unverified,
    missing,
  };
}
