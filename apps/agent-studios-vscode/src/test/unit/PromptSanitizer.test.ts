/**
 * Agent Studios — PromptSanitizer & Task Contract Unit Tests
 * Milestone M14 Integration Freeze & M15 UI Execution Preparation
 */

import * as assert from 'assert';
import {
  PromptSanitizer,
  PromptSanitizationError,
  MAX_PROMPT_BYTES,
  TASK_KEY_REGEX,
} from '../../bridge/ipc/PromptSanitizer';
import {
  CapabilityMatrix,
  OperatorConfirmationManager,
} from '../../bridge/ipc/CapabilityMatrix';

describe('PromptSanitizer Unit Tests', () => {
  describe('Prompt Sanitization & Preservation', () => {
    it('preserves multiline Markdown prompts with blank lines verbatim', () => {
      const markdownPrompt = [
        '# Refactor Authentication Middleware',
        '',
        'Please inspect `auth.rs` and make sure token validation handles expiration correctly:',
        '',
        '```rust',
        'pub fn validate_token(token: &str) -> Result<Claims, AuthError> {',
        '    let claims = parse(token)?;',
        '',
        '    if claims.is_expired() {',
        '        return Err(AuthError::Expired);',
        '    }',
        '    Ok(claims)',
        '}',
        '```',
        '',
        'Ensure unit tests pass.',
      ].join('\n');

      const sanitized = PromptSanitizer.sanitizePrompt(markdownPrompt);
      assert.strictEqual(sanitized, markdownPrompt);

      // Verify that blank lines were NOT stripped
      const originalBlankCount = (markdownPrompt.match(/\n\n/g) || []).length;
      const sanitizedBlankCount = (sanitized.match(/\n\n/g) || []).length;
      assert.strictEqual(sanitizedBlankCount, originalBlankCount);
    });

    it('normalizes CRLF to LF without stripping blank lines or altering code content', () => {
      const crlfPrompt = 'Line 1\r\n\r\nLine 2\r\n    code_indent();\r\n\r\nLine 3';
      const expected = 'Line 1\n\nLine 2\n    code_indent();\n\nLine 3';

      const sanitized = PromptSanitizer.sanitizePrompt(crlfPrompt);
      assert.strictEqual(sanitized, expected);
      assert.ok(!sanitized.includes('\r'));
    });

    it('preserves Unicode, accents, Asian characters, and emojis faithfully', () => {
      const unicodePrompt = [
        '🤖 Agent Task: Quản lý phiên làm việc & kiểm tra bảo mật',
        '• Sửa lỗi xác thực tại `auth.rs`',
        '• 日本語のテストケースを追加してください',
        '• Café résumé naïve façade',
        '🚀 Status: Ready for verification.',
      ].join('\n');

      const sanitized = PromptSanitizer.sanitizePrompt(unicodePrompt);
      assert.strictEqual(sanitized, unicodePrompt);
    });

    it('rejects empty string, whitespace-only, and non-string inputs', () => {
      assert.throws(
        () => PromptSanitizer.sanitizePrompt(''),
        (err: unknown) => {
          assert.ok(err instanceof PromptSanitizationError);
          assert.strictEqual(err.field, 'prompt');
          return true;
        }
      );

      assert.throws(
        () => PromptSanitizer.sanitizePrompt('   \n\t  \n  '),
        (err: unknown) => {
          assert.ok(err instanceof PromptSanitizationError);
          assert.strictEqual(err.field, 'prompt');
          return true;
        }
      );

      assert.throws(
        () => PromptSanitizer.sanitizePrompt(null),
        (err: unknown) => {
          assert.ok(err instanceof PromptSanitizationError);
          return true;
        }
      );

      assert.throws(
        () => PromptSanitizer.sanitizePrompt(12345),
        (err: unknown) => {
          assert.ok(err instanceof PromptSanitizationError);
          return true;
        }
      );
    });

    it('enforces maximum prompt byte length against 4 MiB frame limit', () => {
      // Small prompt passes
      const small = 'Small test prompt';
      assert.strictEqual(PromptSanitizer.sanitizePrompt(small), small);

      // Prompt exceeding MAX_PROMPT_BYTES (4 MiB - 64 KiB = 4,128,768 bytes) throws
      const oversized = Buffer.alloc(MAX_PROMPT_BYTES + 10, 'a').toString('utf8');
      assert.throws(
        () => PromptSanitizer.sanitizePrompt(oversized),
        (err: unknown) => {
          assert.ok(err instanceof PromptSanitizationError);
          assert.strictEqual(err.field, 'prompt');
          assert.ok(err.message.includes('exceeds maximum allowed size'));
          return true;
        }
      );
    });
  });

  describe('Studio ID Validation (RFC 4122 UUID)', () => {
    it('accepts valid RFC 4122 v4 UUIDs and nil UUID', () => {
      const validV4 = 'c3d4e5f6-a7b8-4c9d-8e1f-2a3b4c5d6e7f';
      const nilUuid = '00000000-0000-0000-0000-000000000000';

      assert.strictEqual(PromptSanitizer.validateStudioId(validV4), validV4);
      assert.strictEqual(PromptSanitizer.validateStudioId(nilUuid), nilUuid);
      assert.strictEqual(PromptSanitizer.validateStudioId(`  ${validV4}  `), validV4);
    });

    it('rejects invalid or non-UUID studio IDs that would fail Rust serde deserialization', () => {
      const invalidIds = [
        'studio-1',
        'default',
        '12345',
        'not-a-uuid-format',
        'c3d4e5f6-a7b8-4c9d-8e1f', // truncated
        '',
        '   ',
      ];

      for (const id of invalidIds) {
        assert.throws(
          () => PromptSanitizer.validateStudioId(id),
          (err: unknown) => {
            assert.ok(err instanceof PromptSanitizationError);
            assert.strictEqual(err.field, 'studioId');
            return true;
          },
          `Expected studio ID '${id}' to be rejected`
        );
      }
    });
  });

  describe('Task Key Validation & Generation', () => {
    it('accepts valid task keys', () => {
      const validKeys = [
        'task-1',
        'subtask_01',
        'core.pipeline-check',
        'TASK-ALPHA-99',
      ];
      for (const key of validKeys) {
        assert.strictEqual(PromptSanitizer.validateTaskKey(key), key);
      }
    });

    it('rejects empty, whitespace, control characters, and path separators in task keys', () => {
      const invalidKeys = [
        '',
        '   ',
        'task/1',
        'task\\1',
        'task key with spaces',
        'task@#$%',
      ];
      for (const key of invalidKeys) {
        assert.throws(
          () => PromptSanitizer.validateTaskKey(key),
          (err: unknown) => {
            assert.ok(err instanceof PromptSanitizationError);
            assert.strictEqual(err.field, 'key');
            return true;
          }
        );
      }
    });

    it('generates collision-resistant task keys matching task key pattern', () => {
      const key1 = PromptSanitizer.generateTaskKey();
      const key2 = PromptSanitizer.generateTaskKey();
      assert.notStrictEqual(key1, key2);
      assert.ok(TASK_KEY_REGEX.test(key1));
      assert.ok(TASK_KEY_REGEX.test(key2));
      assert.ok(key1.startsWith('task-'));
    });
  });

  describe('Display Title Extraction', () => {
    it('extracts clean title from markdown header', () => {
      const prompt = '# Implement OAuth2 Flow\n\nDetails here...';
      assert.strictEqual(PromptSanitizer.extractDisplayTitle(prompt), 'Implement OAuth2 Flow');
    });

    it('extracts clean title from list item or bullet', () => {
      const prompt = '- Fix memory leak in buffer pool\nMore details...';
      assert.strictEqual(PromptSanitizer.extractDisplayTitle(prompt), 'Fix memory leak in buffer pool');
    });

    it('truncates long first lines cleanly at maxLength with ellipsis', () => {
      const longLine = 'This is a very long task description that should be truncated cleanly for display in the sidebar tree view and kanban board';
      const title = PromptSanitizer.extractDisplayTitle(longLine, 50);
      assert.ok(title.length <= 50);
      assert.ok(title.endsWith('…'));
    });

    it('handles code fence on first line gracefully', () => {
      const prompt = '```typescript\nconst x = 1;\n```';
      const title = PromptSanitizer.extractDisplayTitle(prompt);
      assert.strictEqual(title, 'Code snippet (typescript)');
    });
  });

  describe('Rust agsd Task Contract Wire Compatibility (connection.rs:1758-1769)', () => {
    it('guarantees 100% prompt fidelity without ": " injection or duplication', () => {
      const multilinePrompt = [
        '# Heading 1',
        '',
        'Paragraph 1 with blank line before it.',
        '',
        '```rust',
        'fn main() {',
        '    println!("hello");',
        '}',
        '```',
      ].join('\n');

      const spec = PromptSanitizer.prepareAuthoritativeTaskSpec({
        prompt: multilinePrompt,
      });

      // Contract invariant: description MUST be empty string
      assert.strictEqual(spec.description, '');
      assert.strictEqual(spec.title, multilinePrompt);

      // Simulate Rust backend resolution:
      // if t.description.is_empty() { t.title.clone() } else { format!("{}: {}", t.title, t.description) }
      const resolvedPrompt = PromptSanitizer.simulateRustPromptResolution({
        title: spec.title,
        description: spec.description,
      });

      assert.strictEqual(resolvedPrompt, multilinePrompt);
      assert.ok(!resolvedPrompt.includes('# Heading 1: '));
    });

    it('prevents single-line prompt duplication in Rust backend', () => {
      const singleLine = 'Fix issue with login redirect';
      const spec = PromptSanitizer.prepareAuthoritativeTaskSpec({
        prompt: singleLine,
      });

      assert.strictEqual(spec.title, singleLine);
      assert.strictEqual(spec.description, '');

      const resolved = PromptSanitizer.simulateRustPromptResolution({
        title: spec.title,
        description: spec.description,
      });

      // Must be exact single line, NOT "Fix issue with login redirect: Fix issue with login redirect"
      assert.strictEqual(resolved, singleLine);
    });
  });

  describe('CapabilityMatrix Preflight Integration', () => {
    const validStudioId = 'a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d';

    it('approves valid task preflight with lossless sanitized spec', () => {
      const prompt = 'Line 1\n\nLine 2';
      const result = CapabilityMatrix.evaluateTaskSubmissionPreflight({
        studioId: validStudioId,
        prompt,
        confirmedByUser: true,
      });

      assert.strictEqual(result.allowed, true);
      assert.strictEqual(result.sanitizedSpec?.title, prompt);
      assert.strictEqual(result.sanitizedSpec?.description, '');
      assert.strictEqual(result.sanitizedSpec?.displayTitle, 'Line 1');
    });

    it('rejects invalid non-UUID studioId during preflight', () => {
      const result = CapabilityMatrix.evaluateTaskSubmissionPreflight({
        studioId: 'invalid-studio-id',
        prompt: 'Valid prompt',
        confirmedByUser: true,
      });

      assert.strictEqual(result.allowed, false);
      assert.ok(result.error?.includes('must be a canonical RFC 4122 UUID'));
    });

    it('rejects empty prompt during preflight', () => {
      const result = CapabilityMatrix.evaluateTaskSubmissionPreflight({
        studioId: validStudioId,
        prompt: '   ',
        confirmedByUser: true,
      });

      assert.strictEqual(result.allowed, false);
      assert.ok(result.error?.includes('cannot be empty'));
    });

    it('rejects unconfirmed user requests during preflight', () => {
      const result = CapabilityMatrix.evaluateTaskSubmissionPreflight({
        studioId: validStudioId,
        prompt: 'Valid prompt',
        confirmedByUser: false,
      });

      assert.strictEqual(result.allowed, false);
      assert.ok(result.error?.includes('Operator confirmation is required'));
    });

    it('verifies valid OperatorConfirmationChallenge when challenge is required', () => {
      const prompt = 'Test challenge prompt';
      const challenge = OperatorConfirmationManager.createChallenge(validStudioId, prompt);

      const result = CapabilityMatrix.evaluateTaskSubmissionPreflight({
        studioId: validStudioId,
        prompt,
        confirmedByUser: true,
        confirmationChallenge: challenge,
        requireChallenge: true,
      });

      assert.strictEqual(result.allowed, true);
      assert.strictEqual(result.sanitizedSpec?.title, prompt);
    });

    it('rejects tampered or forged challenge token during preflight', () => {
      const prompt = 'Test challenge prompt';
      const challenge = OperatorConfirmationManager.createChallenge(validStudioId, prompt);
      const forgedChallenge = { ...challenge, token: 'forged-token-12345' };

      const result = CapabilityMatrix.evaluateTaskSubmissionPreflight({
        studioId: validStudioId,
        prompt,
        confirmedByUser: true,
        confirmationChallenge: forgedChallenge,
        requireChallenge: true,
      });

      assert.strictEqual(result.allowed, false);
      assert.ok(result.error?.includes('challenge verification failed'));
    });
  });
});
