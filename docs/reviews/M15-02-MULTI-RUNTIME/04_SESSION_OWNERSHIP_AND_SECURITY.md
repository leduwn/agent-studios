# M15-02 Session ownership and security

The active ownership map is keyed by task ID and retains `(RuntimeSessionRef, Arc<dyn AgentRuntime>)`. Pending approvals separately retain the requesting session, runtime approval ID and the SAME runtime pointer. Approval delivery occurs to that owner BEFORE mutation of the authoritative Control Plane approval state; cancellation sends `stop()` to the runtime in the active-task ownership map. Never re-resolve a running task using registry defaults when delivering approval or cancellation.

Required invariants for ongoing review: task/run/studio/agent correlation survives event streaming; active sessions cannot cross owners even if adapter registration changes; terminal events are idempotent; cancellation is isolated across adapters; worktree source paths are canonicalized and authorized; process start and event-subscription failures must stop/release resources.

Discovery validation rejects foreign implementation identities and duplicate instance IDs before routing. Diagnostic strings should remain sanitized; no raw runtime environment or credential material is allowed on the authenticated readiness wire.

**Residual acceptance gates:** independently test the narrow race where a task is cancelled during asynchronous worktree creation or runtime startup, and prove physical worktree cleanup on failed runtime start and event subscription. A compile-green workflow alone does not prove these interleavings are safe. Do not mark security READY until these lifecycle cases are evidenced.

See the detailed earlier `01_SECURITY_AND_CORRECTNESS_REPORT.md` and `07_SESSION_OWNERSHIP_AND_LIFECYCLE_INVARIANTS.md`, while treating their pre-hardening claims as historical.
