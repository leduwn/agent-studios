/**
 * Agent Studios — AgsdBinaryResolver Unit Tests
 * Milestone M15 UI-06: Release Hardening & CI Binary Provenance Gate
 */

import * as assert from 'assert';
import * as path from 'path';
import {
  isStaleHistoricalPath,
  isSubpathOrEqual,
  normalizePathForComparison,
  resolveAgsdBinaryPath,
  STALE_WORKTREE_PATTERNS,
} from '../e2e/AgsdBinaryResolver';

describe('AgsdBinaryResolver Provenance Tests (M15 UI-06 Hardening)', () => {
  describe('isStaleHistoricalPath Detection', () => {
    it('detects and flags obsolete historical worktree paths', () => {
      assert.ok(STALE_WORKTREE_PATTERNS.length >= 4, 'Must define at least 4 stale worktree patterns');
      const stalePaths = [
        'D:\\Projects\\agent-studios-m15-04\\agent-studios-rs\\target\\debug\\agsd.exe',
        'D:\\Projects\\agent-studios-m15-03\\agent-studios-rs\\target\\debug\\agsd.exe',
        'D:\\Projects\\agent-studios-m15-02\\agent-studios-rs\\target\\debug\\agsd.exe',
        'D:\\Projects\\agent-studios-m15\\agent-studios-rs\\target\\debug\\agsd.exe',
        '/home/runner/worktrees/agent-studios-m15-03/agent-studios-rs/target/debug/agsd',
        '/tmp/agent-studios-m15-04/target/debug/agsd',
      ];

      for (const p of stalePaths) {
        assert.strictEqual(
          isStaleHistoricalPath(p),
          true,
          `Path '${p}' must be flagged as stale historical path`
        );
      }
    });

    it('permits valid current worktrees and unified baseline paths', () => {
      const validPaths = [
        'D:\\Projects\\agent-studios\\agent-studios-rs\\target\\debug\\agsd.exe',
        'D:\\Projects\\agent-studios-active\\m15-ui-06\\agent-studios-rs\\target\\debug\\agsd.exe',
        '/home/runner/work/agent-studios/agent-studios/agent-studios-rs/target/debug/agsd',
        '/workspace/agent-studios-active/m15-ui-06/agent-studios-rs/target/debug/agsd',
      ];

      for (const p of validPaths) {
        assert.strictEqual(
          isStaleHistoricalPath(p),
          false,
          `Path '${p}' must not be flagged as stale`
        );
      }
    });
  });

  describe('Path Normalization and Subpath Comparison', () => {
    it('handles Windows path normalization and drive-letter case correctly', () => {
      const p1 = 'D:\\Projects\\Agent-Studios\\agent-studios-rs\\target\\debug\\agsd.exe';
      const p2 = 'd:/projects/agent-studios/agent-studios-rs/target/debug/agsd.exe';
      assert.strictEqual(
        normalizePathForComparison(p1, true),
        normalizePathForComparison(p2, true)
      );
    });

    it('verifies subpath containment on Windows regardless of separator or casing', () => {
      const workspace = 'd:\\projects\\agent-studios';
      const targetBinary = 'D:/Projects/Agent-Studios/agent-studios-rs/target/debug/agsd.exe';
      assert.strictEqual(isSubpathOrEqual(targetBinary, workspace, true), true);

      const outsideBinary = 'D:/OtherProjects/agent-studios-rs/target/debug/agsd.exe';
      assert.strictEqual(isSubpathOrEqual(outsideBinary, workspace, true), false);
    });
  });

  describe('Local Development Provenance Policy (CI=false)', () => {
    it('permits local developer explicit override pointing to valid external binary', () => {
      const externalBinary = path.resolve('/custom/standalone/agsd.exe');
      const resolved = resolveAgsdBinaryPath({
        env: { CI: 'false', AGSD_BIN_PATH: externalBinary },
        existsSync: (p) => path.resolve(p) === externalBinary,
      });
      assert.strictEqual(resolved, externalBinary);
    });

    it('rejects explicit AGSD_BIN_PATH if it targets a stale historical worktree', () => {
      const staleBinary = 'D:\\Projects\\agent-studios-m15-03\\agent-studios-rs\\target\\debug\\agsd.exe';
      assert.throws(
        () => {
          resolveAgsdBinaryPath({
            env: { CI: 'false', AGSD_BIN_PATH: staleBinary },
            existsSync: () => true,
          });
        },
        (err: Error) => {
          return err.message.includes('points to stale historical worktree');
        }
      );
    });

    it('rejects explicit AGSD_BIN_PATH if file does not exist on disk', () => {
      const missingBinary = path.resolve('/valid/workspace/missing/agsd.exe');
      assert.throws(
        () => {
          resolveAgsdBinaryPath({
            env: { CI: 'false', AGSD_BIN_PATH: missingBinary },
            existsSync: () => false,
          });
        },
        (err: Error) => {
          return err.message.includes('does not exist on disk');
        }
      );
    });

    it('fails clearly with cargo build instructions when no binary is found locally', () => {
      assert.throws(
        () => {
          resolveAgsdBinaryPath({
            env: { CI: 'false' },
            cwd: '/empty/mock/repo',
            existsSync: () => false,
          });
        },
        (err: Error) => {
          return (
            err.message.includes('Real Rust daemon binary (agsd) not found') &&
            err.message.includes('cargo build --bin agsd -p agent-studios-ipc-transport')
          );
        }
      );
    });
  });

  describe('Strict CI Provenance Enforcement Policy (CI=true)', () => {
    const ciWorkspace = path.resolve('/home/runner/work/agent-studios/agent-studios');

    it('accepts explicit AGSD_BIN_PATH in CI if within current checkout build target', () => {
      const validCiBinary = path.join(ciWorkspace, 'agent-studios-rs', 'target', 'debug', 'agsd');
      const resolved = resolveAgsdBinaryPath({
        env: {
          CI: 'true',
          GITHUB_WORKSPACE: ciWorkspace,
          AGSD_BIN_PATH: validCiBinary,
        },
        platform: 'linux',
        existsSync: (p) => path.resolve(p) === validCiBinary,
      });

      assert.strictEqual(resolved, validCiBinary);
    });

    it('rejects explicit AGSD_BIN_PATH in CI if outside current checked-out repository root', () => {
      const externalBinary = path.resolve('/home/runner/external-tools/agsd');
      assert.throws(
        () => {
          resolveAgsdBinaryPath({
            env: {
              CI: 'true',
              GITHUB_WORKSPACE: ciWorkspace,
              AGSD_BIN_PATH: externalBinary,
            },
            platform: 'linux',
            existsSync: () => true,
          });
        },
        (err: Error) => {
          return (
            err.message.includes('Strict CI provenance error') &&
            err.message.includes('outside the checked-out repository root')
          );
        }
      );
    });

    it('rejects explicit AGSD_BIN_PATH in CI if inside checkout but outside compiled target directories', () => {
      const maliciousScript = path.join(ciWorkspace, 'scripts', 'fake-agsd');
      assert.throws(
        () => {
          resolveAgsdBinaryPath({
            env: {
              CI: 'true',
              GITHUB_WORKSPACE: ciWorkspace,
              AGSD_BIN_PATH: maliciousScript,
            },
            platform: 'linux',
            existsSync: () => true,
          });
        },
        (err: Error) => {
          return (
            err.message.includes('Strict CI provenance error') &&
            err.message.includes('not located in an expected compiled daemon target directory')
          );
        }
      );
    });

    it('rejects explicit AGSD_BIN_PATH in CI if it targets a stale historical worktree', () => {
      const staleCiBinary = '/home/runner/worktrees/agent-studios-m15-04/agent-studios-rs/target/debug/agsd';
      assert.throws(
        () => {
          resolveAgsdBinaryPath({
            env: {
              CI: 'true',
              GITHUB_WORKSPACE: ciWorkspace,
              AGSD_BIN_PATH: staleCiBinary,
            },
            platform: 'linux',
            existsSync: () => true,
          });
        },
        (err: Error) => {
          return err.message.includes('points to stale historical worktree');
        }
      );
    });

    it('resolves valid default binary from current checkout in CI when AGSD_BIN_PATH is unset', () => {
      const defaultBinary = path.join(ciWorkspace, 'agent-studios-rs', 'target', 'debug', 'agsd');
      const resolved = resolveAgsdBinaryPath({
        env: {
          CI: 'true',
          GITHUB_WORKSPACE: ciWorkspace,
        },
        platform: 'linux',
        existsSync: (p) => path.resolve(p) === defaultBinary,
      });

      assert.strictEqual(resolved, defaultBinary);
    });

    it('fails clearly in CI if binary is missing from current checkout and never falls back silently', () => {
      assert.throws(
        () => {
          resolveAgsdBinaryPath({
            env: {
              CI: 'true',
              GITHUB_WORKSPACE: ciWorkspace,
            },
            platform: 'linux',
            existsSync: () => false,
          });
        },
        (err: Error) => {
          return (
            err.message.includes('CI provenance error') &&
            err.message.includes('cargo build --bin agsd')
          );
        }
      );
    });
  });
});
