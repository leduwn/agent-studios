/**
 * Agent Studios — Production Bundle & Manifest Hygiene Gate Unit Tests
 */

import * as assert from 'assert';
import * as fs from 'fs';
import * as path from 'path';

describe('Production Hygiene Verification', () => {
  const extRoot = path.resolve(__dirname, '../../../');

  function collectProductionTsFiles(dir: string): string[] {
    const results: string[] = [];
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        if (entry.name === 'test') {
          continue;
        }
        results.push(...collectProductionTsFiles(full));
      } else if (entry.isFile() && entry.name.endsWith('.ts')) {
        results.push(full);
      }
    }
    return results;
  }

  it('enforces zero FixtureBridge, missionControlFixtures, or Developer Preview references in production src/', () => {
    const srcFiles = collectProductionTsFiles(path.join(extRoot, 'src'));
    const forbidden = [
      'FixtureBridge',
      'missionControlFixtures',
      'Developer Preview Mode',
      'PREVIEW FIXTURE',
      'toggleFixturePreview',
      'Host Modal Gate (HMAC-SHA256)',
      'agent-studios-ide-ui',
      'E2E Genuine Verification Studio',
      'Dynamic Provider Discovery pending',
    ];

    for (const file of srcFiles) {
      // UserStateMigration legitimately checks legacy fixture IDs/names to scrub them from user state
      if (file.endsWith('UserStateMigration.ts')) {
        continue;
      }
      const content = fs.readFileSync(file, 'utf8');
      for (const marker of forbidden) {
        assert.strictEqual(
          content.includes(marker),
          false,
          `Forbidden marker "${marker}" found in production source ${path.relative(extRoot, file)}`
        );
      }
    }
  });

  it('keeps 1 Activity Bar icon with Chat as default primary view and removes previewMode from package.json', () => {
    const pkg = JSON.parse(fs.readFileSync(path.join(extRoot, 'package.json'), 'utf8'));
    assert.strictEqual(pkg.contributes.viewsContainers.activitybar.length, 1);

    const views = pkg.contributes.views['agent-studios-activitybar'] as Array<{
      id: string;
      when?: string;
    }>;
    assert.strictEqual(views[0].id, 'agent-studios.chat');
    assert.strictEqual(views[0].when, undefined);
    for (const secondary of views.slice(1)) {
      assert.strictEqual(secondary.when, 'config.agentStudios.showWorkbenchTreeViews');
    }

    assert.strictEqual(pkg.contributes.configuration.properties['agentStudios.previewMode'], undefined);
    assert.strictEqual(
      pkg.contributes.commands.some((c: { command: string }) => c.command === 'agent-studios.toggleFixturePreview'),
      false
    );
  });
});
