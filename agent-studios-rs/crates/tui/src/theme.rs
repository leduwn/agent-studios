//! Agent Studios CLI Theme and Semantic Token Palette
//!
//! Provides a polished, developer-first minimalist design system
//! adhering to the Quiet Precision philosophy (85% carbon/slate, zero neon fluff).

use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Dark,
    Light,
    Ansi16,
    NoColor,
}

#[derive(Debug, Clone)]
pub struct AgsTheme {
    pub mode: ThemeMode,

    // Background tokens
    pub bg_base: Color,
    pub bg_card: Color,
    pub bg_surface: Color,
    pub bg_highlight: Color,

    // Border tokens
    pub border_subtle: Color,
    pub border_focused: Color,
    pub border_dim: Color,

    // Text tokens
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_muted: Color,
    pub text_inverse: Color,

    // Functional semantic tokens
    pub accent_emerald: Color,
    pub accent_amber: Color,
    pub accent_crimson: Color,
    pub accent_sky: Color,
    pub accent_purple: Color,

    // Risk tiers for Security Gate
    pub risk_readonly: Color,
    pub risk_mutating: Color,
    pub risk_dangerous: Color,
}

impl Default for AgsTheme {
    fn default() -> Self {
        Self::dark()
    }
}

impl AgsTheme {
    /// AGS Dark theme (Default developer palette inspired by GitHub Dark Dimmed / Slate Carbon)
    pub fn dark() -> Self {
        Self {
            mode: ThemeMode::Dark,
            bg_base: Color::Rgb(13, 17, 23),       // #0d1117 carbon
            bg_card: Color::Rgb(22, 27, 34),       // #161b22 surface card
            bg_surface: Color::Rgb(33, 38, 45),    // #21262d elevated
            bg_highlight: Color::Rgb(48, 54, 61),  // #30363d selection
            border_subtle: Color::Rgb(48, 54, 61), // #30363d
            border_focused: Color::Rgb(63, 185, 80), // #3fb950 emerald focus
            border_dim: Color::Rgb(33, 38, 45),
            text_primary: Color::Rgb(230, 237, 243),   // #e6edf3
            text_secondary: Color::Rgb(139, 148, 158), // #8b949e
            text_muted: Color::Rgb(72, 79, 88),        // #484f58
            text_inverse: Color::Rgb(13, 17, 23),
            accent_emerald: Color::Rgb(63, 185, 80),  // #3fb950
            accent_amber: Color::Rgb(210, 153, 34),   // #d29922
            accent_crimson: Color::Rgb(248, 81, 73),  // #f85149
            accent_sky: Color::Rgb(88, 166, 255),     // #58a6ff
            accent_purple: Color::Rgb(188, 140, 255), // #bc8cff
            risk_readonly: Color::Rgb(63, 185, 80),
            risk_mutating: Color::Rgb(210, 153, 34),
            risk_dangerous: Color::Rgb(248, 81, 73),
        }
    }

    /// AGS Light theme (Clean Paper & Forest Emerald)
    pub fn light() -> Self {
        Self {
            mode: ThemeMode::Light,
            bg_base: Color::Rgb(255, 255, 255),    // #ffffff
            bg_card: Color::Rgb(246, 248, 250),    // #f6f8fa
            bg_surface: Color::Rgb(234, 238, 242), // #eaeef2
            bg_highlight: Color::Rgb(208, 215, 222),
            border_subtle: Color::Rgb(208, 215, 222),
            border_focused: Color::Rgb(26, 127, 55), // #1a7f37 forest emerald
            border_dim: Color::Rgb(234, 238, 242),
            text_primary: Color::Rgb(31, 35, 40),      // #1f2328
            text_secondary: Color::Rgb(101, 109, 118), // #656d76
            text_muted: Color::Rgb(140, 149, 159),     // #8c959f
            text_inverse: Color::Rgb(255, 255, 255),
            accent_emerald: Color::Rgb(26, 127, 55),
            accent_amber: Color::Rgb(154, 103, 0),
            accent_crimson: Color::Rgb(207, 34, 46),
            accent_sky: Color::Rgb(9, 105, 218),
            accent_purple: Color::Rgb(130, 80, 223),
            risk_readonly: Color::Rgb(26, 127, 55),
            risk_mutating: Color::Rgb(154, 103, 0),
            risk_dangerous: Color::Rgb(207, 34, 46),
        }
    }

    /// ANSI 16-color fallback for legacy terminals
    pub fn ansi16() -> Self {
        Self {
            mode: ThemeMode::Ansi16,
            bg_base: Color::Reset,
            bg_card: Color::Reset,
            bg_surface: Color::Black,
            bg_highlight: Color::DarkGray,
            border_subtle: Color::DarkGray,
            border_focused: Color::Green,
            border_dim: Color::DarkGray,
            text_primary: Color::White,
            text_secondary: Color::Gray,
            text_muted: Color::DarkGray,
            text_inverse: Color::Black,
            accent_emerald: Color::Green,
            accent_amber: Color::Yellow,
            accent_crimson: Color::Red,
            accent_sky: Color::Cyan,
            accent_purple: Color::Magenta,
            risk_readonly: Color::Green,
            risk_mutating: Color::Yellow,
            risk_dangerous: Color::Red,
        }
    }

    /// NO_COLOR compliance (Styles use bold/dim instead of color)
    pub fn no_color() -> Self {
        Self {
            mode: ThemeMode::NoColor,
            bg_base: Color::Reset,
            bg_card: Color::Reset,
            bg_surface: Color::Reset,
            bg_highlight: Color::Reset,
            border_subtle: Color::Reset,
            border_focused: Color::Reset,
            border_dim: Color::Reset,
            text_primary: Color::Reset,
            text_secondary: Color::Reset,
            text_muted: Color::Reset,
            text_inverse: Color::Reset,
            accent_emerald: Color::Reset,
            accent_amber: Color::Reset,
            accent_crimson: Color::Reset,
            accent_sky: Color::Reset,
            accent_purple: Color::Reset,
            risk_readonly: Color::Reset,
            risk_mutating: Color::Reset,
            risk_dangerous: Color::Reset,
        }
    }

    // Common Style helpers
    pub fn base_style(&self) -> Style {
        Style::default().bg(self.bg_base).fg(self.text_primary)
    }

    pub fn card_style(&self) -> Style {
        Style::default().bg(self.bg_card).fg(self.text_primary)
    }

    pub fn border_subtle_style(&self) -> Style {
        Style::default().fg(self.border_subtle)
    }

    pub fn border_focused_style(&self) -> Style {
        Style::default().fg(self.border_focused).add_modifier(Modifier::BOLD)
    }

    pub fn text_primary_style(&self) -> Style {
        Style::default().fg(self.text_primary)
    }

    pub fn text_secondary_style(&self) -> Style {
        Style::default().fg(self.text_secondary)
    }

    pub fn text_muted_style(&self) -> Style {
        Style::default().fg(self.text_muted)
    }

    pub fn badge_style(&self, bg: Color, fg: Color) -> Style {
        Style::default().bg(bg).fg(fg).add_modifier(Modifier::BOLD)
    }
}
