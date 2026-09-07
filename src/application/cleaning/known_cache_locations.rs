//! Cache folders that are known to grow without bounds on developer machines.

use std::path::{Path, PathBuf};

use crate::domain::cleaning::{CleaningCategory, CleaningLocation};
use crate::domain::ports::FileSystemProbe;

#[derive(Clone, Debug)]
pub struct KnownLocation {
    pub category: CleaningCategory,
    pub label: String,
    pub location: CleaningLocation,
    pub requires_privileges: bool,
}

fn contents(category: CleaningCategory, label: &str, path: PathBuf, privileged: bool) -> KnownLocation {
    KnownLocation {
        category,
        label: label.to_owned(),
        location: CleaningLocation::DirectoryContents(path),
        requires_privileges: privileged,
    }
}

/// Every known location that exists on this machine.
pub fn existing_known_locations(home: Option<&Path>, probe: &dyn FileSystemProbe) -> Vec<KnownLocation> {
    let mut candidates: Vec<KnownLocation> = Vec::new();
    let Some(home) = home else {
        return candidates;
    };

    if cfg!(target_os = "macos") {
        candidates.extend(macos_locations(home));
    } else {
        candidates.extend(linux_locations(home));
    }
    candidates.extend(cross_platform_locations(home));

    candidates.retain(|candidate| match &candidate.location {
        CleaningLocation::Directory(path) | CleaningLocation::DirectoryContents(path) => {
            probe.is_directory(path)
        }
        _ => true,
    });
    candidates
}

fn macos_locations(home: &Path) -> Vec<KnownLocation> {
    use CleaningCategory as Category;
    let mut list = vec![
        contents(Category::Trash, "Trash", home.join(".Trash"), false),
        contents(Category::UserCaches, "User caches (~/Library/Caches)", home.join("Library/Caches"), false),
        contents(
            Category::SystemCaches,
            "System caches (/Library/Caches)",
            PathBuf::from("/Library/Caches"),
            true,
        ),
        contents(Category::Logs, "User logs (~/Library/Logs)", home.join("Library/Logs"), false),
        contents(Category::Logs, "System logs (/Library/Logs)", PathBuf::from("/Library/Logs"), true),
        contents(
            Category::HomebrewCache,
            "Homebrew downloads (~/Library/Caches/Homebrew)",
            home.join("Library/Caches/Homebrew"),
            false,
        ),
        contents(Category::PackageManagerCaches, "pip cache", home.join("Library/Caches/pip"), false),
        contents(Category::PackageManagerCaches, "uv cache", home.join("Library/Caches/uv"), false),
        contents(Category::PackageManagerCaches, "Yarn cache", home.join("Library/Caches/Yarn"), false),
        contents(Category::PackageManagerCaches, "Poetry cache", home.join("Library/Caches/pypoetry"), false),
        contents(
            Category::XcodeDerivedData,
            "Xcode DerivedData",
            home.join("Library/Developer/Xcode/DerivedData"),
            false,
        ),
        contents(
            Category::XcodeArchives,
            "Xcode Archives",
            home.join("Library/Developer/Xcode/Archives"),
            false,
        ),
        contents(
            Category::IosDeviceSupport,
            "iOS DeviceSupport",
            home.join("Library/Developer/Xcode/iOS DeviceSupport"),
            false,
        ),
        contents(
            Category::IosDeviceSupport,
            "watchOS DeviceSupport",
            home.join("Library/Developer/Xcode/watchOS DeviceSupport"),
            false,
        ),
        contents(
            Category::IosDeviceSupport,
            "tvOS DeviceSupport",
            home.join("Library/Developer/Xcode/tvOS DeviceSupport"),
            false,
        ),
        contents(
            Category::SimulatorCaches,
            "CoreSimulator caches",
            home.join("Library/Developer/CoreSimulator/Caches"),
            false,
        ),
    ];
    if let Some(darwin_cache) = darwin_user_cache_directory() {
        list.push(contents(
            Category::UserCaches,
            "Darwin user cache (/var/folders/…/C)",
            darwin_cache,
            false,
        ));
    }
    list
}

/// `$TMPDIR` on macOS is `/var/folders/xx/yyyy/T/`; the per-user cache lives next to it.
fn darwin_user_cache_directory() -> Option<PathBuf> {
    let temporary = std::env::var_os("TMPDIR")?;
    let temporary = PathBuf::from(temporary);
    let trimmed = if temporary.file_name().is_none() { temporary.parent()?.to_path_buf() } else { temporary };
    let parent = trimmed.parent()?;
    parent.starts_with("/var/folders").then(|| parent.join("C"))
}

fn linux_locations(home: &Path) -> Vec<KnownLocation> {
    use CleaningCategory as Category;
    vec![
        contents(Category::Trash, "Trash", home.join(".local/share/Trash"), false),
        contents(Category::UserCaches, "User cache (~/.cache)", home.join(".cache"), false),
        contents(
            Category::SystemCaches,
            "APT package archives",
            PathBuf::from("/var/cache/apt/archives"),
            true,
        ),
        contents(Category::PackageManagerCaches, "pip cache", home.join(".cache/pip"), false),
        contents(Category::PackageManagerCaches, "uv cache", home.join(".cache/uv"), false),
        contents(Category::PackageManagerCaches, "Yarn cache", home.join(".cache/yarn"), false),
        contents(Category::PackageManagerCaches, "Poetry cache", home.join(".cache/pypoetry"), false),
    ]
}

fn cross_platform_locations(home: &Path) -> Vec<KnownLocation> {
    use CleaningCategory as Category;
    vec![
        contents(Category::PackageManagerCaches, "npm cache", home.join(".npm/_cacache"), false),
        contents(Category::PackageManagerCaches, "Bun install cache", home.join(".bun/install/cache"), false),
        contents(
            Category::PackageManagerCaches,
            "Cargo registry cache",
            home.join(".cargo/registry/cache"),
            false,
        ),
        contents(
            Category::PackageManagerCaches,
            "Cargo registry sources",
            home.join(".cargo/registry/src"),
            false,
        ),
        contents(
            Category::PackageManagerCaches,
            "Cargo git checkouts",
            home.join(".cargo/git/checkouts"),
            false,
        ),
        contents(Category::PackageManagerCaches, "Go module cache", home.join("go/pkg/mod/cache"), false),
        contents(Category::PackageManagerCaches, "Gradle caches", home.join(".gradle/caches"), false),
        contents(Category::PackageManagerCaches, "Maven repository", home.join(".m2/repository"), false),
        contents(Category::PackageManagerCaches, "pre-commit cache", home.join(".cache/pre-commit"), false),
    ]
}

/// External commands that free space on their own.
pub fn command_locations(command_exists: &dyn Fn(&str) -> bool) -> Vec<KnownLocation> {
    let mut list = Vec::new();
    if command_exists("brew") {
        list.push(KnownLocation {
            category: CleaningCategory::HomebrewCache,
            label: "Homebrew old versions (brew cleanup --prune=all -s)".to_owned(),
            location: CleaningLocation::Command {
                program: "brew".to_owned(),
                arguments: vec!["cleanup".to_owned(), "--prune=all".to_owned(), "-s".to_owned()],
            },
            requires_privileges: false,
        });
    }
    list
}
