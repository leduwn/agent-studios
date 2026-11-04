# Agent Studios — Milestone M14-UI-01: Accessibility Review

## Accessibility Compliance Summary
Agent Studios M14-UI-01 adheres to WCAG 2.1 Level AA accessibility standards, VS Code extension accessibility guidelines, and system assistive technology conventions.

---

## 1. ARIA Roles, Semantics & Screen Reader Support

### 1.1 Landmark Roles
- **Header**: Declared with `role="banner"` to identify the assistant top bar, session switcher, and connection status indicator.
- **Conversation Stream**: Container declared with `role="log"` and `aria-live="polite"`. As stream chunks, tool call updates, and assistant messages arrive, screen readers announce updates without interrupting the user's active keyboard navigation.
- **Composer Toolbar**: Form and controls declared with explicit `aria-label` attributes.

### 1.2 Control Semantics

| Component | ARIA Role / Attributes | Description |
|---|---|---|
| **Mode Selector** | `role="radiogroup"` with `aria-label="Agent execution mode selection"` | Groups the 3 modes (Ask, Plan, Agent). |
| **Mode Option Pill** | `role="radio"`, `aria-checked="true/false"`, `aria-label="<Name> mode - <Description>"` | Communicates selection state and capability overview. |
| **Model Dropdown** | `role="combobox"`, `aria-label="Select AI Model and Provider"` | Identifies the dropdown as a searchable/selectable model combobox. |
| **Context Chip** | `title="<full path>"`, button has `aria-label="Remove <filename>"` | Screen readers explicitly identify which file will be removed. |
| **Tool Call Summary** | `<details>` and `<summary>` native semantic HTML | Supported out-of-the-box by screen readers for folding/disclosure. |
| **Approval Card** | `aria-label="Human approval required: <command>"`, button `aria-label="Approve action <id>"` | Clearly announces the required intervention and risk level. |

---

## 2. Keyboard Navigation & Focus Management

### 2.1 Logical Tab Order
The tab sequence proceeds in a natural and predictable top-to-bottom order:
1. Session switcher dropdown (`#sessionHistorySelect`)
2. New session button (`#newSessionBtn`)
3. Message stream action buttons (Approve, Deny, Open Diff, View Task Graph)
4. Context chips remove buttons (`.chip-remove-btn`)
5. Context add buttons (`#addContextBtn`, `#addActiveEditorBtn`)
6. Composer textarea (`#composerInput`)
7. Mode selector dropdown (`#modeSelect`)
8. Model selector dropdown (`#modelSelect`)
9. Stop button (`#stopBtn` when active)
10. Send prompt button (`#sendBtn`)

### 2.2 Focus Restoration
- When a context chip is removed using its remove button, keyboard focus is smoothly redirected to the Composer textarea (`composerInput.focus()`), preventing focus loss or resetting to document body.
- When a prompt is submitted via `Enter` or clicking Send, focus remains anchored in the composer for immediate follow-up input.

### 2.3 Focus Visible Indicators
All interactive elements (buttons, inputs, dropdowns, mode pills, chips) include explicit focus-visible styles mapped directly to the VS Code native focus border token:

```css
:focus-visible {
  outline: 1px solid var(--vscode-focusBorder, #007acc) !important;
  outline-offset: 1px !important;
}
```

---

## 3. High-Contrast Themes & System Forced-Colors

### 3.1 Theme Variable Conformance
All colors, backgrounds, borders, and typography utilize native VS Code CSS custom properties that adapt automatically when switching between Light, Dark, High Contrast Dark, and High Contrast Light:
- `--vscode-editor-foreground`
- `--vscode-editorWidget-background`
- `--vscode-editorWidget-border`
- `--vscode-focusBorder`
- `--vscode-badge-background` / `--vscode-badge-foreground`
- `--vscode-testing-iconPassed` / `--vscode-testing-iconFailed`
- `--vscode-editorWarning-foreground`

### 3.2 `@media (forced-colors: active)` Mode
For Windows High Contrast and system-level forced-color accessibility modes:
```css
@media (forced-colors: active) {
  .agent-studios-card,
  .composer-box,
  .context-chip,
  .btn-primary,
  .btn-secondary {
    border: 2px solid ButtonText !important;
  }

  .status-dot {
    forced-color-adjust: none;
    border: 1px solid ButtonText;
  }
}
```
This ensures clear boundaries around cards, buttons, and status dots when custom color schemes are overridden by accessibility utilities.

---

## 4. Reduced Motion Support

For users sensitive to motion or animation:
```css
@media (prefers-reduced-motion: reduce) {
  *,
  *::before,
  *::after {
    animation-duration: 0.01ms !important;
    animation-iteration-count: 1 !important;
    transition-duration: 0.01ms !important;
  }

  .agent-studios-spin {
    animation: none !important;
  }

  .streaming-cursor {
    animation: none !important;
    opacity: 1 !important;
  }
}
```
Animated spinners and blinking cursors halt motion, displaying steady status indicators instead.
