/**
 * Agent Studios — Extensibility Management Commands (Wave 4 / Group 6)
 *
 * Workspace inspection & management helpers for Skills (SKILL.md), MCP servers
 * (strictly secret-redacted), Lifecycle Hooks, and hierarchical instructions
 * (AGENTS.md / CLAUDE.md).
 */

import * as path from 'path';
import * as vscode from 'vscode';

export interface DiscoveredExtensibilitySummary {
  skills: string[];
  mcpServers: Array<{ name: string; transport: string; endpointRedacted: string }>;
  hookConfigs: string[];
  instructionFiles: string[];
}

export function redactMcpEndpoint(raw: string): string {
  return raw
    .replace(/([?&](?:token|key|api_key|secret|password|auth)=)[^&\s]+/gi, '$1***REDACTED***')
    .replace(/(Bearer\s+)[A-Za-z0-9._-]+/gi, '$1***REDACTED***');
}

export async function discoverWorkspaceExtensibility(): Promise<DiscoveredExtensibilitySummary> {
  const [skillUris, hookUris, instructionUris, mcpUris] = await Promise.all([
    vscode.workspace.findFiles('**/SKILL.md', '{**/node_modules/**,**/.git/**,**/target/**}', 50),
    vscode.workspace.findFiles(
      '**/{.agent-studios,.codex,.claude}/hooks*.json',
      '{**/node_modules/**,**/.git/**}',
      20
    ),
    vscode.workspace.findFiles(
      '**/{AGENTS.md,CLAUDE.md,GEMINI.md}',
      '{**/node_modules/**,**/.git/**,**/target/**}',
      50
    ),
    vscode.workspace.findFiles('**/{mcp.json,.mcp.json}', '{**/node_modules/**,**/.git/**}', 20),
  ]);

  const rel = (u: vscode.Uri) => vscode.workspace.asRelativePath(u);
  const skills = skillUris
    .map(rel)
    .filter((p) => path.posix.basename(p).toLowerCase() === 'skill.md');
  const hooks = hookUris
    .map(rel)
    .filter((p) => /^hooks.*\.json$/i.test(path.posix.basename(p)));
  const instructions = instructionUris
    .map(rel)
    .filter((p) => ['agents.md', 'claude.md', 'gemini.md'].includes(path.posix.basename(p).toLowerCase()));
  const filteredMcpUris = mcpUris.filter((u) =>
    ['mcp.json', '.mcp.json'].includes(path.posix.basename(rel(u)).toLowerCase())
  );

  const mcpServers: DiscoveredExtensibilitySummary['mcpServers'] = [];
  for (const uri of filteredMcpUris) {
    try {
      const rawBytes = await vscode.workspace.fs.readFile(uri);
      const parsed = JSON.parse(Buffer.from(rawBytes).toString('utf8')) as {
        mcpServers?: Record<string, { command?: string; url?: string }>;
      };
      if (parsed.mcpServers) {
        for (const [name, cfg] of Object.entries(parsed.mcpServers)) {
          const transport = cfg.url ? 'http' : 'stdio';
          const endpoint = cfg.url || cfg.command || 'configured';
          mcpServers.push({
            name,
            transport,
            endpointRedacted: redactMcpEndpoint(endpoint),
          });
        }
      }
    } catch {
      // Malformed or unreadable optional MCP manifest is ignored safely
    }
  }

  return {
    skills: Array.from(new Set(skills)).sort(),
    mcpServers,
    hookConfigs: Array.from(new Set(hooks)).sort(),
    instructionFiles: Array.from(new Set(instructions)).sort(
      (a, b) => a.split('/').length - b.split('/').length || a.localeCompare(b)
    ),
  };
}

export function registerExtensibilityCommands(context: vscode.ExtensionContext): vscode.Disposable {
  const disposables = [
    vscode.commands.registerCommand('agent-studios.manageSkills', async () => {
      const summary = await discoverWorkspaceExtensibility();
      if (summary.skills.length === 0) {
        vscode.window.showInformationMessage('No workspace SKILL.md manifests found.');
        return summary;
      }
      const picked = await vscode.window.showQuickPick(
        summary.skills.map((s) => ({ label: path.posix.basename(path.posix.dirname(s)) || s, description: s })),
        { placeHolder: 'Discovered Agent Studios Skills (SKILL.md)' }
      );
      return picked ?? summary;
    }),
    vscode.commands.registerCommand('agent-studios.manageMcpServers', async () => {
      const summary = await discoverWorkspaceExtensibility();
      vscode.window.showInformationMessage(
        `Discovered ${summary.mcpServers.length} MCP server(s) with credential redaction active.`
      );
      return summary.mcpServers;
    }),
    vscode.commands.registerCommand('agent-studios.inspectInstructionsHierarchy', async () => {
      const summary = await discoverWorkspaceExtensibility();
      vscode.window.showInformationMessage(
        `Active instruction chain (${summary.instructionFiles.length} file(s)): ${summary.instructionFiles.join(' -> ') || 'none'}`
      );
      return summary.instructionFiles;
    }),
  ];

  const composite = vscode.Disposable.from(...disposables);
  context.subscriptions.push(composite);
  return composite;
}
