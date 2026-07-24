//! Configurable key bindings.
//!
//! Every action reachable by a single key press is defined here as a named,
//! string-valued binding so users can remap keys from the configuration file
//! without recompiling. Key strings support single characters, named keys
//! (`tab`, `enter`, `esc`, arrows, function keys) and a `ctrl+`/`alt+` prefix.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::{Deserialize, Serialize};

/// User-configurable key bindings. Values are key descriptors (see
/// [`key_matches`]). Missing entries fall back to the defaults below.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyConfig {
    pub quit: String,
    pub help: String,
    pub palette: String,
    pub up: String,
    pub down: String,
    pub left: String,
    pub right: String,
    pub select: String,
    pub back: String,
    pub run: String,
    pub install: String,
    pub search: String,
    pub new: String,
    pub refresh: String,
    pub next_tab: String,
    pub prev_tab: String,
    pub delete: String,
    pub shell: String,
    pub page_up: String,
    pub page_down: String,
}

impl Default for KeyConfig {
    fn default() -> Self {
        KeyConfig {
            quit: "q".into(),
            help: "?".into(),
            palette: "a".into(),
            up: "up".into(),
            down: "down".into(),
            left: "left".into(),
            right: "right".into(),
            select: "enter".into(),
            back: "esc".into(),
            run: "r".into(),
            install: "i".into(),
            search: "/".into(),
            new: "n".into(),
            refresh: "f5".into(),
            next_tab: "tab".into(),
            prev_tab: "backtab".into(),
            delete: "d".into(),
            shell: "s".into(),
            page_up: "pageup".into(),
            page_down: "pagedown".into(),
        }
    }
}

/// Parse a key descriptor string into a code + modifiers pair.
///
/// Examples: `"q"`, `"enter"`, `"esc"`, `"tab"`, `"up"`, `"f5"`,
/// `"ctrl+r"`, `"alt+x"`.
pub fn parse_key(spec: &str) -> Option<(KeyCode, KeyModifiers)> {
    let spec = spec.trim();
    if spec.is_empty() {
        return None;
    }
    let mut mods = KeyModifiers::NONE;
    let mut rest = spec;
    loop {
        let lower = rest.to_ascii_lowercase();
        if let Some(stripped) = lower.strip_prefix("ctrl+") {
            mods |= KeyModifiers::CONTROL;
            rest = &rest[rest.len() - stripped.len()..];
        } else if let Some(stripped) = lower.strip_prefix("alt+") {
            mods |= KeyModifiers::ALT;
            rest = &rest[rest.len() - stripped.len()..];
        } else if let Some(stripped) = lower.strip_prefix("shift+") {
            mods |= KeyModifiers::SHIFT;
            rest = &rest[rest.len() - stripped.len()..];
        } else {
            break;
        }
    }

    let code = match rest.to_ascii_lowercase().as_str() {
        "enter" | "return" => KeyCode::Enter,
        "esc" | "escape" => KeyCode::Esc,
        "tab" => KeyCode::Tab,
        "backtab" => KeyCode::BackTab,
        "space" => KeyCode::Char(' '),
        "backspace" => KeyCode::Backspace,
        "delete" | "del" => KeyCode::Delete,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" | "pgup" => KeyCode::PageUp,
        "pagedown" | "pgdn" => KeyCode::PageDown,
        other => {
            if let Some(num) = other.strip_prefix('f') {
                if let Ok(n) = num.parse::<u8>() {
                    return Some((KeyCode::F(n), mods));
                }
            }
            let mut chars = rest.chars();
            let c = chars.next()?;
            if chars.next().is_some() {
                return None; // more than one character and not a named key
            }
            KeyCode::Char(c)
        }
    };
    Some((code, mods))
}

/// Return `true` when the given key event matches the descriptor `spec`.
///
/// Character comparisons are case-insensitive and tolerant of the SHIFT
/// modifier so that, e.g., `"?"` matches regardless of how the terminal
/// reports the shift state.
pub fn key_matches(event: &KeyEvent, spec: &str) -> bool {
    let Some((code, mods)) = parse_key(spec) else {
        return false;
    };
    match (code, event.code) {
        (KeyCode::Char(a), KeyCode::Char(b)) => {
            if !a.eq_ignore_ascii_case(&b) {
                return false;
            }
            // Only enforce CTRL / ALT; SHIFT is implied by the character case.
            let want = mods.intersection(KeyModifiers::CONTROL | KeyModifiers::ALT);
            let have = event
                .modifiers
                .intersection(KeyModifiers::CONTROL | KeyModifiers::ALT);
            want == have
        }
        (a, b) if a == b => {
            let want = mods.intersection(KeyModifiers::CONTROL | KeyModifiers::ALT);
            let have = event
                .modifiers
                .intersection(KeyModifiers::CONTROL | KeyModifiers::ALT);
            want == have
        }
        _ => false,
    }
}
