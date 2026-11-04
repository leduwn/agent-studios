# M15 UI-04 — Test Taxonomy & Final CI Release Gate

## 1. Test Taxonomy Classification

To maintain audit integrity and avoid false marketing claims:

| Category | Description | Verification Level | CI Execution |
| :--- | :--- | :--- | :--- |
| **`REAL_DAEMON_IPC_TESTS`** | Direct IPC communication with authentic compiled `agsd` binary over Named Pipes / UDS. | Fully verified (startup, auth, crash, restart, recovery, artifacts). | Mandatory in CI (no external dependencies, no API keys). |
| **`REAL_AI_AGENT_EXECUTION_TESTS`** | Invocations of external LLM models (OpenAI, Anthropic) via provider adapters. | NOT verified in automated PR gates (requires operator billing authorization). | Statically disabled / mock-isolated in automated CI. |
| **`MOCK_BRIDGE_TESTS`** | In-memory protocol simulation (`FixtureBridge`, `StandaloneMockDaemon`). | Unit verification of UI layout, state projection, and CSP filters. | Standard unit test suite. |

---

## 2. Quality Gates Matrix

| Gate | Check Command | Success Criteria | Status |
| :--- | :--- | :--- | :--- |
| **Q1: TypeScript Compilation** | `npm run compile` | Zero errors, strict mode compliant | Gate Met |
| **Q2: ESLint Cleanliness** | `npm run lint` | Zero warnings, zero errors | Gate Met |
| **Q3: Unit Test Suite** | `npm run test:unit` | 100% pass across all unit test suites | Gate Met |
| **Q4: Real Daemon IPC Suite** | `npm run test:e2e` | All 20+ authentic daemon scenarios pass | Gate Met |
| **Q5: Ubuntu & Windows CI** | GitHub Actions Workflow | Green on both `ubuntu-latest` and `windows-latest` | Gate Met |

---

## 3. Backend Handoff Protocol

If any defect, protocol desynchronization, or daemon crash is uncovered during frontend integration:
1. Do NOT modify files under `agent-studios-rs/` from the UI branch.
2. File a formal handoff specification in `docs/reviews/M15-UI-04/BACKEND_HANDOFF_<ISSUE>.md`.
3. Escalate to Claude 1 with reproducible test logs, payload traces, and error frames.
