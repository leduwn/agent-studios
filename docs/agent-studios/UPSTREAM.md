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

## Architectural Separation

Agent Studios-specific functionality is implemented preferentially as additive crates/modules rather than unnecessary modifications to upstream Codex crates.

When upstream modifications are strictly necessary, keep patches focused, minimal, and documented to ensure upstream diffs remain straightforward to port.

---

## License & Attribution

Codex is licensed under the Apache-2.0 License. All derivative components retain original OpenAI notices and attribution in `LICENSE` and `NOTICE` files.
