//! The heaviest files below a directory.

use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table};

use crate::presentation::app::App;
use crate::presentation::formatting;
use crate::presentation::widgets::chrome;

impl App {
    pub(super) fn render_heaviest(&mut self, frame: &mut Frame, area: Rect) {
        let theme = self.theme;
        let coordinator = &self.services.coordinator;
        let mode = coordinator.size_mode();
        let query = self.heaviest.query.and_then(|id| coordinator.heaviest_query(id));
        let origin_path = query.map(|query| query.origin_path.clone());
        let title = Line::from(vec![
            Span::styled(" Heaviest files in ", theme.title()),
            Span::styled(
                origin_path
                    .as_deref()
                    .map(|path| formatting::home_relative(path, self.services.home.as_deref()))
                    .unwrap_or_else(|| "—".to_owned()),
                theme.directory(),
            ),
            Span::raw(" "),
        ]);
        let summary = match query {
            Some(query) => {
                let state = if query.is_complete() {
                    Span::styled(" ✓ ", theme.success())
                } else {
                    Span::styled(format!(" {} ", formatting::spinner(self.tick)), theme.key())
                };
                Line::from(vec![
                    Span::styled(
                        format!(
                            " top {} · {} files seen · {} ",
                            query.limit(),
                            formatting::compact_count(query.files_seen),
                            formatting::duration(query.elapsed().as_secs_f64())
                        ),
                        theme.text(),
                    ),
                    state,
                ])
            }
            None => Line::from(Span::styled(" press T in the explorer ", theme.muted())),
        };
        let panel = chrome::panel(&theme, title, true).title_top(summary.right_aligned());
        let inner = panel.inner(area);
        frame.render_widget(panel, area);

        let rows: Vec<Row<'static>> = self
            .heaviest
            .rows
            .iter()
            .enumerate()
            .map(|(index, file)| {
                let relative = origin_path
                    .as_deref()
                    .and_then(|origin| file.path.strip_prefix(origin).ok())
                    .map(|relative| relative.display().to_string())
                    .unwrap_or_else(|| file.path.display().to_string());
                let marked = self.marks.contains(&file.path);
                Row::new(vec![
                    if marked { Cell::from("●").style(theme.key()) } else { Cell::from(" ") },
                    Cell::from(format!("{:>4}", index + 1)).style(theme.faint()),
                    Cell::from(Line::from(vec![
                        Span::styled(format!("{} ", file.kind.symbol()), theme.faint()),
                        Span::styled(relative, theme.file()),
                    ])),
                    Cell::from(format!("{:>10}", formatting::size(file.size.select(mode), self.size_base)))
                        .style(theme.text()),
                ])
            })
            .collect();
        let widths =
            [Constraint::Length(1), Constraint::Length(4), Constraint::Fill(1), Constraint::Length(10)];
        let table = Table::new(rows, widths)
            .header(Row::new(vec!["", "   #", "Path", "      Size"]).style(theme.muted().bold()))
            .column_spacing(1)
            .row_highlight_style(theme.selected_row())
            .highlight_symbol(Span::styled("▶", theme.key()));
        frame.render_stateful_widget(table, inner, &mut self.heaviest.table_state);

        if self.heaviest.rows.is_empty() {
            let message = match query {
                Some(query) if !query.is_complete() => {
                    format!("{} searching…", formatting::spinner(self.tick))
                }
                Some(_) => "no files found".to_owned(),
                None => "open a directory in the explorer and press T".to_owned(),
            };
            let centered = chrome::centered(inner, message.len() as u16 + 4, 1);
            frame.render_widget(Paragraph::new(message).style(theme.muted()), centered);
        }
    }
}
