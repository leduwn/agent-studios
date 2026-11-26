/**
 * Agent Studios — Idempotent User-State Migration & Hygiene Filter
 * Cleans legacy preview/fixture/E2E-provenance state without touching real user data.
 */

import * as vscode from 'vscode';

export const CURRENT_STATE_MIGRATION_VERSION = 2;
export const STATE_MIGRATION_VERSION_KEY = 'agentStudios.stateMigrationVersion';

const KNOWN_FIXTURE_IDS = new Set([
  'session-m14-fixture',
  'session-m13-refactor',
  'session-m12-dag-layout',
  'studio-fixture',
]);

const KNOWN_TEST_STUDIO_NAMES = new Set([
  'E2E Genuine Verification Studio',
]);

const KNOWN_FIXTURE_PATH_MARKER = 'agent-studios-ide-ui';

export interface SessionRecordLike {
  id?: string;
  name?: string;
  goalSummary?: string;
  title?: string;
  workspacePath?: string;
  isPreview?: boolean;
}

export interface UserStateMigrationReport {
  previousVersion: number;
  currentVersion: number;
  migrated: boolean;
  clearedPreviewSetting: boolean;
  removedFixtureEntries: number;
}

/**
 * Returns true ONLY for verified fixture or leaked E2E test records,
 * never for legitimate user sessions or projects.
 */
export function isKnownTestOrFixtureRecord(record: SessionRecordLike | null | undefined): boolean {
  if (!record || typeof record !== 'object') {
    return false;
  }
  if (record.isPreview === true) {
    return true;
  }
  if (typeof record.id === 'string' && KNOWN_FIXTURE_IDS.has(record.id.trim())) {
    return true;
  }
  for (const label of [record.name, record.goalSummary, record.title]) {
    if (typeof label === 'string' && KNOWN_TEST_STUDIO_NAMES.has(label.trim())) {
      return true;
    }
  }
  if (
    typeof record.workspacePath === 'string' &&
    record.workspacePath.toLowerCase().includes(KNOWN_FIXTURE_PATH_MARKER)
  ) {
    return true;
  }
  return false;
}

function filterRecordArray(raw: unknown): { next: unknown[] | null; removed: number } {
  if (!Array.isArray(raw)) {
    return { next: null, removed: 0 };
  }
  let removed = 0;
  const kept = raw.filter((item) => {
    if (isKnownTestOrFixtureRecord(item as SessionRecordLike)) {
      removed += 1;
      return false;
    }
    return true;
  });
  return { next: kept, removed };
}

/**
 * Runs idempotent v2 user-state migration on extension activation.
 * - Clears legacy `agentStudios.previewMode` setting if defined.
 * - Strips known fixture/E2E-leaked entries from globalState / workspaceState
 *   while preserving all real user sessions, workspaces, and provider profiles.
 */
export async function runUserStateMigration(
  context: vscode.ExtensionContext
): Promise<UserStateMigrationReport> {
  const previousVersion = context.globalState.get<number>(STATE_MIGRATION_VERSION_KEY, 0);
  let clearedPreviewSetting = false;
  let removedFixtureEntries = 0;

  try {
    const config = vscode.workspace.getConfiguration('agentStudios');
    const inspectFn = (config as unknown as { inspect?: (section: string) => { globalValue?: unknown; workspaceValue?: unknown } }).inspect;
    if (typeof inspectFn === 'function') {
      const inspected = inspectFn.call(config, 'previewMode');
      if (inspected?.globalValue !== undefined) {
        await config.update('previewMode', undefined, vscode.ConfigurationTarget.Global);
        clearedPreviewSetting = true;
      }
      if (inspected?.workspaceValue !== undefined) {
        await config.update('previewMode', undefined, vscode.ConfigurationTarget.Workspace);
        clearedPreviewSetting = true;
      }
    }
  } catch {
    // Non-fatal in read-only or mock host environments
  }

  const mementos: vscode.Memento[] = [context.globalState, context.workspaceState];
  const scalarSessionKeys = ['agentStudios.activeSessionId', 'agentStudios.lastSessionId'];

  for (const memento of mementos) {
    for (const key of scalarSessionKeys) {
      const val = memento.get<unknown>(key);
      if (typeof val === 'string' && KNOWN_FIXTURE_IDS.has(val.trim())) {
        await memento.update(key, undefined);
        removedFixtureEntries += 1;
      }
    }

    for (const convKey of ['agentStudios.conversations.v1', 'agentStudios.chatConversations']) {
      const conversations = memento.get<unknown>(convKey);
      const convFiltered = filterRecordArray(conversations);
      if (convFiltered.next !== null && convFiltered.removed > 0) {
        await memento.update(convKey, convFiltered.next);
        removedFixtureEntries += convFiltered.removed;
      }
    }

    const selectedModel = memento.get<{ instanceId?: string; modelId?: string }>(
      'agentStudios.selectedProviderModel'
    );
    if (
      selectedModel &&
      typeof selectedModel === 'object' &&
      (selectedModel.instanceId === 'ollama-local' || selectedModel.instanceId === 'fixture')
    ) {
      await memento.update('agentStudios.selectedProviderModel', undefined);
      removedFixtureEntries += 1;
    }
  }

  const migrated = previousVersion < CURRENT_STATE_MIGRATION_VERSION;
  if (migrated) {
    await context.globalState.update(
      STATE_MIGRATION_VERSION_KEY,
      CURRENT_STATE_MIGRATION_VERSION
    );
  }

  return {
    previousVersion,
    currentVersion: CURRENT_STATE_MIGRATION_VERSION,
    migrated,
    clearedPreviewSetting,
    removedFixtureEntries,
  };
}
