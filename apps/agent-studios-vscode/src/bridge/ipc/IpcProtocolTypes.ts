/**
 * Agent Studios — IPC-03 Integration Types & Protocol Definitions
 * Milestone M14-UI-02: Extension Hardening & IPC-03 Integration Preparation
 *
 * Defines the native wire protocol types for communication between the VS Code
 * Extension Host and the authoritative Agent Studios daemon (agent-studios-rs / M13 IPC).
 */

export const IPC_PROTOCOL_VERSION = '1.0';

export type IpcTransportType = 'unix_socket' | 'named_pipe';

export interface IpcEndpointConfig {
  transport: IpcTransportType;
  path: string;
  timeoutMs: number;
  authToken?: string;
}

export type IpcMethod =
  | 'daemon.handshake'
  | 'daemon.getStatus'
  | 'session.list'
  | 'session.create'
  | 'session.attach'
  | 'session.switch'
  | 'session.sendMessage'
  | 'session.cancelRun'
  | 'approval.list'
  | 'approval.decide'
  | 'artifact.list'
  | 'artifact.getDiff'
  | 'provider.list';

export type IpcEventName =
  | 'session.event'
  | 'session.switched'
  | 'chat.streamChunk'
  | 'approval.required'
  | 'approval.decided'
  | 'task.progress'
  | 'artifact.created'
  | 'daemon.stateChanged';

export interface IpcRequestFrame<T = unknown> {
  version: string;
  id: string;
  method: IpcMethod;
  params: T;
  timestamp: number;
}

export interface IpcResponseFrame<T = unknown> {
  version: string;
  id: string;
  result?: T;
  error?: IpcErrorFrame;
  timestamp: number;
}

export interface IpcErrorFrame {
  code: number;
  message: string;
  data?: unknown;
}

export interface IpcEventFrame<T = unknown> {
  version: string;
  event: IpcEventName;
  data: T;
  timestamp: number;
  sequenceNumber: number;
}

export interface HandshakeParams {
  clientId: string;
  clientType: 'vscode_extension';
  clientVersion: string;
  protocolVersion: string;
  pid: number;
  originWorkspace?: string;
}

export interface HandshakeResult {
  daemonVersion: string;
  protocolVersion: string;
  serverPid: number;
  supportedMethods: IpcMethod[];
  requiresAuth: boolean;
  capabilities: {
    multiSession: boolean;
    streaming: boolean;
    taskGraph: boolean;
    diffInspection: boolean;
    providerDiscovery: boolean;
  };
}

/* eslint-disable @typescript-eslint/naming-convention */
export enum IpcErrorCode {
  ParseError = -32700,
  InvalidRequest = -32600,
  MethodNotFound = -32601,
  InvalidParams = -32602,
  InternalError = -32603,
  Unauthorized = -32001,
  SessionNotFound = -32002,
  ApprovalNotFound = -32003,
  ArtifactNotFound = -32004,
  DaemonBusy = -32005,
  DaemonTerminating = -32006,
}
/* eslint-enable @typescript-eslint/naming-convention */
