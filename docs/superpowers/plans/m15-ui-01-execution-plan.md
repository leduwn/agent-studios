# M15 UI-01 Interactive Agent Execution Workbench Plan

## Goal
Deliver a production-ready, accessible, fail-closed, and secure Chat-first interactive agent execution workbench in `agent-studios-vscode` on branch `feat/m15-real-agent-execution-ui`, with zero Rust edits, zero billable AI tasks, and complete wire protocol parity with `agsd`.

## Tasks
1. **Task 1 (Agent B)**: Typed IPC execution client & contract compatibility (`CancelTaskAdapter`, `ExecutionClient`, protocol parity, capability gates).
2. **Task 2 (Agent D)**: Operator confirmation security lifecycle & native VS Code modal dialog (`OperatorConfirmationModal`, human-facing summary, test injection, parameter binding).
3. **Task 3 (Agent C)**: Shared task/run state & streaming projection (`TaskExecutionStore`, event mapping, projection lifecycle, zero fabrication metrics).
4. **Task 4 (Agent A)**: Chat composer & execution settings UI (`ChatHtmlBuilder`, `ChatWebviewViewProvider`, selectors, multiline composer, execution summary, live streaming pane, honest disabled states).
5. **Task 5 (Agent E)**: Comprehensive safety, regression, Extension Host & accessibility test suite.
6. **Task 6 (Agent F)**: Independent architecture & security audit against Golden Rules.
7. **Task 7 (Agent G)**: Documentation artifacts (01-07 in `docs/reviews/M15-UI-01/`), branch push, Draft PR creation, and CI validation.
