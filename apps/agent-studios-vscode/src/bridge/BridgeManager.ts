/**
 * Agent Studios — Bridge Manager
 * Milestone M14-UI-00: Code-OSS Native Agents Workbench
 */

import * as vscode from 'vscode';
import { AgentStudiosBridge } from './AgentStudiosBridge';
import { DisconnectedBridge } from './DisconnectedBridge';
import { DaemonConnectedBridge } from './DaemonConnectedBridge';
import { EmbeddedDaemonSupervisor } from './EmbeddedDaemonSupervisor';
import { CapabilityMatrix, CapabilityDescriptor, OperatorConfirmationManager } from './ipc/CapabilityMatrix';
import { SessionStateProjection } from '../state/SessionStateProjection';

/**
 * Manages the active bridge instance and synchronizes with session state projection.
 * Supports switching between DisconnectedBridge and DaemonConnectedBridge (plus injected test bridges).
 */
export class BridgeManager implements vscode.Disposable {
  private _bridge: AgentStudiosBridge;
  private readonly _stateProjection: SessionStateProjection;
  private readonly _onBridgeChanged = new vscode.EventEmitter<AgentStudiosBridge>();
  public readonly onBridgeChanged: vscode.Event<AgentStudiosBridge> = this._onBridgeChanged.event;

  private readonly _disposables: vscode.Disposable[] = [];

  constructor(initialBridge?: AgentStudiosBridge) {
    this._disposables.push(this._onBridgeChanged);

    this._bridge = initialBridge ?? new DisconnectedBridge();

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
   * Truthfully evaluates if a capability is enabled on the current active bridge.
   */
  public isCapabilityEnabled(capabilityId: string): boolean {
    if (!this.isConnected) {
      return false;
    }
    return CapabilityMatrix.isCapabilityEnabled(capabilityId);
  }

  /**
   * Retrieves the effective capability descriptor for the current bridge.
   */
  public getCapabilityDescriptor(capabilityId: string): CapabilityDescriptor | undefined {
    const desc = CapabilityMatrix.getDescriptor(capabilityId);
    if (!desc) {
      return undefined;
    }
    return {
      ...desc,
      enabled: this.isCapabilityEnabled(capabilityId),
    };
  }

  /**
   * Connects to the local/remote agsd daemon and binds to projection.
   * Automatically starts bundled agsd if autoStart options are supplied.
   */
  public async connectDaemon(
    bootstrapPath?: string,
    autoStartOptions?: { extensionPath?: string; workspacePath?: string; autoStart?: boolean }
  ): Promise<DaemonConnectedBridge> {
    let resolvedBootstrap = bootstrapPath;
    if (autoStartOptions?.autoStart !== false) {
      const started = await EmbeddedDaemonSupervisor.ensureDaemonRunning({
        extensionPath: autoStartOptions?.extensionPath,
        bootstrapPath,
        workspacePath: autoStartOptions?.workspacePath,
      });
      if (started.running && started.bootstrapPath) {
        resolvedBootstrap = started.bootstrapPath;
      }
    }
    const daemonBridge = new DaemonConnectedBridge({ bootstrapPath: resolvedBootstrap });
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

    // Clear any pending operator confirmations on bridge mode transition
    OperatorConfirmationManager.clearPendingChallenges();

    // Dispose old bridge if it is internal disconnected or fixture bridge
    const oldBridge = this._bridge;
    this._bridge = newBridge;

    this._stateProjection.bindBridge(newBridge);
    this._onBridgeChanged.fire(newBridge);

    this._stateProjection.initializeFromBridge().catch((err) => {
      console.error('[BridgeManager] Failed projection hydration after bridge switch:', err);
    });

    // Dispose old bridge if it is disconnected
    if (oldBridge instanceof DisconnectedBridge) {
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
