/**
 * Agent Studios — Localization, Language Decoupling & Canonical Paths Unit Tests
 * Milestone M15.1 (W4-T1): First-Class Bilingual Localization (en-US, vi-VN) & Windows Canonical Paths
 */
/* eslint-disable @typescript-eslint/naming-convention */

import * as assert from 'assert';
import * as fs from 'fs';
import * as path from 'path';
import {
  containsPathTraversal,
  isPathContainedIn,
  resolveCanonicalPaths,
  resolveSafeContainedPath,
} from '../../config/CanonicalPaths';
import { LocalizationManager } from '../../i18n/LocalizationManager';
import { EN_US_CATALOG, TranslationKey } from '../../i18n/locales/en';
import { VI_VN_CATALOG } from '../../i18n/locales/vi';

describe('Bilingual Localization, Language Decoupling & Windows Canonical Paths (W4-T1)', () => {
  describe('1. English (en-US) & Vietnamese (vi-VN) 100% Key Parity & Domain Coverage', () => {
    it('enforces 100% exact key parity between EN_US_CATALOG and VI_VN_CATALOG', () => {
      const enKeys = Object.keys(EN_US_CATALOG).sort();
      const viKeys = Object.keys(VI_VN_CATALOG).sort();

      assert.ok(enKeys.length >= 60, `Expected comprehensive catalog (>= 60 keys), found ${enKeys.length}`);
      assert.deepStrictEqual(viKeys, enKeys, 'Vietnamese catalog keys must match English catalog keys 100%');

      for (const key of enKeys as TranslationKey[]) {
        const enVal = EN_US_CATALOG[key];
        const viVal = VI_VN_CATALOG[key];
        assert.ok(typeof enVal === 'string' && enVal.trim().length > 0, `English key "${key}" must be non-empty`);
        assert.ok(typeof viVal === 'string' && viVal.trim().length > 0, `Vietnamese key "${key}" must be non-empty`);
      }
    });

    it('preserves placeholder token parity between English and Vietnamese strings', () => {
      const tokenRegex = /\{([a-zA-Z0-9_]+)\}/g;
      for (const key of Object.keys(EN_US_CATALOG) as TranslationKey[]) {
        const enTokens = Array.from(EN_US_CATALOG[key].matchAll(tokenRegex), (m) => m[1]).sort();
        const viTokens = Array.from(VI_VN_CATALOG[key].matchAll(tokenRegex), (m) => m[1]).sort();
        assert.deepStrictEqual(
          viTokens,
          enTokens,
          `Interpolation tokens for key "${key}" must match between en-US and vi-VN`
        );
      }
    });

    it('covers all 9 Mission Control canonical tabs, 4 approval risk tiers, mode transitions, onboarding, and diagnostics', () => {
      const requiredMissionControlTabs: TranslationKey[] = [
        'missionControl.tab.overview',
        'missionControl.tab.fleet',
        'missionControl.tab.tasks',
        'missionControl.tab.runs',
        'missionControl.tab.timeline',
        'missionControl.tab.review',
        'missionControl.tab.approvals',
        'missionControl.tab.providers',
        'missionControl.tab.diagnostics',
      ];
      for (const tabKey of requiredMissionControlTabs) {
        assert.ok(EN_US_CATALOG[tabKey], `Missing English Mission Control tab key: ${tabKey}`);
        assert.ok(VI_VN_CATALOG[tabKey], `Missing Vietnamese Mission Control tab key: ${tabKey}`);
      }

      const requiredRiskTiers: TranslationKey[] = [
        'approvals.riskTier.low',
        'approvals.riskTier.medium',
        'approvals.riskTier.high',
        'approvals.riskTier.critical',
      ];
      for (const tierKey of requiredRiskTiers) {
        assert.ok(EN_US_CATALOG[tierKey], `Missing English risk tier key: ${tierKey}`);
        assert.ok(VI_VN_CATALOG[tierKey], `Missing Vietnamese risk tier key: ${tierKey}`);
      }

      assert.ok(EN_US_CATALOG['mode.openInIde']);
      assert.ok(EN_US_CATALOG['mode.switchToAgentMode']);
      assert.ok(EN_US_CATALOG['approvals.modal.twoPhaseTitle']);
      assert.ok(EN_US_CATALOG['onboarding.welcomeHeader']);
      assert.ok(EN_US_CATALOG['diagnostics.statusHealthy']);
    });
  });

  describe('2. Vietnamese NFC Unicode Normalization & Natural Developer Terminology', () => {
    it('enforces NFC Unicode normalization across all Vietnamese translation strings', () => {
      for (const [key, value] of Object.entries(VI_VN_CATALOG)) {
        assert.strictEqual(
          value,
          value.normalize('NFC'),
          `Vietnamese translation for "${key}" must be NFC-normalized`
        );
        // Verify it does not match NFD when diacritics are present
        if (value !== value.normalize('NFD')) {
          assert.notStrictEqual(
            value,
            value.normalize('NFD'),
            `Vietnamese string "${key}" must use precomposed NFC characters rather than decomposed NFD`
          );
        }
      }
    });

    it('uses natural, professional Vietnamese developer terminology', () => {
      const allViValues = Object.values(VI_VN_CATALOG).join(' | ');
      const expectedTerms = [
        'Chế độ Tác tử',
        'Chế độ IDE',
        'Trung tâm Điều hành',
        'Cây tác vụ (DAG)',
        'Nhánh công việc (Worktree)',
        'Phê duyệt',
        'Bản vá (Diff)',
        'Không gian làm việc',
      ];

      for (const term of expectedTerms) {
        assert.strictEqual(term, term.normalize('NFC'), `Expected term "${term}" must be NFC`);
        assert.ok(
          allViValues.includes(term),
          `Vietnamese catalog must include natural developer terminology: "${term}"`
        );
      }
    });
  });

  describe('3. Deterministic 4-Stage Fallback Hierarchy & Parameter Interpolation', () => {
    it('resolves through selected locale (vi-VN) -> base language (vi) -> canonical default (en-US) -> [missing: key]', () => {
      const i18n = new LocalizationManager({
        displayLanguage: 'vi-VN',
        customBundles: {
          'vi-VN': {
            'mode.agentMode': 'Chế độ Tác tử (vi-VN)',
          },
          vi: {
            'mode.agentMode': 'Chế độ Tác tử (vi)',
            'mode.ideMode': 'Chế độ IDE (từ gói cơ sở vi)',
          },
          'en-US': {
            'mode.agentMode': 'Agent Mode',
            'mode.ideMode': 'IDE Mode',
            'mode.missionControl': 'Mission Control (Canonical Default)',
          },
        },
      });

      // Stage 1: Hit in exact selected locale ('vi-VN')
      const stage1 = i18n.resolveTranslation('mode.agentMode');
      assert.strictEqual(stage1.resolvedFrom, 'selected-locale');
      assert.strictEqual(stage1.resolvedLocale, 'vi-VN');
      assert.strictEqual(stage1.text, 'Chế độ Tác tử (vi-VN)');

      // Stage 2: Missing in 'vi-VN', hit in base language bundle ('vi')
      const stage2 = i18n.resolveTranslation('mode.ideMode');
      assert.strictEqual(stage2.resolvedFrom, 'base-language');
      assert.strictEqual(stage2.resolvedLocale, 'vi');
      assert.strictEqual(stage2.text, 'Chế độ IDE (từ gói cơ sở vi)');

      // Stage 3: Missing in 'vi-VN' and 'vi', hit in canonical default ('en-US')
      const stage3 = i18n.resolveTranslation('mode.missionControl');
      assert.strictEqual(stage3.resolvedFrom, 'canonical-default');
      assert.strictEqual(stage3.resolvedLocale, 'en-US');
      assert.strictEqual(stage3.text, 'Mission Control (Canonical Default)');

      // Stage 4: Missing everywhere -> deterministic raw key identifier
      const stage4 = i18n.resolveTranslation('tasks.graph.reconciliation');
      assert.strictEqual(stage4.resolvedFrom, 'missing-key');
      assert.strictEqual(stage4.resolvedLocale, null);
      assert.strictEqual(stage4.text, '[missing: tasks.graph.reconciliation]');
    });

    it('interpolates dynamic parameters ({count}, {taskId}, {riskTier}) across English and Vietnamese', () => {
      const i18n = new LocalizationManager({ displayLanguage: 'en-US' });
      assert.strictEqual(
        i18n.t('missionControl.activeTasksSummary', { count: 7 }),
        '7 active tasks in execution graph'
      );
      assert.strictEqual(
        i18n.t('approvals.modal.twoPhasePrompt', { taskId: 'W4-T1', riskTier: 'CRITICAL' }),
        'Review high-risk action for task W4-T1 (CRITICAL) before authorizing execution.'
      );

      i18n.setDisplayLanguage('vi-VN');
      assert.strictEqual(
        i18n.t('missionControl.activeTasksSummary', { count: 7 }),
        '7 tác vụ đang hoạt động trong Cây tác vụ (DAG)'
      );
      assert.strictEqual(
        i18n.t('approvals.modal.twoPhasePrompt', { taskId: 'W4-T1', riskTier: 'CRITICAL' }),
        'Kiểm tra kỹ thao tác rủi ro cao của tác vụ W4-T1 (CRITICAL) trước khi cấp quyền thực thi.'
      );
    });
  });

  describe('4. Invariant: Display Language !== Agent Response Language', () => {
    it('proves changing displayLanguage never mutates agentResponseLanguage or effective LLM prompt', () => {
      const i18n = new LocalizationManager({
        displayLanguage: 'en-US',
        agentResponseLanguage: 'auto',
      });

      const englishPrompt = 'Refactor the state projection service and add unit tests.';
      const vietnamesePrompt = 'Tối ưu hóa luồng xử lý sự kiện IPC trong tiến trình nền.';

      // In 'auto' mode, prompt is untouched whether UI is English or Vietnamese
      assert.strictEqual(i18n.resolveEffectiveAgentPrompt(englishPrompt), englishPrompt);
      assert.strictEqual(i18n.resolveEffectiveAgentPrompt(vietnamesePrompt), vietnamesePrompt);

      i18n.setDisplayLanguage('vi-VN');
      assert.strictEqual(i18n.getDisplayLanguage(), 'vi-VN');
      assert.strictEqual(i18n.getAgentResponseLanguage(), 'auto');
      assert.strictEqual(i18n.t('mode.agentMode'), 'Chế độ Tác tử');
      assert.strictEqual(
        i18n.resolveEffectiveAgentPrompt(englishPrompt),
        englishPrompt,
        'Switching UI to Vietnamese must NEVER force Vietnamese response language on English prompts'
      );
      assert.strictEqual(i18n.resolveEffectiveAgentPrompt(vietnamesePrompt), vietnamesePrompt);
    });

    it('allows Vietnamese UI with forced English agent responses and English UI with forced Vietnamese agent responses', () => {
      const i18n = new LocalizationManager({
        displayLanguage: 'vi-VN',
        agentResponseLanguage: 'en-US',
      });

      const prompt = 'Analyze task graph critical path';
      assert.strictEqual(i18n.t('mode.missionControl'), 'Trung tâm Điều hành');
      assert.strictEqual(
        i18n.resolveEffectiveAgentPrompt(prompt),
        `${prompt}\n\n[Agent Response Language: en-US]`
      );

      // Flip to English UI + Vietnamese agent responses
      i18n.setDisplayLanguage('en-US');
      i18n.setAgentResponseLanguage('vi-VN');
      assert.strictEqual(i18n.t('mode.missionControl'), 'Mission Control');
      assert.strictEqual(
        i18n.resolveEffectiveAgentPrompt(prompt),
        `${prompt}\n\n[Agent Response Language: vi-VN]`
      );

      assert.strictEqual(i18n.verifyLanguageDecouplingInvariant(), true);
    });
  });

  describe('5. Windows-First Canonical OS Paths & Path Traversal Protection', () => {
    it('resolves canonical Windows paths under %APPDATA% and %LOCALAPPDATA%', () => {
      const winEnv: NodeJS.ProcessEnv = {
        APPDATA: 'C:\\Users\\Developer\\AppData\\Roaming',
        LOCALAPPDATA: 'C:\\Users\\Developer\\AppData\\Local',
      };

      const resolved = resolveCanonicalPaths(winEnv, 'win32');
      assert.strictEqual(resolved.platform, 'win32');
      assert.strictEqual(
        resolved.configPath,
        'C:\\Users\\Developer\\AppData\\Roaming\\AgentStudios\\config.toml'
      );
      assert.strictEqual(
        resolved.stateDbPath,
        'C:\\Users\\Developer\\AppData\\Local\\AgentStudios\\state.db'
      );
      assert.strictEqual(
        resolved.artifactsDir,
        'C:\\Users\\Developer\\AppData\\Local\\AgentStudios\\artifacts\\'
      );
    });

    it('falls back to %USERPROFILE%\\AppData when APPDATA/LOCALAPPDATA are unset on Windows', () => {
      const winFallbackEnv: NodeJS.ProcessEnv = {
        USERPROFILE: 'D:\\Profiles\\Operator',
      };

      const resolved = resolveCanonicalPaths(winFallbackEnv, 'win32');
      assert.strictEqual(
        resolved.configPath,
        'D:\\Profiles\\Operator\\AppData\\Roaming\\AgentStudios\\config.toml'
      );
      assert.strictEqual(
        resolved.stateDbPath,
        'D:\\Profiles\\Operator\\AppData\\Local\\AgentStudios\\state.db'
      );
      assert.strictEqual(
        resolved.artifactsDir,
        'D:\\Profiles\\Operator\\AppData\\Local\\AgentStudios\\artifacts\\'
      );
    });

    it('resolves canonical POSIX paths under ~/.config and ~/.local/share', () => {
      const posixEnv: NodeJS.ProcessEnv = {
        HOME: '/home/developer',
      };

      const resolved = resolveCanonicalPaths(posixEnv, 'linux');
      assert.strictEqual(resolved.platform, 'linux');
      assert.strictEqual(resolved.configPath, '/home/developer/.config/AgentStudios/config.toml');
      assert.strictEqual(resolved.stateDbPath, '/home/developer/.local/share/AgentStudios/state.db');
      assert.strictEqual(
        resolved.artifactsDir,
        '/home/developer/.local/share/AgentStudios/artifacts/'
      );
    });

    it('enforces path containment and rejects path traversal (..), URL-encoded traversal, null bytes, and drive escapes', () => {
      const winArtifactsDir = 'C:\\Users\\Developer\\AppData\\Local\\AgentStudios\\artifacts\\';

      assert.strictEqual(containsPathTraversal('..\\secret\\config.toml'), true);
      assert.strictEqual(containsPathTraversal('run-01/../../passwd'), true);
      assert.strictEqual(containsPathTraversal('run-01/%2e%2e/secret'), true);
      assert.strictEqual(containsPathTraversal('run-01/patch.diff\0.txt'), true);
      assert.strictEqual(containsPathTraversal('run-01\\task-02\\patch.diff'), false);

      assert.strictEqual(
        isPathContainedIn(
          winArtifactsDir,
          'C:\\Users\\Developer\\AppData\\Local\\AgentStudios\\artifacts\\sha256-abc.diff',
          'win32'
        ),
        true
      );
      assert.strictEqual(
        isPathContainedIn(
          winArtifactsDir,
          'C:\\Users\\Developer\\AppData\\Local\\AgentStudios\\state.db',
          'win32'
        ),
        false
      );
      assert.strictEqual(
        isPathContainedIn(
          winArtifactsDir,
          'D:\\Users\\Developer\\AppData\\Local\\AgentStudios\\artifacts\\sha256-abc.diff',
          'win32'
        ),
        false
      );

      const safeArtifact = resolveSafeContainedPath(
        winArtifactsDir,
        'run-42\\patch-01.diff',
        'win32'
      );
      assert.strictEqual(
        safeArtifact,
        'C:\\Users\\Developer\\AppData\\Local\\AgentStudios\\artifacts\\run-42\\patch-01.diff'
      );

      assert.throws(() => {
        resolveSafeContainedPath(winArtifactsDir, '..\\state.db', 'win32');
      }, /Path traversal detected/);

      assert.throws(() => {
        resolveSafeContainedPath(winArtifactsDir, 'C:\\Windows\\System32\\cmd.exe', 'win32');
      }, /Relative path required/);
    });
  });

  describe('6. Package Manifest Configuration & One Activity Bar Icon Invariant', () => {
    it('declares displayLanguage and agentResponseLanguage in package.json while preserving single activitybar icon', () => {
      const pkgPath = path.resolve(__dirname, '../../../package.json');
      const pkgRaw = fs.readFileSync(pkgPath, 'utf8');
      const pkg = JSON.parse(pkgRaw) as {
        contributes: {
          viewsContainers: { activitybar: Array<{ id: string }> };
          configuration: {
            properties: Record<
              string,
              { type: string; default: unknown; enum?: string[]; description?: string }
            >;
          };
        };
      };

      const activityBarContainers = pkg.contributes.viewsContainers.activitybar;
      assert.strictEqual(
        activityBarContainers.length,
        1,
        'Must strictly preserve One Activity Bar Icon Rule'
      );
      assert.strictEqual(activityBarContainers[0].id, 'agent-studios-activitybar');

      const props = pkg.contributes.configuration.properties;
      assert.ok(props['agentStudios.displayLanguage'], 'Missing agentStudios.displayLanguage');
      assert.strictEqual(props['agentStudios.displayLanguage'].type, 'string');
      assert.strictEqual(props['agentStudios.displayLanguage'].default, 'en-US');
      assert.deepStrictEqual(props['agentStudios.displayLanguage'].enum, ['en-US', 'vi-VN']);

      assert.ok(
        props['agentStudios.agentResponseLanguage'],
        'Missing agentStudios.agentResponseLanguage'
      );
      assert.strictEqual(props['agentStudios.agentResponseLanguage'].type, 'string');
      assert.strictEqual(props['agentStudios.agentResponseLanguage'].default, 'auto');
      assert.deepStrictEqual(props['agentStudios.agentResponseLanguage'].enum, [
        'auto',
        'en-US',
        'vi-VN',
      ]);
    });
  });
});
