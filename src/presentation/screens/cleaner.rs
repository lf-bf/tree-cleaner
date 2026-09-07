//! The cleaner: reclaimable space grouped by category.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table};

use crate::application::cleaning::DockerStatus;
use crate::domain::cleaning::{CleaningLocation, Protection};
use crate::domain::storage::ByteSize;
use crate::presentation::app::App;
use crate::presentation::app::cleaner_state::CleanerRow;
use crate::presentation::formatting;
use crate::presentation::widgets::chrome;

impl App {
    pub(super) fn render_cleaner(&mut self, frame: &mut Frame, area: Rect) {
        let theme = self.theme;
        let cleaning = &self.services.cleaning;
        let coordinator = &self.services.coordinator;
        let (selected_count, selected_total) = self.cleaner_selected_total();
        let busy = cleaning.is_busy(coordinator);
        let title = Line::from(Span::styled(" Cleaner ", theme.title()));
        let state = if self.cleaner.running {
            Span::styled(format!(" {} cleaning… ", formatting::spinner(self.tick)), theme.key())
        } else if busy {
            Span::styled(
                format!(" {} discovering and measuring… ", formatting::spinner(self.tick)),
                theme.key(),
            )
        } else {
            Span::styled(" ✓ ready ", theme.success())
        };
        let summary = Line::from(vec![
            Span::styled(
                format!(
                    " {} selected · about {} reclaimable ",
                    selected_count,
                    formatting::size(selected_total, self.size_base)
                ),
                theme.text(),
            ),
            state,
        ]);
        let panel = chrome::panel(&theme, title, true).title_top(summary.right_aligned());
        let inner = panel.inner(area);
        frame.render_widget(panel, area);

        let [notice_area, table_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(inner);
        let docker = match cleaning.docker_status() {
            DockerStatus::Disabled => Span::styled("docker: disabled in config", theme.faint()),
            DockerStatus::Checking => {
                Span::styled(format!("docker: {} checking", formatting::spinner(self.tick)), theme.muted())
            }
            DockerStatus::Available { version } => {
                Span::styled(format!("docker: {version}"), theme.success())
            }
            DockerStatus::Unavailable { reason } => Span::styled(
                format!("docker: unavailable ({})", formatting::fit(reason, 60, false)),
                theme.warning(),
            ),
        };
        let mut notice = vec![docker];
        if let Some(report) = &self.cleaner.last_report {
            notice.push(Span::styled(
                format!(
                    "  ·  last run: {} cleaned, {} failed, about {} freed",
                    report.cleaned_count(),
                    report.failed_count(),
                    formatting::size(report.reclaimed_estimate, self.size_base)
                ),
                theme.muted(),
            ));
        }
        frame.render_widget(Paragraph::new(Line::from(notice)), notice_area);

        let candidates = cleaning.candidates();
        let rows: Vec<Row<'static>> = self
            .cleaner
            .rows
            .iter()
            .map(|row| match row {
                CleanerRow::Category(category) => {
                    let members: Vec<_> =
                        candidates.iter().filter(|candidate| candidate.category == *category).collect();
                    let total = members
                        .iter()
                        .filter_map(|candidate| candidate.size)
                        .fold(ByteSize::ZERO, ByteSize::saturating_add);
                    let selected = members.iter().filter(|candidate| candidate.is_actionable()).count();
                    let folded = self.cleaner.collapsed.contains(category);
                    Row::new(vec![
                        Cell::from(if folded { "▸" } else { "▾" }).style(theme.key()),
                        Cell::from(format!("{} ({})", category.label(), members.len())).style(theme.title()),
                        Cell::from(format!("{:>10}", formatting::size(total, self.size_base)))
                            .style(theme.text()),
                        Cell::from(format!("{selected}/{} selected", members.len())).style(theme.muted()),
                    ])
                }
                CleanerRow::Candidate(id) => {
                    let Some(candidate) = candidates.iter().find(|candidate| candidate.id == *id) else {
                        return Row::new(vec![
                            Cell::from(""),
                            Cell::from("?"),
                            Cell::from(""),
                            Cell::from(""),
                        ]);
                    };
                    let (checkbox, checkbox_style) = match (&candidate.protection, candidate.selected) {
                        (Protection::None, true) => ("[x]", theme.success()),
                        (Protection::None, false) => ("[ ]", theme.muted()),
                        (_, _) => ("[-]", theme.faint()),
                    };
                    let measuring = candidate
                        .measurement_node
                        .and_then(|node| coordinator.tree().node(node))
                        .is_some_and(|node| !node.is_complete());
                    let size_text = match candidate.size {
                        Some(size) if !measuring => formatting::size(size, self.size_base),
                        Some(size) => format!(
                            "{} {}",
                            formatting::spinner(self.tick),
                            formatting::size(size, self.size_base)
                        ),
                        None => match candidate.location {
                            CleaningLocation::Command { .. } => "?".to_owned(),
                            _ => formatting::spinner(self.tick).to_owned(),
                        },
                    };
                    let (status_text, status_style) = match &candidate.protection {
                        Protection::Rule(rule) => (format!("protected: {rule}"), theme.warning()),
                        Protection::InUse(reason) => (format!("in use: {reason}"), theme.warning()),
                        Protection::None if candidate.requires_privileges => {
                            ("needs sudo".to_owned(), theme.warning())
                        }
                        Protection::None => match &candidate.location {
                            CleaningLocation::DirectoryContents(_) => {
                                ("empties folder".to_owned(), theme.faint())
                            }
                            CleaningLocation::Directory(_) => ("removes folder".to_owned(), theme.faint()),
                            CleaningLocation::DockerImage { .. } => {
                                ("docker image rm".to_owned(), theme.faint())
                            }
                            CleaningLocation::DockerPrune(_) => ("docker prune".to_owned(), theme.faint()),
                            CleaningLocation::Command { .. } => ("runs command".to_owned(), theme.faint()),
                        },
                    };
                    let label_style = if candidate.is_protected() { theme.faint() } else { theme.file() };
                    Row::new(vec![
                        Cell::from(checkbox).style(checkbox_style),
                        Cell::from(format!("   {}", candidate.label)).style(label_style),
                        Cell::from(format!("{:>10}", size_text)).style(theme.text()),
                        Cell::from(status_text).style(status_style),
                    ])
                }
            })
            .collect();
        let widths =
            [Constraint::Length(3), Constraint::Fill(1), Constraint::Length(12), Constraint::Length(30)];
        let table = Table::new(rows, widths)
            .header(Row::new(vec!["", "Target", "      Size", "Notes"]).style(theme.muted().bold()))
            .column_spacing(1)
            .row_highlight_style(theme.selected_row())
            .highlight_symbol(Span::styled("▶", theme.key()));
        frame.render_stateful_widget(table, table_area, &mut self.cleaner.table_state);

        if self.cleaner.rows.is_empty() {
            let message = if busy {
                format!("{} looking for reclaimable space…", formatting::spinner(self.tick))
            } else {
                "nothing to clean was found".to_owned()
            };
            let centered = chrome::centered(table_area, message.len() as u16 + 4, 1);
            frame.render_widget(Paragraph::new(message).style(theme.muted()), centered);
        }
    }
}
