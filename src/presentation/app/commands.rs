//! Execution of `:` developer commands.

use std::io::Write;

use super::App;
use super::Screen;
use super::dialogs::{MessageDialog, Overlay, PendingAction, Severity};
use crate::domain::deletion::DeletionMode;
use crate::domain::storage::{SizeBase, SizeMode};
use crate::presentation::formatting;
use crate::presentation::input::DeveloperCommand;

impl App {
    pub(super) fn execute_command(&mut self, command: DeveloperCommand) {
        match command {
            DeveloperCommand::Quit => self.request_quit(),
            DeveloperCommand::Help => self.overlay = Some(Overlay::Help),
            DeveloperCommand::ShowLog => self.overlay = Some(Overlay::Log),
            DeveloperCommand::ShowStatistics => self.show_statistics(),
            DeveloperCommand::ShowConfigPath => {
                let path = self.services.config_store.path().display().to_string();
                let exists = self.services.config_store.exists();
                self.overlay = Some(Overlay::Message(MessageDialog {
                    title: "Configuration".to_owned(),
                    lines: vec![
                        path,
                        String::new(),
                        if exists {
                            "The file exists. Edit it and restart to apply.".to_owned()
                        } else {
                            "The file does not exist yet. Use :config save to write the current settings."
                                .to_owned()
                        },
                    ],
                    severity: Severity::Info,
                }));
            }
            DeveloperCommand::SaveConfig => self.save_settings(),
            DeveloperCommand::EditConfig => self.pending_foreground = Some(PendingAction::EditConfigFile),
            DeveloperCommand::ReloadConfig => self.reload_config_from_disk(),
            DeveloperCommand::Settings => self.overlay = Some(Overlay::Settings),
            DeveloperCommand::Theme(None) => {
                let mut lines = self.theme_listing();
                lines.push(String::new());
                lines.push(
                    ":theme <name> switches · [theme] in the config file overrides single colours".to_owned(),
                );
                self.overlay = Some(Overlay::Message(MessageDialog {
                    title: "Themes".to_owned(),
                    lines,
                    severity: Severity::Info,
                }));
            }
            DeveloperCommand::Theme(Some(name)) => {
                if self.apply_theme_name(&name) {
                    self.show_status(
                        format!("theme: {} (s in Settings saves it)", self.theme_name),
                        Severity::Info,
                    );
                }
            }
            DeveloperCommand::GoTo(path) => {
                let expanded = crate::application::configuration::expand_tilde(
                    &path.to_string_lossy(),
                    self.services.home.as_deref(),
                );
                self.open_path(&expanded);
            }
            DeveloperCommand::Rescan => {
                if let Some(current) = self.explorer.current {
                    self.services.coordinator.rescan(current, false);
                    self.show_status("rescanning", Severity::Info);
                }
            }
            DeveloperCommand::ExpandDense => {
                if let Some(current) = self.explorer.current {
                    self.services.coordinator.rescan(current, true);
                    self.show_status("expanding dense directory (this may take a while)", Severity::Warning);
                }
            }
            DeveloperCommand::SetRowLimit(limit) => {
                self.explorer.row_limit = limit.clamp(5, 100_000);
                self.show_status(format!("showing up to {} rows", self.explorer.row_limit), Severity::Info);
            }
            DeveloperCommand::SetWorkerThreads(count) => {
                self.services.coordinator.engine().set_worker_count(count);
                self.show_status(format!("scanner threads: {count}"), Severity::Info);
            }
            DeveloperCommand::SetDenseThreshold(threshold) => {
                self.services
                    .coordinator
                    .engine()
                    .update_settings(|settings| settings.policy.dense_directory_threshold = threshold);
                self.config.scan.dense_directory_threshold = threshold;
                self.show_status(format!("dense threshold: {threshold} entries"), Severity::Info);
            }
            DeveloperCommand::SetMaterializeDepth(depth) => {
                self.services
                    .coordinator
                    .engine()
                    .update_settings(|settings| settings.policy.materialize_depth = depth);
                self.config.scan.materialize_depth = depth;
                self.show_status(format!("materialise depth: {depth}"), Severity::Info);
            }
            DeveloperCommand::PauseBackground => {
                self.services.coordinator.engine().set_background_paused(true);
                self.show_status("background scanning paused", Severity::Info);
            }
            DeveloperCommand::ResumeBackground => {
                self.services.coordinator.engine().set_background_paused(false);
                self.show_status("background scanning resumed", Severity::Info);
            }
            DeveloperCommand::SetSizeMode(mode) => {
                let mode = match mode.as_str() {
                    "apparent" | "logical" => SizeMode::Apparent,
                    "allocated" | "disk" | "physical" => SizeMode::Allocated,
                    other => {
                        self.show_status(format!("unknown size mode `{other}`"), Severity::Warning);
                        return;
                    }
                };
                self.services.coordinator.set_size_mode(mode);
                self.show_status(format!("showing {} sizes", mode.label()), Severity::Info);
            }
            DeveloperCommand::SetSizeBase(base) => {
                self.size_base = match base.as_str() {
                    "binary" | "iec" | "1024" => SizeBase::Binary,
                    "decimal" | "si" | "1000" => SizeBase::Decimal,
                    other => {
                        self.show_status(format!("unknown size base `{other}`"), Severity::Warning);
                        return;
                    }
                };
                self.show_status("size base changed", Severity::Info);
            }
            DeveloperCommand::HeaviestFiles(limit) => self.start_heaviest_query(limit),
            DeveloperCommand::Reveal => {
                if let Some(path) = self.selected_explorer_path().or_else(|| self.current_path()) {
                    self.reveal_path(&path);
                }
            }
            DeveloperCommand::ListMounts => {
                let mounts = self.services.volumes.mount_points();
                for mount in &mounts {
                    log::info!("mount point: {}", mount.display());
                }
                self.show_status(
                    format!("{} mount points written to the log (L)", mounts.len()),
                    Severity::Info,
                );
                self.overlay = Some(Overlay::Log);
            }
            DeveloperCommand::SetDeletionMode(mode) => {
                self.deletion_mode = match mode.as_str() {
                    "on" | "trash" => DeletionMode::Trash,
                    "off" | "permanent" => DeletionMode::Permanent,
                    other => {
                        self.show_status(format!("unknown deletion mode `{other}`"), Severity::Warning);
                        return;
                    }
                };
                self.show_status(format!("deletion mode: {}", self.deletion_mode.label()), Severity::Info);
            }
            DeveloperCommand::Filter(filter) => {
                self.explorer.filter = filter.filter(|text| !text.is_empty());
                match &self.explorer.filter {
                    Some(text) => self.show_status(format!("filter: {text}"), Severity::Info),
                    None => self.show_status("filter cleared", Severity::Info),
                }
                self.screen = Screen::Explorer;
            }
            DeveloperCommand::Export(path) => self.export_rows(&path),
            DeveloperCommand::Cleaner => self.switch_screen(Screen::Cleaner),
            DeveloperCommand::Dashboard => self.switch_screen(Screen::Dashboard),
            DeveloperCommand::Explorer => self.switch_screen(Screen::Explorer),
            DeveloperCommand::ClearMarks => {
                self.marks.clear();
                self.show_status("marks cleared", Severity::Info);
            }
            DeveloperCommand::Unknown(reason) => self.show_status(reason, Severity::Warning),
        }
    }

    pub(super) fn sync_config_from_state(&mut self) {
        self.config.view.theme = self.theme_name.clone();
        self.config.view.show_files = self.explorer.show_files;
        self.config.view.row_limit = self.explorer.row_limit;
        self.config.view.heaviest_files_limit = self.heaviest.limit;
        self.config.view.size_mode = self.services.coordinator.size_mode().label().to_owned();
        self.config.view.size_base = match self.size_base {
            SizeBase::Decimal => "decimal".to_owned(),
            SizeBase::Binary => "binary".to_owned(),
        };
        self.config.deletion.default_mode = match self.deletion_mode {
            DeletionMode::Permanent => "permanent".to_owned(),
            DeletionMode::Trash => "trash".to_owned(),
        };
        self.config.scan.worker_threads = self.services.coordinator.engine().desired_worker_count();
    }

    fn show_statistics(&mut self) {
        let engine = self.services.coordinator.engine();
        let snapshot = engine.statistics().snapshot();
        let depths = engine.queue_depths();
        let settings = engine.settings();
        let tree = self.services.coordinator.tree().statistics();
        let lines = vec![
            format!("Elapsed              {}", formatting::duration(snapshot.elapsed_seconds)),
            format!("Directories listed   {}", formatting::count(snapshot.directories_listed)),
            format!("Files measured       {}", formatting::count(snapshot.files_measured)),
            format!(
                "Bytes measured       {}",
                formatting::size(
                    crate::domain::storage::ByteSize::new(snapshot.bytes_measured),
                    self.size_base
                )
            ),
            format!("Throughput           {} files/s", formatting::count(snapshot.files_per_second() as u64)),
            format!("Dense directories    {}", formatting::count(snapshot.dense_directories)),
            format!(
                "Unreadable           {} ({} permission denied)",
                snapshot.unreadable_directories, snapshot.permission_denied
            ),
            format!("Hard links skipped   {}", formatting::count(snapshot.hard_links_skipped)),
            String::new(),
            format!("Workers              {} ({} busy)", snapshot.worker_count, snapshot.busy_workers),
            format!(
                "Queue                urgent {} · focused {} · background {}",
                depths.urgent, depths.focused, depths.background
            ),
            format!("Background paused    {}", engine.is_background_paused()),
            format!("Reader               {}", engine.reader_name()),
            String::new(),
            format!(
                "Tree nodes           {} live · {} roots",
                formatting::count(tree.live_nodes as u64),
                tree.roots
            ),
            format!("Events applied       {}", formatting::count(self.services.coordinator.events_applied())),
            format!(
                "Dense threshold      {} entries",
                formatting::count(settings.policy.dense_directory_threshold as u64)
            ),
            format!("Materialise depth    {}", settings.policy.materialize_depth),
            format!("Max depth            {}", settings.policy.max_depth),
            format!("Mount points known   {}", settings.mount_points.len()),
            format!(
                "Focus                {}",
                engine
                    .queue()
                    .focus()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "none".to_owned())
            ),
        ];
        self.overlay = Some(Overlay::Message(MessageDialog {
            title: "Scanner statistics".to_owned(),
            lines,
            severity: Severity::Info,
        }));
    }

    fn export_rows(&mut self, path: &std::path::Path) {
        let expanded = crate::application::configuration::expand_tilde(
            &path.to_string_lossy(),
            self.services.home.as_deref(),
        );
        let result = std::fs::File::create(&expanded).and_then(|mut file| {
            writeln!(file, "bytes\titems\tkind\tpath")?;
            for row in &self.explorer.rows {
                writeln!(
                    file,
                    "{}\t{}\t{}\t{}",
                    row.size.as_u64(),
                    row.items.map(|items| items.to_string()).unwrap_or_default(),
                    if row.is_directory { "dir" } else { "file" },
                    row.path.display()
                )?;
            }
            Ok(())
        });
        match result {
            Ok(()) => self.show_status(
                format!("exported {} rows to {}", self.explorer.rows.len(), expanded.display()),
                Severity::Success,
            ),
            Err(error) => self.show_status(format!("export failed: {error}"), Severity::Error),
        }
    }
}
