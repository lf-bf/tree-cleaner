//! The settings screen: live editing of preferences and the configuration file behind it.

use std::process::Command;

use ratatui::crossterm::event::{KeyCode, KeyEvent};

use super::App;
use super::Screen;
use super::dialogs::{ConfirmDialog, Confirmation, MessageDialog, Overlay, PendingAction, Severity};
use super::settings_state::SettingId;
use crate::application::configuration::AppConfig;
use crate::application::scanning::ScanEngine;
use crate::domain::deletion::DeletionMode;
use crate::domain::storage::SizeBase;
use crate::presentation::formatting;
use crate::presentation::theme::{THEME_NAMES, Theme};

const ROW_LIMIT_STEPS: [usize; 9] = [10, 25, 50, 100, 200, 500, 1_000, 5_000, 100_000];
const HEAVIEST_STEPS: [usize; 7] = [25, 50, 100, 250, 500, 1_000, 10_000];
const DENSE_STEPS: [usize; 9] = [1_000, 2_000, 5_000, 10_000, 20_000, 50_000, 100_000, 500_000, 1_000_000];

fn step_through(steps: &[usize], current: usize, delta: i32) -> usize {
    let position = steps.iter().position(|step| *step >= current).unwrap_or(steps.len() - 1);
    let next = (position as i64 + i64::from(delta)).clamp(0, steps.len() as i64 - 1);
    steps[next as usize]
}

impl App {
    /// The palette the configuration asks for, degraded to the 16-colour one when the
    /// terminal cannot show truecolor. Returns the effective name, the theme and a warning.
    pub fn resolve_theme(config: &AppConfig, truecolor: bool) -> (String, Theme, Option<String>) {
        let requested = config.view.theme.trim().to_ascii_lowercase();
        let requested = if requested.is_empty() { "claude".to_owned() } else { requested };
        let (name, base) = match Theme::by_name(&requested) {
            Some(theme) => (requested.clone(), theme),
            None => ("claude".to_owned(), Theme::claude()),
        };
        let mut warning = if name != requested {
            Some(format!("unknown theme `{requested}` in the configuration, using claude"))
        } else {
            None
        };
        let themed = match base.with_overrides(&config.theme) {
            Ok(theme) => theme,
            Err(reason) => {
                warning = Some(reason);
                base
            }
        };
        if !truecolor && name != "basic" {
            return (name, Theme::basic(), warning);
        }
        (name, themed, warning)
    }

    pub(super) fn handle_settings_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.settings.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.settings.move_selection(1),
            KeyCode::PageUp => self.settings.move_selection(-6),
            KeyCode::PageDown => self.settings.move_selection(6),
            KeyCode::Home | KeyCode::Char('g') => self.settings.select_first(),
            KeyCode::End | KeyCode::Char('G') => self.settings.select_last(),
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('-') => self.change_selected_setting(-1),
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Char('+') | KeyCode::Char('=') => {
                self.change_selected_setting(1)
            }
            KeyCode::Enter | KeyCode::Char(' ') => match self.settings.selected_setting() {
                Some(id) if id.is_action() => self.run_setting_action(id),
                Some(_) => self.change_selected_setting(1),
                None => {}
            },
            KeyCode::Char('s') => self.save_settings(),
            KeyCode::Char('e') => self.run_setting_action(SettingId::EditConfig),
            KeyCode::Char('R') => self.run_setting_action(SettingId::ResetDefaults),
            KeyCode::Esc => self.switch_screen(Screen::Explorer),
            _ => {}
        }
    }

    fn change_selected_setting(&mut self, delta: i32) {
        if let Some(id) = self.settings.selected_setting() {
            if id.is_action() {
                return;
            }
            self.adjust_setting(id, delta);
        }
    }

    /// Current value of a setting, as shown in the middle column.
    pub fn setting_value(&self, id: SettingId) -> String {
        let config = &self.config;
        let on_off = |value: bool| if value { "on" } else { "off" }.to_owned();
        match id {
            SettingId::Theme => self.theme_name.clone(),
            SettingId::SizeMode => self.services.coordinator.size_mode().label().to_owned(),
            SettingId::SizeBase => match self.size_base {
                SizeBase::Decimal => "decimal (GB)".to_owned(),
                SizeBase::Binary => "binary (GiB)".to_owned(),
            },
            SettingId::ShowFiles => on_off(self.explorer.show_files),
            SettingId::RowLimit => formatting::count(self.explorer.row_limit as u64),
            SettingId::HeaviestLimit => formatting::count(self.heaviest.limit as u64),
            SettingId::WorkerThreads => self.services.coordinator.engine().desired_worker_count().to_string(),
            SettingId::DenseThreshold => formatting::count(config.scan.dense_directory_threshold as u64),
            SettingId::MaterializeDepth => config.scan.materialize_depth.to_string(),
            SettingId::BackgroundRootScan => on_off(config.scan.background_root_scan),
            SettingId::BackgroundPaused => {
                if self.services.coordinator.engine().is_background_paused() { "paused" } else { "running" }
                    .to_owned()
            }
            SettingId::DeletionMode => self.deletion_mode.label().to_owned(),
            SettingId::DockerEnabled => on_off(config.cleaner.docker.enabled),
            SettingId::DockerPruneContainers => on_off(config.cleaner.docker.prune_containers),
            SettingId::DockerPruneVolumes => on_off(config.cleaner.docker.prune_volumes),
            SettingId::DockerPruneBuildCache => on_off(config.cleaner.docker.prune_build_cache),
            SettingId::DiscoveryMaxDepth => config.cleaner.discovery_max_depth.to_string(),
            SettingId::SaveConfig
            | SettingId::EditConfig
            | SettingId::ReloadConfig
            | SettingId::ResetDefaults => "⏎".to_owned(),
        }
    }

    /// Extra context for the right column, when the static description is not enough.
    pub fn setting_note(&self, id: SettingId) -> Option<String> {
        match id {
            SettingId::Theme => {
                let description = Theme::describe(&self.theme_name);
                if !self.truecolor && self.theme_name != "basic" {
                    Some(format!("{description} · this terminal has no truecolor, showing basic"))
                } else if !self.config.theme.is_empty() {
                    Some(format!(
                        "{description} · {} colours overridden in [theme]",
                        self.config.theme.entries().len()
                    ))
                } else {
                    Some(description.to_owned())
                }
            }
            SettingId::SaveConfig => Some(format!(
                "{}{}",
                formatting::home_relative(self.services.config_store.path(), self.services.home.as_deref()),
                if self.services.config_store.exists() { "" } else { " (not written yet)" }
            )),
            _ => None,
        }
    }

    fn adjust_setting(&mut self, id: SettingId, delta: i32) {
        match id {
            SettingId::Theme => {
                let next = Theme::neighbour_name(&self.theme_name, delta as isize);
                self.apply_theme_name(next);
            }
            SettingId::SizeMode => {
                let mode = self.services.coordinator.size_mode().toggled();
                self.services.coordinator.set_size_mode(mode);
                self.config.view.size_mode = mode.label().to_owned();
            }
            SettingId::SizeBase => {
                self.size_base = match self.size_base {
                    SizeBase::Decimal => SizeBase::Binary,
                    SizeBase::Binary => SizeBase::Decimal,
                };
                self.config.view.size_base =
                    if self.size_base == SizeBase::Binary { "binary" } else { "decimal" }.to_owned();
            }
            SettingId::ShowFiles => {
                self.explorer.show_files = !self.explorer.show_files;
                self.config.view.show_files = self.explorer.show_files;
            }
            SettingId::RowLimit => {
                self.explorer.row_limit = step_through(&ROW_LIMIT_STEPS, self.explorer.row_limit, delta);
                self.config.view.row_limit = self.explorer.row_limit;
            }
            SettingId::HeaviestLimit => {
                self.heaviest.limit = step_through(&HEAVIEST_STEPS, self.heaviest.limit, delta);
                self.config.view.heaviest_files_limit = self.heaviest.limit;
            }
            SettingId::WorkerThreads => {
                let current = self.services.coordinator.engine().desired_worker_count() as i64;
                let next = (current + i64::from(delta)).clamp(1, 64) as usize;
                self.services.coordinator.engine().set_worker_count(next);
                self.config.scan.worker_threads = next;
            }
            SettingId::DenseThreshold => {
                let next = step_through(&DENSE_STEPS, self.config.scan.dense_directory_threshold, delta);
                self.config.scan.dense_directory_threshold = next;
                self.services
                    .coordinator
                    .engine()
                    .update_settings(|settings| settings.policy.dense_directory_threshold = next);
            }
            SettingId::MaterializeDepth => {
                let next =
                    (i64::from(self.config.scan.materialize_depth) + i64::from(delta)).clamp(1, 64) as u16;
                self.config.scan.materialize_depth = next;
                self.services
                    .coordinator
                    .engine()
                    .update_settings(|settings| settings.policy.materialize_depth = next);
            }
            SettingId::BackgroundRootScan => {
                self.config.scan.background_root_scan = !self.config.scan.background_root_scan;
            }
            SettingId::BackgroundPaused => {
                let engine = self.services.coordinator.engine();
                engine.set_background_paused(!engine.is_background_paused());
                return;
            }
            SettingId::DeletionMode => {
                self.deletion_mode = match self.deletion_mode {
                    DeletionMode::Permanent => DeletionMode::Trash,
                    DeletionMode::Trash => DeletionMode::Permanent,
                };
                self.config.deletion.default_mode =
                    if self.deletion_mode == DeletionMode::Trash { "trash" } else { "permanent" }.to_owned();
            }
            SettingId::DockerEnabled => {
                self.config.cleaner.docker.enabled = !self.config.cleaner.docker.enabled;
                self.push_config_to_cleaner();
            }
            SettingId::DockerPruneContainers => {
                self.config.cleaner.docker.prune_containers = !self.config.cleaner.docker.prune_containers;
                self.push_config_to_cleaner();
            }
            SettingId::DockerPruneVolumes => {
                self.config.cleaner.docker.prune_volumes = !self.config.cleaner.docker.prune_volumes;
                self.push_config_to_cleaner();
            }
            SettingId::DockerPruneBuildCache => {
                self.config.cleaner.docker.prune_build_cache = !self.config.cleaner.docker.prune_build_cache;
                self.push_config_to_cleaner();
            }
            SettingId::DiscoveryMaxDepth => {
                let next = (i64::from(self.config.cleaner.discovery_max_depth) + i64::from(delta))
                    .clamp(2, 64) as u16;
                self.config.cleaner.discovery_max_depth = next;
                self.push_config_to_cleaner();
            }
            SettingId::SaveConfig
            | SettingId::EditConfig
            | SettingId::ReloadConfig
            | SettingId::ResetDefaults => {
                return;
            }
        }
        if !id.is_session_only() {
            self.settings.dirty = true;
        }
    }

    fn push_config_to_cleaner(&mut self) {
        if let Err(error) = self.services.cleaning.set_config(self.config.clone()) {
            self.show_status(format!("cleaner configuration rejected: {error}"), Severity::Error);
        }
    }

    fn run_setting_action(&mut self, id: SettingId) {
        match id {
            SettingId::SaveConfig => self.save_settings(),
            SettingId::EditConfig => self.pending_foreground = Some(PendingAction::EditConfigFile),
            SettingId::ReloadConfig => self.reload_config_from_disk(),
            SettingId::ResetDefaults => {
                self.overlay = Some(Overlay::Confirm(ConfirmDialog {
                    title: "Reset preferences".to_owned(),
                    lines: vec![
                        "Every setting goes back to the built-in default, including the theme.".to_owned(),
                        "Nothing is written until you save (s).".to_owned(),
                        String::new(),
                        "Press y to reset, n or Esc to keep the current values.".to_owned(),
                    ],
                    confirmation: Confirmation::YesKey,
                    action: PendingAction::ResetSettings,
                    dangerous: false,
                }));
            }
            _ => {}
        }
    }

    /// Switches the palette by name and remembers it in the configuration.
    pub fn apply_theme_name(&mut self, name: &str) -> bool {
        let Some(base) = Theme::by_name(name) else {
            self.show_status(
                format!("unknown theme `{name}`; available: {}", THEME_NAMES.join(", ")),
                Severity::Warning,
            );
            return false;
        };
        let canonical = THEME_NAMES
            .iter()
            .find(|candidate| Theme::by_name(candidate) == Some(base))
            .copied()
            .unwrap_or("claude");
        self.theme_name = canonical.to_owned();
        self.config.view.theme = canonical.to_owned();
        let themed = base.with_overrides(&self.config.theme).unwrap_or(base);
        self.theme = if self.truecolor || canonical == "basic" { themed } else { Theme::basic() };
        self.settings.dirty = true;
        true
    }

    /// Writes every current value to the configuration file.
    pub fn save_settings(&mut self) {
        self.sync_config_from_state();
        match self.services.config_store.save(&self.config) {
            Ok(()) => {
                self.settings.dirty = false;
                self.settings.last_saved = Some(std::time::Instant::now());
                let path = formatting::home_relative(
                    self.services.config_store.path(),
                    self.services.home.as_deref(),
                );
                self.show_status(format!("preferences saved to {path}"), Severity::Success);
            }
            Err(error) => self.show_status(format!("could not save preferences: {error}"), Severity::Error),
        }
    }

    /// Makes a whole configuration effective: theme, view, scanner policy, cleaner rules.
    pub fn apply_config(&mut self, config: AppConfig) -> Result<(), String> {
        self.services.cleaning.set_config(config.clone()).map_err(|error| error.to_string())?;
        let (name, theme, warning) = Self::resolve_theme(&config, self.truecolor);
        self.theme = theme;
        self.theme_name = name;
        self.size_base = config.size_base();
        self.deletion_mode = config.deletion_mode();
        self.services.coordinator.set_size_mode(config.size_mode());
        self.explorer.row_limit = config.view.row_limit.clamp(5, 100_000);
        self.explorer.show_files = config.view.show_files;
        self.heaviest.limit = config.view.heaviest_files_limit.clamp(5, 10_000);
        self.frame_interval = std::time::Duration::from_millis(config.view.frame_interval_ms.clamp(16, 250));
        let policy = config.scan_policy(self.services.home.as_deref());
        let engine = self.services.coordinator.engine();
        engine.update_settings(|settings| settings.policy = policy);
        engine.set_worker_count(config.worker_threads().unwrap_or_else(ScanEngine::recommended_worker_count));
        self.config = config;
        if let Some(warning) = warning {
            self.show_status(warning, Severity::Warning);
        }
        Ok(())
    }

    pub fn reload_config_from_disk(&mut self) {
        match self.services.config_store.load() {
            Ok(config) => match self.apply_config(config) {
                Ok(()) => {
                    self.settings.dirty = false;
                    self.show_status("configuration reloaded", Severity::Success);
                }
                Err(reason) => self.show_status(format!("configuration rejected: {reason}"), Severity::Error),
            },
            Err(error) => {
                self.overlay = Some(Overlay::Message(MessageDialog {
                    title: "Configuration file could not be read".to_owned(),
                    lines: vec![
                        format!("{error:#}"),
                        String::new(),
                        "The previous values stay in effect. Fix the file and reload (:config reload)."
                            .to_owned(),
                    ],
                    severity: Severity::Error,
                }));
            }
        }
    }

    pub fn reset_settings_to_defaults(&mut self) {
        match self.apply_config(AppConfig::default()) {
            Ok(()) => {
                self.settings.dirty = true;
                self.show_status("preferences reset to defaults (not saved yet)", Severity::Info);
            }
            Err(reason) => self.show_status(format!("could not reset: {reason}"), Severity::Error),
        }
    }

    /// Runs with the terminal handed back to the user: opens the configuration file in the
    /// editor, then reloads it. Returns a status line.
    pub(super) fn edit_config_file_in_editor(&mut self) -> (String, Severity) {
        if !self.services.config_store.exists() {
            self.sync_config_from_state();
            if let Err(error) = self.services.config_store.save(&self.config) {
                return (format!("could not create the configuration file: {error}"), Severity::Error);
            }
        }
        let editor = std::env::var("VISUAL")
            .ok()
            .or_else(|| std::env::var("EDITOR").ok())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| {
                if crate::infrastructure::privilege::find_in_path("nano").is_some() { "nano" } else { "vi" }
                    .to_owned()
            });
        let mut parts = editor.split_whitespace();
        let Some(program) = parts.next() else {
            return ("no editor configured".to_owned(), Severity::Error);
        };
        let status = Command::new(program).args(parts).arg(self.services.config_store.path()).status();
        match status {
            Ok(status) if status.success() => {
                self.reload_config_from_disk();
                if self.overlay.is_some() {
                    ("configuration file has errors".to_owned(), Severity::Error)
                } else {
                    ("configuration file edited and reloaded".to_owned(), Severity::Success)
                }
            }
            Ok(status) => {
                (format!("{program} exited with {status}; configuration unchanged"), Severity::Warning)
            }
            Err(error) => (format!("could not start {program}: {error}"), Severity::Error),
        }
    }

    pub fn theme_listing(&self) -> Vec<String> {
        THEME_NAMES
            .iter()
            .map(|name| {
                let marker = if *name == self.theme_name { "▶" } else { " " };
                format!("{marker} {name:<12} {}", Theme::describe(name))
            })
            .collect()
    }
}
