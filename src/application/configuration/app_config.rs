//! User configuration, loaded from `~/.config/tree-cleaner/config.toml`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::domain::cleaning::{CleaningCategory, InvalidPattern, ProtectionRules};
use crate::domain::deletion::{CriticalPathGuard, DeletionMode};
use crate::domain::storage::{ScanPolicy, SizeBase, SizeMode, scan_policy::default_skip_paths};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub scan: ScanConfig,
    pub view: ViewConfig,
    pub deletion: DeletionConfig,
    pub cleaner: CleanerConfig,
    /// Colour overrides applied on top of the chosen theme. Omitted when empty.
    #[serde(skip_serializing_if = "ThemeConfig::is_empty")]
    pub theme: ThemeConfig,
}

/// Optional colour overrides. Each value is `#rrggbb`, `#rgb`, an ANSI name (`red`,
/// `light_blue`, `dark_gray`, ...) or a 0-255 palette index.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    pub accent: Option<String>,
    pub accent_soft: Option<String>,
    pub directory: Option<String>,
    pub file: Option<String>,
    pub text: Option<String>,
    pub muted: Option<String>,
    pub faint: Option<String>,
    pub success: Option<String>,
    pub warning: Option<String>,
    pub danger: Option<String>,
    pub border: Option<String>,
    pub border_focused: Option<String>,
    pub selection_background: Option<String>,
    pub bar_track: Option<String>,
}

impl ThemeConfig {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// `(field name, value)` for every override that is set.
    pub fn entries(&self) -> Vec<(&'static str, &str)> {
        [
            ("accent", &self.accent),
            ("accent_soft", &self.accent_soft),
            ("directory", &self.directory),
            ("file", &self.file),
            ("text", &self.text),
            ("muted", &self.muted),
            ("faint", &self.faint),
            ("success", &self.success),
            ("warning", &self.warning),
            ("danger", &self.danger),
            ("border", &self.border),
            ("border_focused", &self.border_focused),
            ("selection_background", &self.selection_background),
            ("bar_track", &self.bar_track),
        ]
        .into_iter()
        .filter_map(|(name, value)| value.as_deref().map(|value| (name, value)))
        .collect()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ScanConfig {
    /// 0 means "twice the number of cores".
    pub worker_threads: usize,
    /// Directories with more entries than this are measured in aggregate only.
    pub dense_directory_threshold: usize,
    /// Levels below the current scan origin that get their own node.
    pub materialize_depth: u16,
    /// Absolute depth limit.
    pub max_depth: u16,
    /// Never entered. `~` is expanded.
    pub skip_paths: Vec<String>,
    pub cross_mount_points: bool,
    pub deduplicate_hard_links: bool,
    /// Start measuring `/` in the background at startup so the dashboard fills in.
    pub background_root_scan: bool,
}

impl Default for ScanConfig {
    fn default() -> Self {
        let defaults = ScanPolicy::default();
        Self {
            worker_threads: 0,
            dense_directory_threshold: defaults.dense_directory_threshold,
            materialize_depth: defaults.materialize_depth,
            max_depth: defaults.max_depth,
            skip_paths: default_skip_paths().into_iter().map(|path| path.display().to_string()).collect(),
            cross_mount_points: false,
            deduplicate_hard_links: true,
            background_root_scan: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewConfig {
    /// Rows shown per directory (largest first).
    pub row_limit: usize,
    /// Files kept by the "heaviest files" view.
    pub heaviest_files_limit: usize,
    /// `allocated` or `apparent`.
    pub size_mode: String,
    /// `decimal` (1 KB = 1000 B, like Finder) or `binary` (1 KiB = 1024 B).
    pub size_base: String,
    /// Target frame time in milliseconds.
    pub frame_interval_ms: u64,
    /// Built-in palette: claude, btop, nord, dracula, gruvbox, catppuccin, tokyo-night,
    /// solarized, monochrome, light or basic. `[theme]` overrides individual colours.
    pub theme: String,
    /// Whether the explorer lists files next to directories.
    pub show_files: bool,
}

impl Default for ViewConfig {
    fn default() -> Self {
        Self {
            row_limit: 100,
            heaviest_files_limit: 100,
            size_mode: "allocated".to_owned(),
            size_base: "decimal".to_owned(),
            frame_interval_ms: 33,
            theme: "claude".to_owned(),
            show_files: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct DeletionConfig {
    /// `permanent` or `trash`.
    pub default_mode: String,
    /// Extra paths that must never be deleted. `~` is expanded.
    pub never_delete: Vec<String>,
}

impl Default for DeletionConfig {
    fn default() -> Self {
        Self { default_mode: "permanent".to_owned(), never_delete: Vec::new() }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct CleanerConfig {
    /// Where to look for venvs, node_modules, target folders and build artifacts.
    pub search_roots: Vec<String>,
    /// Never entered during discovery. `~` is expanded.
    pub excluded_paths: Vec<String>,
    /// Glob patterns. Anything below a matching path is shown but never cleaned.
    pub protected_paths: Vec<String>,
    /// Categories to leave out entirely (see `tree-cleaner categories`).
    pub disabled_categories: Vec<String>,
    pub discovery_max_depth: u16,
    pub docker: DockerConfig,
}

impl Default for CleanerConfig {
    fn default() -> Self {
        Self {
            search_roots: vec!["~".to_owned()],
            excluded_paths: vec![
                "~/Library".to_owned(),
                "~/.Trash".to_owned(),
                "~/.cache".to_owned(),
                "~/.cargo".to_owned(),
                "~/.rustup".to_owned(),
                "~/.npm".to_owned(),
                "~/.nvm".to_owned(),
                "~/.bun".to_owned(),
                "~/.pyenv".to_owned(),
                "~/.conda".to_owned(),
                "~/.local".to_owned(),
                "~/.vscode".to_owned(),
                "~/.vscode-insiders".to_owned(),
                "~/.cursor".to_owned(),
                "~/.docker".to_owned(),
                "~/.orbstack".to_owned(),
                "~/.ollama".to_owned(),
                "~/miniconda3".to_owned(),
                "~/anaconda3".to_owned(),
                "~/go/pkg".to_owned(),
                "~/Applications".to_owned(),
                "~/Pictures".to_owned(),
                "~/Music".to_owned(),
                "~/Movies".to_owned(),
            ],
            protected_paths: Vec::new(),
            disabled_categories: Vec::new(),
            discovery_max_depth: 14,
            docker: DockerConfig::default(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct DockerConfig {
    pub enabled: bool,
    /// Glob patterns matched against `repository:tag`, `repository` and the image id.
    pub protected_images: Vec<String>,
    pub prune_containers: bool,
    pub prune_volumes: bool,
    pub prune_build_cache: bool,
}

impl Default for DockerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            protected_images: Vec::new(),
            prune_containers: true,
            prune_volumes: false,
            prune_build_cache: true,
        }
    }
}

impl AppConfig {
    pub fn scan_policy(&self, home: Option<&Path>) -> ScanPolicy {
        ScanPolicy {
            dense_directory_threshold: self.scan.dense_directory_threshold.max(100),
            materialize_depth: self.scan.materialize_depth.clamp(1, 64),
            max_depth: self.scan.max_depth.clamp(8, 1024),
            skip_paths: expand_all(&self.scan.skip_paths, home),
            cross_mount_points: self.scan.cross_mount_points,
            deduplicate_hard_links: self.scan.deduplicate_hard_links,
            file_listing_limit: self.view.row_limit.max(100) * 10,
        }
    }

    pub fn worker_threads(&self) -> Option<usize> {
        (self.scan.worker_threads > 0).then_some(self.scan.worker_threads)
    }

    pub fn size_mode(&self) -> SizeMode {
        match self.view.size_mode.trim().to_ascii_lowercase().as_str() {
            "apparent" | "logical" => SizeMode::Apparent,
            _ => SizeMode::Allocated,
        }
    }

    pub fn size_base(&self) -> SizeBase {
        match self.view.size_base.trim().to_ascii_lowercase().as_str() {
            "binary" | "iec" => SizeBase::Binary,
            _ => SizeBase::Decimal,
        }
    }

    pub fn deletion_mode(&self) -> DeletionMode {
        match self.deletion.default_mode.trim().to_ascii_lowercase().as_str() {
            "trash" => DeletionMode::Trash,
            _ => DeletionMode::Permanent,
        }
    }

    pub fn protection_rules(&self, home: Option<&Path>) -> Result<ProtectionRules, InvalidPattern> {
        let paths: Vec<String> = self
            .cleaner
            .protected_paths
            .iter()
            .map(|pattern| expand_tilde_in_pattern(pattern, home))
            .collect();
        ProtectionRules::new(&paths, &self.cleaner.docker.protected_images)
    }

    pub fn enabled_categories(&self) -> Vec<CleaningCategory> {
        CleaningCategory::ALL
            .into_iter()
            .filter(|category| {
                !self.cleaner.disabled_categories.iter().any(|disabled| disabled == category.identifier())
            })
            .filter(|category| self.cleaner.docker.enabled || !category.is_docker())
            .filter(|category| match category {
                CleaningCategory::DockerContainers => self.cleaner.docker.prune_containers,
                CleaningCategory::DockerVolumes => self.cleaner.docker.prune_volumes,
                CleaningCategory::DockerBuildCache => self.cleaner.docker.prune_build_cache,
                _ => true,
            })
            .collect()
    }

    pub fn discovery_roots(&self, home: Option<&Path>) -> Vec<PathBuf> {
        expand_all(&self.cleaner.search_roots, home)
    }

    pub fn discovery_exclusions(&self, home: Option<&Path>) -> Vec<PathBuf> {
        expand_all(&self.cleaner.excluded_paths, home)
    }

    pub fn critical_path_guard(&self, home: Option<PathBuf>) -> CriticalPathGuard {
        let additional = expand_all(&self.deletion.never_delete, home.as_deref());
        CriticalPathGuard::new(home, additional)
    }
}

/// Replaces a leading `~` with the home directory.
pub fn expand_tilde(raw: &str, home: Option<&Path>) -> PathBuf {
    let trimmed = raw.trim();
    if trimmed == "~" {
        return home.map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from(trimmed));
    }
    if let Some(rest) = trimmed.strip_prefix("~/") {
        if let Some(home) = home {
            return home.join(rest);
        }
    }
    PathBuf::from(trimmed)
}

fn expand_tilde_in_pattern(pattern: &str, home: Option<&Path>) -> String {
    match (pattern.strip_prefix("~"), home) {
        (Some(rest), Some(home)) => format!("{}{}", home.display(), rest),
        _ => pattern.to_owned(),
    }
}

fn expand_all(raw: &[String], home: Option<&Path>) -> Vec<PathBuf> {
    raw.iter().filter(|entry| !entry.trim().is_empty()).map(|entry| expand_tilde(entry, home)).collect()
}
