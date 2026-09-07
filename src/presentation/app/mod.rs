//! The interactive application: state, event loop and key handling.

pub mod cleaner_state;
pub mod commands;
pub mod dashboard_state;
pub mod dialogs;
pub mod explorer_state;
pub mod heaviest_state;
pub mod marks;
pub mod operation;
pub mod rows;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use self::cleaner_state::{CleanerRow, CleanerState};
use self::dashboard_state::{DashboardPanel, DashboardState};
use self::dialogs::{
    ConfirmDialog, Confirmation, MessageDialog, Overlay, PendingAction, Severity, StatusMessage,
};
use self::explorer_state::{ExplorerState, RowBadge, RowKey};
use self::heaviest_state::HeaviestState;
use self::marks::{MarkedItem, Marks};
use self::operation::{OperationKind, OperationReport, OperationRun, OperationSource};
use crate::application::cleaning::CleaningService;
use crate::application::configuration::AppConfig;
use crate::application::deletion::DeletionService;
use crate::application::scanning::TreeCoordinator;
use crate::domain::cleaning::{CleaningPlan, CleaningReport, CleaningStatus};
use crate::domain::deletion::{DeletionItem, DeletionMode, DeletionPlan, DeletionReport, DeletionStatus};
use crate::domain::ports::{FileRevealer, PrivilegeEscalator, VolumeProvider};
use crate::domain::storage::{ByteSize, MeasuredSize, NodeId, RootPurpose, SizeBase};
use crate::infrastructure::config::TomlConfigStore;
use crate::presentation::formatting;
use crate::presentation::input::CommandLineState;
use crate::presentation::terminal::TerminalSession;
use crate::presentation::theme::Theme;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Dashboard,
    Explorer,
    Heaviest,
    Cleaner,
}

impl Screen {
    pub const ALL: [Self; 4] = [Self::Dashboard, Self::Explorer, Self::Heaviest, Self::Cleaner];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Dashboard => "Dashboard",
            Self::Explorer => "Explorer",
            Self::Heaviest => "Heaviest",
            Self::Cleaner => "Cleaner",
        }
    }

    pub const fn next(self) -> Self {
        match self {
            Self::Dashboard => Self::Explorer,
            Self::Explorer => Self::Heaviest,
            Self::Heaviest => Self::Cleaner,
            Self::Cleaner => Self::Dashboard,
        }
    }

    pub const fn previous(self) -> Self {
        match self {
            Self::Dashboard => Self::Cleaner,
            Self::Explorer => Self::Dashboard,
            Self::Heaviest => Self::Explorer,
            Self::Cleaner => Self::Heaviest,
        }
    }
}

/// Everything the interface talks to.
pub struct Services {
    pub coordinator: TreeCoordinator,
    pub cleaning: CleaningService,
    pub deletion: DeletionService,
    pub escalator: Arc<dyn PrivilegeEscalator>,
    pub revealer: Arc<dyn FileRevealer>,
    pub volumes: Arc<dyn VolumeProvider>,
    pub config_store: TomlConfigStore,
    pub home: Option<PathBuf>,
}

pub struct App {
    pub services: Services,
    pub config: AppConfig,
    pub theme: Theme,
    pub size_base: SizeBase,
    pub deletion_mode: DeletionMode,
    pub screen: Screen,
    pub overlay: Option<Overlay>,
    pub command_line: CommandLineState,
    pub command_mode: bool,
    pub dashboard: DashboardState,
    pub explorer: ExplorerState,
    pub heaviest: HeaviestState,
    pub cleaner: CleanerState,
    pub marks: Marks,
    /// A deletion or cleaning run being followed in the progress modal. While it is set,
    /// every key except Ctrl+C goes to the modal, so nothing can race with the run.
    pub operation: Option<OperationRun>,
    pub pending_elevation: Option<PendingAction>,
    pub status: Option<StatusMessage>,
    pub tick: usize,
    pub should_quit: bool,
    pub frame_interval: Duration,
    pub started_at: Instant,
}

impl App {
    pub fn new(services: Services, config: AppConfig, theme: Theme, start_path: Option<PathBuf>) -> Self {
        let volumes = services.volumes.volumes();
        let row_limit = config.view.row_limit.max(5);
        let heaviest_limit = config.view.heaviest_files_limit.max(5);
        let mut app = Self {
            size_base: config.size_base(),
            deletion_mode: config.deletion_mode(),
            frame_interval: Duration::from_millis(config.view.frame_interval_ms.clamp(16, 250)),
            services,
            config,
            theme,
            screen: Screen::Dashboard,
            overlay: None,
            command_line: CommandLineState::default(),
            command_mode: false,
            dashboard: DashboardState::new(volumes),
            explorer: ExplorerState::new(row_limit),
            heaviest: HeaviestState::new(heaviest_limit),
            cleaner: CleanerState::default(),
            marks: Marks::default(),
            operation: None,
            pending_elevation: None,
            status: None,
            tick: 0,
            should_quit: false,
            started_at: Instant::now(),
        };
        app.bootstrap_scans(start_path);
        app
    }

    /// Starts the background scan of the system root and opens the explorer where asked.
    fn bootstrap_scans(&mut self, start_path: Option<PathBuf>) {
        let root_path = PathBuf::from("/");
        if self.config.scan.background_root_scan {
            let root = self.services.coordinator.start_root_scan(root_path.clone(), RootPurpose::Filesystem);
            self.dashboard.root_node = Some(root);
        }
        let start = start_path.or_else(|| self.services.home.clone()).unwrap_or(root_path);
        let start = std::fs::canonicalize(&start).unwrap_or(start);
        let node = self.services.coordinator.node_for_path(&start);
        if self.dashboard.root_node.is_none() {
            self.dashboard.root_node = self.services.coordinator.tree().root_of(node).map(|root| root.node);
        }
        self.explorer.current = Some(node);
        self.services.coordinator.focus(node);
        if start_path_was_given(&start, self.services.home.as_deref()) {
            self.screen = Screen::Explorer;
        }
        log::info!("scanning started at {}", start.display());
    }

    // ----------------------------------------------------------------------------------
    // Main loop
    // ----------------------------------------------------------------------------------

    pub fn run(&mut self, session: &mut TerminalSession) -> Result<()> {
        while !self.should_quit {
            let deadline = Instant::now() + self.frame_interval;
            let mut first = true;
            loop {
                let remaining = deadline.saturating_duration_since(Instant::now());
                let timeout = if first { remaining } else { Duration::ZERO };
                first = false;
                if !event::poll(timeout)? {
                    break;
                }
                match event::read()? {
                    Event::Key(key)
                        if key.kind == KeyEventKind::Press || key.kind == KeyEventKind::Repeat =>
                    {
                        self.handle_key(key);
                    }
                    _ => {}
                }
                if self.should_quit {
                    break;
                }
            }
            if let Some(action) = self.pending_elevation.take() {
                let outcome = session.suspend(|| self.run_elevation(action))?;
                self.show_status(outcome.0, outcome.1);
            }
            self.tick();
            session.terminal().draw(|frame| self.render(frame))?;
        }
        self.services.coordinator.engine().shutdown();
        Ok(())
    }

    fn tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        self.services.coordinator.pump_events(Duration::from_millis(6));
        let Services { coordinator, cleaning, .. } = &mut self.services;
        if cleaning.has_started() {
            cleaning.update(coordinator, coordinator.size_mode());
        }
        self.poll_operation();
        if self.status.as_ref().is_some_and(StatusMessage::is_expired) {
            self.status = None;
        }
        if self.dashboard.refreshed_at.elapsed() > Duration::from_secs(15) {
            self.dashboard.volumes = self.services.volumes.volumes();
            self.dashboard.refreshed_at = Instant::now();
        }
        match self.screen {
            Screen::Explorer => self.rebuild_explorer_rows(),
            Screen::Dashboard => self.rebuild_dashboard_rows(),
            Screen::Heaviest => self.rebuild_heaviest_rows(),
            Screen::Cleaner => self.rebuild_cleaner_rows(),
        }
    }

    // ----------------------------------------------------------------------------------
    // Key handling
    // ----------------------------------------------------------------------------------

    fn handle_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
            self.should_quit = true;
            return;
        }
        if self.operation.is_some() {
            self.handle_operation_key(key);
            return;
        }
        if self.overlay.is_some() {
            self.handle_overlay_key(key);
            return;
        }
        if self.command_mode {
            self.handle_command_line_key(key);
            return;
        }
        match key.code {
            KeyCode::Char(':') => {
                self.command_mode = true;
                self.command_line.open();
            }
            KeyCode::Char('?') => self.overlay = Some(Overlay::Help),
            KeyCode::Char('L') => self.overlay = Some(Overlay::Log),
            KeyCode::Tab => self.switch_screen(self.screen.next()),
            KeyCode::BackTab => self.switch_screen(self.screen.previous()),
            KeyCode::Char('1') => self.switch_screen(Screen::Dashboard),
            KeyCode::Char('2') => self.switch_screen(Screen::Explorer),
            KeyCode::Char('3') => self.switch_screen(Screen::Heaviest),
            KeyCode::Char('4') => self.switch_screen(Screen::Cleaner),
            KeyCode::Char('q') => self.request_quit(),
            KeyCode::Char('p') => self.toggle_background_pause(),
            KeyCode::Char('a') if self.screen != Screen::Cleaner => self.toggle_size_mode(),
            _ => match self.screen {
                Screen::Dashboard => self.handle_dashboard_key(key),
                Screen::Explorer => self.handle_explorer_key(key),
                Screen::Heaviest => self.handle_heaviest_key(key),
                Screen::Cleaner => self.handle_cleaner_key(key),
            },
        }
    }

    fn handle_overlay_key(&mut self, key: KeyEvent) {
        let Some(overlay) = self.overlay.take() else {
            return;
        };
        match overlay {
            Overlay::Help | Overlay::Log | Overlay::Message(_) => {
                // Any key dismisses.
            }
            Overlay::Confirm(mut dialog) => match key.code {
                KeyCode::Esc => self.show_status("cancelled", Severity::Info),
                KeyCode::Enter => {
                    if dialog.typed_word_matches() {
                        self.perform(dialog.action);
                    } else {
                        self.overlay = Some(Overlay::Confirm(dialog));
                    }
                }
                KeyCode::Char('y') | KeyCode::Char('Y')
                    if matches!(dialog.confirmation, Confirmation::YesKey) =>
                {
                    self.perform(dialog.action);
                }
                KeyCode::Char('n') | KeyCode::Char('N')
                    if matches!(dialog.confirmation, Confirmation::YesKey) =>
                {
                    self.show_status("cancelled", Severity::Info);
                }
                KeyCode::Backspace => {
                    if let Confirmation::TypedWord { typed, .. } = &mut dialog.confirmation {
                        typed.pop();
                    }
                    self.overlay = Some(Overlay::Confirm(dialog));
                }
                KeyCode::Char(character) => {
                    if let Confirmation::TypedWord { typed, .. } = &mut dialog.confirmation {
                        typed.push(character);
                    }
                    self.overlay = Some(Overlay::Confirm(dialog));
                }
                _ => self.overlay = Some(Overlay::Confirm(dialog)),
            },
        }
    }

    fn handle_command_line_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.command_mode = false,
            KeyCode::Enter => {
                self.command_mode = false;
                if let Some(command) = self.command_line.submit() {
                    self.execute_command(command);
                }
            }
            KeyCode::Backspace => {
                if self.command_line.backspace() {
                    self.command_mode = false;
                }
            }
            KeyCode::Left => self.command_line.move_left(),
            KeyCode::Right => self.command_line.move_right(),
            KeyCode::Home => self.command_line.move_home(),
            KeyCode::End => self.command_line.move_end(),
            KeyCode::Up => self.command_line.history_previous(),
            KeyCode::Down => self.command_line.history_next(),
            KeyCode::Char(character) => self.command_line.insert(character),
            _ => {}
        }
    }

    fn handle_dashboard_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.dashboard.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.dashboard.move_selection(1),
            KeyCode::Left | KeyCode::Right | KeyCode::Char('h') | KeyCode::Char('l') => {
                self.dashboard.toggle_focus()
            }
            KeyCode::Enter | KeyCode::Char('e') => match self.dashboard.focus {
                DashboardPanel::Volumes => {
                    if let Some(volume) = self.dashboard.selected_volume() {
                        let mount = volume.mount_point.clone();
                        self.open_path(&mount);
                    }
                }
                DashboardPanel::RootBreakdown => {
                    if let Some(node) = self.dashboard.selected_root_child() {
                        self.enter_directory(node);
                        self.switch_screen(Screen::Explorer);
                    }
                }
            },
            KeyCode::Char('c') => self.switch_screen(Screen::Cleaner),
            KeyCode::Char('r') => {
                self.dashboard.volumes = self.services.volumes.volumes();
                self.dashboard.refreshed_at = Instant::now();
                if let Some(root) = self.dashboard.root_node {
                    self.services.coordinator.rescan(root, false);
                }
                self.show_status("volumes refreshed, root rescan started", Severity::Info);
            }
            _ => {}
        }
    }

    fn handle_explorer_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.explorer.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.explorer.move_selection(1),
            KeyCode::PageUp => self.explorer.move_selection(-15),
            KeyCode::PageDown => self.explorer.move_selection(15),
            KeyCode::Home | KeyCode::Char('g') => self.explorer.select(0),
            KeyCode::End | KeyCode::Char('G') => {
                let last = self.explorer.rows.len().saturating_sub(1);
                self.explorer.select(last);
            }
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => self.open_selected_row(),
            KeyCode::Backspace | KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('u') => {
                self.go_to_parent()
            }
            KeyCode::Char(' ') => self.toggle_mark_on_selected(),
            KeyCode::Char('d') => self.request_deletion(),
            KeyCode::Char('x') => {
                self.marks.clear();
                self.show_status("marks cleared", Severity::Info);
            }
            KeyCode::Char('r') => {
                if let Some(current) = self.explorer.current {
                    self.services.coordinator.rescan(current, false);
                    self.show_status("rescanning current directory", Severity::Info);
                }
            }
            KeyCode::Char('R') => {
                if let Some(RowKey::Directory(node)) = self.explorer.selected_row().map(|row| row.key.clone())
                {
                    self.services.coordinator.rescan(node, false);
                    self.show_status("rescanning selected directory", Severity::Info);
                }
            }
            KeyCode::Char('T') | KeyCode::Char('t') => self.start_heaviest_query(None),
            KeyCode::Char('f') => {
                self.explorer.show_files = !self.explorer.show_files;
                let state = if self.explorer.show_files { "shown" } else { "hidden" };
                self.show_status(format!("files {state}"), Severity::Info);
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                self.set_row_limit(self.explorer.row_limit.saturating_mul(2))
            }
            KeyCode::Char('-') => self.set_row_limit(self.explorer.row_limit / 2),
            KeyCode::Char('o') => self.reveal_selected(),
            KeyCode::Char('/') => {
                self.command_mode = true;
                self.command_line.open();
                for character in "filter ".chars() {
                    self.command_line.insert(character);
                }
            }
            KeyCode::Char('c') => self.switch_screen(Screen::Cleaner),
            KeyCode::Esc if self.explorer.filter.is_some() => {
                self.explorer.filter = None;
                self.show_status("filter cleared", Severity::Info);
            }
            _ => {}
        }
    }

    fn handle_heaviest_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.heaviest.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.heaviest.move_selection(1),
            KeyCode::PageUp => self.heaviest.move_selection(-15),
            KeyCode::PageDown => self.heaviest.move_selection(15),
            KeyCode::Home | KeyCode::Char('g') => {
                self.heaviest.selected = 0;
                self.heaviest.clamp_selection();
            }
            KeyCode::End | KeyCode::Char('G') => {
                self.heaviest.selected = usize::MAX;
                self.heaviest.clamp_selection();
            }
            KeyCode::Char(' ') => self.toggle_mark_on_heaviest(),
            KeyCode::Char('d') => self.request_deletion(),
            KeyCode::Char('x') => {
                self.marks.clear();
                self.show_status("marks cleared", Severity::Info);
            }
            KeyCode::Enter => self.jump_to_heaviest_file(),
            KeyCode::Esc | KeyCode::Backspace => self.switch_screen(Screen::Explorer),
            KeyCode::Char('r') => self.start_heaviest_query(Some(self.heaviest.limit)),
            KeyCode::Char('o') => {
                if let Some(file) = self.heaviest.selected_file() {
                    let path = file.path.clone();
                    self.reveal_path(&path);
                }
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                let limit = self.heaviest.limit.saturating_mul(2).min(10_000);
                self.start_heaviest_query(Some(limit));
            }
            KeyCode::Char('-') => {
                let limit = (self.heaviest.limit / 2).max(10);
                self.start_heaviest_query(Some(limit));
            }
            _ => {}
        }
    }

    fn handle_cleaner_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.cleaner.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.cleaner.move_selection(1),
            KeyCode::PageUp => self.cleaner.move_selection(-15),
            KeyCode::PageDown => self.cleaner.move_selection(15),
            KeyCode::Home | KeyCode::Char('g') => {
                self.cleaner.selected = 0;
                self.cleaner.clamp_selection();
            }
            KeyCode::End | KeyCode::Char('G') => {
                self.cleaner.selected = usize::MAX;
                self.cleaner.clamp_selection();
            }
            KeyCode::Char(' ') => match self.cleaner.selected_row().cloned() {
                Some(CleanerRow::Candidate(id)) => self.services.cleaning.toggle(id),
                Some(CleanerRow::Category(category)) => self.services.cleaning.toggle_category(category),
                None => {}
            },
            KeyCode::Enter | KeyCode::Right | KeyCode::Left => match self.cleaner.selected_row().cloned() {
                Some(CleanerRow::Category(category)) => {
                    if !self.cleaner.collapsed.remove(&category) {
                        self.cleaner.collapsed.insert(category);
                    }
                }
                Some(CleanerRow::Candidate(id)) => self.services.cleaning.toggle(id),
                None => {}
            },
            KeyCode::Char('a') => self.services.cleaning.set_all(true),
            KeyCode::Char('n') => self.services.cleaning.set_all(false),
            KeyCode::Char('d') | KeyCode::Char('x') | KeyCode::Char('c') => self.request_cleaning(),
            KeyCode::Char('r') => self.restart_cleaner(),
            KeyCode::Char('o') => {
                if let Some(CleanerRow::Candidate(id)) = self.cleaner.selected_row().cloned() {
                    if let Some(path) = self
                        .services
                        .cleaning
                        .candidate(id)
                        .and_then(|candidate| candidate.path().map(Path::to_path_buf))
                    {
                        self.reveal_path(&path);
                    }
                }
            }
            KeyCode::Char('i') => {
                if let Some(row) = self.cleaner.selected_row().cloned() {
                    let category = match row {
                        CleanerRow::Category(category) => category,
                        CleanerRow::Candidate(id) => match self.services.cleaning.candidate(id) {
                            Some(candidate) => candidate.category,
                            None => return,
                        },
                    };
                    self.overlay = Some(Overlay::Message(MessageDialog {
                        title: category.label().to_owned(),
                        lines: vec![
                            category.description().to_owned(),
                            String::new(),
                            format!("Configuration id: {}", category.identifier()),
                        ],
                        severity: Severity::Info,
                    }));
                }
            }
            KeyCode::Esc => self.switch_screen(Screen::Explorer),
            _ => {}
        }
    }

    // ----------------------------------------------------------------------------------
    // Navigation
    // ----------------------------------------------------------------------------------

    pub fn switch_screen(&mut self, screen: Screen) {
        if screen == Screen::Cleaner && !self.services.cleaning.has_started() {
            self.restart_cleaner();
        }
        if screen == Screen::Heaviest && self.heaviest.query.is_none() {
            self.start_heaviest_query(None);
            return;
        }
        self.screen = screen;
    }

    fn restart_cleaner(&mut self) {
        let Services { coordinator, cleaning, .. } = &mut self.services;
        cleaning.begin(coordinator);
        self.cleaner.last_report = None;
        self.screen = Screen::Cleaner;
        self.show_status("looking for reclaimable space…", Severity::Info);
    }

    pub fn open_path(&mut self, path: &Path) {
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        if !path.is_dir() {
            self.show_status(format!("{} is not a directory", path.display()), Severity::Warning);
            return;
        }
        let node = self.services.coordinator.node_for_path(&path);
        self.enter_directory(node);
        self.screen = Screen::Explorer;
    }

    pub fn enter_directory(&mut self, node: NodeId) {
        let tree = self.services.coordinator.tree();
        let Some(tree_node) = tree.node(node) else {
            return;
        };
        if tree_node.flags().boundary {
            // Mount points and skipped paths are measured as roots of their own.
            let path = tree.path_of(node);
            let root = self.services.coordinator.start_root_scan(path, RootPurpose::Filesystem);
            if root == node {
                self.show_status("this directory is not scanned", Severity::Warning);
                return;
            }
            return self.enter_directory(root);
        }
        self.explorer.current = Some(node);
        self.explorer.selected = 0;
        self.explorer.selected_key = None;
        self.explorer.table_state.select(Some(0));
        self.services.coordinator.ensure_expanded(node);
        self.services.coordinator.focus(node);
        self.rebuild_explorer_rows();
    }

    fn open_selected_row(&mut self) {
        let Some(row) = self.explorer.selected_row() else {
            return;
        };
        match &row.key {
            RowKey::Directory(node) if row.badge == RowBadge::Denied => {
                let node = *node;
                self.show_status(
                    "permission denied: grant Full Disk Access to your terminal or run with sudo",
                    Severity::Warning,
                );
                self.enter_directory(node);
            }
            RowKey::Directory(node) => {
                let node = *node;
                self.enter_directory(node);
            }
            RowKey::File(_) => {
                self.show_status("files: space marks, d deletes, o reveals", Severity::Info);
            }
        }
    }

    fn go_to_parent(&mut self) {
        let Some(current) = self.explorer.current else {
            return;
        };
        let parent = self.services.coordinator.tree().node(current).and_then(|node| node.parent());
        match parent {
            Some(parent) => {
                self.explorer.current = Some(parent);
                self.explorer.selected_key = Some(RowKey::Directory(current));
                self.services.coordinator.focus(parent);
                self.rebuild_explorer_rows();
                self.explorer.resync_selection();
            }
            None => {
                let path = self.services.coordinator.tree().path_of(current);
                match path.parent() {
                    Some(parent_path) if parent_path != path => {
                        let parent_path = parent_path.to_path_buf();
                        self.open_path(&parent_path);
                        self.explorer.selected_key = Some(RowKey::Directory(current));
                        self.rebuild_explorer_rows();
                        self.explorer.resync_selection();
                    }
                    _ => self.switch_screen(Screen::Dashboard),
                }
            }
        }
    }

    fn jump_to_heaviest_file(&mut self) {
        let Some(file) = self.heaviest.selected_file() else {
            return;
        };
        let path = file.path.clone();
        let Some(parent) = path.parent() else {
            return;
        };
        let parent = parent.to_path_buf();
        let node = self.services.coordinator.node_for_path(&parent);
        self.enter_directory(node);
        if let Some(name) = path.file_name() {
            self.explorer.selected_key = Some(RowKey::File(name.to_os_string()));
        }
        self.screen = Screen::Explorer;
        self.rebuild_explorer_rows();
        self.explorer.resync_selection();
    }

    fn set_row_limit(&mut self, limit: usize) {
        self.explorer.row_limit = limit.clamp(5, 100_000);
        self.show_status(format!("showing up to {} rows", self.explorer.row_limit), Severity::Info);
    }

    fn toggle_size_mode(&mut self) {
        let mode = self.services.coordinator.size_mode().toggled();
        self.services.coordinator.set_size_mode(mode);
        self.show_status(format!("showing {} sizes", mode.label()), Severity::Info);
    }

    fn toggle_background_pause(&mut self) {
        let engine = self.services.coordinator.engine();
        let paused = !engine.is_background_paused();
        engine.set_background_paused(paused);
        self.show_status(
            if paused {
                "background scanning paused (focused directory still scans)"
            } else {
                "background scanning resumed"
            },
            Severity::Info,
        );
    }

    pub fn start_heaviest_query(&mut self, limit: Option<usize>) {
        let Some(origin) = self.explorer.current else {
            self.show_status("open a directory first", Severity::Warning);
            return;
        };
        if let Some(previous) = self.heaviest.query.take() {
            self.services.coordinator.cancel_heaviest_query(previous);
        }
        if let Some(limit) = limit {
            self.heaviest.limit = limit.clamp(5, 10_000);
        }
        self.heaviest.query = self.services.coordinator.start_heaviest_query(origin, self.heaviest.limit);
        self.heaviest.origin = Some(origin);
        self.heaviest.rows.clear();
        self.heaviest.selected = 0;
        self.screen = Screen::Heaviest;
    }

    // ----------------------------------------------------------------------------------
    // Marks and deletion
    // ----------------------------------------------------------------------------------

    fn toggle_mark_on_selected(&mut self) {
        let Some(row) = self.explorer.selected_row().cloned() else {
            return;
        };
        let node = match &row.key {
            RowKey::Directory(node) => Some(*node),
            RowKey::File(_) => None,
        };
        let item = MarkedItem {
            size: row.size,
            is_directory: row.is_directory,
            node,
            parent_node: self.explorer.current,
        };
        let marked = self.marks.toggle(row.path.clone(), item);
        self.explorer.move_selection(1);
        let total = self.marks.total_size();
        self.show_status(
            format!(
                "{} {} · {} marked · {}",
                if marked { "marked" } else { "unmarked" },
                row.name,
                self.marks.len(),
                formatting::size(total, self.size_base)
            ),
            Severity::Info,
        );
    }

    fn toggle_mark_on_heaviest(&mut self) {
        let Some(file) = self.heaviest.selected_file().cloned() else {
            return;
        };
        let parent_node =
            file.path.parent().and_then(|parent| self.services.coordinator.tree().locate(parent));
        let item = MarkedItem {
            size: file.size.select(self.services.coordinator.size_mode()),
            is_directory: false,
            node: None,
            parent_node,
        };
        let marked = self.marks.toggle(file.path.clone(), item);
        self.heaviest.move_selection(1);
        self.show_status(
            format!(
                "{} · {} marked · {}",
                if marked { "marked" } else { "unmarked" },
                self.marks.len(),
                formatting::size(self.marks.total_size(), self.size_base)
            ),
            Severity::Info,
        );
    }

    fn request_deletion(&mut self) {
        if self.operation.is_some() {
            self.show_status("a deletion is already running", Severity::Warning);
            return;
        }
        if self.marks.is_empty() {
            match self.screen {
                Screen::Explorer => self.toggle_mark_on_selected(),
                Screen::Heaviest => self.toggle_mark_on_heaviest(),
                _ => {}
            }
            if self.marks.is_empty() {
                return;
            }
        }
        // Refresh sizes of marked directories from the tree.
        let tree = self.services.coordinator.tree();
        let mode = self.services.coordinator.size_mode();
        let refreshed: Vec<(PathBuf, ByteSize)> = self
            .marks
            .iter()
            .filter_map(|(path, item)| {
                item.node
                    .and_then(|node| tree.node(node))
                    .map(|node| (path.clone(), node.total_size().select(mode)))
            })
            .collect();
        for (path, size) in refreshed {
            self.marks.update_size(&path, size);
        }
        let plan = self.marks.plan(self.deletion_mode);
        let mut lines: Vec<String> = plan
            .items
            .iter()
            .take(12)
            .map(|item| {
                format!(
                    "{:>10}  {}",
                    formatting::size(item.size, self.size_base),
                    formatting::home_relative(&item.path, self.services.home.as_deref())
                )
            })
            .collect();
        if plan.items.len() > 12 {
            lines.push(format!("… and {} more", plan.items.len() - 12));
        }
        lines.push(String::new());
        lines.push(format!(
            "{} items · {} · mode: {}",
            plan.len(),
            formatting::size(plan.total_size(), self.size_base),
            self.deletion_mode.label()
        ));
        let confirmation = match self.deletion_mode {
            DeletionMode::Permanent => {
                lines.push(String::new());
                lines.push("Type  yes  and press Enter to delete permanently. Esc cancels.".to_owned());
                Confirmation::TypedWord { expected: "yes".to_owned(), typed: String::new() }
            }
            DeletionMode::Trash => {
                lines.push(String::new());
                lines.push("Press y to move to the Trash, n or Esc to cancel.".to_owned());
                Confirmation::YesKey
            }
        };
        self.overlay = Some(Overlay::Confirm(ConfirmDialog {
            title: "Delete".to_owned(),
            lines,
            confirmation,
            action: PendingAction::Delete(plan),
            dangerous: self.deletion_mode == DeletionMode::Permanent,
        }));
    }

    fn request_cleaning(&mut self) {
        if self.operation.is_some() {
            self.show_status("a cleaning run is already in progress", Severity::Warning);
            return;
        }
        let plan = self.services.cleaning.build_plan();
        if plan.is_empty() {
            self.show_status("nothing selected", Severity::Warning);
            return;
        }
        let mut lines: Vec<String> = plan
            .candidates
            .iter()
            .take(14)
            .map(|candidate| {
                format!(
                    "{:>10}  {}",
                    candidate
                        .size
                        .map(|size| formatting::size(size, self.size_base))
                        .unwrap_or_else(|| "?".to_owned()),
                    candidate.label
                )
            })
            .collect();
        if plan.len() > 14 {
            lines.push(format!("… and {} more", plan.len() - 14));
        }
        lines.push(String::new());
        lines.push(format!(
            "{} items · about {} to reclaim{}",
            plan.len(),
            formatting::size(plan.estimated_reclaim(), self.size_base),
            if plan.requires_privileges() { " · some need sudo" } else { "" }
        ));
        lines.push(String::new());
        lines.push("Type  yes  and press Enter to clean. Esc cancels.".to_owned());
        self.overlay = Some(Overlay::Confirm(ConfirmDialog {
            title: "Clean".to_owned(),
            lines,
            confirmation: Confirmation::TypedWord { expected: "yes".to_owned(), typed: String::new() },
            action: PendingAction::Clean(plan),
            dangerous: true,
        }));
    }

    fn request_quit(&mut self) {
        self.should_quit = true;
    }

    fn perform(&mut self, action: PendingAction) {
        match action {
            PendingAction::Delete(plan) => self.start_deletion(plan),
            PendingAction::Clean(plan) => self.start_cleaning(plan),
            PendingAction::Quit => self.should_quit = true,
            elevation @ (PendingAction::ElevateDelete(_) | PendingAction::ElevateClean { .. }) => {
                self.pending_elevation = Some(elevation);
            }
        }
    }

    fn start_deletion(&mut self, plan: DeletionPlan) {
        let total = plan.len();
        let handle = self.services.deletion.execute_in_background(plan);
        self.operation = Some(OperationRun::new(
            OperationKind::Deletion,
            OperationSource::Deletion(handle),
            total,
            self.services.home.clone(),
        ));
        log::info!("deleting {total} items");
    }

    fn start_cleaning(&mut self, plan: CleaningPlan) {
        let total = plan.len();
        let handle = self.services.cleaning.executor().execute_in_background(plan);
        self.operation = Some(OperationRun::new(
            OperationKind::Cleaning,
            OperationSource::Cleaning(handle),
            total,
            self.services.home.clone(),
        ));
        self.cleaner.running = true;
        log::info!("cleaning {total} targets");
    }

    /// Feeds the progress modal. The tree is corrected the moment the run finishes, while
    /// input is still locked, so nothing can race with the update.
    fn poll_operation(&mut self) {
        let report = match &mut self.operation {
            Some(run) => run.drain(),
            None => return,
        };
        match report {
            Some(OperationReport::Deletion(report)) => self.absorb_deletion_report(&report),
            Some(OperationReport::Cleaning(report)) => self.absorb_cleaning_report(&report),
            None => {}
        }
    }

    /// While a run is active every key is swallowed: Esc asks for a cancellation between
    /// items, and once the run finished any key closes the modal. Ctrl+C is handled before
    /// this and still aborts the program.
    fn handle_operation_key(&mut self, key: KeyEvent) {
        let finished = self.operation.as_ref().is_some_and(OperationRun::is_finished);
        if finished {
            self.dismiss_operation();
            return;
        }
        if key.code == KeyCode::Esc {
            if let Some(run) = &mut self.operation {
                run.request_cancel();
                log::info!("cancellation requested");
            }
        }
    }

    fn dismiss_operation(&mut self) {
        let Some(run) = self.operation.take() else {
            return;
        };
        self.cleaner.running = false;
        match run.report {
            Some(OperationReport::Deletion(report)) => self.follow_up_deletion(report),
            Some(OperationReport::Cleaning(report)) => self.follow_up_cleaning(report),
            None => {}
        }
    }

    /// Reflects a finished deletion in the tree: removed items disappear, items that were
    /// only partially removed are measured again.
    fn absorb_deletion_report(&mut self, report: &DeletionReport) {
        for outcome in &report.outcomes {
            if outcome.status.is_success() {
                self.absorb_removed_item(&outcome.item);
            } else if outcome.is_partial() {
                if let Some(node) = outcome.item.node {
                    self.services.coordinator.rescan(node, false);
                }
            }
        }
    }

    /// Dialogs and status shown after the progress modal is closed.
    fn follow_up_deletion(&mut self, report: DeletionReport) {
        let needing = report.needing_privileges();
        let failures = report.failures();
        let reclaimed = formatting::size(report.reclaimed(), self.size_base);
        let succeeded = report.succeeded().count();
        let cancelled = report.cancelled_count();
        if !failures.is_empty() {
            let lines: Vec<String> = failures
                .iter()
                .take(15)
                .map(|outcome| {
                    let reason = match &outcome.status {
                        DeletionStatus::Failed(reason) | DeletionStatus::Refused(reason) => reason.as_str(),
                        _ => "",
                    };
                    format!("{}: {reason}", outcome.item.path.display())
                })
                .collect();
            self.overlay = Some(Overlay::Message(MessageDialog {
                title: format!("{} items could not be deleted", failures.len()),
                lines,
                severity: Severity::Error,
            }));
        }
        if !needing.is_empty() {
            let total: ByteSize =
                needing.iter().map(|item| item.size).fold(ByteSize::ZERO, ByteSize::saturating_add);
            let mut lines: Vec<String> =
                needing.iter().take(10).map(|item| item.path.display().to_string()).collect();
            if needing.len() > 10 {
                lines.push(format!("… and {} more", needing.len() - 10));
            }
            lines.push(String::new());
            lines.push(format!(
                "{} items ({}) need administrator privileges.",
                needing.len(),
                formatting::size(total, self.size_base)
            ));
            lines.push(format!(
                "Press y to run {} in the terminal (you will be asked for your password), n to skip.",
                self.services.escalator.describe()
            ));
            self.overlay = Some(Overlay::Confirm(ConfirmDialog {
                title: "Administrator privileges required".to_owned(),
                lines,
                confirmation: Confirmation::YesKey,
                action: PendingAction::ElevateDelete(needing),
                dangerous: true,
            }));
        }
        let cancelled_note = if cancelled > 0 { format!(" · {cancelled} cancelled") } else { String::new() };
        self.show_status(
            format!("{succeeded} items removed · {reclaimed} reclaimed{cancelled_note}"),
            if succeeded > 0 { Severity::Success } else { Severity::Warning },
        );
    }

    /// Updates the tree and the marks after something disappeared from disk.
    fn absorb_removed_item(&mut self, item: &DeletionItem) {
        let parent_node = self.marks.parent_of(&item.path);
        self.marks.remove(&item.path);
        match item.node {
            Some(node) => self.services.coordinator.remove_subtree(node),
            None => {
                let parent = parent_node.or_else(|| {
                    item.path.parent().and_then(|parent| self.services.coordinator.tree().locate(parent))
                });
                if let Some(parent) = parent {
                    let size = MeasuredSize::new(item.size, item.size);
                    self.services.coordinator.tree_mut().forget_files(parent, size, 1);
                    self.services.coordinator.invalidate_listing(parent);
                    self.services.coordinator.request_file_listing(parent, true);
                }
            }
        }
    }

    fn absorb_cleaning_report(&mut self, report: &CleaningReport) {
        let Services { coordinator, cleaning, .. } = &mut self.services;
        cleaning.absorb_report(report, coordinator);
    }

    /// Dialogs and status shown after the progress modal is closed.
    fn follow_up_cleaning(&mut self, report: CleaningReport) {
        let needing = report.needing_privileges();
        let cleaned = report.cleaned_count();
        let failed = report.failed_count();
        let cancelled = report.cancelled_count();
        let reclaimed = formatting::size(report.reclaimed_estimate, self.size_base);
        if !needing.is_empty() {
            let (whole, contents) = self.services.cleaning.privileged_paths(&needing);
            let mut lines: Vec<String> = whole
                .iter()
                .map(|path| path.display().to_string())
                .chain(contents.iter().map(|path| format!("{}/*", path.display())))
                .take(10)
                .collect();
            lines.push(String::new());
            lines.push(format!(
                "Press y to run {} in the terminal (you will be asked for your password), n to skip.",
                self.services.escalator.describe()
            ));
            self.overlay = Some(Overlay::Confirm(ConfirmDialog {
                title: "Administrator privileges required".to_owned(),
                lines,
                confirmation: Confirmation::YesKey,
                action: PendingAction::ElevateClean { whole, contents, candidates: needing },
                dangerous: true,
            }));
        } else if failed > 0 {
            let lines: Vec<String> = report
                .outcomes
                .iter()
                .filter(|outcome| matches!(outcome.status, CleaningStatus::Failed(_)))
                .take(15)
                .map(|outcome| format!("{}: {}", outcome.label, outcome.detail))
                .collect();
            self.overlay = Some(Overlay::Message(MessageDialog {
                title: format!("{failed} items failed"),
                lines,
                severity: Severity::Error,
            }));
        }
        self.cleaner.last_report = Some(report);
        let mut notes = String::new();
        if failed > 0 {
            notes.push_str(&format!(" · {failed} failed"));
        }
        if cancelled > 0 {
            notes.push_str(&format!(" · {cancelled} cancelled"));
        }
        self.show_status(
            format!("{cleaned} cleaned · about {reclaimed} reclaimed{notes}"),
            if cleaned > 0 { Severity::Success } else { Severity::Warning },
        );
    }

    /// Runs with the terminal handed back to the user. Returns a status line.
    fn run_elevation(&mut self, action: PendingAction) -> (String, Severity) {
        match action {
            PendingAction::ElevateDelete(items) => {
                let paths: Vec<PathBuf> = items.iter().map(|item| item.path.clone()).collect();
                match self.services.escalator.remove_paths(&paths) {
                    Ok(()) => {
                        let total: ByteSize =
                            items.iter().map(|item| item.size).fold(ByteSize::ZERO, ByteSize::saturating_add);
                        for item in &items {
                            self.absorb_removed_item(item);
                        }
                        (
                            format!(
                                "{} items removed with privileges · {}",
                                items.len(),
                                formatting::size(total, self.size_base)
                            ),
                            Severity::Success,
                        )
                    }
                    Err(error) => (format!("privileged deletion failed: {error}"), Severity::Error),
                }
            }
            PendingAction::ElevateClean { whole, contents, candidates } => {
                let result = self
                    .services
                    .escalator
                    .remove_paths(&whole)
                    .and_then(|_| self.services.escalator.remove_directory_contents(&contents));
                match result {
                    Ok(()) => {
                        let report = CleaningReport {
                            outcomes: candidates
                                .iter()
                                .map(|id| crate::domain::cleaning::CleaningOutcome {
                                    candidate: *id,
                                    label: String::new(),
                                    status: CleaningStatus::Cleaned,
                                    detail: String::new(),
                                    reclaimed: ByteSize::ZERO,
                                    entries_removed: 0,
                                })
                                .collect(),
                            reclaimed_estimate: ByteSize::ZERO,
                        };
                        let Services { coordinator, cleaning, .. } = &mut self.services;
                        cleaning.absorb_report(&report, coordinator);
                        (format!("{} locations cleaned with privileges", candidates.len()), Severity::Success)
                    }
                    Err(error) => (format!("privileged cleaning failed: {error}"), Severity::Error),
                }
            }
            _ => ("nothing to do".to_owned(), Severity::Info),
        }
    }

    // ----------------------------------------------------------------------------------
    // Helpers
    // ----------------------------------------------------------------------------------

    pub fn show_status(&mut self, text: impl Into<String>, severity: Severity) {
        let text = text.into();
        match severity {
            Severity::Error => log::error!("{text}"),
            Severity::Warning => log::warn!("{text}"),
            _ => log::info!("{text}"),
        }
        self.status = Some(StatusMessage::new(text, severity));
    }

    fn reveal_selected(&mut self) {
        let path = match self.explorer.selected_row() {
            Some(row) => row.path.clone(),
            None => match self.explorer.current {
                Some(current) => self.services.coordinator.tree().path_of(current),
                None => return,
            },
        };
        self.reveal_path(&path);
    }

    pub fn reveal_path(&mut self, path: &Path) {
        match self.services.revealer.reveal(path) {
            Ok(()) => self.show_status(format!("revealed {}", path.display()), Severity::Info),
            Err(error) => self.show_status(format!("could not reveal: {error}"), Severity::Error),
        }
    }

    pub fn current_path(&self) -> Option<PathBuf> {
        self.explorer.current.map(|node| self.services.coordinator.tree().path_of(node))
    }
}

fn start_path_was_given(start: &Path, home: Option<&Path>) -> bool {
    home.is_none_or(|home| home != start)
}
