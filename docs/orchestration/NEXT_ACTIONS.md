# Next Actions Queue (`NEXT_ACTIONS.md`)

> **Updated Post-RC1**: 2026-11-25T18:33:00+07:00
> **Current Branch**: `feat/rc2-product-completion` (from `main` `00e5c862b8173cb29914a4ff8dfe4dbb1147e03c`)

1. Complete Wave 1 16-file Post-RC1 Forensic Audit (`docs/reviews/POST-RC1-PRODUCT-AUDIT/`) + sanitized audit ZIP `C:\AgentStudiosBuild\release-bundle\AGENT_STUDIOS_POST_RC1_AUDIT_REPORT.zip`.
2. Build Wave 2–3 standalone `AgentStudios.exe` desktop host & embedded `agsd` lifecycle supervisor (`agent-studios-rs/crates/desktop`) with real daemon crash recovery & Agent ↔ IDE Mode continuity tests.
3. Implement Wave 4 VS Code Extension Groups 1–6 (`ConversationManager`, `EditorIntelligenceProvider`, `ContextEngine`, `HunkReviewManager`, `ProviderManagementCommands`, `ExtensibilityCommands`) + unit/E2E coverage.
4. Implement Wave 5–6 Streamable HTTP MCP transport (`crates/extensibility/src/mcp.rs`) + resources/prompts + `LiveAcceptanceHarness` (`crates/cli-client/src/live_harness.rs`).
5. Repair Wave 7 release pipeline (`scripts/packaging/*`, `installer/windows/*`) for `v1.0.0-rc2`, execute real sandbox installation/upgrade/uninstall smoke tests, run full Rust + TypeScript gates, export `docs/reviews/RC2-PRODUCT-COMPLETION/MASTER_REPORT.md` + sanitized ZIP, and push `feat/rc2-product-completion` for operator review (respecting `GATE-03`, `GATE-04`, and protected `main`).
