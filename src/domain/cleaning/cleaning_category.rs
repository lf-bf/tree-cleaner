//! Kinds of reclaimable storage the cleaner knows about.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CleaningCategory {
    Trash,
    UserCaches,
    SystemCaches,
    Logs,
    HomebrewCache,
    PackageManagerCaches,
    XcodeDerivedData,
    XcodeArchives,
    IosDeviceSupport,
    SimulatorCaches,
    PythonVirtualEnvironments,
    NodeModules,
    CargoTargets,
    BuildArtifacts,
    DockerImages,
    DockerContainers,
    DockerVolumes,
    DockerBuildCache,
}

impl CleaningCategory {
    pub const ALL: [Self; 18] = [
        Self::Trash,
        Self::UserCaches,
        Self::SystemCaches,
        Self::Logs,
        Self::HomebrewCache,
        Self::PackageManagerCaches,
        Self::XcodeDerivedData,
        Self::XcodeArchives,
        Self::IosDeviceSupport,
        Self::SimulatorCaches,
        Self::PythonVirtualEnvironments,
        Self::NodeModules,
        Self::CargoTargets,
        Self::BuildArtifacts,
        Self::DockerImages,
        Self::DockerContainers,
        Self::DockerVolumes,
        Self::DockerBuildCache,
    ];

    /// Stable machine readable name, used in the configuration file.
    pub const fn identifier(self) -> &'static str {
        match self {
            Self::Trash => "trash",
            Self::UserCaches => "user_caches",
            Self::SystemCaches => "system_caches",
            Self::Logs => "logs",
            Self::HomebrewCache => "homebrew_cache",
            Self::PackageManagerCaches => "package_manager_caches",
            Self::XcodeDerivedData => "xcode_derived_data",
            Self::XcodeArchives => "xcode_archives",
            Self::IosDeviceSupport => "ios_device_support",
            Self::SimulatorCaches => "simulator_caches",
            Self::PythonVirtualEnvironments => "python_venvs",
            Self::NodeModules => "node_modules",
            Self::CargoTargets => "cargo_targets",
            Self::BuildArtifacts => "build_artifacts",
            Self::DockerImages => "docker_images",
            Self::DockerContainers => "docker_containers",
            Self::DockerVolumes => "docker_volumes",
            Self::DockerBuildCache => "docker_build_cache",
        }
    }

    pub fn from_identifier(identifier: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|category| category.identifier() == identifier)
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Trash => "Trash",
            Self::UserCaches => "User caches",
            Self::SystemCaches => "System caches",
            Self::Logs => "Logs",
            Self::HomebrewCache => "Homebrew cache",
            Self::PackageManagerCaches => "Package manager caches",
            Self::XcodeDerivedData => "Xcode DerivedData",
            Self::XcodeArchives => "Xcode archives",
            Self::IosDeviceSupport => "iOS device support",
            Self::SimulatorCaches => "Simulator caches",
            Self::PythonVirtualEnvironments => "Python virtual environments",
            Self::NodeModules => "node_modules",
            Self::CargoTargets => "Cargo target directories",
            Self::BuildArtifacts => "Build artifacts",
            Self::DockerImages => "Docker images",
            Self::DockerContainers => "Docker stopped containers",
            Self::DockerVolumes => "Docker dangling volumes",
            Self::DockerBuildCache => "Docker build cache",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::Trash => "Files waiting in the Trash still occupy space until it is emptied.",
            Self::UserCaches => "~/Library/Caches and friends. Applications rebuild them on demand.",
            Self::SystemCaches => "/Library/Caches and the per-user darwin cache folder. Needs sudo.",
            Self::Logs => "Application and system logs.",
            Self::HomebrewCache => "Downloaded bottles and old versions. Runs `brew cleanup`.",
            Self::PackageManagerCaches => "pip, uv, npm, yarn, pnpm, Cargo registry and Go module caches.",
            Self::XcodeDerivedData => "Intermediate build products. Xcode rebuilds them.",
            Self::XcodeArchives => "App archives kept for distribution. Check before removing.",
            Self::IosDeviceSupport => "Debug symbols for iOS versions of devices you plugged in.",
            Self::SimulatorCaches => "Caches of the CoreSimulator runtime.",
            Self::PythonVirtualEnvironments => {
                "Virtual environments found in your projects (pyvenv.cfg present)."
            }
            Self::NodeModules => "node_modules folders found in your projects.",
            Self::CargoTargets => "target/ folders next to a Cargo.toml.",
            Self::BuildArtifacts => {
                "__pycache__, .pytest_cache, .mypy_cache, .ruff_cache, .next, .turbo, .parcel-cache."
            }
            Self::DockerImages => "Images not protected by your rules and not used by any container.",
            Self::DockerContainers => "Containers that are not running (docker container prune).",
            Self::DockerVolumes => "Volumes not attached to any container (docker volume prune).",
            Self::DockerBuildCache => "BuildKit cache (docker builder prune --all).",
        }
    }

    /// Whether the category is selected before the user touches anything.
    pub const fn selected_by_default(self) -> bool {
        match self {
            Self::Trash
            | Self::UserCaches
            | Self::HomebrewCache
            | Self::PackageManagerCaches
            | Self::XcodeDerivedData
            | Self::SimulatorCaches
            | Self::PythonVirtualEnvironments
            | Self::NodeModules
            | Self::CargoTargets
            | Self::BuildArtifacts
            | Self::DockerImages
            | Self::DockerContainers
            | Self::DockerBuildCache => true,
            Self::SystemCaches
            | Self::Logs
            | Self::XcodeArchives
            | Self::IosDeviceSupport
            | Self::DockerVolumes => false,
        }
    }

    pub const fn is_docker(self) -> bool {
        matches!(
            self,
            Self::DockerImages | Self::DockerContainers | Self::DockerVolumes | Self::DockerBuildCache
        )
    }

    /// Categories whose instances are found by walking the user's projects.
    pub const fn is_discovered(self) -> bool {
        matches!(
            self,
            Self::PythonVirtualEnvironments | Self::NodeModules | Self::CargoTargets | Self::BuildArtifacts
        )
    }
}
