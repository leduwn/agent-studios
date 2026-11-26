/**
 * Agent Studios — Canonical English (en-US) UI Localization Catalog
 * Milestone M15.1 + MM07: Bilingual English & Vietnamese Localization
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
  'startup.welcomeTitle': 'What would you like to work on?',
  'startup.welcomeSubtitle': 'Ask questions, plan changes, or let Agent mode edit and test your workspace.',
  'startup.goalStartersHeading': 'Suggested Starters',
  'startup.starterAuditArchitecture': 'Explain how this workspace is structured and where core logic lives',
  'startup.starterDiagnoseTests': 'Review recent changes and identify edge cases or missing tests',
  'startup.starterImplementFeature': 'Implement feature with isolated worktree verification',
  'startup.activeWorkspaceCard': 'Workspace',
  'startup.noWorkspaceSelected': 'No folder open',
  'startup.sessionContinuityCard': 'Conversation History',
  'startup.continuePreviousSession': 'Continue Previous Conversation ({sessionId})',

  // Header, Composer & Context Controls
  'header.newChat': 'New Chat',
  'header.history': 'History',
  'header.settings': 'Providers & Models Settings',
  'header.missionControl': 'Mission Control',
  'composer.placeholder': 'Ask a question, plan changes, or describe a task…',
  'composer.submitTask': 'Send',
  'composer.cancelRun': 'Stop',
  'composer.attachContext': 'Attach Context',
  'composer.workspaceSelectorLabel': 'Workspace',
  'composer.workspaceSelectorTooltip': 'Active workspace root ({workspace})',
  'composer.modelSelectorLabel': 'Model',
  'composer.modelSelectorTooltip': 'Select active AI model',
  'composer.runtimeSelectorLabel': 'Runtime',
  'composer.runtimeSelectorTooltip': 'Select execution runtime',
  'composer.reasoningSelectorLabel': 'Reasoning',
  'composer.sessionHistoryLabel': 'Recent Conversations',
  'composer.sessionHistoryEmpty': 'No previous conversations yet',
  'composer.sessionHistoryCount': '{count} saved conversations available',
  'composer.modeAsk': 'Ask',
  'composer.modePlan': 'Plan',
  'composer.modeAgent': 'Agent',

  // 9 Connection States & Action CTAs
  'status.notConfigured': 'Add a provider',
  'status.starting': 'Starting…',
  'status.connecting': 'Connecting…',
  'status.ready': 'Ready',
  'status.running': 'Working…',
  'status.waitingForApproval': 'Approval needed',
  'status.reconnecting': 'Reconnecting…',
  'status.error': 'Error',
  'status.disconnected': 'Offline',
  'action.configureProvider': 'Configure Provider',
  'action.openFolder': 'Open Folder',
  'action.connect': 'Connect',
  'action.retry': 'Retry Connection',

  // Providers & Models Settings
  'settings.title': 'Providers & Models',
  'settings.subtitle': 'Configure cloud or local model endpoints. API keys are saved in VS Code SecretStorage.',
  'settings.addProvider': 'Add Provider',
  'settings.discoverModels': 'Discover Models',
  'settings.testConnection': 'Test Connection',
  'settings.saveProvider': 'Save Provider',

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
  'approvals.inboxTitle': 'Pending Approvals',
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

  // Clean First-Run Onboarding
  'onboarding.welcomeHeader': 'Set up Agent Studios',
  'onboarding.daemonOfflineTitle': 'Engine is offline',
  'onboarding.daemonOfflineBody': 'Start or connect the local assistant engine at {endpoint} to begin.',
  'onboarding.daemonConnectedTitle': 'Engine connected',
  'onboarding.daemonConnectedBody': 'Active conversation {sessionId} is ready.',
  'onboarding.connectDaemonButton': 'Connect',
  'onboarding.retryConnectionButton': 'Retry Connection',
  'onboarding.getStartedButton': 'Configure Provider',
  'onboarding.securityNoticeTitle': 'Secret Storage',
  'onboarding.securityNoticeBody': 'API keys are stored encrypted in VS Code SecretStorage and synced securely to the daemon vault.',

  // Diagnostics & System Status Messages
  'diagnostics.statusHealthy': 'Engine & IPC Transport Ready',
  'diagnostics.statusDegraded': 'Connection Degraded — Reconnecting',
  'diagnostics.statusDisconnected': 'Offline',
  'diagnostics.protocolMismatch': 'Incompatible IPC protocol version detected at {endpoint}.',
  'diagnostics.sqliteStateHealth': 'Authoritative SQLite state store verified at {path}.',
  'diagnostics.artifactStoreHealth': 'Content-addressed artifact store verified ({count} artifacts).',
} as const;

export type TranslationKey = keyof typeof enCatalog;
export type TranslationCatalog = Record<TranslationKey, string>;

export const EN_US_CATALOG: TranslationCatalog = enCatalog;
export default EN_US_CATALOG;
