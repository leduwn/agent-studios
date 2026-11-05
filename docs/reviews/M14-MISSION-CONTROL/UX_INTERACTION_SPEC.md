# UX & Interaction Design Specification: Mission Control
**Milestone:** M14 Mission Control Workbench  
**Status:** Approved Architectural Document  
**Date:** 2026-10-30  

---

## 1. Visual Hierarchy & Spatial Layout

Mission Control provides an expansive, dense, yet organized engineering dashboard within the VS Code editor area.

```
┌────────────────────────────────────────────────────────────────────────────────────────┐
│ [Hubot Icon] Mission Control  [Session: Autonomous Architect]  [Badge: Connected]       │
├────────────────────────────────────────────────────────────────────────────────────────┤
│ [Overview] [Agents] [Tasks] [Runs] [Timeline] [Review] [Approvals] [Providers] [Diag]  │
├────────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                        │
│                                                                                        │
│                               Active Tab Content Viewport                              │
│                                                                                        │
│                                                                                        │
│                                                                                        │
├────────────────────────────────────────────────────────────────────────────────────────┤
│ Status Bar: 4 Agents Active | 12 Tasks Total | Watermark: seq#42 | Latency: 2ms        │
└────────────────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. VS Code Theme Token Integration

To look native across any installed VS Code theme (Dark+, Light+, Monokai, Solarized, High Contrast), the UI binds strictly to standard VS Code CSS variables:

```css
:root {
  --bg-primary: var(--vscode-editor-background);
  --bg-secondary: var(--vscode-sideBar-background);
  --bg-card: var(--vscode-editorWidget-background);
  --bg-hover: var(--vscode-list-hoverBackground);
  --border-color: var(--vscode-widget-border, var(--vscode-editorGroup-border, #333));
  --text-primary: var(--vscode-foreground);
  --text-secondary: var(--vscode-descriptionForeground);
  --focus-border: var(--vscode-focusBorder);
  
  /* Semantic Status Colors */
  --status-running: var(--vscode-progressBar-background, #3794ff);
  --status-success: var(--vscode-testing-iconPassed, #4ec9b0);
  --status-warning: var(--vscode-warningForeground, #cca700);
  --status-error: var(--vscode-errorForeground, #f14c4c);
  --status-neutral: var(--vscode-descriptionForeground, #8c8c8c);
}
```

### High Contrast Support
Under `@media (forced-colors: active)` or high-contrast theme classes:
- Elements render explicit borders using `var(--vscode-contrastBorder)`.
- Background opacity layers are replaced with solid contrasting fills.
- Text contrast ratios strictly satisfy WCAG 2.1 AA ($\ge 4.5:1$).

---

## 3. Responsive Adaptations

| Viewport Width | Layout Behavior |
| :--- | :--- |
| **Wide Screen (> 1200px)** | Multi-column grid for overview cards; full Kanban board (all columns side-by-side); split inspector panel on right. |
| **Standard Editor (800px – 1200px)** | 2-column overview grid; scrollable Kanban board; slide-over inspector drawer. |
| **Narrow / Split View (< 800px)** | Single column stacked cards; tabbed sub-views for Kanban columns; full-width modal inspector. |

---

## 4. Accessibility & Keyboard Navigation

1. **ARIA Roles & States:**
   - Tabs: `role="tablist"` with child `role="tab"` buttons declaring `aria-selected="true|false"` and `aria-controls="panel-{id}"`.
   - Content views: `role="tabpanel"` with `aria-labelledby="tab-{id}"`.
   - Modals and drawers: `role="dialog"` with `aria-modal="true"`.
2. **Keyboard Focus:**
   - Left/Right arrows navigate between tabs.
   - Enter/Space activates the focused tab.
   - Escape closes the active Agent or Run Inspector drawer.
   - Focus rings: Clear 1px solid `var(--vscode-focusBorder)` on `:focus-visible`.
3. **Screen Readers:**
   - All interactive icons (close, refresh, diff, zoom) have explicit `aria-label` or `title` attributes.
   - Live status badges include text alternatives (e.g., `<span class="sr-only">Status: Running</span>`).

---

## 5. Localization & Unicode Support

- Complete UTF-8 encoding support.
- Full compatibility with accented characters, Vietnamese language diacritics (`Tiếng Việt: tác vụ, phê duyệt, nhật ký`), and Unicode symbols.
