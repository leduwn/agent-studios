# Agent Studios — Milestone M14-UI-01: Visual QA Specification

## Visual Design Philosophy
Agent Studios M14-UI-01 implements a **clean, monochrome developer tool aesthetic** consistent with Code-OSS and VS Code design principles. Rather than using loud brand gradients or intrusive accents, the interface relies on native editor surface tokens, subtle 1px border delineations, and functional semantic status colors (green for success, amber for warnings, red for errors, muted gray for inactive).

---

## 1. Visual Layout Specifications

```
+-----------------------------------------------------------------------+
|  ⚡ Agent Studios  [Preview Fixture]                (• Connected) [v] |  <- Header (44px, sticky top)
+-----------------------------------------------------------------------+
|                                                                       |
|  You                                                            AGENT |  <- User Message Bubble
|  Refactor authentication middleware to use constant-time compare.     |
|  [📄 auth.ts]                                                         |
|                                                                       |
|  ⚡ Agent Studios                                               AGENT |  <- Assistant Bubble
|  Analyzing `src/auth.ts` for cryptographic vulnerabilities.           |
|                                                                       |
|  +-----------------------------------------------------------------+  |
|  | [🔧 Read File] src/auth.ts                 (completed)   42 ms  |  |  <- ToolCallCard
|  +-----------------------------------------------------------------+  |
|                                                                       |
|  Proposed constant-time string comparison in `src/auth.ts`:           |
|                                                                       |
|  +-----------------------------------------------------------------+  |
|  | Changed Files (1 file)                             +12 / -4     |  |  <- ChangedFilesCard
|  | - src/auth.ts                                      +12 / -4     |  |
|  | [Inspect Diff in Working Tree]                                  |  |
|  +-----------------------------------------------------------------+  |
|                                                                       |
|  +-----------------------------------------------------------------+  |
|  | [APPROVAL REQUIRED — HIGH RISK]                                 |  |  <- ApprovalCard
|  | Target: apply_patch: src/auth.ts                                |  |
|  | [Approve]  [Deny]                                               |  |
|  +-----------------------------------------------------------------+  |
|                                                                       |
+-----------------------------------------------------------------------+
|  [📄 src/auth.ts (lines 10-45) (x)]  [+ Add File] [Attach Active]    |  <- Context Chips Bar
+-----------------------------------------------------------------------+
|  +-----------------------------------------------------------------+  |
|  | Ask Agent Studios or type '/' for commands...                   |  |  <- Composer Textarea
|  |                                                                 |  |     (min 40px, max 150px)
|  |                                                                 |  |
|  | [Agent (v)]  [Claude 3.5 Sonnet (200k) (v)]      [Stop]  [Send] |  |  <- Action Toolbar
|  +-----------------------------------------------------------------+  |
+-----------------------------------------------------------------------+
```

---

## 2. Responsive Width Adaptations

The sidebar webview accommodates widths ranging from compact sidebars (300px) to wide secondary sidebars (600px+) and editor tab panels:

### 2.1 Narrow Layout (`<= 300px`)
- **Header**: Session dropdown drops extra padding; session titles truncate with ellipsis (`overflow: hidden; text-overflow: ellipsis; white-space: nowrap;`).
- **Context Chips**: Wrap onto multiple lines (`flex-wrap: wrap`); chip labels truncate at 120px.
- **Action Toolbar**:
  - Mode selector dropdown and Model selector dropdown wrap to line 1.
  - Send button and Stop button expand across line 2 for easy tapping/clicking.
- **Cards**: Footer action buttons stack vertically to prevent text overflow.

### 2.2 Standard Layout (`301px - 599px`)
- Standard sidebar width in default VS Code layouts.
- Header, conversation, and composer display in single-line horizontal toolbars.
- Mode selector and Model selector display side-by-side.
- Send and Stop buttons display on the far right.

### 2.3 Wide Layout (`>= 600px` / Editor Panel)
- **Container**: Max content width can center or expand smoothly.
- **Message Bubbles**: Max-width capped at 85% of container width to preserve readable line lengths (avoiding 120-character line scans).
- **Cards**: Two-column layout for metadata and actions where appropriate.

---

## 3. Light & Dark Theme Compliance

### 3.1 Token Mapping Table

| UI Element | CSS Variable Token | Dark Theme Value (Fallback) | Light Theme Value (Fallback) |
|---|---|---|---|
| Webview Background | `--vscode-sideBar-background` | `#1e1e1e` | `#f3f3f3` |
| Card Background | `--vscode-editorWidget-background` | `#252526` | `#ffffff` |
| Card Border | `--vscode-editorWidget-border` | `#3c3c3c` | `#e5e5e5` |
| Primary Text | `--vscode-editor-foreground` | `#cccccc` | `#333333` |
| Secondary / Muted Text | `--vscode-descriptionForeground` | `#858585` | `#717171` |
| Primary Action Button | `--vscode-button-background` | `#0e639c` | `#007acc` |
| Secondary Action Button | `--vscode-button-secondaryBackground`| `#3a3d41` | `#e8e8e8` |
| Success Badge (Passed) | `--vscode-testing-iconPassed` | `#73c991` | `#388a34` |
| Error Badge (Failed) | `--vscode-testing-iconFailed` | `#f14c4c` | `#e51400` |
| Warning Badge (Pending) | `--vscode-editorWarning-foreground` | `#cca700` | `#bf8803` |

### 3.2 Visual Contrast Rules
- All text meets a minimum contrast ratio of 4.5:1 against its background.
- Code blocks in assistant messages use `--vscode-textCodeBlock-background` with 1px border.
- Diff additions display subtle green tint (`rgba(115, 201, 145, 0.1)`) and deletions display subtle red tint (`rgba(241, 76, 76, 0.1)`), ensuring legibility in both light and dark modes.

---

## 4. Visual QA Checklist

| Test Item | Verification Criteria | Status |
|---|---|---|
| **Header Layout** | Title, brand icon, connection dot, and session switcher align horizontally without clipping | PASS |
| **Preview Fixture Tag** | `[Preview Fixture]` tag displays with muted border when in preview mode | PASS |
| **Composer Auto-Grow** | Textarea starts at 40px, expands smoothly on multiline input, and stops growing at 150px with scrollbar | PASS |
| **Context Chips** | Chip displays file icon, truncated path, and remove button; hover highlights border | PASS |
| **Streaming Cursor** | Blinking cursor displays immediately on stream start and disappears cleanly on message complete | PASS |
| **Tool Call Cards** | Displays tool icon, title, duration badge, and expandable input/output disclosure | PASS |
| **Approval Cards** | Risk badge color-coded (LOW=gray, MEDIUM=blue, HIGH=yellow, CRITICAL=red); buttons clear and responsive | PASS |
| **Changed Files Card** | +X in green, -Y in red; one-click diff button fires `agent-studios.inspectArtifact` | PASS |
| **Stop Button** | Visible only while agent is executing; red background with clear "Stop" label | PASS |
| **High Contrast Mode** | Borders remain crisp (2px solid) when tested in High Contrast Dark theme | PASS |
