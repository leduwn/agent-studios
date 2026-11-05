# 06 — E2E Verification & Cross-Process Evidence: M15 Consolidation

**Document Reference**: `docs/reviews/M15-UNIFIED-CONSOLIDATION/06_E2E_VERIFICATION_EVIDENCE.md`  
**Milestone**: M15 Consolidation & Workspace Audit  
**Integration Branch**: `integration/m15-unified-codebase`  
**Auditor**: Lead Integration Engineer & Test Execution Lead  

---

## 1. Executive Summary

End-to-end verification confirms that the integrated codebase operates seamlessly across process boundaries between the real compiled Rust daemon (`agsd.exe`) and the TypeScript VS Code extension host (`apps/agent-studios-vscode`).

Key operational flows verified:
1. **Live Daemon Discovery & Authenticated Handshake**: Extension discovers `agsd.json`, performs two-phase capability negotiation, and establishes an HMAC-SHA256 authenticated Named Pipe connection.
2. **Real Subprocess Lifecycle & Clean Shutdown**: Extension spawns `agsd.exe` with isolated bootstrap parameters, verifies health, active clients, and round-trip ping, then gracefully terminates the daemon via authenticated admin `shutdown`.
3. **Subprocess Crash & Forced Kill Recovery**: When the daemon is forcibly killed (`SIGKILL` / `TerminateProcess`), upon restart it detects the unreleased lock, reconciles interrupted runs to `Failed`, and restores the studio and task registry from persistent SQLite WAL.
4. **Lossless Multi-Chunk Artifact Streaming**: High-volume Git diffs and artifacts are streamed in bounded chunks over IPC with SHA-256 cryptographic reassembly and multi-byte Unicode boundary preservation.
5. **Zero Billable Invariant**: Automated execution of `claude --version`, read-only queries, and mock agent harnesses incur **$0.00** in billable cloud API tokens.

---

## 2. Test Execution Summary

```
Running E2E Suite: node out/test/e2e/runner.js

  Tier 2: Mock Daemon Mission Control Projection E2E
    √ 1. Standard client fails closed on mutations while admin/raw IPC can seed data
    √ 2. Connect DaemonConnectedBridge and initialize MissionControlProjection refresh
    √ 3. Studio appears in Mission Control (goalSummary and activeStudio match seeded state)
    √ 4. Real task ID and description display accurately in state.tasks
    √ 5. Task status matches Rust state (succeeded, running)
    √ 6. Task progress is truthful: 100% for succeeded, undefined (indeterminate) for running
    √ 7. Real providers and models hydrated from daemon (Anthropic Claude, OpenAI GPT)
    √ 8. Provider latency is unmeasured (undefined), NOT hardcoded 120ms
    √ 9. Event subscription updates timeline and attention queue when events are published
    √ 10. Query errors produce diagnostics alert in AttentionQueue, not false zeroes
    √ 11. Real compiled Rust daemon (agsd.exe) verification if binary present (2859ms)

  Tier 3: Real Agent Execution & Runtime E2E Suite
    √ 1. locates real claude CLI binary path in environment or local bin
    √ 2. verifies real claude CLI binary is a valid non-empty executable
    √ 3. runs claude --version without billable token usage and validates version output (156ms)
    √ 4. runs claude --help in non-interactive read-only mode (1024ms)
    √ 5. fail-closed detection: non-existent CLI binary path throws error
    √ 6. detects verified claude_code adapter when present in available_runtimes
    √ 7. detects unverified claude_code adapter when missing from available_runtimes
    √ 8. detects opencode adapter and validates healthy runtime instance
    √ 9. detects degraded/unhealthy opencode runtime and disables option
    √ 10. disables duplicate runtime instance IDs to prevent ambiguous routing
    √ 11. returns safe disabled fallback on null or unready daemon report
    √ 12. validates runtime capabilities tri-state enforcement for execution and cancellation
    √ 13. fails closed when task submission attempted with task.submit disabled in CapabilityMatrix
    √ 14. fails closed when task submission attempted without operator confirmation challenge
    √ 15. fails closed when operator confirmation challenge signature is forged or invalid
    √ 16. fails closed when operator confirmation challenge has expired
    √ 17. fails closed when studio ID or prompt hash does not match challenge
    √ 18. verifies DaemonConnectedBridge mutation methods fail closed in read-only slice
    √ 19. enforces zero wire dispatch and in-process TaskExecutionStore immutability
    √ 20. locates real compiled Rust agsd.exe binary
    √ 21. spawns real agsd.exe daemon with isolated temporary bootstrap and named pipe (2759ms)
    √ 22. verifies standard and admin discovery credentials separation
    √ 23. connects DaemonConnectedBridge with HMAC authentication to real daemon
    √ 24. verifies real daemon health and active client count
    √ 25. verifies real daemon ping roundtrip latency
    √ 26. lists available providers from real daemon
    √ 27. lists available models and verifies provider model routing mappings
    √ 28. verifies standard client shutdown denial (fails closed with permission denied)
    √ 29. executes graceful daemon shutdown using authenticated admin key
    √ 30. verifies daemon child process termination with exit code 0 and cleans up resources

  Tier 3: Real Artifact IPC Integration
    √ 1. real daemon returns error response for non-existent task diff without socket disruption
    √ 2. real daemon rejects get_task_diff with zero limit (infinite loop guard)
    √ 3. real daemon rejects list_task_artifacts for non-existent task
    √ 4. real daemon returns null artifact for non-existent artifact ID
    √ 5. ArtifactClient.fetchTaskDiff throws ArtifactRetrievalError against real daemon
    √ 6. multi-chunk streaming with lossless UTF-8 reassembly and SHA-256 verification (VERIFIED)
    √ 7. preserves multi-byte Vietnamese Unicode split across arbitrary byte boundaries
    √ 8. binary Git patch chunk streaming with isBinary: true and exact byte preservation
    √ 9. cryptographic tampering detection yielding INTEGRITY_FAILED
    √ 10. studio ID mismatch rejection (STUDIO_ID_MISMATCH)
    √ 11. task ID mismatch rejection (TASK_ID_MISMATCH)

  Tier 3: Real Compiled Rust agsd E2E Test Suite
    √ 1. Daemon process startup (132ms)
    √ 2. Bootstrap file discovery (DaemonDiscovery.loadBootstrapInfo)
    √ 3. Standard vs admin credential separation (agsd.json vs agsd-admin.json)
    √ 4. Two-phase capability negotiation and HMAC authentication (DaemonConnectedBridge.connect())
    √ 5. Invalid HMAC rejection (assert auth fails with wrong key)
    √ 6. Health query (getHealth() -> healthy, active_clients: 1)
    √ 7. Ping latency (ping() -> >= 0ms)
    √ 8. Studio listing (list_studios / getCurrentSession()) (50ms)
    √ 9. Task listing (listTasks())
    √ 10. Provider listing (listProviderSummaries())
    √ 11. Model listing (models inside provider summary)
    √ 12. Event subscription (daemon activates streaming on connect) (63ms)
    √ 13. Sequence watermark handling (tracking lastWatermark)
    √ 14. Connection loss handling (disconnect socket)
    √ 15. Reconnect with monotonic sequence watermark and replay dedup
    √ 16. Read-only mutation rejection (assert requestApprovalDecision and submitEditorAction fail closed)
    √ 17. Standard-client shutdown denial (assert standard client sending shutdown receives permission denied)
    √ 18. Authorized admin shutdown (connect with admin key from agsd-admin.json and send shutdown)
    √ 19. Child-process termination (assert child process exits with code 0)
    √ 20. Temporary resource cleanup (assert bootstrap files and pipe/socket cleaned up)

  Tier 3: Real Compiled Rust agsd Crash Recovery E2E Test Suite
    √ 1. Clean Daemon Restart & State Persistence: studios and tasks preserved across graceful restarts (367ms)
    √ 2. Abnormal Termination (Forced Kill): recovers database and reconciles interrupted tasks to failed (1434ms)
    √ 3. Role-Based Diagnostic Redaction: standard clients receive null database_path, admin receives verified path (148ms)
    √ 4. Watermark Resynchronization: replay backfill from persistent SQLite WAL (201ms)
    √ 5. UI Safety Invariant: Bridge reconnect NEVER automatically dispatches task re-execution (163ms)
    √ 6. ReplayBuffer SequenceGap Across Restart: recovers from sequence gap and rehydrates state (1554ms)

  78 passing (12s)
```

---

## 3. Cryptographic Verification & Replay Protection Evidence

1. **HMAC-SHA256 Challenge-Response**: Every connection exchange uses unique random nonces verified by `agent-studios-ipc-protocol::auth::HmacSigner`. Tampered signatures or replayed nonces are rejected with `InvalidAuthKey`.
2. **Two-Phase Approval Delivery (2PC)**:
   - Approval resolution requires valid session token and matching client identity.
   - Delivery to runtime stdin is verified before committing the approval state in `ControlPlane`.
   - Stdin write failure triggers immediate fail-closed state abort.
3. **Sequential Monotonicity**:
   - Replay buffer rejects non-monotonic sequences (`SequenceGapUnrecoverable`).
   - Client subscriptions deduplicate live broadcast events where `event.sequence <= current_seq`.
