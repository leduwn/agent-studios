/**
 * Agent Studios — Card Component Types and Contracts
 * Milestone M14-UI-01: In-Conversation Activities & Diff Review
 */

import { ApprovalRiskLevel, ApprovalStatus } from '../bridge/types';

export type SupportedTool =
  | 'read_file'
  | 'edit_file'
  | 'search_codebase'
  | 'exec_command'
  | 'run_tests'
  | 'apply_patch'
  | 'review_code'
  | 'delegate_agent'
  | string;

export type ToolCallStatus = 'running' | 'completed' | 'failed';

export interface ToolCallCardData {
  tool: SupportedTool;
  target?: string;
  status: ToolCallStatus;
  durationMs?: number;
  summary?: string;
  output?: string;
  error?: string;
  id?: string;
}

export type CardRiskLevel =
  | 'LOW'
  | 'MEDIUM'
  | 'HIGH'
  | 'CRITICAL'
  | 'low'
  | 'medium'
  | 'high'
  | 'critical'
  | ApprovalRiskLevel;

export interface ApprovalCardData {
  id: string;
  status: ApprovalStatus;
  riskLevel: CardRiskLevel;
  commandOrTarget: string;
  actionType?: string;
  taskTitle?: string;
  requestedPermissions?: string[];
  createdAt?: number;
  expiresAt?: number;
}

export interface TaskProgressCardData {
  coordinatorName?: string;
  activeAgentName?: string;
  completedTasks: number;
  totalTasks: number;
  progressPercentage?: number;
  status?: string;
}

export interface ChangedFileEntry {
  path: string;
  additions: number;
  deletions: number;
  artifactId?: string;
  artifactName?: string;
}

export interface ChangedFilesCardData {
  files: ChangedFileEntry[];
  title?: string;
  totalAdditions?: number;
  totalDeletions?: number;
}

export interface BackendReconciliationProof {
  verified: boolean;
  reconciliationId: string;
  signature?: string;
  timestamp?: number;
}

export interface ReconciliationResult {
  success: boolean;
  reconciled: boolean;
  filePath: string;
  message: string;
}
