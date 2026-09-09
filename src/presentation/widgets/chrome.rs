//! Reusable pieces of the frame: bordered panels, key hints, dialogs.

use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph, Wrap};
use unicode_width::UnicodeWidthStr;

use crate::presentation::theme::Theme;

/// A rounded panel with a title, optionally highlighted as focused.
pub fn panel<'a>(theme: &Theme, title: impl Into<Line<'a>>, focused: bool) -> Block<'a> {
    Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.border(focused))
        .title_top(title.into())
        .padding(Padding::horizontal(1))
}

/// Footer line of key hints: `⏎ open · ⌫ up · space mark`.
pub fn key_hints<'a>(theme: &Theme, hints: &[(&'a str, &'a str)]) -> Line<'a> {
    let mut spans: Vec<Span<'a>> = Vec::with_capacity(hints.len() * 3);
    for (index, (key, description)) in hints.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled("  ·  ", theme.faint()));
        }
        spans.push(Span::styled(*key, theme.key()));
        spans.push(Span::styled(format!(" {description}"), theme.muted()));
    }
    Line::from(spans)
}

/// Centres a box of the given size inside `area`.
pub fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let [vertical] =
        Layout::vertical([Constraint::Length(height.min(area.height))]).flex(Flex::Center).areas(area);
    let [horizontal] =
        Layout::horizontal([Constraint::Length(width.min(area.width))]).flex(Flex::Center).areas(vertical);
    horizontal
}

/// Draws a modal dialog with the given lines and returns the inner area. The height
/// accounts for lines that wrap inside the dialog.
pub fn dialog(
    frame: &mut Frame,
    theme: &Theme,
    title: &str,
    lines: Vec<Line<'_>>,
    width: u16,
    border: Style,
) -> Rect {
    let width = width.min(frame.area().width.saturating_sub(2)).max(12);
    let inner_width = usize::from(width.saturating_sub(6)).max(1);
    let wrapped_rows: usize = lines
        .iter()
        .map(|line| {
            let line_width: usize =
                line.spans.iter().map(|span| UnicodeWidthStr::width(span.content.as_ref())).sum();
            line_width.max(1).div_ceil(inner_width)
        })
        .sum();
    let height = (wrapped_rows as u16).saturating_add(4).min(frame.area().height);
    let area = centered(frame.area(), width, height);
    frame.render_widget(Clear, area);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border)
        .title_top(Line::from(format!(" {title} ")).style(theme.title()))
        .padding(Padding::new(2, 2, 1, 1));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
    inner
}
