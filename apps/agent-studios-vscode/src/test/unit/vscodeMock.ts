/**
 * Mock of minimal vscode API needed for unit testing in Node environment.
 */
/* eslint-disable @typescript-eslint/naming-convention */

export class MockDisposable {
  constructor(private readonly callOnDispose?: () => void) {}

  public dispose(): void {
    if (this.callOnDispose) {
      this.callOnDispose();
    }
  }

  public static from(...disposables: { dispose(): void }[]): MockDisposable {
    return new MockDisposable(() => {
      for (const d of disposables) {
        d.dispose();
      }
    });
  }
}

export class MockEventEmitter<T> {
  private _listeners: Array<(e: T) => void> = [];

  public event = (
    listener: (e: T) => void,
    thisArgs?: unknown,
    disposables?: { dispose(): void }[]
  ): MockDisposable => {
    const bound = thisArgs ? listener.bind(thisArgs) : listener;
    this._listeners.push(bound);

    const disposable = new MockDisposable(() => {
      const idx = this._listeners.indexOf(bound);
      if (idx >= 0) {
        this._listeners.splice(idx, 1);
      }
    });

    if (disposables) {
      disposables.push(disposable);
    }

    return disposable;
  };

  public fire(data: T): void {
    for (const listener of [...this._listeners]) {
      listener(data);
    }
  }

  public dispose(): void {
    this._listeners = [];
  }
}

export enum MockTreeItemCollapsibleState {
  None = 0,
  Collapsed = 1,
  Expanded = 2,
}

export class MockTreeItem {
  public label?: string;
  public collapsibleState?: MockTreeItemCollapsibleState;
  public iconPath?: unknown;
  public description?: string;
  public tooltip?: string | unknown;
  public command?: unknown;
  public contextValue?: string;

  constructor(
    label?: string,
    collapsibleState: MockTreeItemCollapsibleState = MockTreeItemCollapsibleState.None
  ) {
    this.label = label;
    this.collapsibleState = collapsibleState;
  }
}

export class MockThemeIcon {
  constructor(
    public readonly id: string,
    public readonly color?: unknown
  ) {}
}

export class MockThemeColor {
  constructor(public readonly id: string) {}
}

export enum MockFileType {
  Unknown = 0,
  File = 1,
  Directory = 2,
  SymbolicLink = 64,
}

export enum MockViewColumn {
  Active = -1,
  Beside = -2,
  One = 1,
  Two = 2,
  Three = 3,
}

export class MockWebview {
  public cspSource = 'https://*.vscode-cdn.net';
  public html = '';
  public readonly _messageEmitter = new MockEventEmitter<unknown>();
  public readonly _sentMessages: unknown[] = [];

  public onDidReceiveMessage = this._messageEmitter.event;

  public async postMessage(message: unknown): Promise<boolean> {
    this._sentMessages.push(message);
    return true;
  }
}

export class MockWebviewPanel {
  public visible = true;
  private _isDisposing = false;
  public readonly webview = new MockWebview();
  public readonly _disposeEmitter = new MockEventEmitter<void>();
  public onDidDispose = this._disposeEmitter.event;

  constructor(
    public readonly viewType: string,
    public title: string,
    public viewColumn: number,
    public readonly options: unknown
  ) {}

  public reveal(column?: number): void {
    if (column !== undefined) {
      this.viewColumn = column;
    }
    this.visible = true;
  }

  public dispose(): void {
    if (this._isDisposing) {
      return;
    }
    this._isDisposing = true;
    this.visible = false;
    this._disposeEmitter.fire();
  }
}

export enum MockConfigurationTarget {
  Global = 1,
  Workspace = 2,
  WorkspaceFolder = 3,
}

export class MockUri {
  public readonly scheme: string;
  public readonly authority: string;
  public readonly path: string;
  public readonly query: string;
  public readonly fragment: string;
  public readonly fsPath: string;

  constructor(scheme: string, authority: string, pathStr: string, query: string = '', fragment: string = '') {
    this.scheme = scheme;
    this.authority = authority;
    this.path = pathStr;
    this.query = query;
    this.fragment = fragment;
    this.fsPath = pathStr;
  }

  public toString(): string {
    let res = this.scheme ? `${this.scheme}://` : '';
    if (this.authority) {
      res += this.authority;
    }
    res += this.path;
    if (this.query) {
      res += `?${this.query}`;
    }
    if (this.fragment) {
      res += `#${this.fragment}`;
    }
    return res;
  }

  public static parse(value: string): MockUri {
    const match = value.match(/^([a-zA-Z0-9+.-]+):\/\/([^/?#]*)([^?#]*)(?:\?([^#]*))?(?:#(.*))?$/);
    if (match) {
      return new MockUri(match[1], match[2], match[3], match[4] || '', match[5] || '');
    }
    const simple = value.match(/^([a-zA-Z0-9+.-]+):([^?#]*)(?:\?([^#]*))?(?:#(.*))?$/);
    if (simple) {
      return new MockUri(simple[1], '', simple[2], simple[3] || '', simple[4] || '');
    }
    return new MockUri('file', '', value);
  }

  public static file(fsPath: string): MockUri {
    const normalized = fsPath.replace(/\\/g, '/');
    return new MockUri('file', '', normalized.startsWith('/') ? normalized : '/' + normalized);
  }

  public static joinPath(base: MockUri, ...pathSegments: string[]): MockUri {
    const joined = [base.path, ...pathSegments].join('/').replace(/\/+/g, '/');
    return new MockUri(base.scheme, base.authority, joined, base.query, base.fragment);
  }
}

export interface RegisteredCommand {
  command: string;
  callback: (...args: unknown[]) => unknown;
}

export const registeredCommands = new Map<string, (...args: unknown[]) => unknown>();
export const executedCommands: Array<{ command: string; args: unknown[] }> = [];

export const mockWindowMessages = {
  info: [] as string[],
  warning: [] as string[],
  error: [] as string[],
  modalWarningResponse: undefined as string | undefined,
  quickPickSelection: undefined as unknown,
};

export function resetMockState(): void {
  registeredCommands.clear();
  executedCommands.length = 0;
  mockWindowMessages.info.length = 0;
  mockWindowMessages.warning.length = 0;
  mockWindowMessages.error.length = 0;
  mockWindowMessages.modalWarningResponse = undefined;
  mockWindowMessages.quickPickSelection = undefined;
}

export const vscodeMock = {
  Disposable: MockDisposable,
  EventEmitter: MockEventEmitter,
  TreeItemCollapsibleState: MockTreeItemCollapsibleState,
  TreeItem: MockTreeItem,
  ThemeIcon: MockThemeIcon,
  ThemeColor: MockThemeColor,
  FileType: MockFileType,
  ViewColumn: MockViewColumn,
  Uri: MockUri,
  commands: {
    registerCommand: (command: string, callback: (...args: unknown[]) => unknown): MockDisposable => {
      registeredCommands.set(command, callback);
      return new MockDisposable(() => {
        registeredCommands.delete(command);
      });
    },
    executeCommand: async (command: string, ...args: unknown[]): Promise<unknown> => {
      executedCommands.push({ command, args });
      const handler = registeredCommands.get(command);
      if (handler) {
        return handler(...args);
      }
      return undefined;
    },
  },
  window: {
    showInformationMessage: async (message: string): Promise<string | undefined> => {
      mockWindowMessages.info.push(message);
      return message;
    },
    showWarningMessage: async (
      message: string,
      optionsOrFirstItem?: unknown,
      ...items: string[]
    ): Promise<string | undefined> => {
      mockWindowMessages.warning.push(message);
      if (mockWindowMessages.modalWarningResponse !== undefined) {
        return mockWindowMessages.modalWarningResponse;
      }
      if (typeof optionsOrFirstItem === 'string') {
        return optionsOrFirstItem;
      }
      return items[0];
    },
    showErrorMessage: async (message: string): Promise<string | undefined> => {
      mockWindowMessages.error.push(message);
      return message;
    },
    showQuickPick: async (items: unknown[]): Promise<unknown> => {
      if (mockWindowMessages.quickPickSelection !== undefined) {
        return mockWindowMessages.quickPickSelection;
      }
      return items[0];
    },
    showTextDocument: async (doc: unknown): Promise<unknown> => {
      return { document: doc };
    },
    createWebviewPanel: (
      viewType: string,
      title: string,
      viewColumn: number,
      options: unknown
    ): MockWebviewPanel => {
      return new MockWebviewPanel(viewType, title, viewColumn, options);
    },
  },
  workspace: {
    workspaceFolders: [] as Array<{ uri: MockUri; name: string }>,
    fs: {
      stat: async (_uri: MockUri): Promise<{ type: MockFileType; size: number }> => {
        return { type: MockFileType.File, size: 100 };
      },
      readFile: async (_uri: MockUri): Promise<Uint8Array> => {
        return Buffer.from('');
      },
    },
    openTextDocument: async (uriOrOptions: unknown): Promise<{ uri: unknown }> => {
      return { uri: uriOrOptions };
    },
    registerTextDocumentContentProvider: (
      _scheme: string,
      _provider: unknown
    ): MockDisposable => {
      return new MockDisposable();
    },
    getConfiguration: (_section?: string) => ({
      get: <T>(_key: string, defaultValue: T): T => defaultValue,
      update: async (_key: string, _value: unknown, _target?: unknown): Promise<void> => {},
    }),
  },
  ConfigurationTarget: MockConfigurationTarget,
};

