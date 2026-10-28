# Agent Studios CLI (`ags`) — Visual Design System & Semantic Tokens

> **Document**: `AGS_CLI_VISUAL_DESIGN_SYSTEM.md`  
> **Status**: Canonical Visual Language & Token Specification (Audited & Honesty-Tagged)  
> **Target Subsystem**: `ags` / Ratatui Theme Engine / `agent-studios-tui` Crate  
> **Precedence**: Aligned with `PRODUCT_PRINCIPLES.md` (Principles #26, #27).  
> **Audit Status Legend**:
> - `[IMPLEMENTED / VERIFIED]`: Fully implemented in code and verified by automated tests.
> - `[IMPLEMENTED / NOT VERIFIED]`: Implemented in code but awaiting live remote / daemon verification.
> - `[PREVIEW ONLY]`: Visual/interaction design demonstrable in `ags --preview` with sample data.
> - `[PLANNED]`: Architectural design finalized; scheduled for Milestone M13+.
> - `[BLOCKED]`: Implementation blocked on external dependency.

---

## 1. Visual Design Philosophy: Quiet Precision

The visual identity of `ags` is rooted in **Quiet Precision**—a high-contrast, distraction-free aesthetic designed for developer focus. It discards the sci-fi tropes of AI tools (pulsing gradients, excessive borders, dense status matrices) in favor of typography, whitespace, and functional color hierarchy.

### Core Principles
1. **Color Has Meaning**: Color is never decorative. Green means verified or added; amber means caution or review required; red means failure or destructive action; blue/cyan means active reasoning or informational context.
2. **Monochrome Dominance**: 85% of the visual space is composed of neutral black, carbon, gray, and white tones.
3. **Typography First**: Hierarchy is established through indentation, font weight (bold/dim), and glyph signifiers rather than rainbow colors.
4. **Resilient Degradation**: The design system degrades gracefully across TrueColor (24-bit), ANSI 256, ANSI 16, and strict `NO_COLOR` environments.

---

## 2. Semantic Token System

All widgets in `ags` consume semantic tokens defined in `AgsTheme`. Direct hardcoded hex values in UI widgets are strictly forbidden.

### 2.1 Token Categories

```text
                  ┌───────────────────────────────┐
                  │           AgsTheme            │
                  └───────────────┬───────────────┘
         ┌────────────────────────┼────────────────────────┐
         ▼                        ▼                        ▼
  Surfaces & Borders        Text & Hierarchy          Semantic & Diff
  • SurfaceBase             • TextPrimary             • StatusSuccess
  • SurfaceCard             • TextSecondary           • StatusWarning
  • SurfaceElevated         • TextMuted               • StatusError
  • BorderBase              • TextInverted            • StatusInfo
  • BorderSubtle            • TextAccent              • StatusRunning
  • BorderFocus             • TextCode                • DiffAdded / Removed
```

---

## 3. Theme Specifications

### 3.1 Theme 1: `AGS Dark` (Default)
Engineered for dark terminal environments (Windows Terminal Dark, Alacritty, iTerm2, WezTerm). Deep carbon base with high-contrast text and studio emerald accents.

| Token | Semantic Purpose | 24-bit Hex | ANSI 256 | ANSI 16 | Ratatui Style |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `surface_base` | Main terminal background | `#0d1117` | `233` | `Black` | `Color::Rgb(13, 17, 23)` |
| `surface_card` | Tool cards, composer box | `#161b22` | `234` | `Black` | `Color::Rgb(22, 27, 34)` |
| `surface_elevated`| Modals, floating popups | `#21262d` | `235` | `DarkGray` | `Color::Rgb(33, 38, 45)` |
| `border_base` | Primary container borders | `#30363d` | `237` | `DarkGray` | `Color::Rgb(48, 54, 61)` |
| `border_subtle` | Dividers, collapsed cards | `#21262d` | `235` | `DarkGray` | `Color::Rgb(33, 38, 45)` |
| `border_focus` | Active composer, focused card| `#3fb950` | `71` | `Green` | `Color::Rgb(63, 185, 80)` |
| `border_danger` | Destructive modal border | `#f85149` | `203` | `Red` | `Color::Rgb(248, 81, 73)` |
| `text_primary` | Standard user/assistant text | `#e6edf3` | `254` | `White` | `Color::Rgb(230, 237, 243)` |
| `text_secondary` | Headers, labels, paths | `#8b949e` | `245` | `Gray` | `Color::Rgb(139, 148, 158)` |
| `text_muted` | Timestamps, hints, shortcuts | `#484f58` | `239` | `DarkGray` | `Color::Rgb(72, 79, 88)` |
| `text_inverted` | Text on active badge/highlight| `#0d1117` | `233` | `Black` | `Color::Rgb(13, 17, 23)` |
| `brand_accent` | Studio Emerald signature mark| `#2ea043` | `35` | `Green` | `Color::Rgb(46, 160, 67)` |
| `status_success` | Passed tests, approved tools | `#3fb950` | `71` | `Green` | `Color::Rgb(63, 185, 80)` |
| `status_warning` | Review needed, dirty tree | `#d29922` | `178` | `Yellow` | `Color::Rgb(210, 153, 34)` |
| `status_error` | Errors, failed tests, denied | `#f85149` | `203` | `Red` | `Color::Rgb(248, 81, 73)` |
| `status_info` | Model info, hints, notes | `#58a6ff` | `75` | `Blue` | `Color::Rgb(88, 166, 255)` |
| `status_running` | Active reasoning / tool spin | `#a371f7` | `135` | `Magenta` | `Color::Rgb(163, 113, 247)` |
| `diff_added` | Inserted diff lines text | `#3fb950` | `71` | `Green` | `Color::Rgb(63, 185, 80)` |
| `diff_added_bg` | Inserted diff background | `#13231b` | `234` | `Black` | `Color::Rgb(19, 35, 27)` |
| `diff_removed` | Deleted diff lines text | `#f85149` | `203` | `Red` | `Color::Rgb(248, 81, 73)` |
| `diff_removed_bg`| Deleted diff background | `#271418` | `234` | `Black` | `Color::Rgb(39, 20, 24)` |
| `diff_hunk_hdr` | Unified diff `@@ -a,b +c,d @@` | `#79c0ff` | `111` | `Cyan` | `Color::Rgb(121, 192, 255)` |

---

### 3.2 Theme 2: `AGS Light`
Engineered for light terminal environments or high-ambient-light settings. Clean paper canvas with deep obsidian text and forest emerald accents.

| Token | Semantic Purpose | 24-bit Hex | ANSI 256 | ANSI 16 | Ratatui Style |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `surface_base` | Main terminal background | `#ffffff` | `231` | `White` | `Color::Rgb(255, 255, 255)` |
| `surface_card` | Tool cards, composer box | `#f6f8fa` | `255` | `White` | `Color::Rgb(246, 248, 250)` |
| `surface_elevated`| Modals, floating popups | `#eaedf0` | `254` | `Gray` | `Color::Rgb(234, 237, 240)` |
| `border_base` | Primary container borders | `#d0d7de` | `250` | `Gray` | `Color::Rgb(208, 215, 222)` |
| `border_subtle` | Dividers, collapsed cards | `#e1e4e8` | `253` | `Gray` | `Color::Rgb(225, 228, 232)` |
| `border_focus` | Active composer, focused card| `#1a7f37` | `28` | `Green` | `Color::Rgb(26, 127, 55)` |
| `border_danger` | Destructive modal border | `#cf222e` | `160` | `Red` | `Color::Rgb(207, 34, 46)` |
| `text_primary` | Standard user/assistant text | `#1f2328` | `234` | `Black` | `Color::Rgb(31, 35, 40)` |
| `text_secondary` | Headers, labels, paths | `#656d76` | `242` | `DarkGray` | `Color::Rgb(101, 109, 118)` |
| `text_muted` | Timestamps, hints, shortcuts | `#8c959f` | `246` | `DarkGray` | `Color::Rgb(140, 149, 159)` |
| `text_inverted` | Text on active badge/highlight| `#ffffff` | `231` | `White` | `Color::Rgb(255, 255, 255)` |
| `brand_accent` | Studio Forest Emerald | `#1a7f37` | `28` | `Green` | `Color::Rgb(26, 127, 55)` |
| `status_success` | Passed tests, approved tools | `#1a7f37` | `28` | `Green` | `Color::Rgb(26, 127, 55)` |
| `status_warning` | Review needed, dirty tree | `#9a6700` | `136` | `Yellow` | `Color::Rgb(154, 103, 0)` |
| `status_error` | Errors, failed tests, denied | `#cf222e` | `160` | `Red` | `Color::Rgb(207, 34, 46)` |
| `status_info` | Model info, hints, notes | `#0969da` | `26` | `Blue` | `Color::Rgb(9, 105, 218)` |
| `status_running` | Active reasoning / tool spin | `#8250df` | `98` | `Magenta` | `Color::Rgb(130, 80, 223)` |
| `diff_added` | Inserted diff lines text | `#1a7f37` | `28` | `Green` | `Color::Rgb(26, 127, 55)` |
| `diff_added_bg` | Inserted diff background | `#e6ffec` | `255` | `White` | `Color::Rgb(230, 255, 236)` |
| `diff_removed` | Deleted diff lines text | `#cf222e` | `160` | `Red` | `Color::Rgb(207, 34, 46)` |
| `diff_removed_bg`| Deleted diff background | `#ffebe9` | `255` | `White` | `Color::Rgb(255, 235, 233)` |
| `diff_hunk_hdr` | Unified diff `@@ -a,b +c,d @@` | `#0969da` | `26` | `Blue` | `Color::Rgb(9, 105, 218)` |

---

### 3.3 Theme 3: `Terminal Adaptive` (No Color / 16-Color ANSI Fallback)
Compliant with the `NO_COLOR` standard (https://no-color.org). When `NO_COLOR` is detected or terminal capability is restricted to basic 16 colors:
- All background colors fall back to terminal default (`Color::Reset`).
- Semantic hierarchy is conveyed via text styling (`Modifier::BOLD`, `Modifier::DIM`, `Modifier::REVERSED`, `Modifier::UNDERLINED`).
- Diff lines use leading signifiers `+` and `-` with optional bold.

| Token | Fallback 16-Color ANSI | Fallback Modifiers | Fallback ASCII Indicator |
| :--- | :--- | :--- | :--- |
| `surface_base` | `Color::Reset` | `Modifier::empty()` | (transparent) |
| `surface_card` | `Color::Reset` | `Modifier::empty()` | Enclosed in single lines |
| `surface_elevated`| `Color::Reset` | `Modifier::REVERSED` | Double border |
| `border_base` | `Color::DarkGray` / `White` | `Modifier::empty()` | `│ ─ ┌ ┐ └ ┘` |
| `border_focus` | `Color::Green` / `White` | `Modifier::BOLD` | `┃ ━ ┏ ┓ ┗ ┛` |
| `text_primary` | `Color::Reset` | `Modifier::empty()` | Standard text |
| `text_secondary` | `Color::White` | `Modifier::empty()` | Standard text |
| `text_muted` | `Color::DarkGray` | `Modifier::DIM` | `[dimmed]` |
| `text_inverted` | `Color::Black` | `Modifier::REVERSED` | Highlighted text |
| `brand_accent` | `Color::Green` | `Modifier::BOLD` | `[ags]` |
| `status_success` | `Color::Green` | `Modifier::BOLD` | `[OK]` or `[v]` |
| `status_warning` | `Color::Yellow` | `Modifier::BOLD` | `[WARN]` or `[!]` |
| `status_error` | `Color::Red` | `Modifier::BOLD` | `[ERR]` or `[X]` |
| `status_info` | `Color::Cyan` | `Modifier::empty()` | `[INFO]` or `[i]` |
| `status_running` | `Color::Magenta` | `Modifier::DIM` | `[*]` or `[..]` |
| `diff_added` | `Color::Green` | `Modifier::empty()` | `+ ` prefix |
| `diff_removed` | `Color::Red` | `Modifier::empty()` | `- ` prefix |
| `diff_hunk_hdr` | `Color::Cyan` | `Modifier::DIM` | `@@ ... @@` |

---

## 4. Typography, Glyphs & Unicode Architecture

### 4.1 Iconography & Fallback Hierarchy
`ags` prefers modern Nerd Font icons when available, but automatically verifies terminal glyph capability at startup (`terminal-probe`) and provides seamless ASCII fallbacks.

| Symbol Concept | Nerd Font Glyph | Unicode Standard Glyph | Strict ASCII Fallback | Description |
| :--- | :---: | :---: | :---: | :--- |
| **Studio Mark** | `󰘳 ags` | `[ags]` | `ags>` | Command Prompt Brand Mark |
| **Git Branch** | `󰊢` | `⎇` | `branch:` | Active Git checkout |
| **Clean Tree** | `✓` | `✓` | `clean` | No uncommitted changes |
| **Dirty Tree** | `●` | `*` | `*` | Working tree has modifications |
| **Provider** | `󰍛` | `⚡` | `prov:` | Provider identifier (Anthropic/OpenAI) |
| **Model** | `󱥤` | `◆` | `model:` | Specific model target |
| **Worktree** | `󰈚` | `⊞` | `[wt]` | Isolated workspace directory |
| **Tool Call** | `` | `▶` | `>` | Invoked tool indicator |
| **Tool Succeeded** | `✓` | `✓` | `+` | Successful tool run |
| **Tool Failed** | `✗` | `✗` | `!` | Errored tool run |
| **Agent Active** | `●` | `●` | `*` | Agent currently executing |
| **Agent Idle** | `○` | `○` | `-` | Agent registered / waiting |
| **Keyboard Key** | `󰌌` | `⌨` | `Key:` | Keyboard shortcut hint |

### 4.2 Box-Drawing Characters
`ags` utilizes precise single-line box drawing for clean, airy layout boundaries.

```text
Thin Unicode (Default):
┌─────────────────────────┐
│ Card Content            │
├─────────────────────────┤
│ Footer Meta             │
└─────────────────────────┘

Rounded Unicode (Optional / macOS & Linux Terminals):
╭─────────────────────────╮
│ Card Content            │
╰─────────────────────────╯

Strict ASCII (Fallback when utf8_box = false):
+-------------------------+
| Card Content            |
+-------------------------+
| Footer Meta             |
+-------------------------+
```

### 4.3 Vietnamese Unicode & Typography Architecture
Per Product Principle #26 and `LOCALIZATION.md`, Vietnamese is a first-class language. Handling Vietnamese in terminal emulators requires strict adherence to text measurement rules:

1. **NFC Normalization**: All incoming user input and localization strings are normalized to Unicode Form C (NFC) via `unicode-normalization`. This prevents decomposed vowel+tone sequences (e.g., `a` + combining acute) from rendering as two disparate cells.
2. **Grapheme Cluster Segmentation**: Cursor positioning and line wrapping MUST use `unicode-segmentation` (`UnicodeSegmentation::graphemes(s, true)`). Never count raw `chars` or bytes.
3. **Double-Width Awareness**: String measurement in Ratatui uses `unicode-width` (`UnicodeWidthStr::width(s)`). Vietnamese diacritic combinations (`á, ắ, ặ, ấ, ầ, ổ, ợ, ỹ...`) are measured as width 1 in standard monospace fonts, while CJK or certain glyph icons are measured as width 2.
4. **No Accent Clipping**: Input box vertical padding ensures tone marks above uppercase letters (e.g., `Đ`, `Ổ`, `Ấ`) are never cut off by box borders.

---

## 5. Ratatui Rust Architecture Implementation

```rust
// ags-theme/src/palette.rs

use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone, Copy)]
pub struct AgsTheme {
    pub surface_base: Color,
    pub surface_card: Color,
    pub surface_elevated: Color,
    pub border_base: Color,
    pub border_subtle: Color,
    pub border_focus: Color,
    pub border_danger: Color,
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_muted: Color,
    pub text_inverted: Color,
    pub brand_accent: Color,
    pub status_success: Color,
    pub status_warning: Color,
    pub status_error: Color,
    pub status_info: Color,
    pub status_running: Color,
    pub diff_added: Color,
    pub diff_added_bg: Color,
    pub diff_removed: Color,
    pub diff_removed_bg: Color,
    pub diff_hunk_hdr: Color,
    pub is_no_color: bool,
}

impl AgsTheme {
    pub fn dark() -> Self {
        Self {
            surface_base: Color::Rgb(13, 17, 23),
            surface_card: Color::Rgb(22, 27, 34),
            surface_elevated: Color::Rgb(33, 38, 45),
            border_base: Color::Rgb(48, 54, 61),
            border_subtle: Color::Rgb(33, 38, 45),
            border_focus: Color::Rgb(63, 185, 80),
            border_danger: Color::Rgb(248, 81, 73),
            text_primary: Color::Rgb(230, 237, 243),
            text_secondary: Color::Rgb(139, 148, 158),
            text_muted: Color::Rgb(72, 79, 88),
            text_inverted: Color::Rgb(13, 17, 23),
            brand_accent: Color::Rgb(46, 160, 67),
            status_success: Color::Rgb(63, 185, 80),
            status_warning: Color::Rgb(210, 153, 34),
            status_error: Color::Rgb(248, 81, 73),
            status_info: Color::Rgb(88, 166, 255),
            status_running: Color::Rgb(163, 113, 247),
            diff_added: Color::Rgb(63, 185, 80),
            diff_added_bg: Color::Rgb(19, 35, 27),
            diff_removed: Color::Rgb(248, 81, 73),
            diff_removed_bg: Color::Rgb(39, 20, 24),
            diff_hunk_hdr: Color::Rgb(121, 192, 255),
            is_no_color: false,
        }
    }

    pub fn light() -> Self {
        Self {
            surface_base: Color::Rgb(255, 255, 255),
            surface_card: Color::Rgb(246, 248, 250),
            surface_elevated: Color::Rgb(234, 237, 240),
            border_base: Color::Rgb(208, 215, 222),
            border_subtle: Color::Rgb(225, 228, 232),
            border_focus: Color::Rgb(26, 127, 55),
            border_danger: Color::Rgb(207, 34, 46),
            text_primary: Color::Rgb(31, 35, 40),
            text_secondary: Color::Rgb(101, 109, 118),
            text_muted: Color::Rgb(140, 149, 159),
            text_inverted: Color::Rgb(255, 255, 255),
            brand_accent: Color::Rgb(26, 127, 55),
            status_success: Color::Rgb(26, 127, 55),
            status_warning: Color::Rgb(154, 103, 0),
            status_error: Color::Rgb(207, 34, 46),
            status_info: Color::Rgb(9, 105, 218),
            status_running: Color::Rgb(130, 80, 223),
            diff_added: Color::Rgb(26, 127, 55),
            diff_added_bg: Color::Rgb(230, 255, 236),
            diff_removed: Color::Rgb(207, 34, 46),
            diff_removed_bg: Color::Rgb(255, 235, 233),
            diff_hunk_hdr: Color::Rgb(9, 105, 218),
            is_no_color: false,
        }
    }

    pub fn no_color() -> Self {
        Self {
            surface_base: Color::Reset,
            surface_card: Color::Reset,
            surface_elevated: Color::Reset,
            border_base: Color::Reset,
            border_subtle: Color::Reset,
            border_focus: Color::Reset,
            border_danger: Color::Reset,
            text_primary: Color::Reset,
            text_secondary: Color::Reset,
            text_muted: Color::Reset,
            text_inverted: Color::Reset,
            brand_accent: Color::Reset,
            status_success: Color::Reset,
            status_warning: Color::Reset,
            status_error: Color::Reset,
            status_info: Color::Reset,
            status_running: Color::Reset,
            diff_added: Color::Reset,
            diff_added_bg: Color::Reset,
            diff_removed: Color::Reset,
            diff_removed_bg: Color::Reset,
            diff_hunk_hdr: Color::Reset,
            is_no_color: true,
        }
    }
}
```

---

## 6. Visual Design System Feature-to-Test Evidence Matrix

| Visual Design Feature | Honesty Status | Implementation File | Verification Test Module |
| :--- | :--- | :--- | :--- |
| **Dark Theme Tokens** (`AgsTheme::dark()`) | `[IMPLEMENTED / VERIFIED]` | `crates/tui/src/theme.rs` | `crates/tui/tests/widget_tests.rs::test_theme_tokens_contrast` |
| **Light Theme Tokens** (`AgsTheme::light()`) | `[IMPLEMENTED / VERIFIED]` | `crates/tui/src/theme.rs` | `crates/tui/tests/widget_tests.rs::test_theme_tokens_contrast` |
| **ANSI 16 Fallback** (`AgsTheme::ansi16()`) | `[IMPLEMENTED / VERIFIED]` | `crates/tui/src/theme.rs` | `crates/tui/tests/widget_tests.rs::test_theme_tokens_contrast` |
| **NO_COLOR Standard** (`AgsTheme::no_color()`) | `[IMPLEMENTED / VERIFIED]` | `crates/tui/src/theme.rs` | `crates/tui/tests/widget_tests.rs::test_theme_tokens_contrast` |
| **Vietnamese NFC Normalization** | `[IMPLEMENTED / VERIFIED]` | `crates/tui/src/widgets/composer.rs` | `crates/tui/tests/widget_tests.rs::test_vietnamese_unicode_nfc_grapheme_integrity` |
| **Grapheme Cluster Segmentation** | `[IMPLEMENTED / VERIFIED]` | `crates/tui/src/app.rs` | `crates/tui/tests/widget_tests.rs::test_vietnamese_unicode_nfc_grapheme_integrity` |
| **Terminal Title Sanitization** | `[IMPLEMENTED / VERIFIED]` | `crates/tui/src/terminal.rs` | `crates/tui/tests/widget_tests.rs::test_terminal_title_sanitization` |
| **Centered Modal Math** | `[IMPLEMENTED / VERIFIED]` | `crates/tui/src/layout.rs` | `crates/tui/tests/widget_tests.rs::test_centered_rect_math` |
| **Dynamic Nerd Font Probe** | `[PLANNED]` (M13+) | - | Scheduled M13+ |
| **Rounded Unicode Box Drawing** | `[PREVIEW ONLY]` | `crates/tui/src/preview.rs` | Manual inspection via `ags --preview` |
