/**
 * Agent Studios — Security Guards & Runtime Validation Unit Tests
 * Milestone M14-UI-02.1: Extension Hardening & Security Assurance
 */

import * as assert from 'assert';
import {
  ALLOWED_WEBVIEW_COMMANDS,
  ChatSecurityGuards,
  SECURITY_LIMITS,
} from '../../chat/ChatSecurityGuards';
import { resetMockState } from './vscodeMock';

describe('ChatSecurityGuards Unit Tests', () => {
  beforeEach(() => {
    resetMockState();
  });

  describe('Command Allowlist Validation', () => {
    it('approves all legitimate webview commands in allowlist', () => {
      for (const cmd of ALLOWED_WEBVIEW_COMMANDS) {
        const payload: Record<string, unknown> = { command: cmd };
        if (cmd === 'approveAction' || cmd === 'denyAction') {
          payload.approvalId = 'app-valid-1';
        }
        if (cmd === 'submitTask') {
          payload.prompt = 'Execute unit test task';
        }
        const result = ChatSecurityGuards.validateWebviewMessage(payload);
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

  describe('Prototype Pollution Protection (Fail-Closed & Array Inspection)', () => {
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

    it('fails closed when nesting depth exceeds MAX_OBJECT_DEPTH (depth 6+)', () => {
      const payload = {
        command: 'sendMessage',
        a: { b: { c: { d: { e: { f: { safeProp: 123 } } } } } },
      };
      const result = ChatSecurityGuards.validateWebviewMessage(payload);
      assert.strictEqual(result.valid, false);
      assert.ok(result.error?.includes('Maximum object nesting depth'));
    });

    it('detects prototype pollution nested inside arrays', () => {
      const payload = {
        command: 'sendMessage',
        attachments: [
          JSON.parse('{"__proto__": {"admin": true}}'),
        ],
      };
      const result = ChatSecurityGuards.validateWebviewMessage(payload);
      assert.strictEqual(result.valid, false);
      assert.ok(result.error?.includes('prototype pollution'));
    });

    it('detects constructor pollution in multidimensional arrays', () => {
      const payload = {
        command: 'sendMessage',
        metadata: [[JSON.parse('{"constructor": {"prototype": {"polluted": true}}}')]],
      };
      const result = ChatSecurityGuards.validateWebviewMessage(payload);
      assert.strictEqual(result.valid, false);
      assert.ok(result.error?.includes('prototype pollution'));
    });
  });

  describe('Command Schemas & Size Bounds', () => {
    it('rejects oversized prompt strings in sendMessage', () => {
      const hugeText = 'A'.repeat(SECURITY_LIMITS.MAX_TEXT_LENGTH + 1);
      const result = ChatSecurityGuards.validateWebviewMessage({
        command: 'sendMessage',
        text: hugeText,
      });
      assert.strictEqual(result.valid, false);
      assert.ok(result.error?.includes('exceeds limit'));
    });

    it('rejects oversized draft strings in saveDraft', () => {
      const hugeDraft = 'B'.repeat(SECURITY_LIMITS.MAX_TEXT_LENGTH + 1);
      const result = ChatSecurityGuards.validateWebviewMessage({
        command: 'saveDraft',
        draft: hugeDraft,
      });
      assert.strictEqual(result.valid, false);
      assert.ok(result.error?.includes('exceeds maximum length'));
    });

    it('rejects invalid mode enums in switchMode and sendMessage', () => {
      const r1 = ChatSecurityGuards.validateWebviewMessage({
        command: 'switchMode',
        mode: 'superAdmin',
      });
      assert.strictEqual(r1.valid, false);
      assert.ok(r1.error?.includes('Invalid mode'));

      const r2 = ChatSecurityGuards.validateWebviewMessage({
        command: 'sendMessage',
        mode: 'malicious',
      });
      assert.strictEqual(r2.valid, false);
      assert.ok(r2.error?.includes('Invalid mode'));
    });

    it('rejects excessive attachments count (limit 10)', () => {
      const result = ChatSecurityGuards.validateWebviewMessage({
        command: 'sendMessage',
        attachments: new Array(11).fill({ type: 'file', path: 'src/index.ts' }),
      });
      assert.strictEqual(result.valid, false);
      assert.ok(result.error?.includes('Attachments count exceeds maximum limit'));
    });

    it('rejects empty or oversized approvalId in approveAction/denyAction', () => {
      assert.strictEqual(
        ChatSecurityGuards.validateWebviewMessage({ command: 'approveAction', approvalId: '' }).valid,
        false
      );
      assert.strictEqual(
        ChatSecurityGuards.validateWebviewMessage({ command: 'approveAction', approvalId: '   ' }).valid,
        false
      );
      assert.strictEqual(
        ChatSecurityGuards.validateWebviewMessage({
          command: 'approveAction',
          approvalId: 'x'.repeat(SECURITY_LIMITS.MAX_ID_LENGTH + 1),
        }).valid,
        false
      );
      assert.strictEqual(
        ChatSecurityGuards.validateWebviewMessage({ command: 'denyAction', approvalId: '' }).valid,
        false
      );
    });

    it('rejects approvalId containing illegal injection characters', () => {
      const result = ChatSecurityGuards.validateWebviewMessage({
        command: 'approveAction',
        approvalId: 'app-id; DROP TABLE approvals;',
      });
      assert.strictEqual(result.valid, false);
      assert.ok(result.error?.includes('invalid characters'));
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

    it('blocks Windows UNC paths', () => {
      assert.strictEqual(ChatSecurityGuards.isPathTraversal('\\\\attacker\\share\\file.ts'), true);
      assert.strictEqual(ChatSecurityGuards.isPathTraversal('//attacker/share/file.ts'), true);
    });

    it('blocks URL-encoded traversal sequences', () => {
      assert.strictEqual(ChatSecurityGuards.isPathTraversal('%2e%2e%2fetc%2fpasswd'), true);
      assert.strictEqual(ChatSecurityGuards.isPathTraversal('src%2f%2e%2e%2fsecret.ts'), true);
    });

    it('blocks dangerous URI protocols while permitting agent-studios-diff://', () => {
      assert.strictEqual(ChatSecurityGuards.isPathTraversal('javascript:alert(1)'), true);
      assert.strictEqual(ChatSecurityGuards.isPathTraversal('data:text/html,<script>'), true);
      assert.strictEqual(ChatSecurityGuards.isPathTraversal('file:///etc/shadow'), true);
      assert.strictEqual(ChatSecurityGuards.isPathTraversal('agent-studios-diff://artifact/123/diff'), false);
    });

    it('distinguishes Windows drive letter prefixes from URI schemes', () => {
      // Windows absolute drive letter C: must not be treated as a disallowed URI scheme
      assert.strictEqual(ChatSecurityGuards.isPathTraversal('C:\\projects\\agent-studios\\file.ts'), false);
      assert.strictEqual(ChatSecurityGuards.isPathTraversal('d:/repo/src/index.ts'), false);
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
