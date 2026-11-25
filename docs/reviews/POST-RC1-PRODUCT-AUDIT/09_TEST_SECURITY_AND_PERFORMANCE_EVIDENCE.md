# 09 — Test, Security, Performance & CI Evidence Ledger

> **Canonical Evidence Rule**: Never aggregate test counts across different SHAs without tagging the exact source commit tested.

---

## 1. Exact Verified Test & CI Runs Leading to `00e5c862b8`

| Layer / Command | Commit SHA Tested | Exit Code | Observed Result | Scope & Conditions |
|---|---|---:|---|---|
| **GitHub Actions Full CI (`45597809243`)** | `00e5c862b8` (`main` / PR `#35`) | `0` | `ubuntu-latest` + `windows-latest` **success** | Docs update after `41480cfa02` code SHA |
| **GitHub Actions Full CI (`45590072851`, `45590064391`)** | `41480cfa02` (latest code SHA) | `0` | `ubuntu-latest`: **success**, `windows-latest`: **success** | Full Rust workspace, Clippy `-D warnings`, build, `lint`, `typecheck`, `test:unit`, `test:e2e`, `test:integration`, VSIX |
| **Rust `cargo test --workspace`** (`task-489`) | `402312516a` | `0` | **`1014 passed; 0 failed; 0 ignored`** across `114` test targets | `RUST_MIN_STACK=8388608`, `$0.00` (`AGENT_STUDIOS_LIVE_CLI_SMOKE` unset) |
| **Rust `thread_e2e_tests` w/o `RUST_MIN_STACK`** (`task-593`) | `ace0f6b026` | `0` | **`7 passed; 0 failed`** (`7.89s`) | Verified Windows default stack safety after adding 8 MB thread helper |
| **Rust `cargo test -p agent-studios-cli` + `clippy -D warnings`** (`task-624`) | `ace0f6b026` | `0` | `6` unit + `7` CLI acceptance passed | Tested Route 3 daemon connection & `fmt`/`clippy` |
| **Frontend `npm run lint`** (`task-727`) | `41480cfa02` | `0` | `0` errors, **`352` pre-existing warnings** | Audited: primarily `@typescript-eslint/naming-convention` on snake_case Rust IPC JSON fields (`350`), plus `2` `curly` brace warnings in `TaskExecutionStore.ts` |
| **Frontend `npm run typecheck` & `npm run build`** (`task-727`) | `41480cfa02` | `0` | `0` errors | Strict `tsc --noEmit` + `tsc -p ./` |
| **Frontend `npm run test:unit`** (`task-727`) | `41480cfa02` | `0` | **`700 passing (3s)`** | Mocha + `vscodeMock` including cross-platform `AgsdBinaryResolver` fix |
| **Frontend `npm run test:e2e`** (`task-727`) | `41480cfa02` | `0` | **`84 passing (7s)`** | Tier 2 + Tier 3 live compiled `agsd.exe` IPC, HMAC, crash recovery, artifact streaming |
| **Frontend `npm run test:integration`** (`task-727`) | `41480cfa02` | `0` | **`25 passing (19s)`** | Real VS Code Extension Development Host GUI (`@vscode/test-electron`) |
| **RC1 `Verify-ReleaseBundle.ps1`** (`task-819` post-cleanup) | `41480cfa02` (`RELEASE_MANIFEST.json`) | `0` | `9` file SHA-256 + `.zip` SHA-256 verified | Re-verified post `+96.85 GB` cache cleanup |

---

## 2. Security, Stress & Performance Observations

- **Stress & Security Suites Included in `1014` Workspace Tests**:
  - `m18_stress_and_hardening_tests.rs` (`control-plane`: 12 parallel workers, `1,000+` turns, `100,000+` events).
  - `m18_concurrency_security_tests.rs` (`ipc-transport`: concurrent HMAC challenges, malformed framing, path traversal, privilege separation).
- **Pre-Existing ESLint Warnings Audit (`352` total)**:
  - ~`350` are `@typescript-eslint/naming-convention` notices on snake_case wire fields (`studio_id`, `task_spec`, `client_key`, etc.) required by the Rust serde IPC contract in `Bridge` and E2E fixtures — **zero runtime impact**.
  - `2` style warnings (`curly`) exist in `src/state/TaskExecutionStore.ts:282,525` (low impact, cleanable incrementally).
- **Performance & Timing Baselines Measured**:
  - Full TypeScript test suites: unit `3s`, live-daemon E2E `7s`, GUI Extension Host `19s`.
  - Release binary compile (`--release` incremental `agsd` + `ags`): `2m 02s` (cold `--release`: `10m 26s`).
  - Missing benchmarks to establish in RC2 (`Wave 8`): `AgentStudios.exe` cold desktop startup latency, context engine indexing throughput, and sustained memory RSS profiling.
