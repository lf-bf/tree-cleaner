//! The dashboard: volumes, breakdown of the system root, scanner health.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table};

use crate::presentation::app::App;
use crate::presentation::app::dashboard_state::DashboardPanel;
use crate::presentation::app::explorer_state::RowBadge;
use crate::presentation::formatting;
use crate::presentation::widgets::chrome;

impl App {
    pub(super) fn render_dashboard(&mut self, frame: &mut Frame, area: Rect) {
        let [top, bottom] = Layout::vertical([Constraint::Min(8), Constraint::Length(6)]).areas(area);
        let [volumes_area, root_area] =
            Layout::horizontal([Constraint::Percentage(40), Constraint::Fill(1)]).areas(top);
        self.render_volumes(frame, volumes_area);
        self.render_root_breakdown(frame, root_area);
        self.render_scanner_panel(frame, bottom);
    }

    fn render_volumes(&self, frame: &mut Frame, area: Rect) {
        let theme = self.theme;
        let focused = self.dashboard.focus == DashboardPanel::Volumes;
        let panel = chrome::panel(&theme, Line::from(Span::styled(" Volumes ", theme.title())), focused);
        let inner = panel.inner(area);
        frame.render_widget(panel, area);
        let bar_width = inner.width.saturating_sub(4).max(10) as usize;
        let mut lines: Vec<Line<'static>> = Vec::new();
        for (index, volume) in self.dashboard.volumes.iter().enumerate() {
            let selected = index == self.dashboard.selected_volume;
            let ratio = volume.usage_ratio();
            let marker = if selected && focused { "▶ " } else { "  " };
            let name_style = if selected { theme.directory() } else { theme.text() };
            lines.push(Line::from(vec![
                Span::styled(marker.to_owned(), theme.key()),
                Span::styled(volume.name.clone(), name_style),
                Span::styled(format!("  {}", volume.mount_point.display()), theme.muted()),
                Span::styled(
                    format!(
                        "  {}{}",
                        volume.file_system,
                        // The sealed macOS system volume reports read-only although the
                        // data it exposes through firmlinks is writable; do not alarm.
                        if volume.is_read_only && !volume.is_system_root() { " · read-only" } else { "" }
                    ),
                    theme.faint(),
                ),
            ]));
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    formatting::bar(ratio, bar_width),
                    Style::new().fg(theme.usage_color(ratio)).bg(theme.bar_track),
                ),
            ]));
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!(
                        "{} used of {}",
                        formatting::size(volume.used(), self.size_base),
                        formatting::size(volume.total, self.size_base)
                    ),
                    theme.text(),
                ),
                Span::styled(
                    format!(
                        "  ·  {} free  ·  {}",
                        formatting::size(volume.available, self.size_base),
                        formatting::percent(ratio)
                    ),
                    Style::new().fg(theme.usage_color(ratio)),
                ),
            ]));
            lines.push(Line::raw(""));
        }
        if lines.is_empty() {
            lines.push(Line::styled("no volumes reported", theme.muted()));
        }
        frame.render_widget(Paragraph::new(lines), inner);
    }

    fn render_root_breakdown(&mut self, frame: &mut Frame, area: Rect) {
        let theme = self.theme;
        let focused = self.dashboard.focus == DashboardPanel::RootBreakdown;
        let coordinator = &self.services.coordinator;
        let tree = coordinator.tree();
        let mode = coordinator.size_mode();
        let root = self.dashboard.root_node.and_then(|id| tree.node(id).map(|node| (id, node)));
        let title = match root {
            Some((id, _)) => {
                let path = tree.path_of(id);
                Line::from(Span::styled(format!(" {} ", path.display()), theme.title()))
            }
            None => Line::from(Span::styled(" Root ", theme.title())),
        };
        let summary = match root {
            Some((_, node)) => {
                let progress = if node.is_complete() {
                    Span::styled(" ✓ ", theme.success())
                } else {
                    Span::styled(format!(" {} ", formatting::spinner(self.tick)), theme.key())
                };
                Line::from(vec![
                    Span::styled(
                        format!(
                            " {} · {} files ",
                            formatting::size(node.total_size().select(mode), self.size_base),
                            formatting::compact_count(node.total_file_count())
                        ),
                        theme.text(),
                    ),
                    progress,
                ])
            }
            None => Line::from(Span::styled(" background root scan disabled ", theme.muted())),
        };
        let panel = chrome::panel(&theme, title, focused).title_top(summary.right_aligned());
        let inner = panel.inner(area);
        frame.render_widget(panel, area);

        let Some((_, root_node)) = root else {
            frame.render_widget(
                Paragraph::new(
                    "Enable scan.background_root_scan in the configuration to see the breakdown of /.",
                )
                .style(theme.muted()),
                inner,
            );
            return;
        };
        let total = root_node.total_size().select(mode);
        let bar_width = inner.width.saturating_sub(46).clamp(6, 24) as usize;
        let rows: Vec<Row<'static>> = self
            .dashboard
            .root_rows
            .iter()
            .filter_map(|id| tree.node(*id))
            .map(|node| {
                let size = node.total_size().select(mode);
                let share = size.ratio_of(total);
                let badge =
                    RowBadge::from_flags(node.flags(), node.is_complete(), !node.children().is_empty());
                let (badge_text, badge_style) = match badge {
                    RowBadge::Scanning => (formatting::spinner(self.tick).to_owned(), theme.key()),
                    RowBadge::Complete => ("✓".to_owned(), theme.faint()),
                    RowBadge::Dense => ("≡".to_owned(), theme.warning()),
                    RowBadge::Denied => ("⊘".to_owned(), theme.danger()),
                    RowBadge::Failed => ("!".to_owned(), theme.danger()),
                    RowBadge::Boundary => ("⤳".to_owned(), theme.muted()),
                    RowBadge::Lazy | RowBadge::File => (String::new(), theme.faint()),
                };
                Row::new(vec![
                    Cell::from(node.display_name()).style(theme.directory()),
                    Cell::from(format!("{:>10}", formatting::size(size, self.size_base))).style(theme.text()),
                    Cell::from(format!("{:>6}", formatting::percent(share))).style(theme.muted()),
                    Cell::from(formatting::bar(share, bar_width))
                        .style(Style::new().fg(theme.share_color(share)).bg(theme.bar_track)),
                    Cell::from(format!("{:>8}", formatting::compact_count(node.total_item_count())))
                        .style(theme.muted()),
                    Cell::from(badge_text).style(badge_style),
                ])
            })
            .collect();
        let widths = [
            Constraint::Fill(1),
            Constraint::Length(10),
            Constraint::Length(6),
            Constraint::Length(bar_width as u16),
            Constraint::Length(8),
            Constraint::Length(2),
        ];
        let table = Table::new(rows, widths)
            .header(
                Row::new(vec!["Directory", "      Size", "     %", "", "   Items", ""])
                    .style(theme.muted().bold()),
            )
            .column_spacing(1)
            .row_highlight_style(if focused { theme.selected_row() } else { Style::default() })
            .highlight_symbol(Span::styled(if focused { "▶" } else { " " }, theme.key()));
        frame.render_stateful_widget(table, inner, &mut self.dashboard.root_table_state);
    }

    fn render_scanner_panel(&self, frame: &mut Frame, area: Rect) {
        let theme = self.theme;
        let engine = self.services.coordinator.engine();
        let snapshot = engine.statistics().snapshot();
        let depths = engine.queue_depths();
        let tree = self.services.coordinator.tree().statistics();
        let panel = chrome::panel(&theme, Line::from(Span::styled(" Scanner ", theme.title())), false);
        let inner = panel.inner(area);
        frame.render_widget(panel, area);
        let denied_hint = if snapshot.permission_denied > 0 {
            "  · grant Full Disk Access to your terminal to read protected folders"
        } else {
            ""
        };
        let lines = vec![
            Line::from(vec![
                Span::styled(
                    format!("{} files/s", formatting::count(snapshot.files_per_second() as u64)),
                    theme.key(),
                ),
                Span::styled(
                    format!(
                        "  ·  {} files  ·  {} directories  ·  {} measured  ·  {}",
                        formatting::count(snapshot.files_measured),
                        formatting::count(snapshot.directories_listed),
                        formatting::size(
                            crate::domain::storage::ByteSize::new(snapshot.bytes_measured),
                            self.size_base
                        ),
                        formatting::duration(snapshot.elapsed_seconds)
                    ),
                    theme.text(),
                ),
            ]),
            Line::from(vec![Span::styled(
                format!(
                    "{}/{} threads busy  ·  queue: {} focused, {} background  ·  {} nodes in memory  ·  reader: {}{}",
                    snapshot.busy_workers,
                    snapshot.worker_count,
                    formatting::count(depths.focused as u64),
                    formatting::count(depths.background as u64),
                    formatting::count(tree.live_nodes as u64),
                    engine.reader_name(),
                    if engine.is_background_paused() { "  ·  background paused (p)" } else { "" }
                ),
                theme.muted(),
            )]),
            Line::from(vec![
                Span::styled(
                    format!(
                        "denied: {}  ·  dense: {}  ·  hard links skipped: {}",
                        snapshot.permission_denied, snapshot.dense_directories, snapshot.hard_links_skipped
                    ),
                    if snapshot.permission_denied > 0 { theme.warning() } else { theme.faint() },
                ),
                Span::styled(denied_hint, theme.faint()),
            ]),
            chrome::key_hints(
                &theme,
                &[("2", "explorer"), ("4", "cleaner"), ("T", "heaviest files"), (":stats", "details")],
            ),
        ];
        frame.render_widget(Paragraph::new(lines), inner);
    }
}
