/**
 * Agent Studios — Providers TreeDataProvider
 * Milestone M14-UI-00: Code-OSS Native Agents Workbench
 *
 * STRICT PRODUCT PRINCIPLE:
 * Providers are strictly READ-ONLY.
 * No editing, no adding keys, no configuring.
 * Provider management is owned by Rust backend.
 */

import * as vscode from 'vscode';
import { AgentStudiosBridge } from '../bridge/AgentStudiosBridge';
import { BridgeManager } from '../bridge/BridgeManager';
import { ProviderModel, ProviderSummary } from '../bridge/types';

export class ProviderTreeItem extends vscode.TreeItem {
  public readonly provider?: ProviderSummary;
  public readonly model?: ProviderModel;

  constructor(
    providerOrModelOrLabel: ProviderSummary | ProviderModel | string,
    parentProvider?: ProviderSummary
  ) {
    if (typeof providerOrModelOrLabel === 'string') {
      super(providerOrModelOrLabel, vscode.TreeItemCollapsibleState.None);
      return;
    }

    // Model item
    if ('contextWindow' in providerOrModelOrLabel) {
      const model = providerOrModelOrLabel as ProviderModel;
      const label = `${model.name}${model.isDefault ? ' (Default)' : ''}`;
      super(label, vscode.TreeItemCollapsibleState.None);
      this.model = model;
      this.provider = parentProvider;
      this.id = `model-${parentProvider?.instanceId ?? 'unknown'}-${model.id}`;

      // Strictly read-only context value
      this.contextValue = 'providerModelReadOnly';

      const ctxLabel = `${Math.round(model.contextWindow / 1000)}k ctx`;
      this.description = `${ctxLabel} [${model.id}]`;

      this.iconPath = new vscode.ThemeIcon('symbol-property');

      this.tooltip = [
        `Model: ${model.name}`,
        `ID: ${model.id}`,
        `Context Window: ${model.contextWindow.toLocaleString()} tokens`,
        model.isDefault ? 'Status: Default Model' : null,
        parentProvider ? `Provider: ${parentProvider.displayName}` : null,
        '',
        '(Read-Only: Managed by Rust backend)',
      ]
        .filter((x): x is string => x !== null)
        .join('\n');

      return;
    }

    // Provider item
    const provider = providerOrModelOrLabel as ProviderSummary;
    super(
      provider.displayName,
      provider.models.length > 0
        ? vscode.TreeItemCollapsibleState.Expanded
        : vscode.TreeItemCollapsibleState.None
    );
    this.provider = provider;
    this.id = `provider-${provider.instanceId}`;

    // Strictly read-only context value
    this.contextValue = 'providerItemReadOnly';

    const modelCountText = `${provider.models.length} model${
      provider.models.length === 1 ? '' : 's'
    }`;
    this.description = `[${provider.status.toUpperCase()}] ${modelCountText}`;

    this.iconPath = ProviderTreeItem.getIconForStatus(provider.status);

    this.tooltip = [
      `Provider: ${provider.displayName}`,
      `Type: ${provider.providerType}`,
      `Status: ${provider.status}`,
      `Instance: ${provider.instanceId}`,
      `Available Models: ${provider.models.length}`,
      '',
      '(Read-Only: Managed by Rust backend)',
    ].join('\n');
  }

  public static getIconForStatus(
    status: ProviderSummary['status']
  ): vscode.ThemeIcon {
    switch (status) {
      case 'available':
        return new vscode.ThemeIcon(
          'pass-filled',
          new vscode.ThemeColor('testing.iconPassed')
        );
      case 'offline':
        return new vscode.ThemeIcon(
          'circle-slash',
          new vscode.ThemeColor('testing.iconSkipped')
        );
      case 'unconfigured':
      default:
        return new vscode.ThemeIcon(
          'question',
          new vscode.ThemeColor('problemsWarningIcon.foreground')
        );
    }
  }
}

export class ProvidersTreeProvider
  implements vscode.TreeDataProvider<ProviderTreeItem>, vscode.Disposable
{
  private _bridgeManager?: BridgeManager;
  private _bridge?: AgentStudiosBridge;
  private readonly _disposables: vscode.Disposable[] = [];
  private readonly _bridgeDisposables: vscode.Disposable[] = [];

  private readonly _onDidChangeTreeData = new vscode.EventEmitter<
    ProviderTreeItem | undefined | void
  >();
  public readonly onDidChangeTreeData: vscode.Event<ProviderTreeItem | undefined | void> =
    this._onDidChangeTreeData.event;

  constructor(bridgeOrManager: AgentStudiosBridge | BridgeManager) {
    this._disposables.push(this._onDidChangeTreeData);

    if ('stateProjection' in bridgeOrManager) {
      this._bridgeManager = bridgeOrManager as BridgeManager;
      this._bridge = this._bridgeManager.bridge;
      this._disposables.push(
        this._bridgeManager.onBridgeChanged((newBridge) => {
          this._bridge = newBridge;
          this.bindBridgeListeners();
          this.refresh();
        })
      );
    } else {
      this._bridge = bridgeOrManager;
    }

    this.bindBridgeListeners();
  }

  private bindBridgeListeners(): void {
    while (this._bridgeDisposables.length > 0) {
      const d = this._bridgeDisposables.pop();
      d?.dispose();
    }
    if (this._bridge) {
      this._bridgeDisposables.push(
        this._bridge.onConnectionStateChanged(() => this.refresh())
      );
    }
  }

  public refresh(): void {
    this._onDidChangeTreeData.fire();
  }

  public getTreeItem(element: ProviderTreeItem): vscode.TreeItem {
    return element;
  }

  public async getChildren(
    element?: ProviderTreeItem
  ): Promise<ProviderTreeItem[]> {
    if (element) {
      if (element.provider && element.provider.models.length > 0) {
        return element.provider.models.map(
          (model) => new ProviderTreeItem(model, element.provider)
        );
      }
      return [];
    }

    if (!this._bridge) {
      const placeholder = new ProviderTreeItem('No bridge connected');
      placeholder.iconPath = new vscode.ThemeIcon('circle-slash');
      placeholder.contextValue = 'noProviders';
      return [placeholder];
    }

    try {
      const summaries = await this._bridge.listProviderSummaries();

      if (summaries.length === 0) {
        const placeholder = new ProviderTreeItem('No providers configured');
        placeholder.description = 'Read-Only (Managed by Rust backend)';
        placeholder.iconPath = new vscode.ThemeIcon('info');
        placeholder.tooltip =
          'No model providers configured.\nProviders are strictly read-only and configured by the Rust backend daemon.';
        placeholder.contextValue = 'noProviders';
        return [placeholder];
      }

      return summaries.map((summary) => new ProviderTreeItem(summary));
    } catch (err) {
      const errItem = new ProviderTreeItem('Error loading providers');
      errItem.description = String(err);
      errItem.iconPath = new vscode.ThemeIcon(
        'error',
        new vscode.ThemeColor('testing.iconFailed')
      );
      errItem.contextValue = 'providerError';
      return [errItem];
    }
  }

  public dispose(): void {
    while (this._bridgeDisposables.length > 0) {
      const d = this._bridgeDisposables.pop();
      d?.dispose();
    }
    while (this._disposables.length > 0) {
      const d = this._disposables.pop();
      d?.dispose();
    }
  }
}
