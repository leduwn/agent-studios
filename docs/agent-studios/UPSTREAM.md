# Upstream Tracking & Attribution

Agent Studios is derived from the open-source OpenAI Codex project.

- **Upstream**: https://github.com/openai/codex
- **Initial imported baseline**: `d25c114d494ddb693290b76bf5e5f64ecbdb38fc`
- **Baseline date**: 2026-10-02
- **Synchronization strategy**: Snapshot-based upstream synchronization

---

## Clean History & Snapshot Strategy

Agent Studios Git history intentionally does not contain the historical Codex commit ancestry. This keeps Agent Studios' repository history focused on Agent Studios development while retaining required attribution and license notices.

The upstream baseline commit is locked in machine-readable format at `upstream/codex.lock.json`.

---

## Upstream Synchronization Workflow

Future upstream updates use the recorded baseline and target Codex SHAs to calculate differences and port them into Agent Studios as explicit, audited upstream synchronization commits.

**Do NOT merge `upstream/main` directly into Agent Studios `main`.**

The local Git remote named `upstream` is used for fetching new commits from Codex:

```powershell
git remote add upstream https://github.com/openai/codex.git
git fetch upstream
```

### Concept & Steps

1. **Identify Commits**:
   - `old` = Commit recorded in `upstream/codex.lock.json`
   - `new` = Target upstream Codex commit (e.g., `upstream/main` or specific release tag)

2. **Calculate Diff**:
   ```powershell
   git diff <old>..<new>
   ```

3. **Port & Reconcile**:
   - Analyze upstream changes against Agent Studios codebase.
   - Apply relevant patches to Codex-derived crates.
   - Resolve any Agent Studios architectural divergence or conflicts.

4. **Verify & Validate**:
   - Run compilation and tests on Windows native (`cargo build -p codex-cli`, etc.).
   - Verify that Agent Studios control plane and multi-agent systems function properly.

5. **Commit & Lock Update**:
   - Commit ported changes as an explicit synchronization commit:
     ```text
     chore(upstream): sync codex changes from <old> to <new>
     ```
   - Update `upstream/codex.lock.json` with the new target commit SHA.

---

## Architectural Separation & Intentional Patches

Agent Studios-specific functionality is implemented preferentially as additive crates/modules outside `codex-rs/` (in `agent-studios-rs/crates/`).

Starting in Milestones M07.5, M07.6, M08, and M08.1.1, `codex-rs/` is no longer byte-identical to the
upstream snapshot. To allow Agent Studios to supply custom model inference backends (such as
Chat Completions, Anthropic Messages, and Gemini generateContent), thread-scoped model runtime
overrides, multi-agent child spawn runtime overrides, and host extension contexts for dynamically
spawned sub-agents while fully preserving Codex's native agent loop, tool router, and execution
engine, four minimal, provider-neutral, default-off seams are maintained:
1. `ModelInferenceBackend` seam in `codex-rs/model-provider` and `codex-rs/core` (Patch 001).
2. `ModelRuntimeOverride` thread injection seam in `codex-rs/core`, `codex-rs/core-api`, and
   `codex-rs/model-provider` (Patch 002).
3. `SpawnRequest` model runtime override and `CodexThread::agent_control` façade in
   `codex-rs/core` (Patch 003).
4. `SpawnRequest.thread_extension_init` host extension seam for spawned sub-agents in
   `codex-rs/core` (Patch 004).

All intentional modifications to `codex-rs/` are strictly registered and audited in
`docs/agent-studios/CODEX_PATCHES.md`. During upstream synchronization, these minimal patches
must be re-applied and verified against target upstream commits. Agent Studios protocol, catalog,
and provider code remains strictly isolated outside `codex-rs/`. Agent Studios does not claim
authorship of upstream OpenAI Codex code.

---

## License & Attribution

Codex is licensed under the Apache-2.0 License. All derivative components retain original OpenAI notices and attribution in `LICENSE` and `NOTICE` files.
