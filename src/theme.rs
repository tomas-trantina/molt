//! Theming for the terminal UI.
//!
//! A theme is a set of named colour *tokens*. Built-in themes are provided,
//! and any token can be overridden from the user configuration
//! (`[ui.theme_overrides]`) without touching the source code. This keeps the
//! visual language fully configurable, as required by the project design.

use std::collections::BTreeMap;

use ratatui::style::Color;

/// A fully resolved palette used by the renderer.
#[derive(Clone, Debug)]
pub struct ThemePalette {
    pub name: String,
    pub accent: Color,
    pub accent_alt: Color,
    pub success: Color,
    pub warning: Color,
    pub danger: Color,
    pub info: Color,
    pub fg: Color,
    pub fg_dim: Color,
    pub muted: Color,
    pub border: Color,
    pub border_focus: Color,
    pub selection_bg: Color,
    pub selection_fg: Color,
    pub panel_bg: Color,
}

impl ThemePalette {
    /// Resolve a palette by name, then apply any user token overrides.
    ///
    /// Unknown theme names fall back to the default `moss` theme. Unknown
    /// override tokens are ignored. Invalid hex values are ignored.
    pub fn resolve(name: &str, overrides: &BTreeMap<String, String>) -> ThemePalette {
        let mut palette = builtin(name);
        for (token, hex) in overrides {
            if let Some(color) = parse_hex(hex) {
                palette.set_token(token, color);
            }
        }
        palette
    }

    fn set_token(&mut self, token: &str, color: Color) {
        match token {
            "accent" => self.accent = color,
            "accent_alt" => self.accent_alt = color,
            "success" => self.success = color,
            "warning" => self.warning = color,
            "danger" => self.danger = color,
            "info" => self.info = color,
            "fg" => self.fg = color,
            "fg_dim" => self.fg_dim = color,
            "muted" => self.muted = color,
            "border" => self.border = color,
            "border_focus" => self.border_focus = color,
            "selection_bg" => self.selection_bg = color,
            "selection_fg" => self.selection_fg = color,
            "panel_bg" => self.panel_bg = color,
            _ => {}
        }
    }
}

/// Names of the built-in themes, for help text and validation.
pub const BUILTIN_THEMES: &[&str] = &["moss", "amber", "ocean", "mono", "high-contrast"];

/// Return a built-in palette by name, defaulting to `moss`.
pub fn builtin(name: &str) -> ThemePalette {
    match name {
        "amber" => amber(),
        "ocean" => ocean(),
        "mono" => mono(),
        "high-contrast" => high_contrast(),
        _ => moss(),
    }
}

/// Parse a `#rrggbb` (or `rrggbb`) hex string into a [`Color`].
pub fn parse_hex(hex: &str) -> Option<Color> {
    let h = hex.trim().trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&h[0..2], 16).ok()?;
    let g = u8::from_str_radix(&h[2..4], 16).ok()?;
    let b = u8::from_str_radix(&h[4..6], 16).ok()?;
    Some(Color::Rgb(r, g, b))
}

fn moss() -> ThemePalette {
    ThemePalette {
        name: "moss".into(),
        accent: Color::Rgb(0x9E, 0xD9, 0x6B),
        accent_alt: Color::Rgb(0x6B, 0xBF, 0x7B),
        success: Color::Rgb(0x6B, 0xBF, 0x7B),
        warning: Color::Rgb(0xD9, 0xA4, 0x41),
        danger: Color::Rgb(0xE0, 0x6C, 0x75),
        info: Color::Rgb(0x58, 0xA6, 0xC9),
        fg: Color::Rgb(0xE6, 0xE9, 0xE1),
        fg_dim: Color::Rgb(0xB4, 0xBA, 0xAF),
        muted: Color::Rgb(0x6B, 0x72, 0x80),
        border: Color::Rgb(0x3A, 0x40, 0x38),
        border_focus: Color::Rgb(0x9E, 0xD9, 0x6B),
        selection_bg: Color::Rgb(0x2C, 0x38, 0x24),
        selection_fg: Color::Rgb(0xE6, 0xE9, 0xE1),
        panel_bg: Color::Reset,
    }
}

fn amber() -> ThemePalette {
    ThemePalette {
        name: "amber".into(),
        accent: Color::Rgb(0xE8, 0xB8, 0x4B),
        accent_alt: Color::Rgb(0xD9, 0xA4, 0x41),
        success: Color::Rgb(0x6B, 0xBF, 0x7B),
        warning: Color::Rgb(0xD9, 0xA4, 0x41),
        danger: Color::Rgb(0xE0, 0x6C, 0x75),
        info: Color::Rgb(0x58, 0xA6, 0xC9),
        fg: Color::Rgb(0xEC, 0xE6, 0xD8),
        fg_dim: Color::Rgb(0xBE, 0xB6, 0xA2),
        muted: Color::Rgb(0x6B, 0x72, 0x80),
        border: Color::Rgb(0x40, 0x3A, 0x2C),
        border_focus: Color::Rgb(0xE8, 0xB8, 0x4B),
        selection_bg: Color::Rgb(0x3A, 0x30, 0x1C),
        selection_fg: Color::Rgb(0xEC, 0xE6, 0xD8),
        panel_bg: Color::Reset,
    }
}

fn ocean() -> ThemePalette {
    ThemePalette {
        name: "ocean".into(),
        accent: Color::Rgb(0x56, 0xB6, 0xD6),
        accent_alt: Color::Rgb(0x4A, 0x90, 0xC2),
        success: Color::Rgb(0x5F, 0xC9, 0xA8),
        warning: Color::Rgb(0xE5, 0xC0, 0x7B),
        danger: Color::Rgb(0xE0, 0x6C, 0x75),
        info: Color::Rgb(0x7A, 0xB8, 0xE0),
        fg: Color::Rgb(0xDC, 0xE6, 0xEC),
        fg_dim: Color::Rgb(0xA8, 0xB6, 0xC0),
        muted: Color::Rgb(0x60, 0x6C, 0x78),
        border: Color::Rgb(0x2C, 0x38, 0x42),
        border_focus: Color::Rgb(0x56, 0xB6, 0xD6),
        selection_bg: Color::Rgb(0x1C, 0x30, 0x3C),
        selection_fg: Color::Rgb(0xDC, 0xE6, 0xEC),
        panel_bg: Color::Reset,
    }
}

fn mono() -> ThemePalette {
    ThemePalette {
        name: "mono".into(),
        accent: Color::Rgb(0xE6, 0xE6, 0xE6),
        accent_alt: Color::Rgb(0xB0, 0xB0, 0xB0),
        success: Color::Rgb(0xC8, 0xC8, 0xC8),
        warning: Color::Rgb(0xD0, 0xD0, 0xD0),
        danger: Color::Rgb(0xF0, 0xF0, 0xF0),
        info: Color::Rgb(0xB8, 0xB8, 0xB8),
        fg: Color::Rgb(0xE0, 0xE0, 0xE0),
        fg_dim: Color::Rgb(0xA0, 0xA0, 0xA0),
        muted: Color::Rgb(0x70, 0x70, 0x70),
        border: Color::Rgb(0x40, 0x40, 0x40),
        border_focus: Color::Rgb(0xE6, 0xE6, 0xE6),
        selection_bg: Color::Rgb(0x30, 0x30, 0x30),
        selection_fg: Color::Rgb(0xF4, 0xF4, 0xF4),
        panel_bg: Color::Reset,
    }
}

fn high_contrast() -> ThemePalette {
    ThemePalette {
        name: "high-contrast".into(),
        accent: Color::Yellow,
        accent_alt: Color::LightYellow,
        success: Color::LightGreen,
        warning: Color::LightYellow,
        danger: Color::LightRed,
        info: Color::LightCyan,
        fg: Color::White,
        fg_dim: Color::Gray,
        muted: Color::DarkGray,
        border: Color::Gray,
        border_focus: Color::Yellow,
        selection_bg: Color::Blue,
        selection_fg: Color::White,
        panel_bg: Color::Reset,
    }
}
