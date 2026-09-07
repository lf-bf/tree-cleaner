//! tree-cleaner: an interactive terminal explorer and cleaner for disk usage.
//!
//! `main` is the composition root: it wires concrete adapters to the application services
//! and hands everything to the interface.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use tree_cleaner::application::cleaning::{CleaningPorts, CleaningService};
use tree_cleaner::application::configuration::AppConfig;
use tree_cleaner::application::deletion::DeletionService;
use tree_cleaner::application::scanning::{ScanEngine, TreeCoordinator};
use tree_cleaner::domain::cleaning::CleaningCategory;
use tree_cleaner::domain::ports::{DirectoryReader, FileSystemProbe, VolumeProvider};
use tree_cleaner::infrastructure::config::TomlConfigStore;
use tree_cleaner::infrastructure::docker::DockerCli;
use tree_cleaner::infrastructure::filesystem::{
    StdDirectoryReader, StdFileRemover, StdFileSystemProbe, SysinfoVolumeProvider,
};
use tree_cleaner::infrastructure::privilege::SudoEscalator;
use tree_cleaner::infrastructure::shell::{StdCommandRunner, SystemFileRevealer};
use tree_cleaner::presentation::app::{App, Services};
use tree_cleaner::presentation::headless::{MeasureOptions, run_measure};
use tree_cleaner::presentation::terminal::TerminalSession;
use tree_cleaner::presentation::theme::Theme;

#[derive(Parser, Debug)]
#[command(
    name = "tree-cleaner",
    version,
    about = "Interactive terminal explorer and cleaner for disk usage.",
    long_about = "Walks your filesystem like a tree, shows what weighs the most, and lets you delete it.\n\
                  Sizes are measured lazily: the directory you are looking at gets every thread,\n\
                  the rest fills in around it."
)]
struct Cli {
    /// Directory to open in the explorer (defaults to your home directory).
    path: Option<PathBuf>,

    /// Configuration file (defaults to ~/.config/tree-cleaner/config.toml).
    #[arg(long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Number of scanner threads (default: up to 8 on macOS, twice the cores on Linux).
    #[arg(long, short = 'j', value_name = "N")]
    threads: Option<usize>,

    /// Directory reader: `std` (lstat per entry, fastest on macOS) or `rustix` (fstatat
    /// relative to the directory descriptor, for comparison and Linux experiments).
    #[arg(long, value_enum, default_value_t = ReaderChoice::Std)]
    reader: ReaderChoice,

    /// Do not scan `/` in the background at startup.
    #[arg(long)]
    no_root_scan: bool,

    /// Use the 16-colour palette (also enabled by NO_COLOR or a terminal without truecolor).
    #[arg(long)]
    basic_colors: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum ReaderChoice {
    Std,
    Rustix,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Show or create the configuration file.
    Config {
        #[command(subcommand)]
        action: Option<ConfigAction>,
    },
    /// List the cleaning categories and their configuration identifiers.
    Categories,
    /// Measure a directory without the interface and print a summary (developer tool).
    Measure {
        /// Directory to measure.
        path: PathBuf,
        /// How many of the largest children to print.
        #[arg(long, default_value_t = 15)]
        top: usize,
        /// Print machine readable JSON instead of a table.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug)]
enum ConfigAction {
    /// Print the path of the configuration file.
    Path,
    /// Write a configuration file with the defaults, if none exists.
    Init,
    /// Print the effective configuration.
    Show,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    tree_cleaner::presentation::log_buffer::install();

    let store = TomlConfigStore::new(cli.config.clone().unwrap_or_else(TomlConfigStore::default_path));
    let config = store.load()?;

    match &cli.command {
        Some(Command::Config { action }) => return run_config_command(&store, &config, action.as_ref()),
        Some(Command::Categories) => {
            for category in CleaningCategory::ALL {
                println!("{:<26} {:<30} {}", category.identifier(), category.label(), category.description());
            }
            return Ok(());
        }
        Some(Command::Measure { path, top, json }) => {
            let probe: Arc<dyn FileSystemProbe> = Arc::new(StdFileSystemProbe);
            let home = probe.home_directory();
            let volumes = SysinfoVolumeProvider::new();
            let options = MeasureOptions {
                path: path.clone(),
                top: *top,
                json: *json,
                worker_count: cli
                    .threads
                    .or_else(|| config.worker_threads())
                    .unwrap_or_else(ScanEngine::recommended_worker_count),
                size_base: config.size_base(),
            };
            return run_measure(
                options,
                build_reader(cli.reader),
                probe,
                config.scan_policy(home.as_deref()),
                volumes.mount_points().into_iter().collect(),
            );
        }
        None => {}
    }

    run_interface(cli, config, store)
}

fn run_config_command(
    store: &TomlConfigStore,
    config: &AppConfig,
    action: Option<&ConfigAction>,
) -> Result<()> {
    match action.unwrap_or(&ConfigAction::Path) {
        ConfigAction::Path => println!("{}", store.path().display()),
        ConfigAction::Init => {
            if store.ensure_exists()? {
                println!("wrote {}", store.path().display());
            } else {
                println!("{} already exists", store.path().display());
            }
        }
        ConfigAction::Show => {
            print!("{}", toml::to_string_pretty(config).context("serialising configuration")?)
        }
    }
    Ok(())
}

fn run_interface(cli: Cli, mut config: AppConfig, store: TomlConfigStore) -> Result<()> {
    if cli.no_root_scan {
        config.scan.background_root_scan = false;
    }

    let probe: Arc<dyn FileSystemProbe> = Arc::new(StdFileSystemProbe);
    let home = probe.home_directory();
    let reader: Arc<dyn DirectoryReader> = build_reader(cli.reader);
    let volumes: Arc<dyn VolumeProvider> = Arc::new(SysinfoVolumeProvider::new());
    let mount_points: HashSet<PathBuf> = volumes.mount_points().into_iter().collect();

    let policy = config.scan_policy(home.as_deref());
    let worker_count =
        cli.threads.or_else(|| config.worker_threads()).unwrap_or_else(ScanEngine::recommended_worker_count);
    let engine = Arc::new(ScanEngine::start(
        Arc::clone(&reader),
        Arc::clone(&probe),
        policy,
        mount_points,
        worker_count,
    ));
    let coordinator = TreeCoordinator::new(Arc::clone(&engine), config.size_mode());

    let guard = Arc::new(config.critical_path_guard(home.clone()));
    let remover = Arc::new(StdFileRemover::new());
    let deletion = DeletionService::new(remover.clone(), Arc::clone(&guard));

    let protection =
        config.protection_rules(home.as_deref()).context("invalid glob pattern in the configuration")?;
    let container_engine = Arc::new(DockerCli::detect());
    let command_runner = Arc::new(StdCommandRunner);
    let cleaning = CleaningService::new(
        config.clone(),
        home.clone(),
        protection,
        Arc::clone(&guard),
        CleaningPorts { probe: Arc::clone(&probe), remover, container_engine, command_runner },
    );

    let services = Services {
        coordinator,
        cleaning,
        deletion,
        escalator: Arc::new(SudoEscalator::detect()),
        revealer: Arc::new(SystemFileRevealer),
        volumes,
        config_store: store,
        home,
    };

    let theme =
        if cli.basic_colors || !supports_truecolor() { Theme::basic() } else { Theme::default_dark() };

    let mut app = App::new(services, config, theme, cli.path);
    let mut session = TerminalSession::open().context("opening the terminal")?;
    let result = app.run(&mut session);
    drop(session);
    result
}

fn build_reader(choice: ReaderChoice) -> Arc<dyn DirectoryReader> {
    #[cfg(unix)]
    {
        match choice {
            ReaderChoice::Std => Arc::new(StdDirectoryReader::new()),
            ReaderChoice::Rustix => {
                Arc::new(tree_cleaner::infrastructure::filesystem::RustixDirectoryReader::new())
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = choice;
        Arc::new(StdDirectoryReader::new())
    }
}

fn supports_truecolor() -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    match std::env::var("COLORTERM") {
        Ok(value) => {
            let value = value.to_ascii_lowercase();
            value.contains("truecolor") || value.contains("24bit")
        }
        // Most modern terminal emulators support truecolor even without advertising it.
        Err(_) => {
            std::env::var("TERM_PROGRAM").is_ok()
                || std::env::var("TERM").is_ok_and(|term| term.contains("256color"))
        }
    }
}
