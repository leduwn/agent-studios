# M15-02 Cross-platform test evidence and limitations

Baseline: `b307b9a6e2bea51873d22e98127562b745eed4fb` passed GitHub Actions [workflow 41166485959](https://github.com/leduwn/agent-studios/actions/runs/41166485959) on BOTH `ubuntu-latest` and `windows-latest`. This was BEFORE the identity-integrity, readiness and regression-test changes.

Later hardening adds tests for duplicate discovered instance IDs, a runtime claiming a foreign implementation identity, filtering unhealthy implementations, multi-instance ambiguity and the new additive readiness fields. Its final SHA must be checked independently; do NOT reuse the baseline green CI badge to certify hardening.

CI workflow `.github/workflows/agent-studios-m13-ipc-ci.yml` has been extended to format/check/clippy/test ALL affected crates: `agent-studios-external-runtime`, `agent-studios-ipc-protocol`, `agent-studios-ipc-transport`, on Windows and Ubuntu where applicable. Cargo clippy runs `--all-targets -- -D warnings`.

Hermetic fakes test daemon IPC and runtime routing, NOT real external-model execution. No billable AI calls. OpenCode operational-path regression is a separate requirement, not proof that Claude Code or Codex adapters have been implemented.

Release gate: obtain completed SUCCESS for BOTH OS jobs at the exact final pushed SHA, examine job logs for test/compile failures, and perform lifecycle race review. Until then, verdict is CONDITIONAL.
