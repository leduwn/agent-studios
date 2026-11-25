/**
 * Agent Studios — Conversation & Session History Manager (Wave 4 / Group 1)
 *
 * Manages conversation lifecycle (create, list, rename, search, archive/unarchive,
 * pin, Markdown export) and deterministic per-turn token/context budget accounting
 * synchronized with the authoritative AgentStudiosBridge when connected.
 */

import * as vscode from 'vscode';
import type { AgentStudiosBridge } from '../bridge/AgentStudiosBridge';
import type { ChatMessage, SessionStatus, SessionSummary } from '../bridge/types';

export interface ConversationRecord {
  id: string;
  title: string;
  status: SessionStatus;
  archived: boolean;
  pinned: boolean;
  createdAt: number;
  updatedAt: number;
  estimatedPromptTokens: number;
  estimatedCompletionTokens: number;
  contextLimitTokens: number;
  messages: ChatMessage[];
}

export interface TokenBudgetSnapshot {
  conversationId: string;
  promptTokens: number;
  completionTokens: number;
  totalTokens: number;
  contextLimitTokens: number;
  utilizationRatio: number;
  isNearLimit: boolean;
}

const CONVERSATIONS_STORAGE_KEY = 'agentStudios.conversations.v1';

export function estimateTextTokens(text: string): number {
  if (!text) {
    return 0;
  }
  return Math.max(1, Math.ceil(text.length / 4));
}

export class ConversationManager implements vscode.Disposable {
  private readonly records = new Map<string, ConversationRecord>();
  private activeConversationId: string | undefined;
  private readonly onDidChangeEmitter = new vscode.EventEmitter<void>();
  public readonly onDidChange = this.onDidChangeEmitter.event;

  constructor(
    private readonly context: vscode.ExtensionContext,
    private readonly bridgeProvider: () => AgentStudiosBridge
  ) {
    this.loadFromState();
  }

  public dispose(): void {
    this.onDidChangeEmitter.dispose();
  }

  private loadFromState(): void {
    const saved = this.context.workspaceState.get<ConversationRecord[]>(
      CONVERSATIONS_STORAGE_KEY,
      []
    );
    for (const rec of saved) {
      if (rec && rec.id) {
        this.records.set(rec.id, {
          ...rec,
          messages: Array.isArray(rec.messages) ? rec.messages : [],
        });
      }
    }
    if (saved.length > 0 && !this.activeConversationId) {
      this.activeConversationId = saved[0].id;
    }
  }

  private async persist(): Promise<void> {
    const serialized = Array.from(this.records.values()).sort(
      (a, b) => Number(b.pinned) - Number(a.pinned) || b.updatedAt - a.updatedAt
    );
    await this.context.workspaceState.update(CONVERSATIONS_STORAGE_KEY, serialized);
    this.onDidChangeEmitter.fire();
  }

  public async createConversation(
    titleOrGoal: string,
    contextLimitTokens = 128000
  ): Promise<ConversationRecord> {
    const cleanTitle = titleOrGoal.trim() || 'Untitled Agent Conversation';
    const bridge = this.bridgeProvider();
    let id = `conv-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;

    if (bridge.connectionState === 'connected') {
      try {
        const created = await bridge.createSession(cleanTitle);
        if (created?.id) {
          id = created.id;
        }
      } catch {
        // Fall back to local deterministic ID when daemon is offline
      }
    }

    const now = Date.now();
    const record: ConversationRecord = {
      id,
      title: cleanTitle,
      status: 'idle',
      archived: false,
      pinned: false,
      createdAt: now,
      updatedAt: now,
      estimatedPromptTokens: 0,
      estimatedCompletionTokens: 0,
      contextLimitTokens,
      messages: [],
    };
    this.records.set(id, record);
    this.activeConversationId = id;
    await this.persist();
    return record;
  }

  public async syncWithDaemonSessions(): Promise<SessionSummary[]> {
    const bridge = this.bridgeProvider();
    if (bridge.connectionState !== 'connected') {
      return this.listConversations().map((c) => ({
        id: c.id,
        title: c.title,
        status: c.status,
        updatedAt: c.updatedAt,
      }));
    }

    const remote = await bridge.listSessions();
    for (const s of remote) {
      const existing = this.records.get(s.id);
      if (existing) {
        existing.title = s.title || existing.title;
        existing.status = s.status;
        existing.updatedAt = Math.max(existing.updatedAt, s.updatedAt);
      } else {
        this.records.set(s.id, {
          id: s.id,
          title: s.title,
          status: s.status,
          archived: false,
          pinned: false,
          createdAt: s.updatedAt,
          updatedAt: s.updatedAt,
          estimatedPromptTokens: 0,
          estimatedCompletionTokens: 0,
          contextLimitTokens: 128000,
          messages: [],
        });
      }
    }
    await this.persist();
    return remote;
  }

  public listConversations(options?: {
    includeArchived?: boolean;
    searchQuery?: string;
  }): ConversationRecord[] {
    const includeArchived = options?.includeArchived ?? false;
    const query = (options?.searchQuery ?? '').trim().toLowerCase();

    return Array.from(this.records.values())
      .filter((c) => (includeArchived ? true : !c.archived))
      .filter((c) => {
        if (!query) {
          return true;
        }
        if (c.title.toLowerCase().includes(query) || c.id.toLowerCase().includes(query)) {
          return true;
        }
        return c.messages.some((m) => m.content.toLowerCase().includes(query));
      })
      .sort((a, b) => Number(b.pinned) - Number(a.pinned) || b.updatedAt - a.updatedAt);
  }

  public getConversation(id: string): ConversationRecord | undefined {
    return this.records.get(id);
  }

  public getActiveConversation(): ConversationRecord | undefined {
    return this.activeConversationId ? this.records.get(this.activeConversationId) : undefined;
  }

  public async switchConversation(id: string): Promise<ConversationRecord | undefined> {
    const record = this.records.get(id);
    if (!record) {
      return undefined;
    }
    const bridge = this.bridgeProvider();
    if (bridge.connectionState === 'connected') {
      try {
        await bridge.switchSession(id);
      } catch {
        // Local switch remains authoritative in preview/offline state
      }
    }
    this.activeConversationId = id;
    this.onDidChangeEmitter.fire();
    return record;
  }

  public async renameConversation(id: string, nextTitle: string): Promise<boolean> {
    const record = this.records.get(id);
    const trimmed = nextTitle.trim();
    if (!record || !trimmed) {
      return false;
    }
    record.title = trimmed;
    record.updatedAt = Date.now();
    await this.persist();
    return true;
  }

  public async setArchived(id: string, archived: boolean): Promise<boolean> {
    const record = this.records.get(id);
    if (!record) {
      return false;
    }
    record.archived = archived;
    record.updatedAt = Date.now();
    if (archived && this.activeConversationId === id) {
      const next = this.listConversations({ includeArchived: false })[0];
      this.activeConversationId = next?.id;
    }
    await this.persist();
    return true;
  }

  public async setPinned(id: string, pinned: boolean): Promise<boolean> {
    const record = this.records.get(id);
    if (!record) {
      return false;
    }
    record.pinned = pinned;
    record.updatedAt = Date.now();
    await this.persist();
    return true;
  }

  public async appendMessage(conversationId: string, message: ChatMessage): Promise<TokenBudgetSnapshot | undefined> {
    const record = this.records.get(conversationId);
    if (!record) {
      return undefined;
    }
    record.messages.push(message);
    const tokens = estimateTextTokens(message.content);
    if (message.role === 'assistant') {
      record.estimatedCompletionTokens += tokens;
    } else {
      record.estimatedPromptTokens += tokens;
    }
    record.updatedAt = Math.max(record.updatedAt, message.timestamp || Date.now());
    await this.persist();
    return this.getTokenBudget(conversationId);
  }

  public getTokenBudget(conversationId: string): TokenBudgetSnapshot | undefined {
    const record = this.records.get(conversationId);
    if (!record) {
      return undefined;
    }
    const totalTokens = record.estimatedPromptTokens + record.estimatedCompletionTokens;
    const limit = Math.max(1, record.contextLimitTokens);
    const utilizationRatio = Math.min(1, totalTokens / limit);
    return {
      conversationId,
      promptTokens: record.estimatedPromptTokens,
      completionTokens: record.estimatedCompletionTokens,
      totalTokens,
      contextLimitTokens: limit,
      utilizationRatio,
      isNearLimit: utilizationRatio >= 0.85,
    };
  }

  public exportAsMarkdown(conversationId: string): string | undefined {
    const record = this.records.get(conversationId);
    if (!record) {
      return undefined;
    }
    const lines = [
      `# ${record.title}`,
      '',
      `- Session ID: \`${record.id}\``,
      `- Status: ${record.status}`,
      `- Estimated Tokens: ${record.estimatedPromptTokens + record.estimatedCompletionTokens} / ${record.contextLimitTokens}`,
      '',
    ];
    for (const msg of record.messages) {
      lines.push(`## ${msg.role.toUpperCase()} (${new Date(msg.timestamp).toISOString()})`);
      lines.push('');
      lines.push(msg.content);
      lines.push('');
    }
    return lines.join('\n');
  }
}

export function registerConversationCommands(
  context: vscode.ExtensionContext,
  manager: ConversationManager
): vscode.Disposable {
  const disposables: vscode.Disposable[] = [
    vscode.commands.registerCommand('agent-studios.newConversation', async () => {
      const title = await vscode.window.showInputBox?.({
        prompt: 'Conversation title or initial coding goal',
        placeHolder: 'e.g. Implement stream reconnect backoff',
      });
      if (!title) {
        return undefined;
      }
      const created = await manager.createConversation(title);
      vscode.window.showInformationMessage(`Created conversation: ${created.title}`);
      return created;
    }),
    vscode.commands.registerCommand('agent-studios.searchConversations', async () => {
      const query = await vscode.window.showInputBox?.({
        prompt: 'Filter conversations by title or message content',
        placeHolder: 'Leave empty to list all active conversations',
      });
      const matches = manager.listConversations({ searchQuery: query ?? '' });
      if (matches.length === 0) {
        vscode.window.showInformationMessage('No matching Agent Studios conversations found.');
        return undefined;
      }
      const picked = await vscode.window.showQuickPick(
        matches.map((c) => ({
          label: `${c.pinned ? '[Pinned] ' : ''}${c.title}`,
          description: `${c.messages.length} msgs · ${c.estimatedPromptTokens + c.estimatedCompletionTokens} est. tokens`,
          conversationId: c.id,
        })),
        { placeHolder: 'Select conversation to open' }
      );
      if (picked && typeof picked === 'object' && 'conversationId' in picked) {
        const target = (picked as { conversationId: string }).conversationId;
        await manager.switchConversation(target);
        return target;
      }
      return undefined;
    }),
    vscode.commands.registerCommand('agent-studios.renameConversation', async (id?: string) => {
      const targetId = id || manager.getActiveConversation()?.id;
      if (!targetId) {
        return false;
      }
      const next = await vscode.window.showInputBox?.({
        prompt: 'New conversation title',
        value: manager.getConversation(targetId)?.title,
      });
      if (!next) {
        return false;
      }
      return manager.renameConversation(targetId, next);
    }),
    vscode.commands.registerCommand('agent-studios.archiveConversation', async (id?: string) => {
      const targetId = id || manager.getActiveConversation()?.id;
      if (!targetId) {
        return false;
      }
      const ok = await manager.setArchived(targetId, true);
      if (ok) {
        vscode.window.showInformationMessage(`Archived conversation ${targetId}.`);
      }
      return ok;
    }),
  ];

  const composite = vscode.Disposable.from(...disposables);
  context.subscriptions.push(composite);
  return composite;
}
