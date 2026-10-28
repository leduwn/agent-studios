# Milestone AGS CLI 1.3 — Terminal UX, Polish & Onboarding Verification

- **Milestone**: `AGS CLI 1.3 Security & Reliability Gate + M13 Preparation`
- **Execution Run**: `RUN-20261028-CLI-1.3-gate`
- **Date**: `2026-10-28T07:25:00+07:00`
- **Lead Owner**: Terminal UX Lead & Product Architect

---

## 1. Graceful Unconfigured AI Runtime Onboarding

When a developer runs `ags` in a workspace without configured remote AI runtime credentials (e.g. no `opencode` binary or unconfigured API keys), the CLI launches the **Interactive Onboarding Screen** rather than abruptly exiting with code 1 or faking mock responses.

### Screen Hierarchy & Keybindings

```text
┌────────────────────────────────────────────────────────┐
│  AGENT STUDIOS CLI (ags) — DEVELOPER WORKBENCH          │
├────────────────────────────────────────────────────────┤
│  Workspace: agent-studios-ags-cli (feat/ags-cli-tui...) │
│  Path:      D:\Projects\agent-studios-ags-cli           │
│                                                        │
│  [!] No AI runtime backend currently available         │
│                                                        │
│  Options:                                              │
│    [D] Launch Simulated Demo Mode                      │
│    [P] Launch Offline Design Previewer                 │
│    [R] Run System Doctor Diagnostics                   │
│    [T] Toggle Theme (Dark / Light)                     │
│    [Q] Exit                                            │
│                                                        │
│  To configure OpenCode runtime:                        │
│    1. Install opencode binary in your PATH             │
│    2. Export ANTHROPIC_API_KEY or OPENAI_API_KEY       │
│    3. Re-run `ags` to begin live multi-agent execution │
└────────────────────────────────────────────────────────┘
```

- Key `D` / `d`: Switches into `--demo` mode (runs `DeterministicLocalRuntime` for offline exploration).
- Key `P` / `p`: Launches the offline Ratatui design system previewer.
- Key `R` / `r`: Executes the comprehensive `doctor` diagnostics report.
- Key `T` / `t`: Toggles between dark and light themes with dynamic token contrast recalculation.
- Key `Q` / `Esc` / `Ctrl+C`: Gracefully restores terminal state, unhides cursor, and exits with code 0.

---

## 2. Truthful Execution Mode & Token Transparency

1. **Explicit Mode Indicators**:
   - `[LIVE]`: Only displayed when an external AI runtime (e.g. OpenCode) is actively connected with verified model credentials.
   - `[DEMO]`: Displayed when running simulated deterministic scenarios via explicit `--demo` flag.
   - `[PREVIEW]`: Displayed when evaluating static UI widgets via `--preview`.
2. **Truthful Token Reporting**:
   - Displays `"tokens: unknown"` whenever token measurement is unmeasured or zero, eliminating developer confusion over free or 0-token executions.

---

## 3. Streaming Chunk Coalescing & Scroll Lock Pinning

1. **Chunk Coalescing**:
   - Consecutive `UiEvent::AssistantChunk` events coalesce into the active `TranscriptItem::AssistantMessage` block without line fragmentation or visual flickering.
2. **Adaptive Scroll Lock**:
   - Viewport dynamically sticks to the latest streaming tokens at the bottom while `scroll_offset == 0`.
   - `PageUp` smoothly unpins the viewport to allow reviewing transcript history without being yanked back by newly arriving tokens. `PageDown` smoothly returns to bottom lock.

---

## 4. Two-Stage Ctrl+C Semantics with Visible Action Error Feedback

1. **Stage 1 (Text in Composer)**:
   - Clears the composer text buffer and resets the cursor grapheme index. Does not cancel active runs or exit.
2. **Stage 2 (Empty Composer with Active Task)**:
   - Dispatches `cancel_active_run` to the runtime client.
   - If cancellation succeeds: Posts a warning notice in the transcript: `"Run cancelled. Press Ctrl+C again within 1.5s to exit."`.
   - If cancellation fails: Renders an error notice in the transcript: `"Action failed: {e}"` without silent error drops.
3. **Stage 3 (Idle or Second Press within 1.5s)**:
   - Sets `should_quit = true` for clean terminal restoration.

---

## 5. Vietnamese Unicode NFC & Responsive Breakpoints

- Unicode NFC normalization ensures tone marks in Vietnamese text (`Tiếng Việt`, `hướng dẫn`, `kiểm tra`) render without diacritic clipping or grapheme width distortion across all layout breakpoints (`Compact`, `Standard`, `Wide`).
