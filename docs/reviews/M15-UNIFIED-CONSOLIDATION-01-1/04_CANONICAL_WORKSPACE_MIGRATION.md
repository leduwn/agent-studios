# 04 — Canonical Workspace Migration & Preservation: M15 Consolidation 01.1

**Document Reference**: `docs/reviews/M15-UNIFIED-CONSOLIDATION-01-1/04_CANONICAL_WORKSPACE_MIGRATION.md`  
**Milestone**: M15 Consolidation & Canonical Preflight  
**Canonical Path**: `D:\Projects\agent-studios`  
**Integration Branch**: `integration/m15-unified-codebase`  
**Unified HEAD**: `5c8f5fd6d84376c4b1c2420f7f8b911fb58d3a3c`  
**Auditor**: Lead Integration Engineer & Release Manager  

---

## 1. Executive Summary & Core Invariant

The ultimate target of Milestone M15 consolidation is to establish `D:\Projects\agent-studios` as the single canonical repository root.

However, `D:\Projects\agent-studios` currently contains **uncommitted provider model discovery work** that belongs to `feat/provider-management-v1` (PR #20).

Under the Phase 0 Safety Gate:
- **Zero Destructive Actions**: Never run `git reset --hard`, `git clean -fdx`, `git checkout -f`, or automatic `git stash` without explicit operator intent.
- **Zero Renaming / Movement**: Do not rename, move, or delete `D:\Projects\agent-studios`.
- **Zero Data Loss**: All untracked and modified files must be fully inventoried and protected prior to any checkout migration.

---

## 2. Complete Inventory of Uncommitted Files in Canonical Root

Audit of `git -C D:\Projects\agent-studios status -s` identifies the exact uncommitted state:

```
Tracked Modified Files:
 M agent-studios-rs/Cargo.lock
 M agent-studios-rs/crates/provider/Cargo.toml
 M agent-studios-rs/crates/provider/src/error.rs
 M agent-studios-rs/crates/provider/src/lib.rs
 M agent-studios-rs/crates/provider/src/secret.rs

Untracked Work Files:
?? agent-studios-rs/crates/provider/src/discovery.rs
?? agent-studios-rs/crates/provider/tests/discovery_tests.rs
?? AGENTS.md
?? 01_LOCAL_DIRECTORY_INVENTORY.md
?? 09_SAFE_WORKTREE_CLEANUP_PLAN.md
?? inventory_manifest.json
?? docs/reviews/M13-IPC-02.2.2/
```

### Analysis of Uncommitted Provider Work:
- `discovery.rs` and `discovery_tests.rs` implement dynamic model discovery via HTTP/Wiremock for Anthropic, OpenAI, and custom providers (`ModelDiscoveryClient`, `DiscoveryConfig`).
- `crates/provider/Cargo.toml` adds `reqwest` and `wiremock`.
- `crates/provider/src/secret.rs` adds `InMemorySecretStore` and `EnvSecretStore`.
- This work is active feature code for PR #20 (`feat/provider-management-v1`). Overwriting or wiping this directory would destroy active operator development work.

---

## 3. Recommended Phased Migration Strategy

To transition `D:\Projects\agent-studios` to `integration/m15-unified-codebase` without losing any uncommitted work:

### Phase 1: Operator-Directed Preservation Branch (Recommended)
Before switching branches in `D:\Projects\agent-studios`, preserve the uncommitted provider work on a dedicated backup branch:
```bash
# Executed from D:\Projects\agent-studios:
git checkout -b feat/provider-discovery-v1.2-wip
git add agent-studios-rs/crates/provider/ agent-studios-rs/Cargo.lock
git commit -m "wip(provider): preserve uncommitted dynamic model discovery implementation"
```
*Benefit: Zero data loss, preserves full commit metadata, completely restorable.*

### Phase 2: Alternative Stash with Descriptive Identifier
If the operator prefers not to commit to a branch:
```bash
# Executed from D:\Projects\agent-studios:
git stash push -u -m "STASH-PRE-M15-CONSOLIDATION-PROVIDER-DISCOVERY-20261104"
```

### Phase 3: Transition Canonical Root to Unified Branch
Once the uncommitted work is safely preserved:
```bash
# Executed from D:\Projects\agent-studios:
git fetch origin
git checkout integration/m15-unified-codebase
git pull origin integration/m15-unified-codebase
```

---

## 4. Interim Staging Worktree Operational Model

Until the operator reviews and chooses the preservation option for the canonical root:
1. **`D:\Projects\agent-studios-unified-staging` remains the active operational worktree**:
   - All unified build verification, CI workflows, and PR branch updates are committed and pushed from `D:\Projects\agent-studios-unified-staging`.
2. **Canonical Root Remains Untouched**:
   - `D:\Projects\agent-studios` continues to host the shared `.git` object database and retains its uncommitted provider files completely undisturbed.
3. **Seamless Transition**:
   - Once the operator signs off and executes Phase 1 or Phase 2 above in `D:\Projects\agent-studios`, the temporary staging worktree can be safely removed via `git worktree remove D:/Projects/agent-studios-unified-staging`.
