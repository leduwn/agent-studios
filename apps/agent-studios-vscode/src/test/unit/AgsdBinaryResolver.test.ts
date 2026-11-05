/**
 * Agent Studios — AgsdBinaryResolver Unit Tests
 * Milestone M15 UI-06: Release Hardening & Test Provenance
 */

import * as assert from 'assert';
import * as path from 'path';
import {
  isStaleHistoricalPath,
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

  describe('Explicit AGSD_BIN_PATH Policy', () => {
    it('returns valid AGSD_BIN_PATH when pointing to existing binary', () => {
      const fakeBinary = path.resolve('/valid/workspace/agent-studios-rs/target/debug/agsd.exe');
      const resolved = resolveAgsdBinaryPath({
        env: { AGSD_BIN_PATH: fakeBinary },
        existsSync: (p) => path.resolve(p) === fakeBinary,
      });

      assert.strictEqual(resolved, fakeBinary);
    });

    it('rejects explicit AGSD_BIN_PATH if it targets a stale historical worktree', () => {
      const staleBinary = 'D:\\Projects\\agent-studios-m15-03\\agent-studios-rs\\target\\debug\\agsd.exe';
      assert.throws(
        () => {
          resolveAgsdBinaryPath({
            env: { AGSD_BIN_PATH: staleBinary },
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
            env: { AGSD_BIN_PATH: missingBinary },
            existsSync: () => false,
          });
        },
        (err: Error) => {
          return err.message.includes('does not exist on disk');
        }
      );
    });
  });

  describe('CI Provenance Enforcement Policy', () => {
    it('strictly requires binary from checked-out workspace in CI environment', () => {
      const ciWorkspace = path.resolve('/home/runner/work/agent-studios/agent-studios');
      const expectedBinary = path.join(ciWorkspace, 'agent-studios-rs', 'target', 'debug', 'agsd');

      const resolved = resolveAgsdBinaryPath({
        env: {
          CI: 'true',
          GITHUB_WORKSPACE: ciWorkspace,
        },
        platform: 'linux',
        existsSync: (p) => path.resolve(p) === expectedBinary,
      });

      assert.strictEqual(resolved, expectedBinary);
    });

    it('fails clearly in CI if binary is missing from checked-out commit', () => {
      const ciWorkspace = path.resolve('/home/runner/work/agent-studios/agent-studios');

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

  describe('Local Development Provenance Policy', () => {
    it('fails clearly with cargo build instructions when no binary is found', () => {
      assert.throws(
        () => {
          resolveAgsdBinaryPath({
            env: {},
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
});
