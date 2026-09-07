//! `tree-cleaner measure`: runs the scanner without the interface. Useful to benchmark the
//! engine, to compare with `du`, and to script.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

use crate::application::scanning::{ScanEngine, TreeCoordinator};
use crate::domain::ports::{DirectoryReader, FileSystemProbe};
use crate::domain::storage::{RootPurpose, ScanPolicy, SizeBase, SizeMode};
use crate::presentation::formatting;

pub struct MeasureOptions {
    pub path: PathBuf,
    pub top: usize,
    pub json: bool,
    pub worker_count: usize,
    pub size_base: SizeBase,
}

pub fn run_measure(
    options: MeasureOptions,
    reader: Arc<dyn DirectoryReader>,
    probe: Arc<dyn FileSystemProbe>,
    policy: ScanPolicy,
    mount_points: HashSet<PathBuf>,
) -> Result<()> {
    let path = std::fs::canonicalize(&options.path)
        .with_context(|| format!("resolving {}", options.path.display()))?;
    if !path.is_dir() {
        bail!("{} is not a directory", path.display());
    }
    let started = Instant::now();
    let engine = Arc::new(ScanEngine::start(reader, probe, policy, mount_points, options.worker_count));
    let mut coordinator = TreeCoordinator::new(Arc::clone(&engine), SizeMode::Allocated);
    let root = coordinator.start_root_scan(path.clone(), RootPurpose::Filesystem);

    let mut last_report = Instant::now();
    loop {
        coordinator.pump_events(Duration::from_millis(20));
        let complete = coordinator.tree().node(root).is_some_and(|node| node.is_complete());
        if complete {
            break;
        }
        if !options.json && last_report.elapsed() >= Duration::from_secs(1) {
            let snapshot = engine.statistics().snapshot();
            eprintln!(
                "  {} files · {} dirs · {} files/s · queue {}",
                formatting::count(snapshot.files_measured),
                formatting::count(snapshot.directories_listed),
                formatting::count(snapshot.files_per_second() as u64),
                engine.queue_depths().total()
            );
            last_report = Instant::now();
        }
        if engine.is_idle() && engine.events().is_empty() {
            // Nothing left to do but the root did not complete: report what we have.
            coordinator.pump_events(Duration::from_millis(50));
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let elapsed = started.elapsed();
    let snapshot = engine.statistics().snapshot();
    let tree = coordinator.tree();
    let Some(node) = tree.node(root) else {
        bail!("the root vanished during the scan");
    };
    let children = tree.children_sorted_by_size(root, SizeMode::Allocated);

    if options.json {
        println!("{{");
        println!("  \"path\": {:?},", path.display().to_string());
        println!("  \"allocated_bytes\": {},", node.total_size().allocated.as_u64());
        println!("  \"apparent_bytes\": {},", node.total_size().apparent.as_u64());
        println!("  \"files\": {},", node.total_file_count());
        println!("  \"directories\": {},", node.total_directory_count());
        println!("  \"complete\": {},", node.is_complete());
        println!("  \"elapsed_seconds\": {:.3},", elapsed.as_secs_f64());
        println!("  \"files_per_second\": {:.0},", snapshot.files_per_second());
        println!("  \"dense_directories\": {},", snapshot.dense_directories);
        println!("  \"permission_denied\": {},", snapshot.permission_denied);
        println!("  \"hard_links_skipped\": {},", snapshot.hard_links_skipped);
        println!("  \"children\": [");
        let shown: Vec<_> = children.iter().take(options.top).collect();
        for (index, child) in shown.iter().enumerate() {
            if let Some(child_node) = tree.node(**child) {
                println!(
                    "    {{\"name\": {:?}, \"allocated_bytes\": {}, \"apparent_bytes\": {}, \"files\": {}, \"dense\": {}, \"denied\": {}}}{}",
                    child_node.display_name(),
                    child_node.total_size().allocated.as_u64(),
                    child_node.total_size().apparent.as_u64(),
                    child_node.total_file_count(),
                    child_node.flags().dense,
                    child_node.flags().access_denied,
                    if index + 1 < shown.len() { "," } else { "" }
                );
            }
        }
        println!("  ]");
        println!("}}");
        return Ok(());
    }

    println!();
    println!("  {}", path.display());
    println!(
        "  {} allocated · {} apparent · {} files · {} directories",
        formatting::size(node.total_size().allocated, options.size_base),
        formatting::size(node.total_size().apparent, options.size_base),
        formatting::count(node.total_file_count()),
        formatting::count(node.total_directory_count()),
    );
    println!(
        "  {} · {} files/s · {} threads · reader {} · dense {} · denied {} · hard links skipped {}{}",
        formatting::duration(elapsed.as_secs_f64()),
        formatting::count(snapshot.files_per_second() as u64),
        options.worker_count,
        engine.reader_name(),
        snapshot.dense_directories,
        snapshot.permission_denied,
        snapshot.hard_links_skipped,
        if node.is_complete() { "" } else { " · INCOMPLETE" }
    );
    println!();
    let total = node.total_size().allocated;
    for child in children.iter().take(options.top) {
        let Some(child_node) = tree.node(*child) else {
            continue;
        };
        let size = child_node.total_size().allocated;
        let mut flags = String::new();
        if child_node.flags().dense {
            flags.push_str(" dense");
        }
        if child_node.flags().access_denied {
            flags.push_str(" denied");
        }
        if child_node.flags().boundary {
            flags.push_str(" volume");
        }
        println!(
            "  {:>10}  {:>6}  {}  {}{}",
            formatting::size(size, options.size_base),
            formatting::percent(size.ratio_of(total)),
            formatting::bar(size.ratio_of(total), 20),
            child_node.display_name(),
            flags
        );
    }
    if children.len() > options.top {
        println!("  … {} more", children.len() - options.top);
    }
    engine.shutdown();
    Ok(())
}
