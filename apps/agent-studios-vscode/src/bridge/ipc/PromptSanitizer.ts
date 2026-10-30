/**
 * Agent Studios — Prompt Sanitizer & Task Contract Preservation
 * Milestone M14 Integration Freeze & M15 UI Execution Preparation
 *
 * Enforces prompt integrity, character encoding fidelity, and bounds against the authoritative
 * Rust daemon (agsd) task execution lifecycle in agent-studios-rs/crates/ipc-transport/src/connection.rs:
 *
 * Rust agsd Task Contract (connection.rs:1758-1769):
 * ```rust
 * let prompt = {
 *     let cp = server_state.control_plane.read().await;
 *     cp.get_task(task_id)
 *         .map(|t| {
 *             if t.description.is_empty() {
 *                 t.title.clone()
 *             } else {
 *                 format!("{}: {}", t.title, t.description)
 *             }
 *         })
 *         .unwrap_or_default()
 * };
 * ```
 *
 * Key Invariants:
 * 1. Single-Payload Prompt Fidelity: When t.description is empty (""), the daemon uses t.title
 *    verbatim. Placing the entire prompt in `title` and setting `description` to "" prevents
 *    lossy line stripping, delimiter injection (": "), and single-line prompt duplication.
 * 2. Whitespace & Markdown Preservation: Blank lines inside Markdown paragraphs and code blocks
 *    are strictly preserved (never stripped via filter). CRLF is normalized to LF.
 * 3. Unicode & Multi-byte Integrity: Supports arbitrary UTF-8 characters without corruption.
 * 4. Wire Framing & Buffer Overflow Protection: Maximum prompt size is bounded against the 4 MiB
 *    IPC frame limit (RUST_DEFAULT_MAX_FRAME_SIZE - 64 KiB envelope buffer).
 * 5. Studio ID Validation: Validates that studioId conforms to canonical UUID format before IPC transmission.
 * 6. Task Key Validation: Validates task keys to prevent injection, control characters, or empty keys.
 */

import { RUST_DEFAULT_MAX_FRAME_SIZE } from './RustIpcCodec';

/** Headroom allocated for JSON envelope, command tag, studio ID, and auth overhead */
export const ENVELOPE_BUFFER_HEADROOM_BYTES = 64 * 1024; // 64 KiB

/** Authoritative maximum prompt size in UTF-8 bytes (4 MiB - 64 KiB = 4,128,768 bytes) */
export const MAX_PROMPT_BYTES = RUST_DEFAULT_MAX_FRAME_SIZE - ENVELOPE_BUFFER_HEADROOM_BYTES;

/** Canonical UUID regex matching RFC 4122 v1-v5 and nil UUID */
export const UUID_REGEX =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-5][0-9a-f]{3}-[089ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

/** Task key validation regex: alphanumeric, hyphen, underscore, period (1-128 chars) */
export const TASK_KEY_REGEX = /^[a-zA-Z0-9_\-\.]{1,128}$/;

export class PromptSanitizationError extends Error {
  constructor(message: string, public readonly field?: string) {
    super(message);
    this.name = 'PromptSanitizationError';
  }
}

export interface SanitizedPromptSpec {
  key: string;
  title: string;
  description: string;
  displayTitle: string;
}

export class PromptSanitizer {
  /**
   * Validates and normalizes user prompt text.
   * Preserves all Markdown, code fences, blank lines, indentation, and Unicode.
   * Normalizes CRLF (\r\n) to LF (\n).
   * Enforces non-empty check and wire frame byte limit.
   */
  public static sanitizePrompt(prompt: unknown): string {
    if (typeof prompt !== 'string') {
      throw new PromptSanitizationError('Prompt must be a string.', 'prompt');
    }

    if (prompt.trim().length === 0) {
      throw new PromptSanitizationError('Prompt cannot be empty or whitespace only.', 'prompt');
    }

    // Normalize Windows CRLF to standard LF while keeping all whitespace and line breaks intact
    const normalized = prompt.replace(/\r\n/g, '\n');

    const byteLength = Buffer.byteLength(normalized, 'utf8');
    if (byteLength > MAX_PROMPT_BYTES) {
      throw new PromptSanitizationError(
        `Prompt size (${byteLength} bytes) exceeds maximum allowed size (${MAX_PROMPT_BYTES} bytes / ~3.94 MiB).`,
        'prompt'
      );
    }

    return normalized;
  }

  /**
   * Validates studio ID format (must be a valid UUID string).
   * Rust's StudioId serde deserializer rejects any non-UUID value.
   */
  public static validateStudioId(studioId: unknown): string {
    if (typeof studioId !== 'string' || studioId.trim().length === 0) {
      throw new PromptSanitizationError(
        'Studio ID is required and cannot be empty.',
        'studioId'
      );
    }

    const trimmed = studioId.trim();
    if (!UUID_REGEX.test(trimmed)) {
      throw new PromptSanitizationError(
        `Studio ID '${trimmed}' is invalid: must be a canonical RFC 4122 UUID.`,
        'studioId'
      );
    }

    return trimmed;
  }

  /**
   * Validates task key format.
   * Rust control plane rejects empty keys and duplicate keys in batches.
   */
  public static validateTaskKey(key: unknown): string {
    if (typeof key !== 'string' || key.trim().length === 0) {
      throw new PromptSanitizationError(
        'Task key cannot be empty.',
        'key'
      );
    }

    const trimmed = key.trim();
    if (!TASK_KEY_REGEX.test(trimmed)) {
      throw new PromptSanitizationError(
        `Task key '${trimmed}' contains invalid characters. Must be 1-128 characters matching [a-zA-Z0-9_-.].`,
        'key'
      );
    }

    return trimmed;
  }

  /**
   * Generates a collision-resistant task key.
   */
  public static generateTaskKey(prefix: string = 'task'): string {
    const timestamp = Date.now().toString(36);
    const randomHex = Math.random().toString(36).slice(2, 7);
    return `${prefix}-${timestamp}-${randomHex}`;
  }

  /**
   * Extracts a clean, single-line human-readable display title for UI components (trees, kanban, tabs).
   * Does NOT alter the underlying runtime prompt.
   */
  public static extractDisplayTitle(prompt: string, maxLength: number = 80): string {
    if (!prompt) {
      return 'Untitled Task';
    }

    // Find the first non-empty line
    const lines = prompt.split('\n');
    let firstLine = '';
    for (const line of lines) {
      const trimmed = line.trim();
      if (trimmed.length > 0) {
        firstLine = trimmed;
        break;
      }
    }

    if (!firstLine) {
      return 'Untitled Task';
    }

    // Strip leading Markdown header or bullet syntax (e.g. "# ", "## ", "- ", "* ", "> ")
    let clean = firstLine.replace(/^(?:#{1,6}\s+|[-*+]\s+|>\s+)/, '').trim();

    if (clean.startsWith('```')) {
      const lang = clean.slice(3).trim();
      clean = lang ? `Code snippet (${lang})` : 'Code snippet';
    }

    if (clean.length === 0) {
      clean = 'Untitled Task';
    }

    if (clean.length > maxLength) {
      return clean.slice(0, maxLength - 1).trimEnd() + '…';
    }

    return clean;
  }

  /**
   * Prepares a BatchTaskSpec conforming to the Rust agsd task execution contract.
   *
   * To prevent lossy mutation:
   * - `title` holds the verbatim sanitized prompt.
   * - `description` is set to "" (empty string).
   *
   * Rust agsd lifecycle evaluates `if t.description.is_empty() { t.title.clone() }`.
   * This guarantees that the runtime receives the exact prompt verbatim without ": " injection,
   * without duplicate text, and with all code blocks and whitespace intact.
   */
  public static prepareAuthoritativeTaskSpec(params: {
    prompt: string;
    customKey?: string;
  }): SanitizedPromptSpec {
    const sanitizedPrompt = this.sanitizePrompt(params.prompt);
    const key = params.customKey
      ? this.validateTaskKey(params.customKey)
      : this.generateTaskKey();
    const displayTitle = this.extractDisplayTitle(sanitizedPrompt);

    return {
      key,
      title: sanitizedPrompt,
      description: '',
      displayTitle,
    };
  }

  /**
   * Simulates the exact Rust agsd prompt resolution logic from connection.rs:1758-1769.
   * Useful for testing and verifying wire compatibility.
   */
  public static simulateRustPromptResolution(task: {
    title: string;
    description: string;
  }): string {
    if (task.description.length === 0) {
      return task.title;
    }
    return `${task.title}: ${task.description}`;
  }
}
