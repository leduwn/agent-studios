# M15-02 final review gate

**Verdict: CONDITIONAL — do not enable unrestricted production execution.**

Verified baseline: PR #26 HEAD `a5dbbfc5ebada254727db9a788c0ee40e3035ea8`; initial M15-02 branch commit `b307b9a6e2bea51873d22e98127562b745eed4fb` with BOTH OS jobs successful in [run 41166485959](https://github.com/leduwn/agent-studios/actions/runs/41166485959).

Hardening afterwards rejects duplicate/foreign discovered instance identities, makes `available_runtimes` mean *healthy*, adds `registered_runtimes` and capability profiles as additive wire fields, prevents implicit fallback when discovery is incomplete, and adds regression tests. Verify success on the **final** pushed SHA before changing verdict.

Supported: `AgentRuntime` registry, fake multi-runtime integration tests and the existing OpenCode adapter path. Unavailable/unverified: production Claude Code native adapter, Codex native adapter, cross-runtime real-model paid integration tests. Do not fabricate adapters or claim test doubles run real models.

Outstanding: independent cancellation/startup race test, physical worktree rollback on startup failures, real OpenCode smoke regression, explicit auth and lifecycle verification for any future adapter, and CI for final SHA. No native Claude Code subagent IDs or user-local git worktree inspection can be asserted by the GitHub connector. No billable model executions were run by this review.

PR policy: one stacked Draft PR from `feat/m15-02-multi-runtime-registry` into `feat/m15-01-per-task-execution-contract`, without merging, rebasing, deleting branches or editing PR #26/#27 or `codex-rs/`.
