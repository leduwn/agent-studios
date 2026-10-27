//! Terminal Lifecycle, Raw Mode Guard, Panic Recovery, and Title Sanitization
//!
//! Ensures clean entry and exit from raw mode and alternate screen, preventing
//! broken terminal state on crash or early termination.

use std::io::{self, stdout, Stdout, Write};
use std::panic;
use crossterm::{
    cursor,
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
    },
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

/// Practical upper bound on terminal title characters
pub const MAX_TERMINAL_TITLE_CHARS: usize = 240;

/// RAII Guard ensuring terminal restoration when dropped.
pub struct TerminalGuard {
    _private: (),
}

impl TerminalGuard {
    /// Initializes terminal in raw mode, alternate screen, and installs panic hook.
    pub fn enter() -> io::Result<(Terminal<CrosstermBackend<Stdout>>, Self)> {
        enable_raw_mode()?;
        let mut stdout = stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture, cursor::Hide)?;

        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        // Install panic hook to cleanly restore terminal before printing panic backtrace
        let default_panic_hook = panic::take_hook();
        panic::set_hook(Box::new(move |panic_info| {
            let _ = Self::restore_raw_terminal();
            default_panic_hook(panic_info);
        }));

        Ok((terminal, Self { _private: () }))
    }

    /// Explicitly restores terminal state.
    pub fn restore_raw_terminal() -> io::Result<()> {
        let mut stdout = stdout();
        let _ = execute!(
            stdout,
            cursor::Show,
            DisableMouseCapture,
            LeaveAlternateScreen
        );
        let _ = disable_raw_mode();
        let _ = stdout.flush();
        Ok(())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = Self::restore_raw_terminal();
    }
}

/// Sanitizes untrusted text and sets terminal window title via OSC 0.
pub fn set_terminal_title(title: &str) -> io::Result<()> {
    let sanitized = sanitize_terminal_title(title);
    if sanitized.is_empty() {
        return Ok(());
    }
    let mut stdout = stdout();
    write!(stdout, "\x1b]0;{sanitized}\x07")?;
    stdout.flush()?;
    Ok(())
}

/// Normalizes untrusted title text into a single bounded display line.
///
/// Strips control characters, invisible/bidi formatting codepoints, collapses
/// any whitespace run into a single ASCII space, and truncates to [`MAX_TERMINAL_TITLE_CHARS`].
pub fn sanitize_terminal_title(title: &str) -> String {
    let mut sanitized = String::with_capacity(title.len());
    let mut chars_written = 0;
    let mut pending_space = false;

    for ch in title.chars() {
        if ch.is_whitespace() {
            pending_space = !sanitized.is_empty();
            continue;
        }

        if is_disallowed_terminal_title_char(ch) {
            continue;
        }

        if pending_space {
            let remaining = MAX_TERMINAL_TITLE_CHARS.saturating_sub(chars_written);
            if remaining > 1 {
                sanitized.push(' ');
                chars_written += 1;
                pending_space = false;
            }
        }

        if chars_written >= MAX_TERMINAL_TITLE_CHARS {
            break;
        }

        sanitized.push(ch);
        chars_written += 1;
    }

    sanitized
}

fn is_disallowed_terminal_title_char(ch: char) -> bool {
    if ch.is_control() {
        return true;
    }

    matches!(
        ch,
        '\u{00AD}'
            | '\u{034F}'
            | '\u{061C}'
            | '\u{180E}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{206F}'
    )
}
