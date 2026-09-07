//! Builds the list of cleaning candidates, keeps their sizes fresh, and executes plans.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};

use super::discovery_rules::DiscoveryRules;
use super::docker_inventory::{DockerInventory, DockerStatus, spawn_docker_inventory};
use super::known_cache_locations::{KnownLocation, command_locations, existing_known_locations};
use crate::application::configuration::AppConfig;
use crate::application::scanning::TreeCoordinator;
use crate::domain::cleaning::{
    CandidateId, CleaningCandidate, CleaningCategory, CleaningLocation, CleaningOutcome, CleaningPlan,
    CleaningReport, CleaningStatus, DockerPruneKind, Protection, ProtectionRules,
};
use crate::domain::deletion::{CriticalPathGuard, DeletionMode};
use crate::domain::ports::{
    CommandRunner, ContainerEngine, FileRemover, FileSystemProbe, RemovalError, RemovalObserver,
};
use crate::domain::storage::{ByteSize, RootPurpose, SearchId, SizeMode};

/// How often progress inside one candidate is reported at most.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(50);

/// Progress messages sent while a plan runs on a background thread.
#[derive(Clone, Debug)]
pub enum CleaningProgress {
    Started { total: usize },
    ItemStarted { index: usize, label: String },
    ItemProgress { index: usize, entries_removed: u64, bytes_removed: u64, current: PathBuf },
    ItemFinished { index: usize, outcome: CleaningOutcome },
    Finished(CleaningReport),
}

/// Lets the interface follow and cancel a run.
#[derive(Debug)]
pub struct CleaningHandle {
    pub receiver: Receiver<CleaningProgress>,
    cancel: Arc<AtomicBool>,
}

impl CleaningHandle {
    pub fn request_cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    pub fn is_cancel_requested(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }
}

/// Forwards removal progress for one candidate, throttled, and relays cancellation.
struct ItemObserver<'a> {
    index: usize,
    sender: &'a Sender<CleaningProgress>,
    cancel: &'a AtomicBool,
    entries_removed: u64,
    bytes_removed: u64,
    last_report: Instant,
}

impl<'a> ItemObserver<'a> {
    fn new(index: usize, sender: &'a Sender<CleaningProgress>, cancel: &'a AtomicBool) -> Self {
        Self { index, sender, cancel, entries_removed: 0, bytes_removed: 0, last_report: Instant::now() }
    }
}

impl RemovalObserver for ItemObserver<'_> {
    fn entry_removed(&mut self, path: &Path, bytes: u64) {
        self.entries_removed += 1;
        self.bytes_removed = self.bytes_removed.saturating_add(bytes);
        if self.last_report.elapsed() >= PROGRESS_INTERVAL {
            self.last_report = Instant::now();
            let _ = self.sender.send(CleaningProgress::ItemProgress {
                index: self.index,
                entries_removed: self.entries_removed,
                bytes_removed: self.bytes_removed,
                current: path.to_path_buf(),
            });
        }
    }

    fn should_cancel(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

/// Everything needed to run a plan, cheap to clone onto a background thread.
#[derive(Clone)]
pub struct CleaningExecutor {
    remover: Arc<dyn FileRemover>,
    container_engine: Arc<dyn ContainerEngine>,
    command_runner: Arc<dyn CommandRunner>,
    guard: Arc<CriticalPathGuard>,
}

impl CleaningExecutor {
    /// Runs the plan, reporting each candidate as it starts and finishes. Candidates after a
    /// cancellation request are reported as cancelled without being touched.
    pub fn execute(
        &self,
        plan: &CleaningPlan,
        progress: &Sender<CleaningProgress>,
        cancel: &AtomicBool,
    ) -> CleaningReport {
        let _ = progress.send(CleaningProgress::Started { total: plan.candidates.len() });
        let mut report = CleaningReport { outcomes: Vec::new(), reclaimed_estimate: ByteSize::ZERO };
        for (index, candidate) in plan.candidates.iter().enumerate() {
            let outcome = if cancel.load(Ordering::SeqCst) {
                CleaningOutcome {
                    candidate: candidate.id,
                    label: candidate.label.clone(),
                    status: CleaningStatus::Cancelled,
                    detail: "cancelled before it started".to_owned(),
                    reclaimed: ByteSize::ZERO,
                    entries_removed: 0,
                }
            } else {
                let _ =
                    progress.send(CleaningProgress::ItemStarted { index, label: candidate.label.clone() });
                let mut observer = ItemObserver::new(index, progress, cancel);
                let (status, detail, engine_reclaimed) = self.execute_one(candidate, &mut observer);
                let reclaimed = if observer.bytes_removed > 0 {
                    ByteSize::new(observer.bytes_removed)
                } else if status == CleaningStatus::Cleaned {
                    engine_reclaimed.or(candidate.size).unwrap_or(ByteSize::ZERO)
                } else {
                    ByteSize::ZERO
                };
                CleaningOutcome {
                    candidate: candidate.id,
                    label: candidate.label.clone(),
                    status,
                    detail,
                    reclaimed,
                    entries_removed: observer.entries_removed,
                }
            };
            report.reclaimed_estimate += outcome.reclaimed;
            let _ = progress.send(CleaningProgress::ItemFinished { index, outcome: outcome.clone() });
            report.outcomes.push(outcome);
        }
        let _ = progress.send(CleaningProgress::Finished(report.clone()));
        report
    }

    /// Spawns the plan on its own thread; progress arrives through the handle.
    pub fn execute_in_background(self, plan: CleaningPlan) -> CleaningHandle {
        let (sender, receiver) = crossbeam_channel::unbounded();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_flag = Arc::clone(&cancel);
        std::thread::Builder::new()
            .name("tree-cleaner-clean".to_owned())
            .spawn(move || {
                self.execute(&plan, &sender, &cancel_flag);
            })
            .expect("failed to spawn cleaning thread");
        CleaningHandle { receiver, cancel }
    }

    /// Returns the status, a detail line and the bytes the engine itself reported.
    fn execute_one(
        &self,
        candidate: &CleaningCandidate,
        observer: &mut dyn RemovalObserver,
    ) -> (CleaningStatus, String, Option<ByteSize>) {
        match &candidate.location {
            CleaningLocation::Directory(path) => {
                if let Err(reason) = self.guard.check(path) {
                    return (CleaningStatus::Skipped(reason.clone()), reason, None);
                }
                match self.remover.remove(path, DeletionMode::Permanent, observer) {
                    Ok(_) => (CleaningStatus::Cleaned, path.display().to_string(), None),
                    Err(RemovalError::NotFound) => (CleaningStatus::Cleaned, "already gone".to_owned(), None),
                    Err(RemovalError::PermissionDenied) => {
                        (CleaningStatus::NeedsPrivileges, path.display().to_string(), None)
                    }
                    Err(RemovalError::Cancelled) => (CleaningStatus::Cancelled, "cancelled".to_owned(), None),
                    Err(RemovalError::Other(reason)) => {
                        (CleaningStatus::Failed(reason.clone()), reason, None)
                    }
                }
            }
            CleaningLocation::DirectoryContents(path) => {
                if path.components().count() <= 2 {
                    let reason = format!("refusing to empty {}", path.display());
                    return (CleaningStatus::Skipped(reason.clone()), reason, None);
                }
                match self.remover.remove_contents(path, DeletionMode::Permanent, observer) {
                    Ok(result) if result.failed.is_empty() => {
                        (CleaningStatus::Cleaned, format!("{} entries removed", result.removed), None)
                    }
                    Ok(result) => {
                        let denied =
                            result.failed.iter().filter(|(_, error)| error.needs_privileges()).count();
                        let detail = format!(
                            "{} removed, {} failed ({} permission denied)",
                            result.removed,
                            result.failed.len(),
                            denied
                        );
                        if denied > 0 {
                            (CleaningStatus::NeedsPrivileges, detail, None)
                        } else {
                            (CleaningStatus::Failed(detail.clone()), detail, None)
                        }
                    }
                    Err(RemovalError::PermissionDenied) => {
                        (CleaningStatus::NeedsPrivileges, path.display().to_string(), None)
                    }
                    Err(RemovalError::NotFound) => (CleaningStatus::Cleaned, "already gone".to_owned(), None),
                    Err(RemovalError::Cancelled) => (CleaningStatus::Cancelled, "cancelled".to_owned(), None),
                    Err(RemovalError::Other(reason)) => {
                        (CleaningStatus::Failed(reason.clone()), reason, None)
                    }
                }
            }
            CleaningLocation::DockerImage { id, reference } => {
                match self.container_engine.remove_images(std::slice::from_ref(id)) {
                    Ok(outcome) => {
                        let summary =
                            if outcome.summary.is_empty() { reference.clone() } else { outcome.summary };
                        (CleaningStatus::Cleaned, summary, outcome.reclaimed)
                    }
                    Err(error) => (CleaningStatus::Failed(error.to_string()), error.to_string(), None),
                }
            }
            CleaningLocation::DockerPrune(kind) => match self.container_engine.prune(*kind) {
                Ok(outcome) => (CleaningStatus::Cleaned, outcome.summary, outcome.reclaimed),
                Err(error) => (CleaningStatus::Failed(error.to_string()), error.to_string(), None),
            },
            CleaningLocation::Command { program, arguments } => {
                match self.command_runner.run(program, arguments) {
                    Ok(output) => (CleaningStatus::Cleaned, last_line(&output.stdout, program), None),
                    Err(failure) => (CleaningStatus::Failed(failure.detail.clone()), failure.detail, None),
                }
            }
        }
    }
}

fn last_line(text: &str, fallback: &str) -> String {
    text.lines().rev().map(str::trim).find(|line| !line.is_empty()).unwrap_or(fallback).to_owned()
}

/// The adapters the cleaning service depends on.
pub struct CleaningPorts {
    pub probe: Arc<dyn FileSystemProbe>,
    pub remover: Arc<dyn FileRemover>,
    pub container_engine: Arc<dyn ContainerEngine>,
    pub command_runner: Arc<dyn CommandRunner>,
}

pub struct CleaningService {
    config: AppConfig,
    home: Option<PathBuf>,
    protection: ProtectionRules,
    probe: Arc<dyn FileSystemProbe>,
    container_engine: Arc<dyn ContainerEngine>,
    command_runner: Arc<dyn CommandRunner>,
    executor: CleaningExecutor,
    candidates: Vec<CleaningCandidate>,
    next_candidate: u32,
    discovery: Option<SearchId>,
    docker_receiver: Option<Receiver<DockerInventory>>,
    docker_status: DockerStatus,
    known_paths: HashSet<PathBuf>,
}

impl CleaningService {
    pub fn new(
        config: AppConfig,
        home: Option<PathBuf>,
        protection: ProtectionRules,
        guard: Arc<CriticalPathGuard>,
        ports: CleaningPorts,
    ) -> Self {
        let executor = CleaningExecutor {
            remover: ports.remover,
            container_engine: Arc::clone(&ports.container_engine),
            command_runner: Arc::clone(&ports.command_runner),
            guard,
        };
        Self {
            config,
            home,
            protection,
            probe: ports.probe,
            container_engine: ports.container_engine,
            command_runner: ports.command_runner,
            executor,
            candidates: Vec::new(),
            next_candidate: 1,
            discovery: None,
            docker_receiver: None,
            docker_status: DockerStatus::Disabled,
            known_paths: HashSet::new(),
        }
    }

    /// Replaces the configuration; takes effect the next time candidates are collected.
    pub fn set_config(&mut self, config: AppConfig) -> Result<(), crate::domain::cleaning::InvalidPattern> {
        self.protection = config.protection_rules(self.home.as_deref())?;
        self.config = config;
        Ok(())
    }

    pub fn executor(&self) -> CleaningExecutor {
        self.executor.clone()
    }

    pub fn candidates(&self) -> &[CleaningCandidate] {
        &self.candidates
    }

    pub fn docker_status(&self) -> &DockerStatus {
        &self.docker_status
    }

    pub fn has_started(&self) -> bool {
        self.discovery.is_some() || !self.candidates.is_empty()
    }

    /// Forgets previous results and starts collecting candidates again.
    pub fn begin(&mut self, coordinator: &mut TreeCoordinator) {
        self.forget_measurements(coordinator);
        self.candidates.clear();
        self.known_paths.clear();
        self.next_candidate = 1;
        let enabled = self.config.enabled_categories();

        let known = existing_known_locations(self.home.as_deref(), &*self.probe);
        for location in known {
            if enabled.contains(&location.category) {
                self.add_known(location, coordinator);
            }
        }
        let runner = Arc::clone(&self.command_runner);
        for location in command_locations(&|program| runner.is_installed(program)) {
            if enabled.contains(&location.category) {
                self.add_known(location, coordinator);
            }
        }

        let discovered_categories: Vec<CleaningCategory> =
            enabled.iter().copied().filter(|category| category.is_discovered()).collect();
        if !discovered_categories.is_empty() {
            let rules = Arc::new(DiscoveryRules::standard(
                &discovered_categories,
                self.config.discovery_exclusions(self.home.as_deref()),
                self.config.cleaner.discovery_max_depth.clamp(2, 64),
            ));
            let roots = self.config.discovery_roots(self.home.as_deref());
            self.discovery = Some(coordinator.start_discovery(roots, rules));
        } else {
            self.discovery = None;
        }

        if enabled.iter().any(|category| category.is_docker()) {
            self.docker_status = DockerStatus::Checking;
            self.docker_receiver = Some(spawn_docker_inventory(Arc::clone(&self.container_engine)));
        } else {
            self.docker_status = DockerStatus::Disabled;
            self.docker_receiver = None;
        }
    }

    fn forget_measurements(&mut self, coordinator: &mut TreeCoordinator) {
        for candidate in &self.candidates {
            if let Some(node) = candidate.measurement_node {
                coordinator.remove_subtree(node);
            }
        }
    }

    fn add_known(&mut self, location: KnownLocation, coordinator: &mut TreeCoordinator) {
        let path = location.location.path().map(Path::to_path_buf);
        if let Some(path) = &path {
            if !self.known_paths.insert(path.clone()) {
                return;
            }
        }
        let protection = path
            .as_deref()
            .and_then(|path| self.protection.protecting_path_rule(path))
            .map(|rule| Protection::Rule(rule.to_owned()))
            .unwrap_or(Protection::None);
        let measurement_node = path.map(|path| coordinator.start_root_scan(path, RootPurpose::Measurement));
        let candidate = CleaningCandidate {
            id: self.allocate_id(),
            category: location.category,
            label: location.label,
            location: location.location,
            size: None,
            requires_privileges: location.requires_privileges,
            protection: protection.clone(),
            selected: location.category.selected_by_default() && !protection.is_protected(),
            measurement_node,
        };
        self.candidates.push(candidate);
    }

    fn allocate_id(&mut self) -> CandidateId {
        let id = CandidateId(self.next_candidate);
        self.next_candidate += 1;
        id
    }

    /// Pulls in freshly discovered directories, docker results and current sizes.
    pub fn update(&mut self, coordinator: &mut TreeCoordinator, size_mode: SizeMode) {
        if let Some(search) = self.discovery {
            for target in coordinator.take_discovered(search) {
                if !self.known_paths.insert(target.path.clone()) {
                    continue;
                }
                let protection = self
                    .protection
                    .protecting_path_rule(&target.path)
                    .map(|rule| Protection::Rule(rule.to_owned()))
                    .unwrap_or(Protection::None);
                let label = self.relative_label(&target.path);
                let node = coordinator.start_root_scan(target.path.clone(), RootPurpose::Measurement);
                let candidate = CleaningCandidate {
                    id: self.allocate_id(),
                    category: target.category,
                    label,
                    location: CleaningLocation::Directory(target.path),
                    size: None,
                    requires_privileges: false,
                    protection: protection.clone(),
                    selected: target.category.selected_by_default() && !protection.is_protected(),
                    measurement_node: Some(node),
                };
                self.candidates.push(candidate);
            }
        }

        if let Some(receiver) = &self.docker_receiver {
            if let Ok(inventory) = receiver.try_recv() {
                self.docker_receiver = None;
                self.absorb_docker_inventory(inventory);
            }
        }

        let tree = coordinator.tree();
        for candidate in &mut self.candidates {
            if let Some(node) = candidate.measurement_node {
                if let Some(tree_node) = tree.node(node) {
                    candidate.size = Some(tree_node.total_size().select(size_mode));
                }
            }
        }
    }

    fn relative_label(&self, path: &Path) -> String {
        match &self.home {
            Some(home) => match path.strip_prefix(home) {
                Ok(relative) => format!("~/{}", relative.display()),
                Err(_) => path.display().to_string(),
            },
            None => path.display().to_string(),
        }
    }

    fn absorb_docker_inventory(&mut self, inventory: DockerInventory) {
        self.docker_status = inventory.status.clone();
        if !matches!(inventory.status, DockerStatus::Available { .. }) {
            return;
        }
        let enabled = self.config.enabled_categories();
        if enabled.contains(&CleaningCategory::DockerImages) {
            for image in inventory.images {
                let reference = image.reference();
                let in_use = inventory
                    .image_ids_in_use
                    .iter()
                    .any(|used| used.starts_with(&image.id) || image.id.starts_with(used));
                let protection = if in_use {
                    Protection::InUse("used by a container".to_owned())
                } else if let Some(rule) =
                    self.protection.protecting_image_rule(&reference, &image.repository, &image.id)
                {
                    Protection::Rule(rule.to_owned())
                } else {
                    Protection::None
                };
                let candidate = CleaningCandidate {
                    id: self.allocate_id(),
                    category: CleaningCategory::DockerImages,
                    label: format!("{reference}  ({})", image.created),
                    location: CleaningLocation::DockerImage { id: image.id.clone(), reference },
                    size: Some(image.size),
                    requires_privileges: false,
                    protection: protection.clone(),
                    selected: !protection.is_protected(),
                    measurement_node: None,
                };
                self.candidates.push(candidate);
            }
        }
        let prunes = [
            (
                CleaningCategory::DockerContainers,
                DockerPruneKind::StoppedContainers,
                inventory.usage.containers_reclaimable,
            ),
            (
                CleaningCategory::DockerVolumes,
                DockerPruneKind::DanglingVolumes,
                inventory.usage.volumes_reclaimable,
            ),
            (
                CleaningCategory::DockerBuildCache,
                DockerPruneKind::BuildCache,
                inventory.usage.build_cache_reclaimable,
            ),
        ];
        for (category, kind, reclaimable) in prunes {
            if !enabled.contains(&category) {
                continue;
            }
            let candidate = CleaningCandidate {
                id: self.allocate_id(),
                category,
                label: kind.label().to_owned(),
                location: CleaningLocation::DockerPrune(kind),
                size: Some(reclaimable),
                requires_privileges: false,
                protection: Protection::None,
                selected: category.selected_by_default(),
                measurement_node: None,
            };
            self.candidates.push(candidate);
        }
    }

    pub fn is_busy(&self, coordinator: &TreeCoordinator) -> bool {
        let discovering = self
            .discovery
            .and_then(|search| coordinator.discovery_search(search))
            .is_some_and(|search| !search.is_complete());
        let measuring = self.candidates.iter().any(|candidate| {
            candidate
                .measurement_node
                .and_then(|node| coordinator.tree().node(node))
                .is_some_and(|node| !node.is_complete())
        });
        discovering || measuring || matches!(self.docker_status, DockerStatus::Checking)
    }

    pub fn toggle(&mut self, id: CandidateId) {
        if let Some(candidate) = self.candidates.iter_mut().find(|candidate| candidate.id == id) {
            if !candidate.is_protected() {
                candidate.selected = !candidate.selected;
            }
        }
    }

    pub fn set_all(&mut self, selected: bool) {
        for candidate in &mut self.candidates {
            if !candidate.is_protected() {
                candidate.selected = selected;
            }
        }
    }

    pub fn toggle_category(&mut self, category: CleaningCategory) {
        let any_selected =
            self.candidates.iter().any(|candidate| candidate.category == category && candidate.selected);
        for candidate in &mut self.candidates {
            if candidate.category == category && !candidate.is_protected() {
                candidate.selected = !any_selected;
            }
        }
    }

    /// Selected, unprotected candidates with nested paths collapsed into their parents.
    pub fn build_plan(&self) -> CleaningPlan {
        let mut plan = CleaningPlan::from_actionable(&self.candidates);
        let paths: Vec<PathBuf> =
            plan.candidates.iter().filter_map(|candidate| candidate.path().map(Path::to_path_buf)).collect();
        plan.candidates.retain(|candidate| {
            let Some(path) = candidate.path() else {
                return true;
            };
            !paths.iter().any(|other| other != path && path.starts_with(other))
        });
        plan
    }

    /// Removes candidates that were cleaned and re-measures those that were only partially
    /// removed (cancelled, failed half way, waiting for privileges).
    pub fn absorb_report(&mut self, report: &CleaningReport, coordinator: &mut TreeCoordinator) {
        let cleaned: HashSet<CandidateId> = report
            .outcomes
            .iter()
            .filter(|outcome| outcome.status == CleaningStatus::Cleaned)
            .map(|outcome| outcome.candidate)
            .collect();
        let partially_removed: HashSet<CandidateId> = report
            .outcomes
            .iter()
            .filter(|outcome| outcome.status != CleaningStatus::Cleaned && outcome.entries_removed > 0)
            .map(|outcome| outcome.candidate)
            .collect();
        for candidate in &self.candidates {
            let Some(node) = candidate.measurement_node else {
                continue;
            };
            if cleaned.contains(&candidate.id) {
                coordinator.remove_subtree(node);
            } else if partially_removed.contains(&candidate.id) {
                coordinator.rescan(node, false);
            }
        }
        self.candidates.retain(|candidate| !cleaned.contains(&candidate.id));
    }

    pub fn candidate(&self, id: CandidateId) -> Option<&CleaningCandidate> {
        self.candidates.iter().find(|candidate| candidate.id == id)
    }

    /// Paths of candidates that need privileges, split by how they must be removed.
    pub fn privileged_paths(&self, ids: &[CandidateId]) -> (Vec<PathBuf>, Vec<PathBuf>) {
        let mut whole = Vec::new();
        let mut contents = Vec::new();
        for id in ids {
            let Some(candidate) = self.candidate(*id) else {
                continue;
            };
            match &candidate.location {
                CleaningLocation::Directory(path) => whole.push(path.clone()),
                CleaningLocation::DirectoryContents(path) => contents.push(path.clone()),
                _ => {}
            }
        }
        (whole, contents)
    }
}
