//! The settings screen.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table};

use crate::presentation::app::App;
use crate::presentation::app::settings_state::{SettingId, SettingsRow};
use crate::presentation::formatting;
use crate::presentation::widgets::chrome;

impl App {
    pub(super) fn render_settings(&mut self, frame: &mut Frame, area: Rect) {
        let theme = self.theme;
        let path =
            formatting::home_relative(self.services.config_store.path(), self.services.home.as_deref());
        let state_text = if self.settings.dirty {
            Span::styled(" ● unsaved changes ", theme.warning())
        } else if self.services.config_store.exists() {
            Span::styled(" ✓ saved ", theme.success())
        } else {
            Span::styled(" not written yet ", theme.muted())
        };
        let title = Line::from(Span::styled(" Settings ", theme.title()));
        let summary = Line::from(vec![Span::styled(format!(" {path} "), theme.muted()), state_text]);
        let panel = chrome::panel(&theme, title, true).title_top(summary.right_aligned());
        let inner = panel.inner(area);
        frame.render_widget(panel, area);

        let [notice_area, table_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(inner);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "Changes apply immediately. s writes them to the config file; e opens the file in your editor.",
                theme.faint(),
            ))),
            notice_area,
        );

        let selected = self.settings.selected;
        let rows: Vec<Row<'static>> = self
            .settings
            .rows
            .iter()
            .enumerate()
            .map(|(index, row)| match row {
                SettingsRow::Section(name) => Row::new(vec![
                    Cell::from(Line::from(Span::styled((*name).to_owned(), theme.title()))),
                    Cell::from(""),
                    Cell::from(""),
                ]),
                SettingsRow::Setting(id) => {
                    let is_selected = index == selected;
                    let value = self.setting_value(*id);
                    let value_cell = if *id == SettingId::Theme {
                        let mut spans = vec![Span::styled(
                            format!("‹ {value} › "),
                            if is_selected { theme.key() } else { theme.text() },
                        )];
                        for color in
                            [theme.accent, theme.directory, theme.success, theme.warning, theme.danger]
                        {
                            spans.push(Span::styled("█", Style::new().fg(color)));
                        }
                        Cell::from(Line::from(spans))
                    } else if id.is_action() {
                        Cell::from(Span::styled(value, theme.key()))
                    } else {
                        Cell::from(Span::styled(
                            format!("‹ {value} ›"),
                            if is_selected { theme.key() } else { theme.text() },
                        ))
                    };
                    let note = self.setting_note(*id).unwrap_or_else(|| id.description().to_owned());
                    Row::new(vec![
                        Cell::from(Span::styled(format!("  {}", id.label()), theme.text())),
                        value_cell,
                        Cell::from(Span::styled(note, theme.muted())),
                    ])
                }
            })
            .collect();
        let widths = [Constraint::Length(30), Constraint::Length(28), Constraint::Fill(1)];
        let table = Table::new(rows, widths)
            .column_spacing(1)
            .row_highlight_style(theme.selected_row())
            .highlight_symbol(Span::styled("▶", theme.key()));
        frame.render_stateful_widget(table, table_area, &mut self.settings.table_state);
    }
}
