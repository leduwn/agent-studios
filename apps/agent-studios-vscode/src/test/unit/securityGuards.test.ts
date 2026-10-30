/**
 * Agent Studios — Security Guards & Runtime Validation Unit Tests
 * Milestone M14-UI-02: Extension Hardening & Security Assurance
 */

import * as assert from 'assert';
import {
  ALLOWED_WEBVIEW_COMMANDS,
  ChatSecurityGuards,
} from '../../chat/ChatSecurityGuards';
import { resetMockState } from './vscodeMock';

describe('ChatSecurityGuards Unit Tests', () => {
  beforeEach(() => {
    resetMockState();
  });

  describe('Command Allowlist Validation', () => {
    it('approves all legitimate webview commands in allowlist', () => {
      for (const cmd of ALLOWED_WEBVIEW_COMMANDS) {
        const result = ChatSecurityGuards.validateWebviewMessage({ command: cmd });
        assert.strictEqual(result.valid, true, `Command '${cmd}' should be allowed`);
      }
    });

    it('rejects unrecognized or injected commands', () => {
      const malicious = ['eval', 'executeShell', 'readSecret', 'dropDatabase', 'system.exec'];
      for (const cmd of malicious) {
        const result = ChatSecurityGuards.validateWebviewMessage({ command: cmd });
        assert.strictEqual(result.valid, false, `Command '${cmd}' should be rejected`);
        assert.ok(result.error?.includes('Disallowed or unknown webview command'));
      }
    });

    it('rejects malformed message payloads', () => {
      assert.strictEqual(ChatSecurityGuards.validateWebviewMessage(null).valid, false);
      assert.strictEqual(ChatSecurityGuards.validateWebviewMessage(undefined).valid, false);
      assert.strictEqual(ChatSecurityGuards.validateWebviewMessage('string').valid, false);
      assert.strictEqual(ChatSecurityGuards.validateWebviewMessage([1, 2, 3]).valid, false);
      assert.strictEqual(ChatSecurityGuards.validateWebviewMessage({}).valid, false);
    });
  });

  describe('Prototype Pollution Protection', () => {
    it('detects top-level __proto__ property injection', () => {
      const payload = JSON.parse('{"command": "sendMessage", "__proto__": {"polluted": true}}');
      const result = ChatSecurityGuards.validateWebviewMessage(payload);
      assert.strictEqual(result.valid, false);
      assert.ok(result.error?.includes('prototype pollution'));
    });

    it('detects constructor or prototype property injection', () => {
      const payload1 = JSON.parse('{"command": "sendMessage", "constructor": {}}');
      const payload2 = JSON.parse('{"command": "sendMessage", "prototype": {}}');
      assert.strictEqual(ChatSecurityGuards.validateWebviewMessage(payload1).valid, false);
      assert.strictEqual(ChatSecurityGuards.validateWebviewMessage(payload2).valid, false);
    });

    it('detects nested prototype pollution attacks', () => {
      const payload = JSON.parse('{"command": "sendMessage", "metadata": {"nested": {"__proto__": {}}}}');
      const result = ChatSecurityGuards.validateWebviewMessage(payload);
      assert.strictEqual(result.valid, false);
      assert.ok(result.error?.includes('prototype pollution'));
    });
  });

  describe('Path Traversal & Protocol Breakout Protection', () => {
    it('blocks relative path traversal sequences (..)', () => {
      const testCases = [
        '../../etc/passwd',
        '..\\..\\windows\\system32',
        'src/../../sensitive.key',
        'foo/bar/../..',
      ];

      for (const p of testCases) {
        const msg = { command: 'openDiff', filePath: p };
        const result = ChatSecurityGuards.validateWebviewMessage(msg);
        assert.strictEqual(result.valid, false, `Path '${p}' should be blocked as traversal`);
        assert.ok(result.error?.includes('path traversal'));
      }
    });

    it('blocks null-byte injection in paths', () => {
      const nullPaths = ['safe/path.ts\0.exe', 'target/diff%00.sh'];
      for (const p of nullPaths) {
        assert.strictEqual(ChatSecurityGuards.isPathTraversal(p), true);
        const result = ChatSecurityGuards.validateWebviewMessage({ command: 'openDiff', path: p });
        assert.strictEqual(result.valid, false);
      }
    });

    it('blocks dangerous URI protocols while permitting agent-studios-diff://', () => {
      assert.strictEqual(ChatSecurityGuards.isPathTraversal('javascript:alert(1)'), true);
      assert.strictEqual(ChatSecurityGuards.isPathTraversal('data:text/html,<script>'), true);
      assert.strictEqual(ChatSecurityGuards.isPathTraversal('file:///etc/shadow'), true);
      assert.strictEqual(ChatSecurityGuards.isPathTraversal('agent-studios-diff://artifact/123/diff'), false);
    });

    it('allows clean workspace-relative paths', () => {
      const safePaths = [
        'src/index.ts',
        'package.json',
        'apps/vscode/README.md',
        'components/Button.tsx',
      ];
      for (const p of safePaths) {
        const msg = { command: 'openDiff', filePath: p };
        const result = ChatSecurityGuards.validateWebviewMessage(msg);
        assert.strictEqual(result.valid, true, `Safe path '${p}' should be accepted`);
      }
    });
  });

  describe('Workspace Trust Enforcement', () => {
    it('allows non-mutating commands in untrusted workspace', () => {
      const nonMutating = ['switchMode', 'switchProviderModel', 'openTaskGraph', 'saveDraft', 'dismissOnboarding'];
      for (const cmd of nonMutating) {
        assert.strictEqual(ChatSecurityGuards.verifyWorkspaceTrust(cmd), true);
      }
    });
  });
});
