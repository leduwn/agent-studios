/**
 * Agent Studios — Bridge Manager
 * Milestone M14-UI-00: Code-OSS Native Agents Workbench
 */

import * as vscode from 'vscode';
import { AgentStudiosBridge } from './AgentStudiosBridge';
import { DisconnectedBridge } from './DisconnectedBridge';
import { FixtureBridge } from './FixtureBridge';
import { DaemonConnectedBridge } from './DaemonConnectedBridge';
import { SessionStateProjection } from '../state/SessionStateProjection';

/**
 * Manages the active bridge instance and synchronizes with session state projection.
 * Supports switching between DisconnectedBridge, FixtureBridge, and daemon bridges.
 */
export class BridgeManager implements vscode.Disposable {
  private _bridge: AgentStudiosBridge;
  private readonly _stateProjection: SessionStateProjection;
  private readonly _onBridgeChanged = new vscode.EventEmitter<AgentStudiosBridge>();
  public readonly onBridgeChanged: vscode.Event<AgentStudiosBridge> = this._onBridgeChanged.event;

  private readonly _disposables: vscode.Disposable[] = [];

  constructor(initialBridge?: AgentStudiosBridge) {
    this._disposables.push(this._onBridgeChanged);

    if (initialBridge) {
      this._bridge = initialBridge;
    } else {
      const config = vscode.workspace.getConfiguration('agentStudios');
      const isPreview = config.get<boolean>('previewMode', false);
      this._bridge = isPreview ? new FixtureBridge() : new DisconnectedBridge();
    }

    this._stateProjection = new SessionStateProjection(this._bridge);
    this._disposables.push(this._stateProjection);

    // Initial snapshot hydration
    this._stateProjection.initializeFromBridge().catch((err) => {
      console.error('[BridgeManager] Failed initial projection hydration:', err);
    });
  }

  /**
   * Currently active bridge instance.
   */
  public get bridge(): AgentStudiosBridge {
    return this._bridge;
  }

  /**
   * Alias for bridge instance.
   */
  public get currentBridge(): AgentStudiosBridge {
    return this._bridge;
  }

  /**
   * Session state projection bound to active bridge.
   */
  public get stateProjection(): SessionStateProjection {
    return this._stateProjection;
  }

  /**
   * Alias for session state projection.
   */
  public get projection(): SessionStateProjection {
    return this._stateProjection;
  }

  /**
   * Whether currently running against the developer fixture bridge.
   */
  public get isPreviewMode(): boolean {
    return this._bridge instanceof FixtureBridge;
  }

  /**
   * Whether currently connected to an active backend bridge.
   */
  public get isConnected(): boolean {
    return this._bridge.connectionState === 'connected';
  }

  /**
   * Whether active bridge is a real DaemonConnectedBridge.
   */
  public get isDaemonMode(): boolean {
    return this._bridge instanceof DaemonConnectedBridge;
  }

  /**
   * Connects to the local/remote agsd daemon and binds to projection.
   */
  public async connectDaemon(bootstrapPath?: string): Promise<DaemonConnectedBridge> {
    const daemonBridge = new DaemonConnectedBridge({ bootstrapPath });
    await daemonBridge.connect();
    this.setBridge(daemonBridge);
    return daemonBridge;
  }

  /**
   * Disconnects current daemon connection and reverts to DisconnectedBridge.
   */
  public async disconnectDaemon(): Promise<void> {
    if (this._bridge instanceof DaemonConnectedBridge) {
      await this._bridge.disconnect();
      this.setBridge(new DisconnectedBridge());
    }
  }

  /**
   * Replaces current bridge with new bridge and re-binds projection.
   */
  public setBridge(newBridge: AgentStudiosBridge): void {
    if (this._bridge === newBridge) {
      return;
    }

    // Dispose old bridge if it is internal disconnected or fixture bridge
    const oldBridge = this._bridge;
    this._bridge = newBridge;

    this._stateProjection.bindBridge(newBridge);
    this._onBridgeChanged.fire(newBridge);

    this._stateProjection.initializeFromBridge().catch((err) => {
      console.error('[BridgeManager] Failed projection hydration after bridge switch:', err);
    });

    if (oldBridge instanceof DisconnectedBridge || oldBridge instanceof FixtureBridge) {
      oldBridge.dispose();
    }
  }

  public dispose(): void {
    while (this._disposables.length > 0) {
      const item = this._disposables.pop();
      item?.dispose();
    }
    this._bridge.dispose();
  }
}
