//! Help, log and dialogs drawn on top of the current screen.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph};

use crate::presentation::app::App;
use crate::presentation::app::dialogs::{Confirmation, Overlay, Severity};
use crate::presentation::input::COMMAND_REFERENCE;
use crate::presentation::log_buffer::LogBuffer;
use crate::presentation::widgets::chrome;

const KEY_REFERENCE: &[(&str, &[(&str, &str)])] = &[
    (
        "Everywhere",
        &[
            ("1 2 3 4 / Tab", "switch screen"),
            (":", "command line"),
            ("?", "this help"),
            ("L", "log"),
            ("a", "allocated ↔ apparent sizes"),
            ("p", "pause / resume background scanning"),
            ("q / Ctrl+C", "quit"),
        ],
    ),
    (
        "Explorer",
        &[
            ("⏎ → l", "open directory"),
            ("⌫ ← h u", "parent directory"),
            ("↑↓ j k, PgUp/PgDn, g G", "move"),
            ("space", "mark / unmark for deletion"),
            ("d", "delete marked (or current)"),
            ("x", "clear marks"),
            ("T", "heaviest files below here"),
            ("r / R", "rescan current / selected"),
            ("f", "show / hide files"),
            ("+ / -", "more / fewer rows"),
            ("o", "reveal in Finder"),
            ("/", "filter rows"),
        ],
    ),
    (
        "Heaviest files",
        &[
            ("space / d", "mark / delete"),
            ("⏎", "jump to the containing folder"),
            ("r", "run again"),
            ("+ / -", "change the limit"),
        ],
    ),
    (
        "Cleaner",
        &[
            ("space", "toggle item or whole category"),
            ("⏎", "fold / unfold category"),
            ("a / n", "select all / none"),
            ("d", "clean what is selected"),
            ("i", "what is this category?"),
            ("r", "search again"),
        ],
    ),
    (
        "Settings (:settings)",
        &[
            ("←→ ⏎ space", "change the value (applies at once)"),
            ("s", "save to the config file"),
            ("e", "edit the config file in $EDITOR"),
            ("R", "reset to defaults"),
        ],
    ),
];

impl App {
    pub(super) fn render_overlay(&self, frame: &mut Frame, overlay: &Overlay) {
        let theme = self.theme;
        match overlay {
            Overlay::Help => self.render_help(frame),
            // Rendered by `render_settings_overlay`, which needs mutable table state.
            Overlay::Settings => {}
            Overlay::Log => {
                let snapshot = LogBuffer::global().snapshot();
                let max_lines = frame.area().height.saturating_sub(8) as usize;
                let skip = snapshot.len().saturating_sub(max_lines);
                let mut lines: Vec<Line<'static>> = snapshot
                    .into_iter()
                    .skip(skip)
                    .map(|line| {
                        let style = match line.level {
                            log::Level::Error => theme.danger(),
                            log::Level::Warn => theme.warning(),
                            log::Level::Info => theme.text(),
                            _ => theme.muted(),
                        };
                        let age = line.at.elapsed().as_secs();
                        Line::from(vec![
                            Span::styled(format!("{age:>5}s ago  "), theme.faint()),
                            Span::styled(line.message, style),
                        ])
                    })
                    .collect();
                if lines.is_empty() {
                    lines.push(Line::from(Span::styled("nothing logged yet", theme.muted())));
                }
                let width = frame.area().width.saturating_sub(6).min(140);
                chrome::dialog(frame, &theme, "Log", lines, width, theme.border(true));
            }
            Overlay::Confirm(dialog) => {
                let mut lines: Vec<Line<'static>> = dialog
                    .lines
                    .iter()
                    .map(|line| Line::from(Span::styled(line.clone(), theme.text())))
                    .collect();
                if let Confirmation::TypedWord { typed, .. } = &dialog.confirmation {
                    lines.push(Line::from(vec![
                        Span::styled("> ", theme.key()),
                        Span::styled(typed.clone(), theme.text()),
                        Span::styled("▏", theme.key()),
                    ]));
                }
                let border: Style = if dialog.dangerous { theme.danger() } else { theme.border(true) };
                chrome::dialog(frame, &theme, &dialog.title, lines, 92, border);
            }
            Overlay::Message(message) => {
                let style = match message.severity {
                    Severity::Info => theme.text(),
                    Severity::Success => theme.success(),
                    Severity::Warning => theme.warning(),
                    Severity::Error => theme.danger(),
                };
                let mut lines: Vec<Line<'static>> =
                    message.lines.iter().map(|line| Line::from(Span::styled(line.clone(), style))).collect();
                lines.push(Line::raw(""));
                lines.push(Line::from(Span::styled("any key closes", theme.faint())));
                let border = match message.severity {
                    Severity::Error => theme.danger(),
                    Severity::Warning => theme.warning(),
                    _ => theme.border(true),
                };
                chrome::dialog(frame, &theme, &message.title, lines, 96, border);
            }
        }
    }
}

impl App {
    fn render_help(&self, frame: &mut Frame) {
        let theme = self.theme;
        let mut left: Vec<Line<'static>> = Vec::new();
        let mut right: Vec<Line<'static>> = Vec::new();
        for (index, (group, bindings)) in KEY_REFERENCE.iter().enumerate() {
            let column = if index < 2 { &mut left } else { &mut right };
            let _ = index;
            column.push(Line::from(Span::styled((*group).to_owned(), theme.title())));
            for (key, description) in *bindings {
                column.push(Line::from(vec![
                    Span::styled(format!("  {key:<26}"), theme.key()),
                    Span::styled((*description).to_owned(), theme.text()),
                ]));
            }
            column.push(Line::raw(""));
        }
        right.push(Line::from(Span::styled("Commands", theme.title())));
        for (command, description) in COMMAND_REFERENCE {
            right.push(Line::from(vec![
                Span::styled(format!("  {command:<26}"), theme.key()),
                Span::styled((*description).to_owned(), theme.text()),
            ]));
        }
        left.push(Line::from(Span::styled("any key closes", theme.faint())));

        let area = frame.area();
        let height = (left.len().max(right.len()) as u16 + 4).min(area.height.saturating_sub(2));
        let width = 132.min(area.width.saturating_sub(4));
        let dialog_area = chrome::centered(area, width, height);
        frame.render_widget(Clear, dialog_area);
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.border(true))
            .title_top(Line::from(" Help ").style(theme.title()))
            .padding(Padding::new(2, 2, 1, 1));
        let inner = block.inner(dialog_area);
        frame.render_widget(block, dialog_area);
        let [left_area, right_area] =
            Layout::horizontal([Constraint::Percentage(40), Constraint::Fill(1)]).areas(inner);
        frame.render_widget(Paragraph::new(left), left_area);
        frame.render_widget(Paragraph::new(right), right_area);
    }
}
