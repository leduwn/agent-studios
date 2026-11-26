/**
 * Agent Studios — UserStateMigration Unit Tests
 */

import * as assert from 'assert';
import * as vscode from 'vscode';
import {
  CURRENT_STATE_MIGRATION_VERSION,
  STATE_MIGRATION_VERSION_KEY,
  isKnownTestOrFixtureRecord,
  runUserStateMigration,
} from '../../state/UserStateMigration';

function createMemoryMemento(initial: Record<string, unknown> = {}): vscode.Memento {
  const store = new Map<string, unknown>(Object.entries(initial));
  return {
    keys: () => Array.from(store.keys()),
    get: <T>(key: string, defaultValue?: T): T =>
      (store.has(key) ? store.get(key) : defaultValue) as T,
    update: async (key: string, value: unknown): Promise<void> => {
      if (value === undefined) {
        store.delete(key);
      } else {
        store.set(key, value);
      }
    },
  };
}

describe('UserStateMigration Unit Tests', () => {
  it('identifies ONLY exact fixture and leaked E2E test records, never real user projects', () => {
    assert.strictEqual(isKnownTestOrFixtureRecord({ id: 'session-m14-fixture' }), true);
    assert.strictEqual(isKnownTestOrFixtureRecord({ goalSummary: 'E2E Genuine Verification Studio' }), true);
    assert.strictEqual(isKnownTestOrFixtureRecord({ workspacePath: 'd:/Projects/agent-studios-ide-ui/apps/agent-studios-vscode' }), true);
    assert.strictEqual(isKnownTestOrFixtureRecord({ isPreview: true }), true);

    assert.strictEqual(
      isKnownTestOrFixtureRecord({
        id: 'session-user-991',
        goalSummary: 'Implement OAuth login in customer portal',
        workspacePath: 'D:\\Projects\\my-real-saas-app',
        isPreview: false,
      }),
      false
    );
  });

  it('cleans leaked fixture entries idempotently while preserving real user conversations', async () => {
    const conversations = [
      { id: 'session-m14-fixture', title: 'Fixture Session' },
      { id: 'user-conv-1', title: 'Fix billing calculation bug', workspacePath: 'D:\\work\\billing' },
      { id: 'e2e-leak', title: 'E2E Genuine Verification Studio' },
    ];

    const globalState = createMemoryMemento({
      'agentStudios.activeSessionId': 'session-m14-fixture',
      'agentStudios.selectedProviderModel': { instanceId: 'ollama-local', modelId: 'deepseek-coder:6.7b' },
    });
    const workspaceState = createMemoryMemento({
      'agentStudios.conversations.v1': conversations,
    });

    const context = {
      globalState,
      workspaceState,
    } as unknown as vscode.ExtensionContext;

    const first = await runUserStateMigration(context);
    assert.strictEqual(first.migrated, true);
    assert.strictEqual(first.currentVersion, CURRENT_STATE_MIGRATION_VERSION);
    assert.ok(first.removedFixtureEntries >= 4);

    assert.strictEqual(globalState.get('agentStudios.activeSessionId'), undefined);
    assert.strictEqual(globalState.get('agentStudios.selectedProviderModel'), undefined);
    assert.strictEqual(globalState.get(STATE_MIGRATION_VERSION_KEY), CURRENT_STATE_MIGRATION_VERSION);

    const remaining = workspaceState.get<Array<{ id: string }>>('agentStudios.conversations.v1', []);
    assert.strictEqual(remaining.length, 1);
    assert.strictEqual(remaining[0].id, 'user-conv-1');

    // Second run is idempotent and preserves remaining real user data
    const second = await runUserStateMigration(context);
    assert.strictEqual(second.migrated, false);
    assert.strictEqual(second.removedFixtureEntries, 0);
    assert.strictEqual(workspaceState.get<Array<{ id: string }>>('agentStudios.conversations.v1', []).length, 1);
  });
});
