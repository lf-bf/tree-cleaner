//! The last line of defence: paths the program refuses to delete no matter what.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct CriticalPathGuard {
    protected: Vec<PathBuf>,
    home: Option<PathBuf>,
}

impl CriticalPathGuard {
    pub fn new(home: Option<PathBuf>, additional: impl IntoIterator<Item = PathBuf>) -> Self {
        let mut protected: Vec<PathBuf> = [
            "/",
            "/System",
            "/Library",
            "/Applications",
            "/Users",
            "/private",
            "/private/var",
            "/private/etc",
            "/var",
            "/etc",
            "/usr",
            "/bin",
            "/sbin",
            "/opt",
            "/home",
            "/root",
            "/boot",
            "/Volumes",
            "/cores",
            "/dev",
            "/proc",
            "/sys",
            "/run",
            "/tmp",
            "/private/tmp",
            "/nix",
        ]
        .into_iter()
        .map(PathBuf::from)
        .collect();
        if let Some(home) = &home {
            protected.push(home.clone());
            protected.push(home.join("Library"));
            protected.push(home.join("Library/Application Support"));
            protected.push(home.join("Library/Containers"));
            protected.push(home.join("Library/Group Containers"));
            protected.push(home.join("Library/Mobile Documents"));
            protected.push(home.join("Library/Keychains"));
            protected.push(home.join("Library/Preferences"));
            protected.push(home.join(".ssh"));
            protected.push(home.join(".gnupg"));
        }
        protected.extend(additional);
        Self { protected, home }
    }

    /// `Ok` when deleting `path` is allowed; `Err` explains why it is not.
    pub fn check(&self, path: &Path) -> Result<(), String> {
        if !path.is_absolute() {
            return Err("only absolute paths can be deleted".to_owned());
        }
        if path.components().count() <= 1 {
            return Err("refusing to delete the filesystem root".to_owned());
        }
        if let Some(protected) = self.protected.iter().find(|protected| *protected == path) {
            return Err(format!("{} is a critical system location", protected.display()));
        }
        if let Some(home) = &self.home {
            if home.starts_with(path) {
                return Err(format!("{} contains your home directory", path.display()));
            }
        }
        Ok(())
    }
}
