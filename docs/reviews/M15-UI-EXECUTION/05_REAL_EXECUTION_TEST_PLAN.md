# M15 — Real AI Coding Execution Test Strategy & Quality Gates

**Document Reference**: `docs/reviews/M15-UI-EXECUTION/05_REAL_EXECUTION_TEST_PLAN.md`  
**Milestone**: M14 Final Integration Freeze & M15 UI Execution Preparation  
**Auditor**: Release QA Lead & Integration Architect  
**Date**: 2026-10-30  
**Status**: APPROVED TEST STRATEGY  

---

## 1. Decoupled Multi-Tier Quality Architecture

Testing is split across 5 decoupled tiers to eliminate false claims of verified execution:

```
+-----------------------------------------------------------------------------+
| Tier 4: Real AI Task Execution Smoke Test (M15 Goal)                         |
| Mutating coding task against external AI model with git worktree diff output|
+-----------------------------------------------------------------------------+
                                       ^
+-----------------------------------------------------------------------------+
| Tier 3: Real Compiled Rust agsd E2E Integration (20 Tests - Passing)        |
| Cross-process live daemon over Named Pipe / UDS, HMAC auth, read-only queries|
+-----------------------------------------------------------------------------+
                                       ^
+-----------------------------------------------------------------------------+
| Tier 2: Mock Daemon Mission Control Projection E2E (11 Tests - Passing)     |
| Cross-process mock daemon validating state projection & indeterminate UI    |
+-----------------------------------------------------------------------------+
                                       ^
+-----------------------------------------------------------------------------+
| Tier 1: Standalone Mock Daemon IPC (Wire Framing & Serde Tagging)           |
+-----------------------------------------------------------------------------+
                                       ^
+-----------------------------------------------------------------------------+
| Tier 0: Pure Unit Tests & Golden Protocol Fixtures (340 Tests - Passing)    |
| Serde tagged contracts, HMAC canonical digests, security breakout guards    |
+-----------------------------------------------------------------------------+
```

---

## 2. Test Tier Classification Criteria

| Test Suite / Tier | Classification Tag | What It Verifies | What It DOES NOT Verify |
| :--- | :--- | :--- | :--- |
| `RustIpcContractFixtures.test.ts` | `MOCK_PROJECTED_TASK_STATE_VERIFIED` | Binary wire framing, Serde serialization, canonical HMAC digests, JSON-RPC rejection. | Does not verify running daemon process. |
| `LiveMissionControlE2E.test.ts` | `MOCK_PROJECTED_TASK_STATE_VERIFIED` | Mission Control data projection against seeded Control Plane state over real OS IPC. | Does not verify real AI task completion or mutating code generation. |
| `RealRustDaemonE2E.test.ts` | `AUTHENTICATED_RUST_IPC_VERIFIED`<br>`LIVE_READ_ONLY_RUST_DATA_VERIFIED` | Compiled Rust `agsd` process lifecycle, bootstrap discovery, HMAC negotiation, health, ping, tasks, providers. | Mutating coding task execution with external AI engine (`REAL_AI_TASK_EXECUTION_NOT_VERIFIED`). |
| M15 Execution Test | `REAL_AI_TASK_EXECUTION_VERIFIED` | User prompt $\to$ task submission $\to$ token streaming $\to$ worktree file edit $\to$ diff review. | Must only be marked verified after full automated run succeeds. |

---

## 3. M15 Isolated Smoke Test Strategy

To verify real execution in M15 without incurring unmetered LLM token costs or flakiness during CI:

### 3.1 Deterministic Fake Runtime Adapter (CI Gate)
1. **Mock Runtime Adapter in Rust**: Implements `AgentRuntime` trait.
2. **Deterministic Token Emission**: When task is submitted, emits 10 chunks of `runtime_output_delta` over 500ms followed by a single tool invocation (`write_file`).
3. **Approval Verification**: Requires client approval for `write_file`.
4. **Outcome Assertion**: Verifies that Mission Control displays task transitioning `pending` $\to$ `running` $\to$ `succeeded` with 100% progress and Chat displays streamed output.

### 3.2 Local Authorized Real-Model Smoke Test (Manual Gate)
1. **Prerequisite**: Set `ANTHROPIC_API_KEY` in environment.
2. **Daemon Launch**: `cargo run -p agsd -- --repo-path <sandbox_git_repo>`
3. **Execution**: Submit prompt: `"Add a hello_world function to src/main.rs"`.
4. **Verification**: Confirm git worktree receives genuine file diff and `agsd` reports successful completion.
