//! Colours and text styles. One place to change the look of the whole interface.

use ratatui::style::{Color, Modifier, Style};

#[derive(Clone, Copy, Debug)]
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

impl Theme {
    /// Warm dark palette, in the spirit of Claude Code and btop.
    pub const fn default_dark() -> Self {
        Self {
            accent: Color::Rgb(217, 119, 87),
            accent_soft: Color::Rgb(232, 170, 140),
            directory: Color::Rgb(97, 175, 239),
            file: Color::Rgb(200, 204, 212),
            text: Color::Rgb(220, 223, 228),
            muted: Color::Rgb(140, 148, 160),
            faint: Color::Rgb(90, 96, 106),
            success: Color::Rgb(152, 195, 121),
            warning: Color::Rgb(229, 192, 123),
            danger: Color::Rgb(224, 108, 117),
            border: Color::Rgb(70, 76, 86),
            border_focused: Color::Rgb(217, 119, 87),
            selection_background: Color::Rgb(52, 56, 66),
            bar_track: Color::Rgb(58, 62, 72),
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
