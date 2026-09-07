//! The modal that follows a deletion or cleaning run and locks the rest of the interface.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Gauge, Padding, Paragraph};

use crate::domain::storage::ByteSize;
use crate::presentation::app::App;
use crate::presentation::app::operation::{EntryStatus, OperationRun};
use crate::presentation::formatting;
use crate::presentation::widgets::chrome;

impl App {
    pub(super) fn render_operation(&self, frame: &mut Frame, run: &OperationRun) {
        let theme = self.theme;
        let area = frame.area();
        let width = 100.min(area.width.saturating_sub(4));
        let height = (12 + run.recent.len() as u16).min(area.height.saturating_sub(2));
        let dialog_area = chrome::centered(area, width, height);
        frame.render_widget(Clear, dialog_area);

        let border = if run.is_finished() { theme.border(true) } else { theme.danger() };
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(border)
            .title_top(Line::from(format!(" {} ", run.title())).style(theme.title()))
            .title_top(Line::from(Span::styled(" other commands are locked ", theme.faint())).right_aligned())
            .padding(Padding::new(2, 2, 1, 0));
        let inner = block.inner(dialog_area);
        frame.render_widget(block, dialog_area);

        let [gauge_area, stats_area, _gap, current_area, detail_area, _gap_two, recent_area, hint_area] =
            Layout::vertical([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Min(0),
                Constraint::Length(1),
            ])
            .areas(inner);

        let gauge_color = if run.is_finished() { theme.success } else { theme.accent };
        let gauge = Gauge::default()
            .ratio(run.ratio())
            .label(format!("{} / {} {}", run.finished_count, run.total, run.kind.noun(run.total)))
            .gauge_style(Style::new().fg(gauge_color).bg(theme.bar_track))
            .use_unicode(true);
        frame.render_widget(gauge, gauge_area);

        let spinner = if run.is_finished() {
            Span::styled("✓", theme.success())
        } else {
            Span::styled(formatting::spinner(self.tick), theme.key())
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                spinner,
                Span::styled(
                    format!(
                        "  {} freed · {} entries removed · {}",
                        formatting::size(ByteSize::new(run.bytes_reclaimed()), self.size_base),
                        formatting::count(run.entries_removed()),
                        formatting::duration(run.elapsed().as_secs_f64())
                    ),
                    theme.text(),
                ),
            ])),
            stats_area,
        );

        if run.is_finished() {
            let summary = match &run.report {
                Some(report) => self.operation_summary(report),
                None => String::new(),
            };
            frame
                .render_widget(Paragraph::new(Line::from(Span::styled(summary, theme.text()))), current_area);
        } else if run.current_index.is_some() {
            let label_width = current_area.width.saturating_sub(2) as usize;
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled("▶ ", theme.key()),
                    Span::styled(formatting::fit(&run.current_label, label_width, true), theme.directory()),
                ])),
                current_area,
            );
            let progress_text = if run.current_entries > 0 {
                format!(
                    "  {} · {} entries",
                    formatting::size(ByteSize::new(run.current_bytes), self.size_base),
                    formatting::count(run.current_entries)
                )
            } else {
                String::new()
            };
            let detail_width = detail_area.width.saturating_sub(progress_text.len() as u16 + 4) as usize;
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled("  ", theme.faint()),
                    Span::styled(formatting::fit(&run.current_detail, detail_width, true), theme.muted()),
                    Span::styled(progress_text, theme.muted()),
                ])),
                detail_area,
            );
        } else {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled("starting…", theme.muted()))),
                current_area,
            );
        }

        let recent_lines: Vec<Line<'static>> = run
            .recent
            .iter()
            .map(|entry| {
                let (glyph, style) = match entry.status {
                    EntryStatus::Success => ("✓", theme.success()),
                    EntryStatus::Warning => ("▲", theme.warning()),
                    EntryStatus::Failure => ("✗", theme.danger()),
                    EntryStatus::Cancelled => ("○", theme.muted()),
                };
                let label_width = recent_area.width.saturating_sub(entry.detail.len() as u16 + 6) as usize;
                Line::from(vec![
                    Span::styled(format!("{glyph} "), style),
                    Span::styled(formatting::fit(&entry.label, label_width.max(10), true), theme.text()),
                    Span::styled(format!("  {}", entry.detail), theme.muted()),
                ])
            })
            .collect();
        frame.render_widget(Paragraph::new(recent_lines), recent_area);

        let hint = if run.is_finished() {
            Line::from(Span::styled("press any key to continue", theme.key()))
        } else if run.cancel_requested {
            Line::from(Span::styled("cancelling after the current item…", theme.warning()))
        } else {
            chrome::key_hints(
                &theme,
                &[("esc", "cancel after the current item"), ("ctrl+c", "abort the program")],
            )
        };
        frame.render_widget(Paragraph::new(hint), hint_area);
    }

    fn operation_summary(&self, report: &crate::presentation::app::operation::OperationReport) -> String {
        use crate::presentation::app::operation::OperationReport;
        match report {
            OperationReport::Deletion(report) => {
                let mut parts = vec![format!("{} removed", report.succeeded().count())];
                let privileges = report.needing_privileges().len();
                if privileges > 0 {
                    parts.push(format!("{privileges} need sudo (asked next)"));
                }
                let failures = report.failures().len();
                if failures > 0 {
                    parts.push(format!("{failures} failed"));
                }
                let cancelled = report.cancelled_count();
                if cancelled > 0 {
                    parts.push(format!("{cancelled} cancelled"));
                }
                parts.join(" · ")
            }
            OperationReport::Cleaning(report) => {
                let mut parts = vec![format!("{} cleaned", report.cleaned_count())];
                let privileges = report.needing_privileges().len();
                if privileges > 0 {
                    parts.push(format!("{privileges} need sudo (asked next)"));
                }
                let failures = report.failed_count();
                if failures > 0 {
                    parts.push(format!("{failures} failed"));
                }
                let cancelled = report.cancelled_count();
                if cancelled > 0 {
                    parts.push(format!("{cancelled} cancelled"));
                }
                parts.join(" · ")
            }
        }
    }
}
