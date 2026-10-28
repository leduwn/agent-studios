# Milestone AGS CLI Foundation 1.2 — Terminal UX & Polish Verification

- **Milestone**: `AGS CLI Foundation 1.2 — Truthful Multi-Agent Runtime & Resilient TUI`
- **Execution Run**: `RUN-20261028-CLI-1.2-foundation`
- **Date**: `2026-10-28T02:35:00+07:00`
- **Lead Owner**: Terminal UX Lead

---

## 1. Streaming Assistant Chunk Coalescing

In `AgsApp::handle_ui_event`:
- Sequential `UiEvent::AssistantChunk` tokens append directly to the active `TranscriptItem::AssistantMessage` string instead of spawning disjoint line items per token.
- Seamless rendering of high-frequency streaming LLM generation without visual flicker or disjoint line fragments.

## 2. Scroll Lock Pinning

In `TranscriptWidget`:
- When `scroll_offset == 0`, viewport height dynamically locks to the latest emitted stream tokens at the bottom.
- When `scroll_offset > 0` (via `PageUp`), viewport unpins and allows inspecting historical scrollback without being yanked to the bottom by incoming tokens. `PageDown` smoothly returns to bottom lock.

## 3. Two-Stage Ctrl+C Semantics

In `AgsApp::handle_key`:
1. **Stage 1 (Text in Composer)**: Clears input text and resets cursor grapheme index. Does not cancel run or exit.
2. **Stage 2 (Empty Composer with Active Task)**: Sends cancellation request to the runtime client (`cancel_active_run`) and warns: `"Run cancelled. Press Ctrl+C again within 1.5s to exit."`.
3. **Clean Quit**: If idle or pressed a second time within 1.5s, sets `should_quit = true` for clean terminal teardown.

## 4. Status Bar Mode & Token Metrics

- Mode indicators explicitly report runtime state: `[LIVE]`, `[DEMO]`, or `[PREVIEW]`.
- Token counters report `"tokens: unknown"` when zero usage has been measured rather than misleading developers into assuming a free 0-token call.
- Full support for Vietnamese Unicode NFC graphemes across composer inputs and transcript displays.
