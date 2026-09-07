//! Colours and text styles. Built-in palettes, optional overrides from the configuration
//! file, and the derived styles every screen uses.

use ratatui::style::{Color, Modifier, Style};

use crate::application::configuration::ThemeConfig;

/// Built-in palettes, in the order the settings screen cycles through them.
pub const THEME_NAMES: [&str; 11] = [
    "claude",
    "btop",
    "nord",
    "dracula",
    "gruvbox",
    "catppuccin",
    "tokyo-night",
    "solarized",
    "monochrome",
    "light",
    "basic",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    pub accent: Color,
    pub accent_soft: Color,
    pub directory: Color,
    pub file: Color,
    pub text: Color,
    pub muted: Color,
    pub faint: Color,
    pub success: Color,
    pub warning: Color,
    pub danger: Color,
    pub border: Color,
    pub border_focused: Color,
    pub selection_background: Color,
    pub bar_track: Color,
}

const fn rgb(value: u32) -> Color {
    Color::Rgb(((value >> 16) & 0xff) as u8, ((value >> 8) & 0xff) as u8, (value & 0xff) as u8)
}

impl Theme {
    /// Warm dark palette, in the spirit of Claude Code. The default.
    pub const fn claude() -> Self {
        Self {
            accent: rgb(0xd97757),
            accent_soft: rgb(0xe8aa8c),
            directory: rgb(0x61afef),
            file: rgb(0xc8ccd4),
            text: rgb(0xdcdfe4),
            muted: rgb(0x8c94a0),
            faint: rgb(0x5a606a),
            success: rgb(0x98c379),
            warning: rgb(0xe5c07b),
            danger: rgb(0xe06c75),
            border: rgb(0x464c56),
            border_focused: rgb(0xd97757),
            selection_background: rgb(0x343842),
            bar_track: rgb(0x3a3e48),
        }
    }

    /// Kept for callers that only want "the default".
    pub const fn default_dark() -> Self {
        Self::claude()
    }

    /// btop's default look: red accents, blue directories.
    pub const fn btop() -> Self {
        Self {
            accent: rgb(0xb54040),
            accent_soft: rgb(0xd9626d),
            directory: rgb(0x4897d4),
            file: rgb(0xcccccc),
            text: rgb(0xeeeeee),
            muted: rgb(0x999999),
            faint: rgb(0x606060),
            success: rgb(0x50f095),
            warning: rgb(0xf2e266),
            danger: rgb(0xfa1e1e),
            border: rgb(0x303030),
            border_focused: rgb(0xb54040),
            selection_background: rgb(0x6a2f2f),
            bar_track: rgb(0x262626),
        }
    }

    pub const fn nord() -> Self {
        Self {
            accent: rgb(0x88c0d0),
            accent_soft: rgb(0x8fbcbb),
            directory: rgb(0x81a1c1),
            file: rgb(0xd8dee9),
            text: rgb(0xeceff4),
            muted: rgb(0x7b88a1),
            faint: rgb(0x4c566a),
            success: rgb(0xa3be8c),
            warning: rgb(0xebcb8b),
            danger: rgb(0xbf616a),
            border: rgb(0x434c5e),
            border_focused: rgb(0x88c0d0),
            selection_background: rgb(0x3b4252),
            bar_track: rgb(0x434c5e),
        }
    }

    pub const fn dracula() -> Self {
        Self {
            accent: rgb(0xbd93f9),
            accent_soft: rgb(0xff79c6),
            directory: rgb(0x8be9fd),
            file: rgb(0xf8f8f2),
            text: rgb(0xf8f8f2),
            muted: rgb(0x6272a4),
            faint: rgb(0x44475a),
            success: rgb(0x50fa7b),
            warning: rgb(0xf1fa8c),
            danger: rgb(0xff5555),
            border: rgb(0x44475a),
            border_focused: rgb(0xbd93f9),
            selection_background: rgb(0x44475a),
            bar_track: rgb(0x383a4a),
        }
    }

    pub const fn gruvbox() -> Self {
        Self {
            accent: rgb(0xfe8019),
            accent_soft: rgb(0xfabd2f),
            directory: rgb(0x83a598),
            file: rgb(0xebdbb2),
            text: rgb(0xebdbb2),
            muted: rgb(0xa89984),
            faint: rgb(0x665c54),
            success: rgb(0xb8bb26),
            warning: rgb(0xfabd2f),
            danger: rgb(0xfb4934),
            border: rgb(0x504945),
            border_focused: rgb(0xfe8019),
            selection_background: rgb(0x3c3836),
            bar_track: rgb(0x3c3836),
        }
    }

    /// Catppuccin Mocha.
    pub const fn catppuccin() -> Self {
        Self {
            accent: rgb(0xcba6f7),
            accent_soft: rgb(0xf5c2e7),
            directory: rgb(0x89b4fa),
            file: rgb(0xcdd6f4),
            text: rgb(0xcdd6f4),
            muted: rgb(0xa6adc8),
            faint: rgb(0x6c7086),
            success: rgb(0xa6e3a1),
            warning: rgb(0xf9e2af),
            danger: rgb(0xf38ba8),
            border: rgb(0x45475a),
            border_focused: rgb(0xcba6f7),
            selection_background: rgb(0x313244),
            bar_track: rgb(0x313244),
        }
    }

    pub const fn tokyo_night() -> Self {
        Self {
            accent: rgb(0x7aa2f7),
            accent_soft: rgb(0xbb9af7),
            directory: rgb(0x7dcfff),
            file: rgb(0xc0caf5),
            text: rgb(0xc0caf5),
            muted: rgb(0xa9b1d6),
            faint: rgb(0x565f89),
            success: rgb(0x9ece6a),
            warning: rgb(0xe0af68),
            danger: rgb(0xf7768e),
            border: rgb(0x3b4261),
            border_focused: rgb(0x7aa2f7),
            selection_background: rgb(0x292e42),
            bar_track: rgb(0x292e42),
        }
    }

    /// Solarized dark.
    pub const fn solarized() -> Self {
        Self {
            accent: rgb(0xb58900),
            accent_soft: rgb(0xcb4b16),
            directory: rgb(0x268bd2),
            file: rgb(0x839496),
            text: rgb(0x93a1a1),
            muted: rgb(0x657b83),
            faint: rgb(0x586e75),
            success: rgb(0x859900),
            warning: rgb(0xb58900),
            danger: rgb(0xdc322f),
            border: rgb(0x586e75),
            border_focused: rgb(0xb58900),
            selection_background: rgb(0x073642),
            bar_track: rgb(0x073642),
        }
    }

    /// Shades of grey only.
    pub const fn monochrome() -> Self {
        Self {
            accent: rgb(0xffffff),
            accent_soft: rgb(0xd0d0d0),
            directory: rgb(0xe0e0e0),
            file: rgb(0xb0b0b0),
            text: rgb(0xd0d0d0),
            muted: rgb(0x8a8a8a),
            faint: rgb(0x5a5a5a),
            success: rgb(0xc0c0c0),
            warning: rgb(0xa0a0a0),
            danger: rgb(0xffffff),
            border: rgb(0x4a4a4a),
            border_focused: rgb(0xffffff),
            selection_background: rgb(0x3a3a3a),
            bar_track: rgb(0x303030),
        }
    }

    /// For terminals with a light background.
    pub const fn light() -> Self {
        Self {
            accent: rgb(0xc2410c),
            accent_soft: rgb(0xea580c),
            directory: rgb(0x0550ae),
            file: rgb(0x24292e),
            text: rgb(0x1f2328),
            muted: rgb(0x57606a),
            faint: rgb(0x8c959f),
            success: rgb(0x1a7f37),
            warning: rgb(0x9a6700),
            danger: rgb(0xcf222e),
            border: rgb(0xd0d7de),
            border_focused: rgb(0xc2410c),
            selection_background: rgb(0xeaeef2),
            bar_track: rgb(0xe6e9ed),
        }
    }

    /// Palette without truecolor for terminals that only support 16 colours.
    pub const fn basic() -> Self {
        Self {
            accent: Color::LightRed,
            accent_soft: Color::Yellow,
            directory: Color::LightBlue,
            file: Color::White,
            text: Color::White,
            muted: Color::Gray,
            faint: Color::DarkGray,
            success: Color::LightGreen,
            warning: Color::LightYellow,
            danger: Color::LightRed,
            border: Color::DarkGray,
            border_focused: Color::LightRed,
            selection_background: Color::DarkGray,
            bar_track: Color::DarkGray,
        }
    }

    pub fn by_name(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().replace('_', "-").as_str() {
            "claude" | "default" => Some(Self::claude()),
            "btop" => Some(Self::btop()),
            "nord" => Some(Self::nord()),
            "dracula" => Some(Self::dracula()),
            "gruvbox" => Some(Self::gruvbox()),
            "catppuccin" | "catppuccin-mocha" | "mocha" => Some(Self::catppuccin()),
            "tokyo-night" | "tokyonight" | "tokyo" => Some(Self::tokyo_night()),
            "solarized" | "solarized-dark" => Some(Self::solarized()),
            "monochrome" | "mono" | "grey" | "gray" => Some(Self::monochrome()),
            "light" => Some(Self::light()),
            "basic" | "ansi" | "16" => Some(Self::basic()),
            _ => None,
        }
    }

    pub fn describe(name: &str) -> &'static str {
        match name {
            "claude" => "warm terracotta accents, blue directories (default)",
            "btop" => "red accents and blue directories, like btop",
            "nord" => "cool arctic blues",
            "dracula" => "purple, pink and cyan",
            "gruvbox" => "retro orange and olive",
            "catppuccin" => "pastel mocha",
            "tokyo-night" => "deep blue night",
            "solarized" => "solarized dark",
            "monochrome" => "greys only",
            "light" => "for light terminal backgrounds",
            "basic" => "16 ANSI colours, for terminals without truecolor",
            _ => "custom",
        }
    }

    /// The built-in name `step` positions away from `current` (wrapping).
    pub fn neighbour_name(current: &str, step: isize) -> &'static str {
        let count = THEME_NAMES.len() as isize;
        let index = THEME_NAMES.iter().position(|name| *name == current).unwrap_or(0) as isize;
        THEME_NAMES[((index + step).rem_euclid(count)) as usize]
    }

    /// Applies the colour overrides from the configuration file.
    pub fn with_overrides(mut self, overrides: &ThemeConfig) -> Result<Self, String> {
        for (field, value) in overrides.entries() {
            let color = parse_color(value).map_err(|reason| format!("theme.{field}: {reason}"))?;
            match field {
                "accent" => self.accent = color,
                "accent_soft" => self.accent_soft = color,
                "directory" => self.directory = color,
                "file" => self.file = color,
                "text" => self.text = color,
                "muted" => self.muted = color,
                "faint" => self.faint = color,
                "success" => self.success = color,
                "warning" => self.warning = color,
                "danger" => self.danger = color,
                "border" => self.border = color,
                "border_focused" => self.border_focused = color,
                "selection_background" => self.selection_background = color,
                "bar_track" => self.bar_track = color,
                _ => {}
            }
        }
        Ok(self)
    }

    // ----------------------------------------------------------------------------------
    // Derived styles
    // ----------------------------------------------------------------------------------

    pub fn title(&self) -> Style {
        Style::new().fg(self.accent).add_modifier(Modifier::BOLD)
    }

    pub fn text(&self) -> Style {
        Style::new().fg(self.text)
    }

    pub fn muted(&self) -> Style {
        Style::new().fg(self.muted)
    }

    pub fn faint(&self) -> Style {
        Style::new().fg(self.faint)
    }

    pub fn directory(&self) -> Style {
        Style::new().fg(self.directory).add_modifier(Modifier::BOLD)
    }

    pub fn file(&self) -> Style {
        Style::new().fg(self.file)
    }

    pub fn key(&self) -> Style {
        Style::new().fg(self.accent).add_modifier(Modifier::BOLD)
    }

    pub fn border(&self, focused: bool) -> Style {
        Style::new().fg(if focused { self.border_focused } else { self.border })
    }

    pub fn selected_row(&self) -> Style {
        Style::new().bg(self.selection_background).add_modifier(Modifier::BOLD)
    }

    pub fn success(&self) -> Style {
        Style::new().fg(self.success)
    }

    pub fn warning(&self) -> Style {
        Style::new().fg(self.warning)
    }

    pub fn danger(&self) -> Style {
        Style::new().fg(self.danger).add_modifier(Modifier::BOLD)
    }

    pub fn tab_active(&self) -> Style {
        Style::new().fg(Color::Black).bg(self.accent).add_modifier(Modifier::BOLD)
    }

    pub fn tab_inactive(&self) -> Style {
        Style::new().fg(self.muted)
    }

    /// Colour of a usage bar for a ratio between 0 and 1 (green, yellow, red).
    pub fn usage_color(&self, ratio: f64) -> Color {
        if ratio >= 0.9 {
            self.danger
        } else if ratio >= 0.75 {
            self.warning
        } else {
            self.success
        }
    }

    /// Colour of a share-of-parent bar: the bigger the share, the warmer.
    pub fn share_color(&self, share: f64) -> Color {
        if share >= 0.5 {
            self.accent
        } else if share >= 0.15 {
            self.accent_soft
        } else {
            self.directory
        }
    }
}

/// Accepts `#rrggbb`, `#rgb`, ANSI colour names and 0-255 palette indices.
pub fn parse_color(text: &str) -> Result<Color, String> {
    let text = text.trim();
    if let Some(hex) = text.strip_prefix('#') {
        let expanded: String = match hex.len() {
            3 => hex.chars().flat_map(|character| [character, character]).collect(),
            6 => hex.to_owned(),
            _ => return Err(format!("`{text}` is not a #rrggbb colour")),
        };
        let value =
            u32::from_str_radix(&expanded, 16).map_err(|_| format!("`{text}` is not a #rrggbb colour"))?;
        return Ok(rgb(value));
    }
    if let Ok(index) = text.parse::<u8>() {
        return Ok(Color::Indexed(index));
    }
    let named = match text.to_ascii_lowercase().replace(['-', ' '], "_").as_str() {
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" | "purple" => Color::Magenta,
        "cyan" => Color::Cyan,
        "white" => Color::White,
        "gray" | "grey" => Color::Gray,
        "dark_gray" | "dark_grey" | "darkgray" | "darkgrey" => Color::DarkGray,
        "light_red" | "lightred" => Color::LightRed,
        "light_green" | "lightgreen" => Color::LightGreen,
        "light_yellow" | "lightyellow" => Color::LightYellow,
        "light_blue" | "lightblue" => Color::LightBlue,
        "light_magenta" | "lightmagenta" => Color::LightMagenta,
        "light_cyan" | "lightcyan" => Color::LightCyan,
        "reset" | "default" | "none" => Color::Reset,
        _ => return Err(format!("`{text}` is not a known colour (use #rrggbb, an ANSI name or 0-255)")),
    };
    Ok(named)
}
