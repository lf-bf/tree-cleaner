//! Single-threaded owner of the [`FileTree`]. Applies scanner events, serves the
//! interface, and translates user intent (focus, rescan, queries) into tasks.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use super::scan_engine::ScanEngine;
use super::scan_task::{
    CollectHeaviestFilesTask, DiscoverTask, ListFilesTask, MeasureDirectoryTask, ScanTask,
};
use crate::application::cleaning::discovery_rules::DiscoveryRules;
use crate::domain::storage::{
    AccessProblem, Attribution, BoundaryReason, DirectoryMeasurement, DiscoveredTarget, FileListing,
    FileTree, HeaviestFiles, HeavyFile, MeasurementDelta, NodeId, NodeRef, QueryId, RequestId, RootPurpose,
    ScanEvent, SearchId, SizeMode,
};

const LISTING_CACHE_CAPACITY: usize = 64;

/// What is known about the files directly inside one directory.
#[derive(Debug, Default)]
pub struct FileListingState {
    pub listing: Option<FileListing>,
    pub problem: Option<AccessProblem>,
    pub pending: Option<RequestId>,
    pub requested_at: Option<Instant>,
}

impl FileListingState {
    pub fn is_loading(&self) -> bool {
        self.pending.is_some()
    }
}

/// A running or finished search for the largest files below a directory.
#[derive(Debug)]
pub struct HeaviestQuery {
    pub origin: NodeId,
    pub origin_path: PathBuf,
    heaviest: HeaviestFiles,
    outstanding: u32,
    pub files_seen: u64,
    floor: Arc<AtomicU64>,
    pub started_at: Instant,
    pub finished_at: Option<Instant>,
}

impl HeaviestQuery {
    pub fn is_complete(&self) -> bool {
        self.finished_at.is_some()
    }

    pub fn limit(&self) -> usize {
        self.heaviest.limit()
    }

    pub fn snapshot(&self) -> Vec<HeavyFile> {
        self.heaviest.sorted_snapshot()
    }

    pub fn elapsed(&self) -> Duration {
        self.finished_at.unwrap_or_else(Instant::now).duration_since(self.started_at)
    }
}

/// A running or finished search for cleaning targets.
#[derive(Debug)]
pub struct DiscoverySearch {
    pub found: Vec<DiscoveredTarget>,
    pub consumed: usize,
    outstanding: u32,
    pub directories_visited: u64,
    pub started_at: Instant,
    pub finished_at: Option<Instant>,
}

impl DiscoverySearch {
    pub fn is_complete(&self) -> bool {
        self.finished_at.is_some()
    }
}

pub struct TreeCoordinator {
    tree: FileTree,
    engine: Arc<ScanEngine>,
    listings: HashMap<NodeId, FileListingState>,
    listing_order: VecDeque<NodeId>,
    heaviest_queries: HashMap<QueryId, HeaviestQuery>,
    discovery_searches: HashMap<SearchId, DiscoverySearch>,
    next_request: u64,
    next_query: u64,
    next_search: u64,
    size_mode: SizeMode,
    events_applied: u64,
}

impl TreeCoordinator {
    pub fn new(engine: Arc<ScanEngine>, size_mode: SizeMode) -> Self {
        Self {
            tree: FileTree::new(),
            engine,
            listings: HashMap::new(),
            listing_order: VecDeque::new(),
            heaviest_queries: HashMap::new(),
            discovery_searches: HashMap::new(),
            next_request: 1,
            next_query: 1,
            next_search: 1,
            size_mode,
            events_applied: 0,
        }
    }

    pub fn tree(&self) -> &FileTree {
        &self.tree
    }

    /// Direct mutable access for corrections the interface knows about (deleted files).
    pub fn tree_mut(&mut self) -> &mut FileTree {
        &mut self.tree
    }

    pub fn engine(&self) -> &Arc<ScanEngine> {
        &self.engine
    }

    pub const fn size_mode(&self) -> SizeMode {
        self.size_mode
    }

    pub fn set_size_mode(&mut self, mode: SizeMode) {
        self.size_mode = mode;
    }

    pub const fn events_applied(&self) -> u64 {
        self.events_applied
    }

    // ----------------------------------------------------------------------------------
    // Roots, focus and rescans
    // ----------------------------------------------------------------------------------

    /// Starts measuring `path` as a root of the tree, or returns the existing root.
    pub fn start_root_scan(&mut self, path: PathBuf, purpose: RootPurpose) -> NodeId {
        if let Some(root) = self.tree.root_by_path(&path) {
            return root.node;
        }
        let id = self.engine.allocator().allocate();
        let reference = self.tree.insert_root(id, path.clone(), purpose);
        self.submit_measurement(reference, path, 0, false);
        id
    }

    fn submit_measurement(&self, target: NodeRef, path: PathBuf, tree_depth: u16, ignore_density: bool) {
        self.engine.submit(ScanTask::MeasureDirectory(MeasureDirectoryTask {
            target,
            path,
            depth_from_origin: 0,
            tree_depth,
            describes_target: true,
            materialize_children: true,
            ignore_density,
        }));
    }

    /// Forgets and re-measures a directory. Also used to expand directories whose children
    /// were only summed so far.
    pub fn rescan(&mut self, node: NodeId, ignore_density: bool) -> Option<NodeRef> {
        let path = self.tree.path_of(node);
        let tree_depth = self.tree.node(node)?.depth();
        self.engine.queue().discard_measurements_under(&path);
        let reference = self.tree.reset_for_rescan(node)?;
        self.forget_listings_of_missing_nodes();
        self.invalidate_listing(node);
        self.submit_measurement(reference, path, tree_depth, ignore_density);
        Some(reference)
    }

    /// Makes sure the children of `node` exist as nodes, starting a scan rooted at it when
    /// they were only summed. Returns true when a scan was started.
    pub fn ensure_expanded(&mut self, node: NodeId) -> bool {
        let Some(tree_node) = self.tree.node(node) else {
            return false;
        };
        let flags = tree_node.flags();
        if flags.has_unmaterialized_children && !flags.dense && tree_node.children().is_empty() {
            return self.rescan(node, false).is_some();
        }
        false
    }

    /// The user is now looking at `node`: its subtree gets every worker, and its files are
    /// listed if they are not known yet.
    pub fn focus(&mut self, node: NodeId) {
        let path = self.tree.path_of(node);
        self.engine.set_focus(Some(path));
        self.request_file_listing(node, false);
    }

    pub fn clear_focus(&self) {
        self.engine.set_focus(None);
    }

    // ----------------------------------------------------------------------------------
    // File listings
    // ----------------------------------------------------------------------------------

    pub fn request_file_listing(&mut self, node: NodeId, force: bool) {
        let Some(reference) = self.tree.reference(node) else {
            return;
        };
        if let Some(state) = self.listings.get(&node) {
            if !force && (state.listing.is_some() || state.is_loading()) {
                return;
            }
        }
        let request = RequestId(self.next_request);
        self.next_request += 1;
        let settings = self.engine.settings();
        let path = self.tree.path_of(node);
        self.remember_listing(node);
        let state = self.listings.entry(node).or_default();
        state.pending = Some(request);
        state.requested_at = Some(Instant::now());
        self.engine.submit(ScanTask::ListFiles(ListFilesTask {
            node: reference,
            path,
            request,
            limit: settings.policy.file_listing_limit,
            size_mode: self.size_mode,
        }));
    }

    fn remember_listing(&mut self, node: NodeId) {
        if !self.listings.contains_key(&node) {
            self.listing_order.push_back(node);
            while self.listing_order.len() > LISTING_CACHE_CAPACITY {
                if let Some(evicted) = self.listing_order.pop_front() {
                    self.listings.remove(&evicted);
                }
            }
        }
    }

    pub fn file_listing(&self, node: NodeId) -> Option<&FileListingState> {
        self.listings.get(&node)
    }

    pub fn invalidate_listing(&mut self, node: NodeId) {
        self.listings.remove(&node);
        self.listing_order.retain(|candidate| *candidate != node);
    }

    fn forget_listings_of_missing_nodes(&mut self) {
        let tree = &self.tree;
        self.listings.retain(|node, _| tree.contains(*node));
        self.listing_order.retain(|node| tree.contains(*node));
    }

    // ----------------------------------------------------------------------------------
    // Heaviest files
    // ----------------------------------------------------------------------------------

    pub fn start_heaviest_query(&mut self, origin: NodeId, limit: usize) -> Option<QueryId> {
        self.tree.node(origin)?;
        let query = QueryId(self.next_query);
        self.next_query += 1;
        let origin_path = self.tree.path_of(origin);
        let floor = Arc::new(AtomicU64::new(0));
        let tree_depth = self.tree.node(origin).map(|node| node.depth()).unwrap_or(0);
        self.heaviest_queries.insert(
            query,
            HeaviestQuery {
                origin,
                origin_path: origin_path.clone(),
                heaviest: HeaviestFiles::new(limit, self.size_mode),
                outstanding: 1,
                files_seen: 0,
                floor: Arc::clone(&floor),
                started_at: Instant::now(),
                finished_at: None,
            },
        );
        self.engine.submit(ScanTask::CollectHeaviestFiles(CollectHeaviestFilesTask {
            query,
            path: origin_path,
            limit,
            size_mode: self.size_mode,
            tree_depth,
            floor,
        }));
        Some(query)
    }

    pub fn heaviest_query(&self, query: QueryId) -> Option<&HeaviestQuery> {
        self.heaviest_queries.get(&query)
    }

    pub fn cancel_heaviest_query(&mut self, query: QueryId) {
        self.engine.queue().discard_matching(
            &|task| matches!(task, ScanTask::CollectHeaviestFiles(task) if task.query == query),
        );
        self.heaviest_queries.remove(&query);
    }

    // ----------------------------------------------------------------------------------
    // Discovery
    // ----------------------------------------------------------------------------------

    pub fn start_discovery(&mut self, roots: Vec<PathBuf>, rules: Arc<DiscoveryRules>) -> SearchId {
        let search = SearchId(self.next_search);
        self.next_search += 1;
        let roots: Vec<PathBuf> = roots.into_iter().filter(|root| !rules.is_excluded(root)).collect();
        self.discovery_searches.insert(
            search,
            DiscoverySearch {
                found: Vec::new(),
                consumed: 0,
                outstanding: roots.len() as u32,
                directories_visited: 0,
                started_at: Instant::now(),
                finished_at: if roots.is_empty() { Some(Instant::now()) } else { None },
            },
        );
        for root in roots {
            self.engine.submit(ScanTask::Discover(DiscoverTask {
                search,
                path: root,
                depth: 0,
                rules: Arc::clone(&rules),
            }));
        }
        search
    }

    pub fn discovery_search(&self, search: SearchId) -> Option<&DiscoverySearch> {
        self.discovery_searches.get(&search)
    }

    /// Targets discovered since the previous call.
    pub fn take_discovered(&mut self, search: SearchId) -> Vec<DiscoveredTarget> {
        let Some(state) = self.discovery_searches.get_mut(&search) else {
            return Vec::new();
        };
        let fresh = state.found[state.consumed..].to_vec();
        state.consumed = state.found.len();
        fresh
    }

    pub fn cancel_discovery(&mut self, search: SearchId) {
        self.engine
            .queue()
            .discard_matching(&|task| matches!(task, ScanTask::Discover(task) if task.search == search));
        self.discovery_searches.remove(&search);
    }

    // ----------------------------------------------------------------------------------
    // Removal
    // ----------------------------------------------------------------------------------

    /// The directory behind `node` no longer exists on disk.
    pub fn remove_subtree(&mut self, node: NodeId) {
        let path = self.tree.path_of(node);
        self.engine.queue().discard_measurements_under(&path);
        self.tree.remove_subtree(node);
        self.forget_listings_of_missing_nodes();
    }

    /// Finds the node for exactly `path`. When it is not materialised yet, the directory
    /// becomes a root of its own so the user can look at it right away; a scan coming from
    /// above later grafts that subtree instead of measuring it again.
    pub fn node_for_path(&mut self, path: &Path) -> NodeId {
        if let Some((nearest, remaining)) = self.tree.locate_nearest(path) {
            if remaining.is_empty() {
                return nearest;
            }
        }
        let node = self.start_root_scan(path.to_path_buf(), RootPurpose::Filesystem);
        let graft_point = path.to_path_buf();
        self.engine.update_settings(|settings| {
            settings.graft_points.insert(graft_point);
        });
        node
    }

    // ----------------------------------------------------------------------------------
    // Event application
    // ----------------------------------------------------------------------------------

    /// Applies queued events for at most `budget`. Returns how many were applied.
    pub fn pump_events(&mut self, budget: Duration) -> usize {
        let started = Instant::now();
        let mut applied = 0usize;
        while let Ok(event) = self.engine.events().try_recv() {
            self.apply_event(event);
            applied += 1;
            if applied % 256 == 0 && started.elapsed() >= budget {
                break;
            }
        }
        self.events_applied += applied as u64;
        applied
    }

    fn apply_event(&mut self, event: ScanEvent) {
        match event {
            ScanEvent::DirectoryMeasured(measurement) => self.apply_measurement(measurement),
            ScanEvent::FilesListed { node, request, listing, problem } => {
                self.apply_file_listing(node, request, listing, problem)
            }
            ScanEvent::HeaviestFilesProgress { query, files, files_seen, spawned } => {
                self.apply_heaviest_progress(query, files, files_seen, spawned)
            }
            ScanEvent::DiscoveryProgress { search, found, spawned } => {
                self.apply_discovery_progress(search, found, spawned)
            }
        }
    }

    fn apply_measurement(&mut self, measurement: DirectoryMeasurement) {
        if !self.tree.is_current(measurement.target) {
            return;
        }
        let target = measurement.target.id;
        let describes_target = measurement.attribution == Attribution::Own;

        if let Some(problem) = measurement.problem {
            self.tree.record_problem(target, describes_target, problem.is_permission_denied());
            self.tree.finish_work_unit(target);
            return;
        }

        let materialized_count = measurement.materialized_children.len() as u64;
        let boundary_count = measurement.boundary_children.len() as u64;
        self.tree.insert_children(target, measurement.materialized_children);
        let mut mount_points = Vec::new();
        let mut skipped = Vec::new();
        let target_path = self.tree.path_of(target);
        for (child, reason) in measurement.boundary_children {
            match reason {
                BoundaryReason::MountPoint => mount_points.push(child),
                BoundaryReason::ConfiguredSkip => skipped.push(child),
                BoundaryReason::OtherRoot => {
                    let child_path = target_path.join(&child.name);
                    self.graft_or_measure(target, child, child_path);
                }
            }
        }
        if !mount_points.is_empty() {
            self.tree.insert_boundary_children(target, mount_points, BoundaryReason::MountPoint);
        }
        if !skipped.is_empty() {
            self.tree.insert_boundary_children(target, skipped, BoundaryReason::ConfiguredSkip);
        }
        self.tree.add_outstanding_work(target, measurement.anonymous_subdirectory_count);
        self.tree.record_measurement(
            target,
            MeasurementDelta {
                own_size: measurement.own_size,
                file_count: measurement.file_count,
                directory_count: materialized_count
                    + boundary_count
                    + u64::from(measurement.anonymous_subdirectory_count)
                    + u64::from(measurement.depth_limited_subdirectory_count),
                dense: describes_target && measurement.dense,
                depth_limited: measurement.depth_limited_subdirectory_count > 0,
                has_unmaterialized_children: describes_target
                    && !measurement.dense
                    && measurement.anonymous_subdirectory_count > 0,
            },
        );
        self.tree.finish_work_unit(target);
    }

    /// A scan reached a directory that is already a root: attach that subtree. When the
    /// root disappeared in the meantime, measure the directory like any other child.
    fn graft_or_measure(
        &mut self,
        parent: NodeId,
        child: crate::domain::storage::ChildDirectory,
        path: PathBuf,
    ) {
        self.engine.update_settings(|settings| {
            settings.graft_points.remove(&path);
        });
        let existing = self
            .tree
            .roots_with_purpose(RootPurpose::Filesystem)
            .find(|root| root.path == path)
            .map(|root| root.node);
        if let Some(root) = existing {
            if self.tree.graft_root(root, parent) {
                return;
            }
        }
        let tree_depth = self.tree.node(parent).map(|node| node.depth().saturating_add(1)).unwrap_or(0);
        let reference = NodeRef { id: child.id, generation: 0 };
        self.tree.insert_children(parent, vec![child]);
        self.engine.submit(ScanTask::MeasureDirectory(MeasureDirectoryTask {
            target: reference,
            path,
            depth_from_origin: 0,
            tree_depth,
            describes_target: true,
            materialize_children: true,
            ignore_density: false,
        }));
    }

    fn apply_file_listing(
        &mut self,
        node: NodeRef,
        request: RequestId,
        listing: FileListing,
        problem: Option<AccessProblem>,
    ) {
        if !self.tree.is_current(node) {
            return;
        }
        let Some(state) = self.listings.get_mut(&node.id) else {
            return;
        };
        if state.pending != Some(request) {
            return;
        }
        state.pending = None;
        state.listing = Some(listing);
        state.problem = problem;
    }

    fn apply_heaviest_progress(
        &mut self,
        query: QueryId,
        files: Vec<HeavyFile>,
        files_seen: u64,
        spawned: u32,
    ) {
        let Some(state) = self.heaviest_queries.get_mut(&query) else {
            return;
        };
        state.files_seen += files_seen;
        state.heaviest.merge(files);
        if state.heaviest.is_full() {
            if let Some(smallest) = state.heaviest.smallest_kept() {
                state.floor.store(smallest.as_u64(), Ordering::Relaxed);
            }
        }
        state.outstanding = state.outstanding.saturating_add(spawned).saturating_sub(1);
        if state.outstanding == 0 && state.finished_at.is_none() {
            state.finished_at = Some(Instant::now());
        }
    }

    fn apply_discovery_progress(&mut self, search: SearchId, found: Vec<DiscoveredTarget>, spawned: u32) {
        let Some(state) = self.discovery_searches.get_mut(&search) else {
            return;
        };
        state.directories_visited += 1;
        state.found.extend(found);
        state.outstanding = state.outstanding.saturating_add(spawned).saturating_sub(1);
        if state.outstanding == 0 && state.finished_at.is_none() {
            state.finished_at = Some(Instant::now());
        }
    }
}
