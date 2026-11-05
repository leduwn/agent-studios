/**
 * Agent Studios — Slash Command Registry
 * Milestone M14-UI-01: Professional AI Coding Assistant Experience
 */

import * as vscode from 'vscode';
import {
  SessionSummary,
  SlashCommandContext,
  SlashCommandDescriptor,
  SlashCommandResult,
} from '../bridge/types';

export {
  SlashCommandContext,
  SlashCommandDescriptor,
  SlashCommandResult,
};

/**
 * Registry managing slash commands for the composer chat input.
 * Supports /help, /clear, /plan, /review, /explain, /tests, /providers, /sessions.
 * Marks unsupported capabilities visibly instead of faking results.
 */
export class SlashCommandRegistry implements vscode.Disposable {
  private readonly _commands = new Map<string, SlashCommandDescriptor>();

  constructor() {
    this.registerDefaultCommands();
  }

  /**
   * Normalizes command name by stripping leading slash, trimming, and converting to lowercase.
   */
  public normalizeName(name: string): string {
    return name.trim().replace(/^\//, '').toLowerCase();
  }

  /**
   * Registers a slash command descriptor.
   */
  public register(command: SlashCommandDescriptor): vscode.Disposable {
    const key = this.normalizeName(command.name);
    this._commands.set(key, command);

    return new vscode.Disposable(() => {
      this.unregister(key);
    });
  }

  /**
   * Unregisters a command by name.
   */
  public unregister(name: string): boolean {
    const key = this.normalizeName(name);
    return this._commands.delete(key);
  }

  /**
   * Retrieves a command descriptor by name (handles with or without leading slash).
   */
  public getCommand(name: string): SlashCommandDescriptor | undefined {
    return this._commands.get(this.normalizeName(name));
  }

  /**
   * Returns all registered command descriptors.
   */
  public listCommands(): SlashCommandDescriptor[] {
    return Array.from(this._commands.values());
  }

  /**
   * Checks whether the given input starts with a slash command trigger.
   */
  public isSlashCommand(input: string): boolean {
    return typeof input === 'string' && input.trim().startsWith('/');
  }

  /**
   * Parses input string into command name and arguments.
   * Returns null if input is not a slash command.
   */
  public parseCommand(input: string): { name: string; args: string } | null {
    if (!this.isSlashCommand(input)) {
      return null;
    }

    const trimmed = input.trim();
    const match = trimmed.match(/^\/([^\s]+)(?:\s+(.*))?$/s);
    if (!match) {
      return null;
    }

    return {
      name: match[1],
      args: match[2] ? match[2].trim() : '',
    };
  }

  /**
   * Executes a slash command string within the given context.
   */
  public async executeCommand(
    input: string,
    context: SlashCommandContext
  ): Promise<SlashCommandResult> {
    const parsed = this.parseCommand(input);
    if (!parsed) {
      return {
        success: false,
        message: `Invalid slash command input: '${input}'.`,
      };
    }

    const command = this.getCommand(parsed.name);
    if (!command) {
      return {
        success: false,
        unsupported: true,
        message: `Command '/${parsed.name}' is not recognized. Type /help to view available commands.`,
      };
    }

    try {
      return await command.execute(parsed.args, context);
    } catch (err: unknown) {
      const isDisconnected =
        err instanceof Error &&
        (err.name === 'BridgeDisconnectedError' ||
          err.message.toLowerCase().includes('disconnected'));

      return {
        success: false,
        unsupported: isDisconnected,
        message: `Failed to execute '/${parsed.name}': ${err instanceof Error ? err.message : String(err)}`,
      };
    }
  }

  /**
   * Registers default M14 slash commands.
   */
  private registerDefaultCommands(): void {
    // 1. /help
    this.register({
      name: '/help',
      description: 'List available commands, descriptions, and keyboard shortcuts',
      execute: async (): Promise<SlashCommandResult> => {
        const commands = this.listCommands();
        const commandRows = commands
          .map((c) => {
            const cmdName = c.name.startsWith('/') ? c.name : `/${c.name}`;
            return `| \`${cmdName}\` | ${c.description} |`;
          })
          .join('\n');

        const markdown = [
          '### Agent Studios Assistant Commands',
          '',
          '| Command | Description |',
          '|---|---|',
          commandRows,
          '',
          '### Keyboard Shortcuts',
          '',
          '| Shortcut | Action |',
          '|---|---|',
          '| `Enter` / `Ctrl+Enter` (`Cmd+Enter`) | Send chat prompt |',
          '| `Shift+Enter` | Insert new line |',
          '| `Ctrl+L` (`Cmd+L`) | Focus chat composer |',
          '| `Escape` | Cancel current generation or dismiss |',
          '| `@` | Mention file or symbol for context attachment |',
          '| `/` | Trigger slash command palette |',
        ].join('\n');

        return {
          success: true,
          actionTaken: 'help',
          markdown,
          message: 'Displayed slash commands and keyboard shortcuts.',
        };
      },
    });

    // 2. /clear
    this.register({
      name: '/clear',
      description: 'Clear chat history in current session',
      execute: async (_args: string, context: SlashCommandContext): Promise<SlashCommandResult> => {
        if (context.clearHistory) {
          await context.clearHistory();
        }
        return {
          success: true,
          actionTaken: 'clear',
          message: 'Chat history cleared for current session.',
        };
      },
    });

    // 3. /plan
    this.register({
      name: '/plan',
      description: 'Set mode to Plan and format prompt for architectural planning',
      execute: async (args: string, context: SlashCommandContext): Promise<SlashCommandResult> => {
        if (context.setMode) {
          context.setMode('plan');
        }

        const hasGoal = args && args.trim().length > 0;
        const formattedPrompt = hasGoal
          ? `[Architectural Plan Request]\nGoal: ${args.trim()}\n\nPlease analyze the workspace architecture and provide a structured plan with task dependencies before taking action.`
          : `[Architectural Plan Request]\nPlease analyze the workspace architecture and formulate an execution plan for the active session.`;

        return {
          success: true,
          mode: 'plan',
          actionTaken: 'plan',
          formattedPrompt,
          message: 'Switched mode to Plan and formatted architectural planning prompt.',
        };
      },
    });

    // 4. /review
    this.register({
      name: '/review',
      description: 'Trigger code review workflow on current diff or git changes',
      execute: async (args: string, context: SlashCommandContext): Promise<SlashCommandResult> => {
        if (!context.bridge || context.bridge.connectionState === 'disconnected') {
          return {
            success: false,
            unsupported: true,
            message: 'Code review workflow unsupported: bridge is disconnected from daemon.',
          };
        }

        if (args && args.trim().length > 0) {
          return {
            success: true,
            actionTaken: 'review',
            formattedPrompt: `[Code Review Request]\nPlease perform a thorough code review of: ${args.trim()}`,
            message: `Triggered code review for: ${args.trim()}`,
          };
        }

        try {
          const artifacts = await context.bridge.listArtifacts();
          if (artifacts && artifacts.length > 0) {
            const patchArtifact =
              [...artifacts].reverse().find((a) => !!a.diffContent) ||
              artifacts[artifacts.length - 1];
            const diff =
              (await context.bridge.getArtifactDiff(patchArtifact.id)) ||
              patchArtifact.diffContent;

            if (diff && diff.trim()) {
              return {
                success: true,
                actionTaken: 'review',
                formattedPrompt: `[Code Review Request]\nPlease review the following patch in '${patchArtifact.name}':\n\n\`\`\`diff\n${diff}\n\`\`\``,
                message: `Triggered code review on artifact '${patchArtifact.name}'.`,
                data: { artifactId: patchArtifact.id, artifactName: patchArtifact.name },
              };
            }
          }
        } catch (err: unknown) {
          return {
            success: false,
            unsupported: true,
            message: `Code review workflow unsupported: ${err instanceof Error ? err.message : String(err)}`,
          };
        }

        return {
          success: false,
          unsupported: true,
          message:
            'Code review workflow unsupported: no active diff, patch artifacts, or target files found in current session.',
        };
      },
    });

    // 5. /explain
    this.register({
      name: '/explain',
      description: 'Explain selected code or active file',
      execute: async (args: string, context: SlashCommandContext): Promise<SlashCommandResult> => {
        if (!context.bridge || context.bridge.connectionState === 'disconnected') {
          return {
            success: false,
            unsupported: true,
            message: 'Code explanation unsupported: bridge is disconnected from daemon.',
          };
        }

        let editorContext = context.activeEditorContext;
        if (!editorContext && context.captureActiveEditorContext) {
          editorContext = await context.captureActiveEditorContext();
        }

        if (!editorContext) {
          return {
            success: false,
            unsupported: true,
            message:
              'Code explanation unsupported: no active text editor or selection found. Please open a file or select code to explain.',
          };
        }

        const rangeStr = editorContext.selectionRange
          ? ` (lines ${editorContext.selectionRange.startLine}-${editorContext.selectionRange.endLine})`
          : '';
        const snippet = editorContext.contentSnippet
          ? `\n\n\`\`\`${editorContext.languageId || ''}\n${editorContext.contentSnippet}\n\`\`\``
          : '';
        const userQuery =
          args && args.trim().length > 0 ? `\n\nFocus/Question: ${args.trim()}` : '';

        const formattedPrompt = `[Code Explanation Request]\nPlease explain the code in '${editorContext.relativePath}'${rangeStr}:${snippet}${userQuery}`;

        return {
          success: true,
          actionTaken: 'explain',
          formattedPrompt,
          message: `Prepared explanation for '${editorContext.relativePath}'${rangeStr}.`,
          data: editorContext,
        };
      },
    });

    // 6. /tests
    this.register({
      name: '/tests',
      description: 'Generate unit tests for active file',
      execute: async (args: string, context: SlashCommandContext): Promise<SlashCommandResult> => {
        if (!context.bridge || context.bridge.connectionState === 'disconnected') {
          return {
            success: false,
            unsupported: true,
            message: 'Unit test generation unsupported: bridge is disconnected from daemon.',
          };
        }

        let editorContext = context.activeEditorContext;
        if (!editorContext && context.captureActiveEditorContext) {
          editorContext = await context.captureActiveEditorContext();
        }

        if (!editorContext) {
          return {
            success: false,
            unsupported: true,
            message:
              'Unit test generation unsupported: no active editor file found. Please open a file first.',
          };
        }

        const snippet = editorContext.contentSnippet
          ? `\n\n\`\`\`${editorContext.languageId || ''}\n${editorContext.contentSnippet}\n\`\`\``
          : '';
        const requirements =
          args && args.trim().length > 0 ? `\n\nTest Requirements: ${args.trim()}` : '';

        const formattedPrompt = `[Unit Test Generation Request]\nPlease generate comprehensive unit tests for '${editorContext.relativePath}' (${editorContext.languageId || 'code'}):${snippet}${requirements}`;

        return {
          success: true,
          actionTaken: 'tests',
          formattedPrompt,
          message: `Generated test generation prompt for '${editorContext.relativePath}'.`,
          data: editorContext,
        };
      },
    });

    // 7. /providers
    this.register({
      name: '/providers',
      description: 'Show summary of configured providers and models',
      execute: async (
        _args: string,
        context: SlashCommandContext
      ): Promise<SlashCommandResult> => {
        if (!context.bridge || context.bridge.connectionState === 'disconnected') {
          return {
            success: false,
            unsupported: true,
            message: 'Providers summary unsupported: bridge is disconnected from daemon.',
          };
        }

        const providers = await context.bridge.listProviderSummaries();
        if (!providers || providers.length === 0) {
          return {
            success: true,
            actionTaken: 'providers',
            markdown: '### Configured Providers\n\nNo providers currently configured.',
            data: [],
            message: 'No providers configured.',
          };
        }

        const rows = providers
          .map((p) => {
            const models = p.models
              .map(
                (m) =>
                  `${m.name}${m.isDefault ? ' (default)' : ''} [${Math.round(m.contextWindow / 1000)}k]`
              )
              .join(', ');
            return `| ${p.displayName} | \`${p.providerType}\` | \`${p.status}\` | ${models} |`;
          })
          .join('\n');

        const markdown = [
          '### Configured Providers & Models',
          '',
          '| Provider | Type | Status | Models (Context Window) |',
          '|---|---|---|---|',
          rows,
        ].join('\n');

        return {
          success: true,
          actionTaken: 'providers',
          markdown,
          message: `Loaded ${providers.length} configured provider(s).`,
          data: providers,
        };
      },
    });

    // 8. /sessions
    this.register({
      name: '/sessions',
      description: 'List sessions with switch affordance',
      execute: async (args: string, context: SlashCommandContext): Promise<SlashCommandResult> => {
        if (!context.bridge || context.bridge.connectionState === 'disconnected') {
          return {
            success: false,
            unsupported: true,
            message: 'Session management unsupported: bridge is disconnected from daemon.',
          };
        }

        const trimmed = args.trim();
        const switchMatch =
          trimmed.match(/^(?:switch|select)\s+([^\s]+)$/i) ||
          (trimmed ? [null, trimmed] : null);

        if (switchMatch && switchMatch[1]) {
          const targetId = switchMatch[1];
          try {
            if (context.switchSession) {
              await context.switchSession(targetId);
            } else if (typeof context.bridge.switchSession === 'function') {
              await context.bridge.switchSession(targetId);
            } else {
              return {
                success: false,
                unsupported: true,
                message: 'Session switching is not supported by the current backend.',
              };
            }
            return {
              success: true,
              actionTaken: 'switch_session',
              message: `Successfully switched to session '${targetId}'.`,
              data: { sessionId: targetId },
            };
          } catch (err: unknown) {
            return {
              success: false,
              message: `Failed to switch to session '${targetId}': ${err instanceof Error ? err.message : String(err)}`,
            };
          }
        }

        let sessions: SessionSummary[] = [];
        try {
          if (typeof context.bridge.listSessions === 'function') {
            sessions = await context.bridge.listSessions();
          }
        } catch {
          // ignore error
        }

        const currentSession = await context.bridge.getCurrentSession();
        if (sessions.length === 0 && !currentSession) {
          return {
            success: true,
            actionTaken: 'sessions',
            markdown: '### Sessions\n\nNo active or saved sessions found.',
            data: [],
            message: 'No sessions found.',
          };
        }

        const lines = ['### Available Sessions', ''];

        if (sessions.length > 0) {
          for (const s of sessions) {
            const isCurrent = currentSession && s.id === currentSession.id;
            const currentBadge = isCurrent ? ' **(Current)**' : '';
            lines.push(`- **\`${s.id}\`** [${s.status}]${currentBadge}: ${s.title}`);
            if (!isCurrent) {
              lines.push(`  → Type \`/sessions switch ${s.id}\` to switch to this session.`);
            }
          }
        } else if (currentSession) {
          lines.push(
            `- **\`${currentSession.id}\`** [${currentSession.status}] **(Current)**: ${currentSession.goalSummary}`
          );
        }

        return {
          success: true,
          actionTaken: 'sessions',
          markdown: lines.join('\n'),
          message: `Found ${sessions.length || 1} session(s).`,
          data: sessions,
        };
      },
    });
  }

  public dispose(): void {
    this._commands.clear();
  }
}
