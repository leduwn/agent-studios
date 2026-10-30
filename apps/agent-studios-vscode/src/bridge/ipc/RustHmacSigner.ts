/**
 * Agent Studios — Rust HMAC-SHA256 Authenticator (Approach A)
 * Milestone M14-UI-02.1: Protocol Correctness & Wire Compatibility
 *
 * Implements authoritative HMAC-SHA256 canonical digest computation and header
 * verification matching agent-studios-rs/crates/ipc-protocol/src/auth.rs.
 */

import * as crypto from 'crypto';
import { RustHmacAuthHeader, RustProtocolVersion } from './RustIpcProtocolTypes';

export class RustHmacSigner {
  /**
   * Computes canonical digest string matching Rust:
   * `{protocol_version}:{key_id}:{session_id}:{nonce}:{timestamp_millis}:{capabilities_str}:{payload_hash}`
   */
  public static computeCanonicalDigest(
    protocolVersion: RustProtocolVersion,
    keyId: string,
    sessionId: string,
    nonce: string,
    timestampMillis: number,
    capabilities: string[] | undefined,
    payload: Buffer | string | null | undefined
  ): string {
    const versionStr = `${protocolVersion.major}.${protocolVersion.minor}.${protocolVersion.patch}`;

    // Lexicographically sort capabilities if provided
    let capabilitiesStr = '';
    if (capabilities && capabilities.length > 0) {
      const sorted = [...capabilities].sort();
      capabilitiesStr = sorted.join(',');
    }

    // SHA-256 lowercase hex of payload
    let payloadBuffer: Buffer;
    if (!payload) {
      payloadBuffer = Buffer.alloc(0);
    } else if (typeof payload === 'string') {
      payloadBuffer = Buffer.from(payload, 'utf8');
    } else {
      payloadBuffer = payload;
    }

    const payloadHash = crypto.createHash('sha256').update(payloadBuffer).digest('hex');

    return `${versionStr}:${keyId}:${sessionId}:${nonce}:${timestampMillis}:${capabilitiesStr}:${payloadHash}`;
  }

  /**
   * Signs an authentication binding to generate a signed RustHmacAuthHeader.
   */
  public static sign(
    secret: Buffer | string,
    protocolVersion: RustProtocolVersion,
    keyId: string,
    sessionId: string,
    nonce: string,
    timestampMillis: number,
    capabilities?: string[],
    payload?: Buffer | string | null
  ): RustHmacAuthHeader {
    const canonical = this.computeCanonicalDigest(
      protocolVersion,
      keyId,
      sessionId,
      nonce,
      timestampMillis,
      capabilities,
      payload
    );

    const secretBuffer = typeof secret === 'string' ? Buffer.from(secret, 'utf8') : secret;
    const signature = crypto.createHmac('sha256', secretBuffer).update(canonical, 'utf8').digest('hex');

    /* eslint-disable @typescript-eslint/naming-convention */
    return {
      protocol_version: protocolVersion,
      key_id: keyId,
      session_id: sessionId,
      nonce,
      timestamp_millis: timestampMillis,
      signature,
    };
    /* eslint-enable @typescript-eslint/naming-convention */
  }

  /**
   * Verifies a RustHmacAuthHeader against expected secret, payload, and capabilities.
   * Performs constant-time signature comparison and timestamp skew verification.
   */
  public static verify(
    secret: Buffer | string,
    header: RustHmacAuthHeader,
    payload: Buffer | string | null | undefined,
    capabilities?: string[],
    maxSkewMillis: number = 60000
  ): { valid: boolean; error?: string } {
    // 1. Signature format check
    if (!header.signature || !/^[0-9a-fA-F]{64}$/.test(header.signature)) {
      return { valid: false, error: 'Invalid signature hex format.' };
    }

    // 2. Canonical digest & constant-time HMAC-SHA256 comparison
    const canonical = this.computeCanonicalDigest(
      header.protocol_version,
      header.key_id,
      header.session_id,
      header.nonce,
      header.timestamp_millis,
      capabilities,
      payload
    );

    const secretBuffer = typeof secret === 'string' ? Buffer.from(secret, 'utf8') : secret;
    const expectedSigHex = crypto.createHmac('sha256', secretBuffer).update(canonical, 'utf8').digest('hex');

    const expectedBuf = Buffer.from(expectedSigHex, 'hex');
    const providedBuf = Buffer.from(header.signature, 'hex');

    if (expectedBuf.length !== providedBuf.length || !crypto.timingSafeEqual(expectedBuf, providedBuf)) {
      return { valid: false, error: 'HMAC signature mismatch.' };
    }

    // 3. Timestamp skew validation
    const now = Date.now();
    const skew = Math.abs(now - header.timestamp_millis);
    if (skew > maxSkewMillis) {
      return {
        valid: false,
        error: `Timestamp skew ${skew}ms exceeds maximum allowed ${maxSkewMillis}ms.`,
      };
    }

    return { valid: true };
  }
}
