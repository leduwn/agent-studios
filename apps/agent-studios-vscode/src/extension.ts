/**
 * Agent Studios — Code-OSS Native Agents Workbench Extension Entry Point
 * Milestone M14-UI-00: Code-OSS Native Agents Workbench
 */

import * as vscode from 'vscode';
import { AgentStudiosBridge } from './bridge/AgentStudiosBridge';
import { BridgeManager } from './bridge/BridgeManager';
import { DisconnectedBridge } from './bridge/DisconnectedBridge';
import { FixtureBridge } from './bridge/FixtureBridge';
import {
  registerAllCommands,
  ViewProviders,
  WebviewManagers,
} from './commands/CommandsRegistry';
import { registerApprovalCommands } from './commands/ApprovalCommands';
import { registerArtifactCommands } from './commands/ArtifactCommands';
import {
  ApprovalsTreeProvider,
  ArtifactsTreeProvider,
  ProvidersTreeProvider,
  SessionTreeProvider,
  TasksTreeProvider,
  TimelineTreeProvider,
} from './views';
import { SessionDetailWebview } from './webviews/SessionDetailWebview';
import { TaskGraphWebview } from './webviews/TaskGraphWebview';
import { ChatWebviewViewProvider } from './chat/ChatWebviewViewProvider';
import { DiffReviewManager } from './cards/DiffReviewManager';
import { MissionControlProjection } from './mission-control/state/MissionControlProjection';
import { registerMissionControlCommands } from './mission-control/MissionControlCommands';

let statusBarItem: vscode.StatusBarItem;

/**
 * Extension activation lifecycle entry point.
 */
export async function activate(context: vscode.ExtensionContext): Promise<void> {
  // 1. Initialize BridgeManager and SessionStateProjection
  const bridgeManager = new BridgeManager();
  context.subscriptions.push(bridgeManager);
  const projection = bridgeManager.stateProjection;

  // 2. Instantiate and register all 6 sidebar tree views
  const sessionView = new SessionTreeProvider(bridgeManager);
  const tasksView = new TasksTreeProvider(bridgeManager);
  const approvalsView = new ApprovalsTreeProvider(bridgeManager);
  const artifactsView = new ArtifactsTreeProvider(bridgeManager);
  const timelineView = new TimelineTreeProvider(bridgeManager);
  const providersView = new ProvidersTreeProvider(bridgeManager);

  context.subscriptions.push(
    sessionView,
    tasksView,
    approvalsView,
    artifactsView,
    timelineView,
    providersView
  );

  context.subscriptions.push(
    vscode.window.registerTreeDataProvider('agent-studios.session', sessionView),
    vscode.window.registerTreeDataProvider('agent-studios.tasks', tasksView),
    vscode.window.registerTreeDataProvider('agent-studios.approvals', approvalsView),
    vscode.window.registerTreeDataProvider('agent-studios.artifacts', artifactsView),
    vscode.window.registerTreeDataProvider('agent-studios.timeline', timelineView),
    vscode.window.registerTreeDataProvider('agent-studios.providers', providersView)
  );

  // 2b. Register Chat & Assistant WebviewView provider
  const chatProvider = new ChatWebviewViewProvider(context.extensionUri, bridgeManager);
  context.subscriptions.push(
    chatProvider,
    vscode.window.registerWebviewViewProvider(ChatWebviewViewProvider.viewType, chatProvider)
  );

  const views: ViewProviders = {
    session: sessionView,
    tasks: tasksView,
    approvals: approvalsView,
    artifacts: artifactsView,
    timeline: timelineView,
    providers: providersView,
  };

  const webviews: WebviewManagers = {
    openSession: async () => {
      const session = await bridgeManager.bridge.getCurrentSession();
      const tasks = await bridgeManager.bridge.listTasks();
      SessionDetailWebview.createOrShow(
        context.extensionUri,
        session,
        tasks,
        undefined,
        bridgeManager.bridge
      );
    },
    showTaskGraph: async () => {
      const graph = await bridgeManager.bridge.getTaskGraph();
      TaskGraphWebview.createOrShow(
        context.extensionUri,
        graph,
        bridgeManager.bridge
      );
    },
  };

  // Dynamic bridge proxy ensuring commands always dispatch against current active bridge
  const dynamicBridge: AgentStudiosBridge = {
    get connectionState() {
      return bridgeManager.bridge.connectionState;
    },
    get onConnectionStateChanged() {
      return bridgeManager.bridge.onConnectionStateChanged;
    },
    get onEvent() {
      return bridgeManager.bridge.onEvent;
    },
    connect: () => bridgeManager.bridge.connect(),
    disconnect: () => bridgeManager.bridge.disconnect(),
    getCurrentSession: () => bridgeManager.bridge.getCurrentSession(),
    getTaskGraph: (runId?: string) => bridgeManager.bridge.getTaskGraph(runId),
    listTasks: () => bridgeManager.bridge.listTasks(),
    listPendingApprovals: () => bridgeManager.bridge.listPendingApprovals(),
    requestApprovalDecision: (id: string, decision: 'approved' | 'denied') =>
      bridgeManager.bridge.requestApprovalDecision(id, decision),
    listArtifacts: () => bridgeManager.bridge.listArtifacts(),
    getArtifactDiff: (id: string) => bridgeManager.bridge.getArtifactDiff(id),
    listProviderSummaries: () => bridgeManager.bridge.listProviderSummaries(),
    submitEditorAction: (action) => bridgeManager.bridge.submitEditorAction(action),
    sendMessage: (request, onChunk) => bridgeManager.bridge.sendMessage(request, onChunk),
    cancelCurrentRun: () => bridgeManager.bridge.cancelCurrentRun(),
    listSessions: () => bridgeManager.bridge.listSessions(),
    createSession: (goal) => bridgeManager.bridge.createSession(goal),
    switchSession: (sessionId) => bridgeManager.bridge.switchSession(sessionId),
    dispose: () => {},
  };

  // 3. Register all commands and providers
  registerAllCommands(context, bridgeManager, views, webviews);
  registerApprovalCommands(context, dynamicBridge, projection);
  registerArtifactCommands(context, dynamicBridge);

  // 3b. Register Mission Control Workbench (Milestone M14)
  const diffReviewManager = new DiffReviewManager(dynamicBridge);
  context.subscriptions.push(diffReviewManager);
  const missionControlProjection = new MissionControlProjection(bridgeManager);
  context.subscriptions.push(missionControlProjection);
  registerMissionControlCommands(context, missionControlProjection, diffReviewManager);

  // 4. Create and update status bar item
  statusBarItem = vscode.window.createStatusBarItem(
    vscode.StatusBarAlignment.Left,
    100
  );
  statusBarItem.command = 'agent-studios.openAgents';
  context.subscriptions.push(statusBarItem);

  const updateStatusBar = () => {
    const isPreview = bridgeManager.isPreviewMode;
    const connState = bridgeManager.bridge.connectionState;
    const session = bridgeManager.stateProjection.session;
    const agentCount = session?.activeAgentCount ?? 0;
    const previewBadge = isPreview ? ' [Preview]' : '';

    if (connState === 'connected') {
      statusBarItem.text = `$(hubot) Agent Studios: ${agentCount} active${previewBadge}`;
      statusBarItem.tooltip = `Agent Studios: Connected (${agentCount} active agents)${previewBadge}`;
    } else if (connState === 'connecting' || connState === 'reconnecting') {
      statusBarItem.text = `$(sync~spin) Agent Studios: Connecting...${previewBadge}`;
      statusBarItem.tooltip = `Agent Studios: Connecting to daemon...${previewBadge}`;
    } else if (connState === 'authenticating') {
      statusBarItem.text = `$(key) Agent Studios: Authenticating...${previewBadge}`;
      statusBarItem.tooltip = `Agent Studios: Authenticating with daemon credentials...${previewBadge}`;
    } else if (connState === 'degraded') {
      statusBarItem.text = `$(warning) Agent Studios: Degraded${previewBadge}`;
      statusBarItem.tooltip = `Agent Studios: Daemon connection degraded${previewBadge}`;
    } else if (connState === 'auth_required' || connState === 'authentication_failed') {
      statusBarItem.text = `$(error) Agent Studios: Auth Failed${previewBadge}`;
      statusBarItem.tooltip = `Agent Studios: Authentication failed. Please verify credentials.${previewBadge}`;
    } else if (connState === 'incompatible' || connState === 'unsupported_protocol') {
      statusBarItem.text = `$(error) Agent Studios: Incompatible Protocol${previewBadge}`;
      statusBarItem.tooltip = `Agent Studios: Incompatible protocol version with daemon.${previewBadge}`;
    } else {
      statusBarItem.text = `$(circle-slash) Agent Studios: Disconnected${previewBadge}`;
      statusBarItem.tooltip = `Agent Studios: Disconnected. Click to open Agents Workbench.${previewBadge}`;
    }
    statusBarItem.show();
  };

  updateStatusBar();

  context.subscriptions.push(
    bridgeManager.onBridgeChanged(() => updateStatusBar()),
    bridgeManager.stateProjection.onSessionChanged(() => updateStatusBar())
  );

  // 5. Handle configuration changes
  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration(async (e) => {
      if (e.affectsConfiguration('agentStudios.previewMode')) {
        const config = vscode.workspace.getConfiguration('agentStudios');
        const isPreview = config.get<boolean>('previewMode', false);
        if (isPreview && !bridgeManager.isPreviewMode) {
          bridgeManager.setBridge(new FixtureBridge());
          vscode.commands.executeCommand('agent-studios.refreshViews');
        } else if (!isPreview && bridgeManager.isPreviewMode) {
          bridgeManager.setBridge(new DisconnectedBridge());
          vscode.commands.executeCommand('agent-studios.refreshViews');
        }
        updateStatusBar();
      }
    })
  );
}

/**
 * Extension deactivation lifecycle hook.
 */
export function deactivate(): void {
  // All resources cleaned up via context.subscriptions
}
