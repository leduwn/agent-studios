/**
 * Agent Studios — Daemon Discovery Unit Tests
 * Milestone M14-IPC-03: Real ConnectedClient Vertical Slice
 */

import * as assert from 'assert';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import { DaemonDiscovery } from '../../bridge/ipc/DaemonDiscovery';

describe('DaemonDiscovery Unit Tests', () => {
  let tempDir: string;

  beforeEach(async () => {
    tempDir = await fs.promises.mkdtemp(path.join(os.tmpdir(), 'ags-discovery-test-'));
  });

  afterEach(async () => {
    try {
      await fs.promises.rm(tempDir, { recursive: true, force: true });
    } catch {
      // Ignore cleanup error
    }
  });

  it('computes platform-default runtime path containing agsd.json', () => {
    const defaultPath = DaemonDiscovery.getDefaultRuntimePath();
    assert.ok(defaultPath.length > 0);
    assert.ok(defaultPath.includes('agsd.json'));
    assert.ok(defaultPath.includes('agent-studios'));
  });

  it('loads valid standard client bootstrap info', async () => {
    const filePath = path.join(tempDir, 'agsd.json');
    const validBootstrap = {
      endpoint: process.platform === 'win32' ? '\\\\.\\pipe\\test-pipe' : '/tmp/test.sock',
      key_id: 'ags-client',
      client_key: 'secret-key-12345',
      pid: 4567,
      created_at: 1761820800,
    };
    await fs.promises.writeFile(filePath, JSON.stringify(validBootstrap, null, 2), 'utf8');

    const loaded = await DaemonDiscovery.loadBootstrapInfo(filePath);
    assert.strictEqual(loaded.endpoint, validBootstrap.endpoint);
    assert.strictEqual(loaded.key_id, 'ags-client');
    assert.strictEqual(loaded.client_key, 'secret-key-12345');
    assert.strictEqual(loaded.admin_key, null);
    assert.strictEqual(loaded.pid, 4567);
  });

  it('suppresses and drops admin_key if present in bootstrap file', async () => {
    const filePath = path.join(tempDir, 'agsd.json');
    const bootstrapWithAdmin = {
      endpoint: 'pipe-or-sock',
      key_id: 'ags-client',
      client_key: 'client-key-abc',
      admin_key: 'admin-super-secret-xyz',
      pid: 9999,
      created_at: 1761820800,
    };
    await fs.promises.writeFile(filePath, JSON.stringify(bootstrapWithAdmin), 'utf8');

    const loaded = await DaemonDiscovery.loadBootstrapInfo(filePath);
    assert.strictEqual(loaded.client_key, 'client-key-abc');
    assert.strictEqual(loaded.admin_key, null);
  });

  it('rejects missing bootstrap file with helpful error', async () => {
    const nonExistentPath = path.join(tempDir, 'missing', 'agsd.json');
    await assert.rejects(async () => {
      await DaemonDiscovery.loadBootstrapInfo(nonExistentPath);
    }, (err: Error) => {
      return err.message.includes('bootstrap file not found') && err.message.includes('Daemon may not be running');
    });
  });

  it('rejects malformed JSON content in bootstrap file', async () => {
    const filePath = path.join(tempDir, 'agsd.json');
    await fs.promises.writeFile(filePath, '{ broken json ...', 'utf8');

    await assert.rejects(async () => {
      await DaemonDiscovery.loadBootstrapInfo(filePath);
    }, (err: Error) => {
      return err.message.includes('contains invalid JSON');
    });
  });

  it('rejects symlink bootstrap file to prevent hijacking attacks', async () => {
    const targetFile = path.join(tempDir, 'real-file.json');
    const symlinkPath = path.join(tempDir, 'symlink-agsd.json');

    await fs.promises.writeFile(targetFile, JSON.stringify({
      endpoint: 'test',
      key_id: 'k',
      client_key: 'c',
    }), 'utf8');

    try {
      await fs.promises.symlink(targetFile, symlinkPath);
    } catch {
      // If symlink creation requires admin privileges on Windows, skip test
      return;
    }

    await assert.rejects(async () => {
      await DaemonDiscovery.loadBootstrapInfo(symlinkPath);
    }, (err: Error) => {
      return err.message.includes('Security Violation') && err.message.includes('is a symlink');
    });
  });

  it('redacts secret credentials for safe logging and diagnostics', () => {
    const info = {
      endpoint: 'test-endpoint',
      key_id: 'test-key-id',
      client_key: 'super-sensitive-secret',
      admin_key: null,
      pid: 1234,
      created_at: 1761820800,
    };

    const redacted = DaemonDiscovery.redact(info);
    assert.strictEqual(redacted.client_key, '[REDACTED]');
    assert.strictEqual(redacted.admin_key, undefined);
    assert.strictEqual(redacted.key_id, 'test-key-id');
    assert.strictEqual(redacted.endpoint, 'test-endpoint');
  });
});
