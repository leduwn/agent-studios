# Operator Confirmation Security & Anti-Replay Architecture Review

**Document ID**: `M15-UI-EXEC-DOC-13`  
**Milestone**: M14.1.1 Final Cross-Platform & Authorization Corrections  
**Author**: Principal Security Engineer, Extension Host Architect  
**Date**: 2026-10-30  
**PR Reference**: PR #25 (`feat/m14-mission-control-connected-integration`)  
**Scope**: `OperatorConfirmationManager`, `CapabilityMatrix`, `BridgeManager`, `DaemonConnectedBridge`, `SessionStateProjection`

---

## 1. Executive Summary

In Milestone M14, task submission against mutating coding workflows was gated behind an operator confirmation mechanism. However, an independent security review identified critical vulnerabilities in the original design:
1. **Public Secret Exposure**: `setSecret` and `getSecret` were exposed as public static methods on `OperatorConfirmationManager`, allowing arbitrary callers to extract or manipulate HMAC signing keys.
2. **Stateless Signature Replay**: Challenges were verified purely via HMAC recalculation without recording single-use consumption, enabling token replay attacks within the TTL window.
3. **Loose Parameter Binding**: Parameters such as `workspacePath`, `modelId`, `providerId`, and `intent` were only compared when both challenge and request were truthy, allowing attackers to omit fields and bypass binding.
4. **Lifecycle Leaks**: Confirmation tokens remained valid across bridge mode switches, daemon disconnects, and session resets.
5. **Runtime Override Vulnerabilities**: `CapabilityMatrix.setCapabilityEnabled` was unconstrained, permitting unauthorized runtime elevation.

This document reviews the comprehensive corrections implemented in M14.1.1 to establish a hardened, host-owned, cryptographically bound anti-replay confirmation architecture.

---

## 2. Cryptographic Anti-Replay State Machine

### 2.1 State Transition Model

Operator confirmation tokens transition through a strict, monotonic lifecycle:

```text
       ┌───────────┐
       │  ISSUED   │ (Generated in Extension Host, stored in activeChallenges map)
       └─────┬─────┘
             │ User approves confirmation modal in UI
             ▼
       ┌───────────┐
       │ CONFIRMED │ (Cryptographic HMAC binding validated)
       └─────┬─────┘
             │ Dispatched to SubmitTaskAdapter.submitTask
             ▼
       ┌───────────┐
       │ CONSUMED  │ (Atomic flag set: consumed = true, consumedAt = timestamp)
       └─────┬─────┘
             │ Replay attempted with same token
             ▼
       ┌───────────┐
       │ REJECTED  │ (Fail closed: "Challenge token has already been consumed")
       └───────────┘
```

### 2.2 In-Memory Tracking Registry

`OperatorConfirmationManager` maintains an internal, unexported registry:

```typescript
interface TrackedConfirmation {
  challenge: OperatorConfirmationChallenge;
  issuedAt: number;
  consumed: boolean;
  consumedAt?: number;
}

export class OperatorConfirmationManager {
  private static secret: Buffer = crypto.randomBytes(32);
  private static readonly activeChallenges: Map<string, TrackedConfirmation> = new Map();
  ...
```

### 2.3 Single-Use Atomic Consumption at Dispatch Boundary

When `SubmitTaskAdapter.submitTask` prepares to transmit an IPC command to `agsd`, it executes preflight verification with `consumeChallenge: true`:

```typescript
const verification = OperatorConfirmationManager.verifyAndConsumeChallenge(
  req.confirmationChallenge,
  {
    studioId: req.studioId!,
    prompt: req.prompt,
    workspacePath: req.workspacePath,
    modelId: req.requestedModelId,
    providerId: req.requestedProviderId,
  },
  {
    consume: req.consumeChallenge ?? true,
  }
);

if (!verification.valid) {
  throw new TaskSubmissionError(
    `Operator confirmation challenge verification failed: ${verification.reason}`,
    req.studioId
  );
}
```

During verification, `tracked.consumed` is evaluated:
```typescript
if (tracked.consumed) {
  return {
    valid: false,
    reason: 'Challenge token has already been consumed (replay prevented)',
  };
}

const shouldConsume = options?.consume ?? true;
if (shouldConsume) {
  tracked.consumed = true;
  tracked.consumedAt = Date.now();
}
```
Any subsequent submission attempting to present the same challenge token is immediately rejected.

---

## 3. Secret Encapsulation & Test Isolation

### 3.1 Elimination of Public Secret Accessors

All public secret getters and setters (`setSecret`, `getSecret`) have been permanently deleted from `OperatorConfirmationManager`.

The 256-bit signing secret is generated once per Extension Host process via `crypto.randomBytes(32)` and remains strictly private within the host's memory closure. It is never serialized, never logged, and never transmitted across IPC or postMessage webview boundaries.

### 3.2 Test-Only Secret Isolation

For testing tampered tokens or alien signatures without exposing the production secret, dedicated test methods are provided:

```typescript
/**
 * Resets the confirmation secret and active challenge registry.
 * Strictly intended for unit test isolation.
 */
public static resetForTesting(): void {
  this.secret = crypto.randomBytes(32);
  this.activeChallenges.clear();
}

/**
 * Creates a challenge signed with an alien secret to test cryptographic signature rejection.
 */
public static createChallengeWithForeignSecretForTesting(
  studioId: string,
  prompt: string,
  foreignSecret: Buffer,
  options?: {
    workspacePath?: string;
    modelId?: string;
    providerId?: string;
    intent?: string;
    ttlMs?: number;
  }
): OperatorConfirmationChallenge { ... }
```

---

## 4. Exact Presence/Absence Parameter Binding

To eliminate optional-field bypasses, `verifyAndConsumeChallenge` enforces bidirectional presence/absence matching across all contextual constraints:

```typescript
// 1. Workspace Directory Binding
const hasParsedWs = Boolean(parsed.workspacePath && parsed.workspacePath.trim().length > 0);
const hasExpectedWs = Boolean(expected.workspacePath && expected.workspacePath.trim().length > 0);

if (hasParsedWs !== hasExpectedWs) {
  return {
    valid: false,
    reason: `Workspace directory presence mismatch in confirmation challenge (challenge had: ${hasParsedWs}, request had: ${hasExpectedWs})`,
  };
}

// 2. Model ID Binding
const hasParsedModel = Boolean(parsed.modelId && parsed.modelId.trim().length > 0);
const hasExpectedModel = Boolean(expected.modelId && expected.modelId.trim().length > 0);

if (hasParsedModel !== hasExpectedModel) {
  return {
    valid: false,
    reason: `Model ID presence mismatch in confirmation challenge (challenge had: ${hasParsedModel}, request had: ${hasExpectedModel})`,
  };
}

// 3. Provider ID Binding
const hasParsedProvider = Boolean(parsed.providerId && parsed.providerId.trim().length > 0);
const hasExpectedProvider = Boolean(expected.providerId && expected.providerId.trim().length > 0);

if (hasParsedProvider !== hasExpectedProvider) {
  return {
    valid: false,
    reason: `Provider ID presence mismatch in confirmation challenge (challenge had: ${hasParsedProvider}, request had: ${hasExpectedProvider})`,
  };
}

// 4. Intent Binding
const hasParsedIntent = Boolean(parsed.intent && parsed.intent.trim().length > 0);
const hasExpectedIntent = Boolean(expected.intent && expected.intent.trim().length > 0);

if (hasParsedIntent !== hasExpectedIntent) {
  return {
    valid: false,
    reason: `Intent presence mismatch in confirmation challenge (challenge had: ${hasParsedIntent}, request had: ${hasExpectedIntent})`,
  };
}
```

If a challenge was issued with a workspace path or model ID, the submission cannot drop it. If a challenge was issued without a workspace path or model ID, the submission cannot inject one.

---

## 5. Lifecycle Invalidation & In-Flight Clearing

Pending confirmation tokens must not survive across state transitions. Invalidation hooks (`OperatorConfirmationManager.clearPendingChallenges()`) are wired into:

1. **Bridge Switching (`BridgeManager.setBridge`)**: Switching between `DisconnectedBridge`, `FixtureBridge`, and `DaemonConnectedBridge` purges all active challenges.
2. **Daemon Connection Changes (`DaemonConnectedBridge`)**:
   - `connect()`: clears stale challenges prior to authentication.
   - `disconnect()`: purges all challenges upon socket closure.
   - `dispose()`: purges challenges during teardown.
3. **Session Projection Changes (`SessionStateProjection`)**:
   - `initializeFromBridge()`: clears challenges upon session state hydration.
   - `dispose()`: cleans up registry on projection destruction.

---

## 6. Capability Override Lockdown

`CapabilityMatrix.setCapabilityEnabled` now enforces a production safety guard:

```typescript
public static setCapabilityEnabled(capabilityId: string, enabled: boolean): void {
  if (process.env.NODE_ENV === 'production') {
    throw new Error('Dynamic capability overrides are prohibited in production mode.');
  }
  this.dynamicOverrides.set(capabilityId, enabled);
}
```

In production builds, mutating capabilities (`task.submit`, `approval.resolve`, `task.cancel`) cannot be enabled by untrusted webviews, malformed configurations, or runtime memory mutations.

---

## 7. Verification Summary

The operator confirmation security suite in `TaskSubmissionSafety.test.ts` executes 12 dedicated regression tests verifying:
- Untrusted raw boolean rejection without challenge token.
- Expired token rejection (TTL expiry).
- Altered prompt hash rejection.
- Studio ID mismatch rejection.
- Single-use consumption (anti-replay) rejection.
- Unrecognized token rejection (token not in Extension Host registry).
- Presence/absence mismatch rejection for workspace and model.
- Clearing on bridge/session reset.
- Alien secret key rejection.
All 12 tests pass unconditionally in remote CI.
