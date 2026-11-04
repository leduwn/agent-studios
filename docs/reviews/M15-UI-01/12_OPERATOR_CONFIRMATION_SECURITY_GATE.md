# Milestone M15 UI-01.1 Review: Operator Confirmation Security Gate

**Component**: `OperatorConfirmationModal.ts` & `CapabilityMatrix.ts`  
**Security Boundary**: Trusted Extension Host  
**Status**: VERIFIED & HARDENED  

---

## 1. Zero Pre-Issued Tokens Lifecycle

### Identified Vulnerability
In previous logic, a cryptographically signed HMAC-SHA256 challenge token was generated *before* presenting the modal dialog to the operator. If the user dismissed or cancelled the modal, or if an attacker intercepted memory or hijacked execution flow, the token had already been registered in the host's active challenge cache.

### Hardened Lifecycle
The lifecycle sequence is strictly linear and gated by human confirmation:

1. **Pre-Validation**:
   - Workspace path canonicalization (`normalizeWorkspacePath`) ensures the target repository exists and is valid on the current platform.
   - Nonce generation (`generateNonce(16)`) prepares a unique dialog session identifier.
2. **Native Modal Dialog Presentation**:
   - The native VS Code dialog (`vscode.window.showWarningMessage('...', { modal: true, detail }, 'Confirm & Execute', 'Cancel')`) blocks execution until the operator explicitly selects an option.
   - **Zero challenge tokens exist in host memory during this phase**.
   - Verified via unit test: `OperatorConfirmationManager.getActiveChallengesCountForTesting() === 0` while the dialog handler is awaiting user response.
3. **Cancellation Handling**:
   - If the operator selects `Cancel` or closes the dialog, `OperatorConfirmationManager.clearPendingChallenges()` purges any uncommitted state.
   - Returns `{ confirmed: false, cancelledByUser: true }`.
   - No token is ever generated.
4. **Post-Confirmation Challenge Issuance**:
   - Only when `confirmed === true` is `OperatorConfirmationManager.createChallenge(...)` called.
   - The challenge is cryptographically bound to:
     - `studioId`
     - SHA-256 hash of prompt text
     - Canonicalized `workspacePath`
     - `modelId` & `providerId`
     - Intent (`task_execution`)
     - Monotonic timestamp and random nonce
5. **Atomic Dispatch Consumption**:
   - `SubmitTaskAdapter` calls `CapabilityMatrix.evaluateTaskSubmissionPreflight` with `consumeChallenge: true`.
   - The challenge is atomically validated and deleted from the active registry in a single step, preventing replay attacks.
