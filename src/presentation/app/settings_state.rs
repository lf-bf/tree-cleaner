//! State of the settings screen: which preferences exist and which row is selected.

use std::time::Instant;

use ratatui::widgets::TableState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingId {
    Theme,
    SizeMode,
    SizeBase,
    ShowFiles,
    RowLimit,
    HeaviestLimit,
    WorkerThreads,
    DenseThreshold,
    MaterializeDepth,
    BackgroundRootScan,
    BackgroundPaused,
    DeletionMode,
    DockerEnabled,
    DockerPruneContainers,
    DockerPruneVolumes,
    DockerPruneBuildCache,
    DiscoveryMaxDepth,
    SaveConfig,
    EditConfig,
    ReloadConfig,
    ResetDefaults,
}

impl SettingId {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Theme => "Theme",
            Self::SizeMode => "Size mode",
            Self::SizeBase => "Units",
            Self::ShowFiles => "Show files in the explorer",
            Self::RowLimit => "Rows per directory",
            Self::HeaviestLimit => "Heaviest files kept",
            Self::WorkerThreads => "Scanner threads",
            Self::DenseThreshold => "Dense directory threshold",
            Self::MaterializeDepth => "Materialise depth",
            Self::BackgroundRootScan => "Scan / in the background",
            Self::BackgroundPaused => "Background scanning",
            Self::DeletionMode => "Deletion mode",
            Self::DockerEnabled => "Docker cleaning",
            Self::DockerPruneContainers => "Prune stopped containers",
            Self::DockerPruneVolumes => "Prune dangling volumes",
            Self::DockerPruneBuildCache => "Prune build cache",
            Self::DiscoveryMaxDepth => "Discovery depth",
            Self::SaveConfig => "Save preferences",
            Self::EditConfig => "Edit the config file",
            Self::ReloadConfig => "Reload the config file",
            Self::ResetDefaults => "Reset to defaults",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::Theme => "colour palette; [theme] in the config file overrides single colours",
            Self::SizeMode => "allocated = blocks on disk (what fills the volume); apparent = file length",
            Self::SizeBase => "decimal (GB, like Finder) or binary (GiB)",
            Self::ShowFiles => "list the largest files of the current directory next to its folders",
            Self::RowLimit => "how many rows a directory shows, largest first",
            Self::HeaviestLimit => "how many files the heaviest-files view keeps",
            Self::WorkerThreads => "applied live; on APFS 4-8 is the sweet spot",
            Self::DenseThreshold => "directories with more entries are summed and never listed",
            Self::MaterializeDepth => "levels below the scan origin that become nodes",
            Self::BackgroundRootScan => {
                "measure / at startup so the dashboard fills in (takes effect on restart)"
            }
            Self::BackgroundPaused => "session only: pause everything outside the focused directory",
            Self::DeletionMode => "permanent (rm -rf) or move to the Trash",
            Self::DockerEnabled => {
                "look at images, containers and caches (applies when the cleaner runs again)"
            }
            Self::DockerPruneContainers => "offer docker container prune",
            Self::DockerPruneVolumes => "offer docker volume prune (data volumes are not protected)",
            Self::DockerPruneBuildCache => "offer docker builder prune --all",
            Self::DiscoveryMaxDepth => "how deep the cleaner looks for venvs, node_modules and targets",
            Self::SaveConfig => "write every current value to the config file",
            Self::EditConfig => "open the file in $VISUAL / $EDITOR, then reload it",
            Self::ReloadConfig => "read the file again and apply it",
            Self::ResetDefaults => "back to the built-in defaults (not written until you save)",
        }
    }

    pub const fn is_action(self) -> bool {
        matches!(self, Self::SaveConfig | Self::EditConfig | Self::ReloadConfig | Self::ResetDefaults)
    }

    /// Runtime-only toggles are not written to the configuration file.
    pub const fn is_session_only(self) -> bool {
        matches!(self, Self::BackgroundPaused)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsRow {
    Section(&'static str),
    Setting(SettingId),
}

const ROWS: [SettingsRow; 26] = [
    SettingsRow::Section("Appearance"),
    SettingsRow::Setting(SettingId::Theme),
    SettingsRow::Setting(SettingId::SizeMode),
    SettingsRow::Setting(SettingId::SizeBase),
    SettingsRow::Setting(SettingId::ShowFiles),
    SettingsRow::Setting(SettingId::RowLimit),
    SettingsRow::Setting(SettingId::HeaviestLimit),
    SettingsRow::Section("Scanner"),
    SettingsRow::Setting(SettingId::WorkerThreads),
    SettingsRow::Setting(SettingId::DenseThreshold),
    SettingsRow::Setting(SettingId::MaterializeDepth),
    SettingsRow::Setting(SettingId::BackgroundRootScan),
    SettingsRow::Setting(SettingId::BackgroundPaused),
    SettingsRow::Section("Deletion"),
    SettingsRow::Setting(SettingId::DeletionMode),
    SettingsRow::Section("Cleaner"),
    SettingsRow::Setting(SettingId::DockerEnabled),
    SettingsRow::Setting(SettingId::DockerPruneContainers),
    SettingsRow::Setting(SettingId::DockerPruneVolumes),
    SettingsRow::Setting(SettingId::DockerPruneBuildCache),
    SettingsRow::Setting(SettingId::DiscoveryMaxDepth),
    SettingsRow::Section("Configuration file"),
    SettingsRow::Setting(SettingId::SaveConfig),
    SettingsRow::Setting(SettingId::EditConfig),
    SettingsRow::Setting(SettingId::ReloadConfig),
    SettingsRow::Setting(SettingId::ResetDefaults),
];

#[derive(Debug)]
pub struct SettingsState {
    pub rows: Vec<SettingsRow>,
    pub selected: usize,
    pub table_state: TableState,
    /// Preferences changed through this screen (or `:theme`) and not yet written.
    pub dirty: bool,
    pub last_saved: Option<Instant>,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self::new()
    }
}

impl SettingsState {
    pub fn new() -> Self {
        let mut state = Self {
            rows: ROWS.to_vec(),
            selected: 1,
            table_state: TableState::default(),
            dirty: false,
            last_saved: None,
        };
        state.table_state.select(Some(state.selected));
        state
    }

    pub fn selected_setting(&self) -> Option<SettingId> {
        match self.rows.get(self.selected) {
            Some(SettingsRow::Setting(id)) => Some(*id),
            _ => None,
        }
    }

    /// Moves to the next selectable row in `direction`, skipping section headers.
    pub fn move_selection(&mut self, delta: isize) {
        if self.rows.is_empty() || delta == 0 {
            return;
        }
        let step = delta.signum();
        let mut remaining = delta.abs();
        let mut index = self.selected as isize;
        let last = self.rows.len() as isize - 1;
        while remaining > 0 {
            let mut candidate = index + step;
            while (0..=last).contains(&candidate)
                && matches!(self.rows[candidate as usize], SettingsRow::Section(_))
            {
                candidate += step;
            }
            if !(0..=last).contains(&candidate) {
                break;
            }
            index = candidate;
            remaining -= 1;
        }
        self.selected = index as usize;
        self.table_state.select(Some(self.selected));
    }

    pub fn select_first(&mut self) {
        self.selected = 0;
        self.move_selection(1);
    }

    pub fn select_last(&mut self) {
        self.selected = self.rows.len().saturating_sub(1);
        if matches!(self.rows.get(self.selected), Some(SettingsRow::Section(_))) {
            self.move_selection(-1);
        }
        self.table_state.select(Some(self.selected));
    }
}
