//! The directory explorer.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table};

use crate::presentation::app::App;
use crate::presentation::app::explorer_state::RowBadge;
use crate::presentation::formatting;
use crate::presentation::widgets::chrome;

const BAR_WIDTH: usize = 18;

impl App {
    pub(super) fn render_explorer(&mut self, frame: &mut Frame, area: Rect) {
        let theme = self.theme;
        let Some(current) = self.explorer.current else {
            frame.render_widget(
                Paragraph::new("No directory selected. Use :goto <path> or pick a volume on the dashboard.")
                    .style(theme.muted())
                    .alignment(Alignment::Center),
                area,
            );
            return;
        };
        let coordinator = &self.services.coordinator;
        let tree = coordinator.tree();
        let mode = coordinator.size_mode();
        let Some(node) = tree.node(current) else {
            return;
        };
        let path = tree.path_of(current);
        let home = self.services.home.clone();

        // Title: breadcrumb + summary.
        let path_text = formatting::home_relative(&path, home.as_deref());
        let title = Line::from(vec![
            Span::styled(" ", Style::default()),
            Span::styled(
                formatting::fit(&path_text, area.width.saturating_sub(48) as usize, true),
                theme.directory(),
            ),
            Span::styled(" ", Style::default()),
        ]);
        let progress = if node.is_complete() {
            Span::styled(" ✓ complete ", theme.success())
        } else {
            Span::styled(
                format!(
                    " {} scanning · {} pending ",
                    formatting::spinner(self.tick),
                    formatting::compact_count(u64::from(node.outstanding_work()))
                ),
                theme.key(),
            )
        };
        let summary = Line::from(vec![
            Span::styled(
                format!(
                    " {} · {} files · {} dirs ",
                    formatting::size(node.total_size().select(mode), self.size_base),
                    formatting::compact_count(node.total_file_count()),
                    formatting::compact_count(node.total_directory_count()),
                ),
                theme.text(),
            ),
            progress,
        ]);
        let panel = chrome::panel(&theme, title, true).title_top(summary.right_aligned());
        let inner = panel.inner(area);
        frame.render_widget(panel, area);

        let [notice_area, table_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(inner);
        self.render_explorer_notice(frame, notice_area, current);

        let total = node.total_size().select(mode);
        let bar_width = BAR_WIDTH.min(inner.width.saturating_sub(60).max(8) as usize);
        let rows: Vec<Row<'static>> = self
            .explorer
            .rows
            .iter()
            .enumerate()
            .map(|(index, row)| {
                let mark = if row.marked { Cell::from("●").style(theme.key()) } else { Cell::from(" ") };
                let name_style = if row.is_directory { theme.directory() } else { theme.file() };
                let name = Cell::from(Line::from(vec![
                    Span::styled(
                        format!("{} ", row.kind.symbol()),
                        if row.is_directory { theme.directory() } else { theme.faint() },
                    ),
                    Span::styled(row.name.clone(), name_style),
                ]));
                let size = Cell::from(format!("{:>10}", formatting::size(row.size, self.size_base)))
                    .style(theme.text());
                let share = Cell::from(format!("{:>6}", formatting::percent(row.share))).style(theme.muted());
                let bar = Cell::from(formatting::bar(row.share, bar_width))
                    .style(Style::new().fg(theme.share_color(row.share)).bg(theme.bar_track));
                let items = Cell::from(format!(
                    "{:>8}",
                    row.items.map(formatting::compact_count).unwrap_or_default()
                ))
                .style(theme.muted());
                let (badge_text, badge_style) = match row.badge {
                    RowBadge::Scanning => {
                        (format!("{} scanning", formatting::spinner(self.tick + index)), theme.key())
                    }
                    RowBadge::Complete => ("✓".to_owned(), theme.faint()),
                    RowBadge::Dense => ("≡ dense".to_owned(), theme.warning()),
                    RowBadge::Denied => ("⊘ denied".to_owned(), theme.danger()),
                    RowBadge::Failed => ("! error".to_owned(), theme.danger()),
                    RowBadge::Boundary => ("⤳ volume".to_owned(), theme.muted()),
                    RowBadge::Lazy => ("↓ lazy".to_owned(), theme.muted()),
                    RowBadge::File => (String::new(), theme.faint()),
                };
                Row::new(vec![
                    mark,
                    Cell::from(format!("{:>4}", index + 1)).style(theme.faint()),
                    name,
                    size,
                    share,
                    bar,
                    items,
                    Cell::from(badge_text).style(badge_style),
                ])
            })
            .collect();

        let header = Row::new(vec![
            Cell::from(""),
            Cell::from("   #"),
            Cell::from("Name"),
            Cell::from("      Size"),
            Cell::from("     %"),
            Cell::from(""),
            Cell::from("   Items"),
            Cell::from("State"),
        ])
        .style(theme.muted().bold());

        let widths = [
            Constraint::Length(1),
            Constraint::Length(4),
            Constraint::Fill(1),
            Constraint::Length(10),
            Constraint::Length(6),
            Constraint::Length(bar_width as u16),
            Constraint::Length(8),
            Constraint::Length(12),
        ];
        let table = Table::new(rows, widths)
            .header(header)
            .column_spacing(1)
            .row_highlight_style(theme.selected_row())
            .highlight_symbol(Span::styled("▶", theme.key()));
        frame.render_stateful_widget(table, table_area, &mut self.explorer.table_state);

        if self.explorer.rows.is_empty() {
            let message = if !node.is_complete() {
                format!("{} measuring…", formatting::spinner(self.tick))
            } else if node.flags().access_denied {
                "permission denied".to_owned()
            } else if self.explorer.filter.is_some() {
                "nothing matches the filter".to_owned()
            } else {
                "empty directory".to_owned()
            };
            let centered = chrome::centered(table_area, message.len() as u16 + 4, 1);
            frame.render_widget(Paragraph::new(message).style(theme.muted()), centered);
        }
        let _ = total;
    }

    fn render_explorer_notice(&self, frame: &mut Frame, area: Rect, current: crate::domain::storage::NodeId) {
        let theme = self.theme;
        let coordinator = &self.services.coordinator;
        let tree = coordinator.tree();
        let Some(node) = tree.node(current) else {
            return;
        };
        let flags = node.flags();
        let mut spans: Vec<Span<'static>> = Vec::new();
        if flags.dense {
            spans.push(Span::styled(
                format!(
                    "≡ dense directory ({} entries): children are summed, not listed · :expand forces a full listing",
                    formatting::compact_count(node.total_item_count())
                ),
                theme.warning(),
            ));
        } else if flags.access_denied {
            spans.push(Span::styled(
                "⊘ permission denied · grant Full Disk Access to your terminal (System Settings → Privacy & Security) or run with sudo",
                theme.danger(),
            ));
        } else if node.unreadable_descendants() > 0 {
            spans.push(Span::styled(
                format!("⊘ {} subdirectories could not be read", node.unreadable_descendants()),
                theme.warning(),
            ));
        }
        if let Some(listing) = coordinator.file_listing(current) {
            if listing.is_loading() {
                spans.push(Span::styled(
                    format!("  {} listing files", formatting::spinner(self.tick)),
                    theme.muted(),
                ));
            } else if let Some(files) = &listing.listing {
                if files.truncated {
                    spans.push(Span::styled(
                        format!(
                            "  showing the {} largest of {} files",
                            files.files.len(),
                            formatting::count(files.total_file_count)
                        ),
                        theme.muted(),
                    ));
                }
            }
        }
        if let Some(filter) = &self.explorer.filter {
            spans.push(Span::styled(format!("  filter: {filter} (esc clears)"), theme.key()));
        }
        if self.explorer.unlimited_row_count > self.explorer.rows.len() {
            spans.push(Span::styled(
                format!(
                    "  {} of {} rows (+/- changes the limit)",
                    self.explorer.rows.len(),
                    formatting::count(self.explorer.unlimited_row_count as u64)
                ),
                theme.faint(),
            ));
        }
        if spans.is_empty() {
            spans.push(Span::styled(
                format!(
                    "{} items · sorted by {} size",
                    formatting::count(self.explorer.unlimited_row_count as u64),
                    coordinator.size_mode().label()
                ),
                theme.faint(),
            ));
        }
        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }
}
