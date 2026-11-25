/**
 * Agent Studios — Canonical English (en-US) UI Localization Catalog
 * Milestone M15.1: First-Class Bilingual English & Vietnamese Localization
 */
/* eslint-disable @typescript-eslint/naming-convention */

export const EN_US_LOCALE = 'en-US' as const;
export const EN_BASE_LOCALE = 'en' as const;

export const enCatalog = {
  // Mode Switching & Workbench Shell
  'mode.agentMode': 'Agent Mode',
  'mode.ideMode': 'IDE Mode',
  'mode.openInIde': 'Open in IDE Mode',
  'mode.switchToAgentMode': 'Switch to Agent Mode',
  'mode.missionControl': 'Mission Control',
  'mode.openMissionControl': 'Open Mission Control Workbench',

  // Agent Mode Startup Screen & Welcome Surface
  'startup.welcomeTitle': 'Agent-First AI Engineering Workbench',
  'startup.welcomeSubtitle': 'Formulate engineering goals, orchestrate multi-agent runs, and review verified diffs.',
  'startup.goalStartersHeading': 'Goal Formulation Starters',
  'startup.starterAuditArchitecture': 'Audit repository architecture and identify coupling hotspots',
  'startup.starterDiagnoseTests': 'Diagnose failing integration tests and propose minimal fixes',
  'startup.starterImplementFeature': 'Implement feature with isolated worktree verification',
  'startup.activeWorkspaceCard': 'Active Workspace',
  'startup.noWorkspaceSelected': 'No workspace folder selected',
  'startup.sessionContinuityCard': 'Session Continuity',
  'startup.continuePreviousSession': 'Continue Previous Session ({sessionId})',

  // Composer & Context Controls
  'composer.placeholder': 'Describe an engineering goal or task for the Coordinator...',
  'composer.submitTask': 'Submit Goal',
  'composer.cancelRun': 'Cancel Active Run',
  'composer.attachContext': 'Attach Context',
  'composer.workspaceSelectorLabel': 'Workspace',
  'composer.workspaceSelectorTooltip': 'Select active workspace root ({workspace})',
  'composer.modelSelectorLabel': 'Model',
  'composer.modelSelectorTooltip': 'Select coordinator or worker model adapter',
  'composer.runtimeSelectorLabel': 'Runtime',
  'composer.runtimeSelectorTooltip': 'Select execution runtime backend',
  'composer.sessionHistoryLabel': 'Session History',
  'composer.sessionHistoryEmpty': 'No previous sessions recorded in this workspace',
  'composer.sessionHistoryCount': '{count} saved sessions available',

  // Mission Control 9 Canonical Tabs
  'missionControl.title': 'Mission Control',
  'missionControl.tab.overview': 'Overview',
  'missionControl.tab.fleet': 'Fleet',
  'missionControl.tab.tasks': 'Tasks (DAG)',
  'missionControl.tab.runs': 'Runs',
  'missionControl.tab.timeline': 'Timeline',
  'missionControl.tab.review': 'Review & Diff',
  'missionControl.tab.approvals': 'Approvals',
  'missionControl.tab.providers': 'Providers',
  'missionControl.tab.diagnostics': 'Diagnostics',

  // Mission Control Views & Domain Concepts
  'missionControl.taskGraphTitle': 'Task Graph (DAG)',
  'missionControl.worktreeLabel': 'Worktree Branch',
  'missionControl.artifactDiffLabel': 'Artifact Patch (Diff)',
  'missionControl.activeTasksSummary': '{count} active tasks in execution graph',
  'missionControl.taskStatusSummary': 'Task {taskId}: {status}',

  // Approval Risk Tiers & 2PC Operator Confirmation Modal
  'approvals.inboxTitle': 'Pending Approvals Inbox',
  'approvals.emptyInbox': 'No pending approval requests',
  'approvals.approveAction': 'Approve',
  'approvals.denyAction': 'Deny',
  'approvals.riskTier.low': 'Low Risk — Read-Only Operation',
  'approvals.riskTier.medium': 'Medium Risk — Scoped Workspace Mutation',
  'approvals.riskTier.high': 'High Risk — Destructive or External Command',
  'approvals.riskTier.critical': 'Critical Risk — Privileged System or Boundary Action',
  'approvals.modal.twoPhaseTitle': 'Two-Phase Operator Confirmation Required',
  'approvals.modal.twoPhasePrompt': 'Review high-risk action for task {taskId} ({riskTier}) before authorizing execution.',
  'approvals.modal.commandPreview': 'Command to execute: {command}',
  'approvals.modal.confirmCommit': 'Authorize & Execute Action',
  'approvals.modal.abortAction': 'Abort & Deny Request',
  'approvals.modal.expiredNotice': 'Approval challenge expired; request a new confirmation token.',

  // First-Run Onboarding Wizard
  'onboarding.welcomeHeader': 'Welcome to Agent Studios',
  'onboarding.daemonOfflineTitle': 'Agent Studios Daemon Offline',
  'onboarding.daemonOfflineBody': 'Unable to reach authoritative daemon at {endpoint}. Start agsd or enable Developer Preview.',
  'onboarding.daemonConnectedTitle': 'Connected to Agent Studios Daemon',
  'onboarding.daemonConnectedBody': 'Active session {sessionId} is synchronized with authoritative daemon state.',
  'onboarding.previewModeTitle': 'Developer Preview Mode Active',
  'onboarding.previewModeBody': 'Exploring workbench UI with deterministic fixtures without modifying your workspace files.',
  'onboarding.enablePreviewButton': 'Enable Developer Preview',
  'onboarding.connectDaemonButton': 'Connect to Daemon',
  'onboarding.retryConnectionButton': 'Retry Connection',
  'onboarding.getStartedButton': 'Get Started',
  'onboarding.securityNoticeTitle': 'Security Notice',
  'onboarding.securityNoticeBody': 'Provider credentials are managed exclusively by the OS keyring and daemon CLI; never enter raw API keys in webviews.',

  // Diagnostics & System Status Messages
  'diagnostics.statusHealthy': 'Daemon & IPC Transport Healthy',
  'diagnostics.statusDegraded': 'Daemon Connection Degraded — Reconnecting',
  'diagnostics.statusDisconnected': 'Disconnected from Authoritative Daemon',
  'diagnostics.protocolMismatch': 'Incompatible IPC protocol version detected with daemon at {endpoint}.',
  'diagnostics.sqliteStateHealth': 'Authoritative SQLite state store verified at {path}.',
  'diagnostics.artifactStoreHealth': 'Content-addressed artifact store verified ({count} artifacts).',
} as const;

export type TranslationKey = keyof typeof enCatalog;
export type TranslationCatalog = Record<TranslationKey, string>;

export const EN_US_CATALOG: TranslationCatalog = enCatalog;
export default EN_US_CATALOG;
