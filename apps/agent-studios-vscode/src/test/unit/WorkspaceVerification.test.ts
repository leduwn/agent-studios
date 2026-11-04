/**
 * Agent Studios — WorkspaceVerification Unit Tests
 * Milestone M15 UI-01.2: Real Rust Execution Integration & Final Safety Gates
 */

import * as assert from 'assert';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { WorkspaceVerification } from '../../security/WorkspaceVerification';
import { RustExecutionReadinessReport } from '../../bridge/ipc/RustIpcProtocolTypes';

describe('M15 WorkspaceVerification Unit Tests', () => {
  let tmpBaseDir: string;
  let repoDir: string;
  let subDir: string;
  let unrelatedDir: string;

  before(() => {
    tmpBaseDir = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'ag-ws-verify-')));
    repoDir = path.join(tmpBaseDir, 'test-repo');
    subDir = path.join(repoDir, 'packages', 'client');
    unrelatedDir = path.join(tmpBaseDir, 'unrelated-project');

    fs.mkdirSync(subDir, { recursive: true });
    fs.mkdirSync(unrelatedDir, { recursive: true });

    // Create .git directory in repoDir
    fs.mkdirSync(path.join(repoDir, '.git'), { recursive: true });
  });

  after(() => {
    try {
      fs.rmSync(tmpBaseDir, { recursive: true, force: true });
    } catch {
      // Ignore cleanup error
    }
  });

  const makeReadiness = (
    authorizedWorkspaces: string[],
    defaultRepoPath?: string | null
  ): RustExecutionReadinessReport => ({
    is_ready: true,
    available_runtimes: ['opencode'],
    runtime_instances: [],
    available_models: [],
    authorized_workspaces: authorizedWorkspaces,
    defaults: {
      default_repo_path: defaultRepoPath ?? null,
      default_provider_instance_id: null,
      default_model_id: null,
      configured_instance_id: null,
    },
    blocking_reasons: [],
  });

  describe('1. UNAVAILABLE State', () => {
    it('returns UNAVAILABLE when readiness report is null or undefined', () => {
      const resNull = WorkspaceVerification.verify(repoDir, null);
      assert.strictEqual(resNull.status, 'UNAVAILABLE');
      assert.strictEqual(resNull.isAuthorized, false);
      assert.ok(resNull.reason.includes('unavailable from daemon'));

      const resUndef = WorkspaceVerification.verify(repoDir, undefined);
      assert.strictEqual(resUndef.status, 'UNAVAILABLE');
      assert.strictEqual(resUndef.isAuthorized, false);
    });

    it('returns UNAVAILABLE when local workspace path is empty or whitespace', () => {
      const readiness = makeReadiness([repoDir], repoDir);
      const resEmpty = WorkspaceVerification.verify('', readiness);
      assert.strictEqual(resEmpty.status, 'UNAVAILABLE');
      assert.strictEqual(resEmpty.isAuthorized, false);
      assert.ok(resEmpty.reason.includes('No active workspace folder'));

      const resSpace = WorkspaceVerification.verify('   ', readiness);
      assert.strictEqual(resSpace.status, 'UNAVAILABLE');
      assert.strictEqual(resSpace.isAuthorized, false);

      const resNullPath = WorkspaceVerification.verify(null, readiness);
      assert.strictEqual(resNullPath.status, 'UNAVAILABLE');
      assert.strictEqual(resNullPath.isAuthorized, false);
    });
  });

  describe('2. UNVERIFIED State', () => {
    it('returns UNVERIFIED when daemon readiness declares no authorized roots and no default repo', () => {
      const readiness = makeReadiness([], null);
      const res = WorkspaceVerification.verify(repoDir, readiness);

      assert.strictEqual(res.status, 'UNVERIFIED');
      assert.strictEqual(res.isAuthorized, false);
      assert.ok(res.reason.includes('no authorized workspace roots'));
    });
  });

  describe('3. AUTHORIZED State', () => {
    it('returns AUTHORIZED on exact match with default_repo_path', () => {
      const readiness = makeReadiness([], repoDir);
      const res = WorkspaceVerification.verify(repoDir, readiness);

      assert.strictEqual(res.status, 'AUTHORIZED');
      assert.strictEqual(res.isAuthorized, true);
      assert.strictEqual(res.matchedRoot, repoDir);
      assert.ok(res.reason.includes('Workspace matched daemon authorized root'));
    });

    it('returns AUTHORIZED on exact match with authorized_workspaces entry', () => {
      const readiness = makeReadiness([repoDir], null);
      const res = WorkspaceVerification.verify(repoDir, readiness);

      assert.strictEqual(res.status, 'AUTHORIZED');
      assert.strictEqual(res.isAuthorized, true);
      assert.strictEqual(res.matchedRoot, repoDir);
    });

    it('returns AUTHORIZED for hierarchically contained subdirectory when strictRoots is false', () => {
      const readiness = makeReadiness([repoDir], repoDir);
      const res = WorkspaceVerification.verify(subDir, readiness, { strictRoots: false });

      assert.strictEqual(res.status, 'AUTHORIZED');
      assert.strictEqual(res.isAuthorized, true);
      assert.strictEqual(res.matchedRoot, repoDir);
      assert.ok(res.reason.includes('hierarchically contained'));
    });
  });

  describe('4. MISMATCH State & Security Violations', () => {
    it('returns MISMATCH when local workspace is completely unrelated to daemon authorized roots', () => {
      const readiness = makeReadiness([repoDir], repoDir);
      const res = WorkspaceVerification.verify(unrelatedDir, readiness);

      assert.strictEqual(res.status, 'MISMATCH');
      assert.strictEqual(res.isAuthorized, false);
      assert.ok(res.reason.includes('does not match daemon authorized repository roots'));
    });

    it('returns MISMATCH when subdirectory is opened but strictRoots is true', () => {
      const readiness = makeReadiness([repoDir], repoDir);
      const res = WorkspaceVerification.verify(subDir, readiness, { strictRoots: true });

      assert.strictEqual(res.status, 'MISMATCH');
      assert.strictEqual(res.isAuthorized, false);
    });

    it('fails closed with MISMATCH on null byte injection', () => {
      const readiness = makeReadiness([repoDir], repoDir);
      const res = WorkspaceVerification.verify(repoDir + '\0evil', readiness);

      assert.strictEqual(res.status, 'MISMATCH');
      assert.strictEqual(res.isAuthorized, false);
      assert.ok(res.reason.includes('Null byte detected'));
    });

    it('fails closed with MISMATCH on UNC network path injection', () => {
      const readiness = makeReadiness([repoDir], repoDir);
      const resWinUnc = WorkspaceVerification.verify('\\\\192.168.1.100\\share\\repo', readiness);
      assert.strictEqual(resWinUnc.status, 'MISMATCH');
      assert.strictEqual(resWinUnc.isAuthorized, false);
      assert.ok(resWinUnc.reason.includes('UNC network paths are rejected'));

      const resUnixUnc = WorkspaceVerification.verify('//server/share/repo', readiness);
      assert.strictEqual(resUnixUnc.status, 'MISMATCH');
      assert.strictEqual(resUnixUnc.isAuthorized, false);
    });

    it('fails closed with MISMATCH on path traversal (..)', () => {
      const readiness = makeReadiness([repoDir], repoDir);
      const traversalPath = repoDir + '/../other-project';
      const res = WorkspaceVerification.verify(traversalPath, readiness);

      assert.strictEqual(res.status, 'MISMATCH');
      assert.strictEqual(res.isAuthorized, false);
      assert.ok(res.reason.includes('Path traversal (..) detected'));
    });
  });

  describe('5. Git Metadata Detection (Directory vs Gitlink)', () => {
    it('detects standard .git directory as git metadata', () => {
      assert.strictEqual(WorkspaceVerification.hasGitMetadata(repoDir), true);
    });

    it('detects .git gitlink file (submodule / worktree) as git metadata', () => {
      const gitlinkDir = path.join(tmpBaseDir, 'gitlink-worktree');
      fs.mkdirSync(gitlinkDir, { recursive: true });
      fs.writeFileSync(path.join(gitlinkDir, '.git'), 'gitdir: ../test-repo/.git/worktrees/wt1\n', 'utf8');

      assert.strictEqual(WorkspaceVerification.hasGitMetadata(gitlinkDir), true);
    });

    it('returns false for directories without .git folder or file', () => {
      assert.strictEqual(WorkspaceVerification.hasGitMetadata(unrelatedDir), false);
      assert.strictEqual(WorkspaceVerification.hasGitMetadata(subDir), false);
    });
  });
});
