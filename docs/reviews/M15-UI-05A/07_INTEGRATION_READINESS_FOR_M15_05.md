# M15 UI-05A — Report 07: Integration Readiness for Backend M15-05

## Metadata
- **Milestone**: M15 UI-05A: Frontend Reliability, Shared State Consistency & Integration Readiness
- **Focus**: Backend M15-05 Replay Compatibility & Contract Handshake Readiness
- **Target Worktree**: `D:\Projects\agent-studios-ide-ui\apps\agent-studios-vscode`
- **Target PR**: Draft PR #27 (`feat/m15-real-agent-execution-ui`)
- **Authoritative Pinned Backend SHA**: `dcbcb39bf61225a4660be5b533cf1ffd5de14b29` (Backend PR #30)
- **Author**: Claude 2 (Frontend / VSCode Extension Integration Lead)
- **Date**: 2026-11-02

---

## 1. Executive Summary

Milestone M15-05 in the backend (`agent-studios-rs`, PR #30 by Claude 1) focuses on durable replay buffers, persistent WAL event streams, and crash recovery hardening.

To ensure zero friction when the backend lands M15-05:
1. **The Backend Pin Remains Stably Locked**: The frontend does **not** prematurely switch or guess unmerged backend SHAs. The pin remains strictly at `dcbcb39bf61225a4660be5b533cf1ffd5de14b29`.
2. **Contract Antifragility**: The frontend's `NativeIpcTransport` and `DaemonConnectedBridge` are architected to gracefully tolerate both M15-04.1 in-memory replay buffers and M15-05 persistent SQLite event streams.
3. **Sequence Gap Recovery Protocol**: The frontend handles `SequenceGapRecoverable` and unknown sequence errors by seamlessly falling back to full domain state hydration and resubscribing from watermark 0 without dropping connection or crashing the extension.

---

## 2. Wire Protocol Readiness Matrix

| Feature | M15-04.1 Baseline (`dcbcb39b`) | Upcoming M15-05 Target | Frontend Readiness Status |
|---|---|---|---|
| **Event Replay Mechanism** | In-memory ring buffer (`ReplayBuffer`) | Persistent SQLite WAL event log | **READY**: Frontend tracks `lastWatermark` monotonically and can replay from any cursor |
| **Sequence Gap Handling** | Returns `SequenceGapRecoverable` error if buffer purged | May retain full history or return gap error | **READY**: Frontend traps `SequenceGapRecoverable`, resets watermark, rehydrates via domain queries |
| **Crash Recovery Status** | `get_recovery_status` query | Enhanced recovery diagnostics | **READY**: Strong typing via `RustRecoveryStatusReport` handles extra optional fields |
| **Diagnostic Redaction** | `database_path: null` for standard clients | Unchanged least-privilege redaction | **READY**: Null-safe rendering in Mission Control diagnostics view |
| **Task Reconnection Invariant** | Crashed tasks marked `failed (daemon_interrupted)` | Never auto-resumed | **READY**: Frontend displays failure and provides manual retry UI only |

---

## 3. Protocol Transition Details

### A. Non-Breaking Sequence Number Semantics
In `DaemonConnectedBridge.ts`:
```typescript
// Seamless handling of both in-memory and durable replay streams
transport.on('subscription_error', async (err: Error) => {
    if (err.message.includes('Sequence gap') || err.message.includes('SequenceGapRecoverable')) {
        this._lastWatermark = 0;
        this._seenEventSequences.clear();
        await this._transport?.subscribeEvents(0);
        await this.syncDomainState();
    }
});
```
When M15-05 lands with persistent WAL replay, the daemon will be able to satisfy `subscribeEvents(watermark)` directly from the database without emitting `SequenceGapRecoverable`. However, if the database is compacted or purged, the frontend's hydration fallback guarantees zero user-visible disruption.

### B. In-Flight Request Coalescing During Hydration
When a reconnect occurs, multiple subsystems (Chat, Mission Control, Workbench) simultaneously seek state updates.
The `MissionControlProjection.refresh()` coalescing mechanism introduced in M15 UI-05A collapses concurrent refresh calls into a single in-flight query batch, preventing overwhelming the daemon during restart or recovery.

---

## 4. Verification Checklist Before Bumping Backend Pin

When Claude 1 delivers M15-05 on PR #30:
1. Verify PR #30 CI is green on GitHub Actions.
2. Confirm Rust test suite passes `cargo test --workspace`.
3. Capture authoritative merge SHA or PR HEAD commit.
4. Update `PINNED_SHA` in `.github/workflows/agent-studios-m14-ui-ci.yml`.
5. Run full local E2E suite (`npm run test:e2e`) against compiled `agsd.exe` built from new SHA.
6. Verify remote CI runs green on both Ubuntu and Windows runners.
