//! Loads and writes the TOML configuration file.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::application::configuration::AppConfig;

const HEADER: &str = "# tree-cleaner configuration\n\
# Every key is optional; missing keys fall back to the defaults shown here.\n\
# Paths may start with `~`. Patterns in `protected_paths` and `protected_images` are globs.\n\n";

#[derive(Debug, Clone)]
pub struct TomlConfigStore {
    path: PathBuf,
}

impl TomlConfigStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// `$XDG_CONFIG_HOME/tree-cleaner/config.toml`, falling back to `~/.config`.
    pub fn default_path() -> PathBuf {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| dirs::home_dir().map(|home| home.join(".config")))
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("tree-cleaner").join("config.toml")
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn exists(&self) -> bool {
        self.path.is_file()
    }

    /// Reads the file, or returns the defaults when it does not exist.
    pub fn load(&self) -> Result<AppConfig> {
        if !self.path.is_file() {
            return Ok(AppConfig::default());
        }
        let content = std::fs::read_to_string(&self.path)
            .with_context(|| format!("reading {}", self.path.display()))?;
        toml::from_str(&content).with_context(|| format!("parsing {}", self.path.display()))
    }

    pub fn save(&self, config: &AppConfig) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        let body = toml::to_string_pretty(config).context("serialising configuration")?;
        std::fs::write(&self.path, format!("{HEADER}{body}"))
            .with_context(|| format!("writing {}", self.path.display()))
    }

    /// Writes the defaults when no file exists yet, so users have something to edit.
    pub fn ensure_exists(&self) -> Result<bool> {
        if self.exists() {
            return Ok(false);
        }
        self.save(&AppConfig::default())?;
        Ok(true)
    }
}
