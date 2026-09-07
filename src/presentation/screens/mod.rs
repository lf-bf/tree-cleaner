//! Rendering. Each screen lives in its own file as an `impl App` block.

mod cleaner;
mod dashboard;
mod explorer;
mod heaviest;
mod overlays;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use crate::presentation::app::dialogs::Severity;
use crate::presentation::app::{App, Screen, VERSION};
use crate::presentation::formatting;
use crate::presentation::widgets::chrome;

impl App {
    pub fn render(&mut self, frame: &mut Frame) {
        let theme = self.theme;
        let area = frame.area();
        let title = Line::from(vec![
            Span::styled(" tree-cleaner ", theme.title()),
            Span::styled(format!("v{VERSION} "), theme.muted()),
        ]);
        let outer = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.border(false))
            .title_top(title)
            .title_top(self.scan_status_line().right_aligned());
        let inner = outer.inner(area);
        frame.render_widget(outer, area);

        let [tabs_area, content_area, footer_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(3), Constraint::Length(1)]).areas(inner);
        self.render_tabs(frame, tabs_area);
        match self.screen {
            Screen::Dashboard => self.render_dashboard(frame, content_area),
            Screen::Explorer => self.render_explorer(frame, content_area),
            Screen::Heaviest => self.render_heaviest(frame, content_area),
            Screen::Cleaner => self.render_cleaner(frame, content_area),
        }
        self.render_footer(frame, footer_area);
        if let Some(overlay) = self.overlay.clone() {
            self.render_overlay(frame, &overlay);
        }
    }

    fn scan_status_line(&self) -> Line<'static> {
        let theme = self.theme;
        let engine = self.services.coordinator.engine();
        let snapshot = engine.statistics().snapshot();
        let depths = engine.queue_depths();
        let mut spans = Vec::new();
        if engine.is_idle() {
            spans.push(Span::styled(" ✓ idle ", theme.success()));
        } else if engine.is_background_paused() && depths.focused == 0 && depths.urgent == 0 {
            spans.push(Span::styled(" ‖ paused ", theme.warning()));
        } else {
            spans.push(Span::styled(format!(" {} scanning ", formatting::spinner(self.tick)), theme.key()));
        }
        spans.push(Span::styled(
            format!(
                "{} files · {} dirs · {} files/s · {}/{} thr · queue {} ",
                formatting::compact_count(snapshot.files_measured),
                formatting::compact_count(snapshot.directories_listed),
                formatting::compact_count(snapshot.files_per_second() as u64),
                snapshot.busy_workers,
                snapshot.worker_count,
                formatting::compact_count(depths.total() as u64),
            ),
            theme.muted(),
        ));
        Line::from(spans)
    }

    fn render_tabs(&self, frame: &mut Frame, area: Rect) {
        let theme = self.theme;
        let mut spans: Vec<Span<'static>> = vec![Span::raw(" ")];
        for (index, screen) in Screen::ALL.into_iter().enumerate() {
            let label = format!(" {} {} ", index + 1, screen.label());
            if screen == self.screen {
                spans.push(Span::styled(label, theme.tab_active()));
            } else {
                spans.push(Span::styled(label, theme.tab_inactive()));
            }
            spans.push(Span::raw(" "));
        }
        frame.render_widget(Paragraph::new(Line::from(spans)), area);

        if !self.marks.is_empty() {
            let summary = format!(
                "● {} marked · {} · d deletes ({}) ",
                self.marks.len(),
                formatting::size(self.marks.total_size(), self.size_base),
                self.deletion_mode.label()
            );
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(summary, theme.key()))).right_aligned(),
                area,
            );
        } else {
            let mode = format!(
                "{} · {} ",
                self.services.coordinator.size_mode().label(),
                match self.size_base {
                    crate::domain::storage::SizeBase::Decimal => "GB",
                    crate::domain::storage::SizeBase::Binary => "GiB",
                }
            );
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(mode, theme.faint()))).right_aligned(),
                area,
            );
        }
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let theme = self.theme;
        if self.command_mode {
            let line = Line::from(vec![
                Span::styled(" :", theme.key()),
                Span::styled(self.command_line.input.clone(), theme.text()),
            ]);
            frame.render_widget(Paragraph::new(line), area);
            let cursor_x = area.x + 2 + self.command_line.cursor as u16;
            frame.set_cursor_position((cursor_x.min(area.right().saturating_sub(1)), area.y));
            return;
        }
        if let Some(status) = &self.status {
            let style = match status.severity {
                Severity::Info => theme.muted(),
                Severity::Success => theme.success(),
                Severity::Warning => theme.warning(),
                Severity::Error => theme.danger(),
            };
            let glyph = match status.severity {
                Severity::Info => "•",
                Severity::Success => "✓",
                Severity::Warning => "▲",
                Severity::Error => "✗",
            };
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(format!(" {glyph} "), style),
                    Span::styled(status.text.clone(), style),
                ])),
                area,
            );
            return;
        }
        let hints: &[(&str, &str)] = match self.screen {
            Screen::Dashboard => &[
                ("↑↓", "select"),
                ("←→", "panel"),
                ("⏎", "explore"),
                ("c", "cleaner"),
                ("r", "refresh"),
                (":", "command"),
                ("?", "help"),
                ("q", "quit"),
            ],
            Screen::Explorer => &[
                ("⏎", "open"),
                ("⌫", "up"),
                ("space", "mark"),
                ("d", "delete"),
                ("T", "heaviest"),
                ("r", "rescan"),
                ("a", "size mode"),
                ("f", "files"),
                ("+/-", "rows"),
                ("o", "reveal"),
                ("/", "filter"),
                (":", "cmd"),
                ("?", "help"),
            ],
            Screen::Heaviest => &[
                ("↑↓", "select"),
                ("space", "mark"),
                ("d", "delete"),
                ("⏎", "go to folder"),
                ("o", "reveal"),
                ("r", "rerun"),
                ("+/-", "limit"),
                ("esc", "back"),
            ],
            Screen::Cleaner => &[
                ("space", "toggle"),
                ("⏎", "fold group"),
                ("a", "all"),
                ("n", "none"),
                ("d", "clean selected"),
                ("i", "info"),
                ("o", "reveal"),
                ("r", "rescan"),
                ("esc", "back"),
            ],
        };
        frame.render_widget(Paragraph::new(chrome::key_hints(&theme, hints)), area);
    }
}
