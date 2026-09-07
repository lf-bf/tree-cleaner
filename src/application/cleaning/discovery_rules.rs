//! How the cleaner recognises project artifacts while walking the user's directories.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::domain::cleaning::CleaningCategory;
use crate::domain::ports::FileSystemProbe;

/// Extra evidence required before a directory name is trusted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verification {
    /// The name alone is conclusive.
    None,
    /// The directory must contain this file (for example `pyvenv.cfg`).
    ContainsFile(&'static str),
    /// The parent directory must contain this file (for example `Cargo.toml`).
    SiblingFile(&'static str),
}

#[derive(Clone, Copy, Debug)]
pub struct DiscoveryRule {
    pub category: CleaningCategory,
    pub names: &'static [&'static str],
    pub verification: Verification,
}

const STANDARD_RULES: &[DiscoveryRule] = &[
    DiscoveryRule {
        category: CleaningCategory::PythonVirtualEnvironments,
        names: &[
            ".venv",
            "venv",
            ".virtualenv",
            "virtualenv",
            "env",
            ".env",
            ".venv310",
            ".venv311",
            ".venv312",
        ],
        verification: Verification::ContainsFile("pyvenv.cfg"),
    },
    DiscoveryRule {
        category: CleaningCategory::NodeModules,
        names: &["node_modules"],
        verification: Verification::None,
    },
    DiscoveryRule {
        category: CleaningCategory::CargoTargets,
        names: &["target"],
        verification: Verification::SiblingFile("Cargo.toml"),
    },
    DiscoveryRule {
        category: CleaningCategory::BuildArtifacts,
        names: &[
            "__pycache__",
            ".pytest_cache",
            ".mypy_cache",
            ".ruff_cache",
            ".tox",
            ".nox",
            ".next",
            ".nuxt",
            ".turbo",
            ".parcel-cache",
            ".svelte-kit",
            ".angular",
            ".gradle",
        ],
        verification: Verification::None,
    },
];

/// Directories that are never worth entering while looking for artifacts.
const NEVER_DESCEND: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    ".Trash",
    ".Trashes",
    ".fseventsd",
    ".Spotlight-V100",
    ".DocumentRevisions-V100",
    "node_modules",
    ".venv",
    "venv",
    ".tox",
    ".nox",
    "__pycache__",
    ".next",
    ".nuxt",
    ".turbo",
    ".gradle",
];

#[derive(Debug)]
pub struct DiscoveryRules {
    rules: Vec<DiscoveryRule>,
    excluded_paths: Vec<PathBuf>,
    never_descend: HashSet<&'static str>,
    pub max_depth: u16,
}

impl DiscoveryRules {
    pub fn standard(enabled: &[CleaningCategory], excluded_paths: Vec<PathBuf>, max_depth: u16) -> Self {
        Self {
            rules: STANDARD_RULES.iter().filter(|rule| enabled.contains(&rule.category)).copied().collect(),
            excluded_paths,
            never_descend: NEVER_DESCEND.iter().copied().collect(),
            max_depth,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    pub fn matching_rule(&self, name: &OsStr) -> Option<&DiscoveryRule> {
        let name = name.to_str()?;
        self.rules.iter().find(|rule| rule.names.contains(&name))
    }

    pub fn rule_index(&self, rule: &DiscoveryRule) -> usize {
        self.rules.iter().position(|candidate| std::ptr::eq(candidate, rule)).unwrap_or(0)
    }

    pub fn rule_at(&self, index: usize) -> Option<&DiscoveryRule> {
        self.rules.get(index)
    }

    pub fn verify(
        &self,
        rule: &DiscoveryRule,
        candidate: &Path,
        parent: &Path,
        probe: &dyn FileSystemProbe,
    ) -> bool {
        match rule.verification {
            Verification::None => true,
            Verification::ContainsFile(file) => probe.is_file(&candidate.join(file)),
            Verification::SiblingFile(file) => probe.is_file(&parent.join(file)),
        }
    }

    pub fn should_descend_into(&self, name: &OsStr) -> bool {
        name.to_str().is_none_or(|name| !self.never_descend.contains(name))
    }

    pub fn is_excluded(&self, path: &Path) -> bool {
        self.excluded_paths.iter().any(|excluded| path.starts_with(excluded))
    }
}
