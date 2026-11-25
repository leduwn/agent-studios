# 13 — Master Technical Handoff (Post-RC1)

- **Repository Root**: `D:\Projects\agent-studios` (Canonical `.git` host at `integration/m15-unified-codebase`, clean except 5 original inventory files)
- **Active RC2 Worktree**: `D:\Projects\agent-studios-active\m15-1-joint-validation` on branch `feat/rc2-product-completion` (branched from `main` at `00e5c862b8173cb29914a4ff8dfe4dbb1147e03c`)
- **Cargo Target Policy**: Set `$env:CARGO_TARGET_DIR = "C:\AgentStudiosBuild\cargo-target-rc2"` to keep Drive `D:` clean (`69.10 GB` free) while using Drive `C:` (`238.11 GB` free).
- **Formatting Rule on Windows**: Run `cargo fmt --check` without `--all` inside `agent-studios-rs/` to avoid OS error 206 command-line length overflow across `codex-rs/`.
- **Immutable Artifacts**: Do NOT modify `C:\AgentStudiosBuild\release-bundle\AgentStudios-v1.0.0-rc1-win-x64*` (`3ce6d4e8fd307c78042bb297171af0c174382ef01f88435dd11c483e4c8c3eb9`).
- **Remaining Gates Requiring Human Action Only After Offline Verification**: `GATE-03` (live cloud LLM spend), `GATE-04` (Authenticode cert), and merging `feat/rc2-product-completion` -> `main`.
