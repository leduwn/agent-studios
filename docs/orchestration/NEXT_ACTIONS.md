# Next Actions Queue (`NEXT_ACTIONS.md`)

1. Commit Wave 1 orchestration control docs on `integration/m15-1-joint-validation`.
2. Execute **Wave 2** (M15.1 & P0 Real Execution):
   - Implement `ConnectedClient`, `connect_or_fallback`, `ClaudeCodeRuntime` discovery, and local worktree action execution in `crates/cli-client`.
   - Implement `ags run`, `ags daemon`, `ags provider discover` in `crates/cli`.
   - Write and verify `p0_real_execution_acceptance_tests.rs` and `cli_acceptance_tests.rs`.
3. Execute **Wave 3** (M16 Extensibility):
   - Implement `agent-studios-extensibility` crate (`skills`, `mcp`, `hooks`, `instructions`) and `ags` CLI commands.
4. Execute **Wave 4** (M17/M18/M19):
   - Implement EN/VI localization bundles, language decoupling, and Windows canonical paths in `apps/agent-studios-vscode`.
   - Implement M18 stress (12 workers, 1,000+ turns, 100k events) and concurrency/security tests.
   - Implement Windows release bundler, installer/uninstaller scripts, and M19 release documentation.
5. Execute **Wave 5** (Mandatory Quality Gates):
   - Run full backend (`fmt`, `check`, `clippy -D warnings`, `test --workspace`, `build --bin agsd --bin ags`), frontend (`lint`, `typecheck`, `build`, `test:unit`, `test:e2e`, `vsce package`), and bundle verification.
