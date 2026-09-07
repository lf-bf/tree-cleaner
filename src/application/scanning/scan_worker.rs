//! Executes scan tasks on a worker thread and reports through events.

use std::collections::HashSet;
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use crossbeam_channel::Sender;
use parking_lot::{Mutex, RwLock};

use super::node_id_allocator::NodeIdAllocator;
use super::scan_statistics::ScanStatistics;
use super::scan_task::{
    CollectHeaviestFilesTask, DiscoverTask, ListFilesTask, MeasureDirectoryTask, ScanTask,
};
use super::task_queue::TaskQueue;
use crate::domain::ports::{DirectoryReader, FileSystemProbe, Visit};
use crate::domain::storage::{
    Attribution, BoundaryReason, ChildDirectory, DirectoryMeasurement, DiscoveredTarget, FileListing,
    HeaviestFiles, HeavyFile, InodeIdentity, ListedFile, MeasuredSize, NodeRef, ScanEvent, ScanPolicy,
};

/// Settings every task consults. Swapped atomically when the user changes them.
#[derive(Clone, Debug)]
pub struct ScanSettings {
    pub policy: ScanPolicy,
    pub mount_points: HashSet<PathBuf>,
    /// Directories measured as roots of their own; a scan reaching them from above grafts
    /// the existing subtree instead of measuring it again.
    pub graft_points: HashSet<PathBuf>,
}

/// Everything a worker thread needs, shared by all workers.
pub struct WorkerContext {
    pub reader: Arc<dyn DirectoryReader>,
    pub probe: Arc<dyn FileSystemProbe>,
    pub queue: Arc<TaskQueue>,
    pub allocator: NodeIdAllocator,
    pub statistics: Arc<ScanStatistics>,
    pub settings: RwLock<Arc<ScanSettings>>,
    pub hard_links: Mutex<HashSet<InodeIdentity>>,
    pub events: Sender<ScanEvent>,
}

impl WorkerContext {
    pub fn settings(&self) -> Arc<ScanSettings> {
        Arc::clone(&self.settings.read())
    }

    fn emit(&self, event: ScanEvent) {
        // The receiver only disappears when the application is shutting down.
        let _ = self.events.send(event);
    }
}

pub struct ScanWorker {
    context: Arc<WorkerContext>,
}

impl ScanWorker {
    pub fn new(context: Arc<WorkerContext>) -> Self {
        Self { context }
    }

    pub fn execute(&self, task: ScanTask) {
        match task {
            ScanTask::MeasureDirectory(task) => self.measure_directory(task),
            ScanTask::ListFiles(task) => self.list_files(task),
            ScanTask::CollectHeaviestFiles(task) => self.collect_heaviest_files(task),
            ScanTask::Discover(task) => self.discover(task),
        }
    }

    // ----------------------------------------------------------------------------------
    // Measuring
    // ----------------------------------------------------------------------------------

    fn measure_directory(&self, task: MeasureDirectoryTask) {
        let context = &*self.context;
        let settings = context.settings();
        let policy = &settings.policy;

        let mut own_size = MeasuredSize::ZERO;
        let mut file_count = 0u64;
        let mut entry_count = 0u64;
        let mut subdirectories: Vec<OsString> = Vec::new();

        let read_result = context.reader.read_directory(&task.path, &mut |entry| {
            entry_count += 1;
            let (kind, prefetched) = match entry.kind_hint() {
                Some(kind) => (kind, None),
                None => match entry.metadata() {
                    Ok(metadata) => (metadata.kind, Some(metadata)),
                    Err(_) => return Visit::Continue,
                },
            };
            if kind.is_directory() {
                subdirectories.push(entry.file_name().to_os_string());
                return Visit::Continue;
            }
            let metadata = match prefetched {
                Some(metadata) => metadata,
                None => match entry.metadata() {
                    Ok(metadata) => metadata,
                    Err(_) => return Visit::Continue,
                },
            };
            if policy.deduplicate_hard_links
                && metadata.has_multiple_hard_links()
                && !context.hard_links.lock().insert(metadata.inode_identity())
            {
                context.statistics.record_hard_link_skipped();
                return Visit::Continue;
            }
            own_size += metadata.size;
            file_count += 1;
            Visit::Continue
        });

        let attribution = if task.describes_target { Attribution::Own } else { Attribution::Anonymous };

        if let Err(error) = read_result {
            context.statistics.record_unreadable(matches!(
                error,
                crate::domain::ports::DirectoryReadError::PermissionDenied
            ));
            context.emit(ScanEvent::DirectoryMeasured(DirectoryMeasurement {
                target: task.target,
                attribution,
                own_size: MeasuredSize::ZERO,
                file_count: 0,
                entry_count: 0,
                materialized_children: Vec::new(),
                boundary_children: Vec::new(),
                anonymous_subdirectory_count: 0,
                depth_limited_subdirectory_count: 0,
                dense: false,
                problem: Some(error.to_access_problem()),
            }));
            return;
        }

        context.statistics.record_directory(file_count, own_size.allocated.as_u64());

        let dense = !task.ignore_density && policy.is_dense(entry_count as usize);
        if dense {
            context.statistics.record_dense();
        }
        let child_depth_from_origin = task.depth_from_origin.saturating_add(1);
        let child_tree_depth = task.tree_depth.saturating_add(1);
        let within_depth_limit = child_tree_depth <= policy.max_depth;
        let materialize =
            task.materialize_children && !dense && child_depth_from_origin <= policy.materialize_depth;
        let show_boundaries = task.describes_target && !dense;

        let mut materialized_children = Vec::new();
        let mut boundary_children = Vec::new();
        let mut child_tasks = Vec::with_capacity(subdirectories.len());
        let mut anonymous_subdirectory_count = 0u32;
        let mut depth_limited_subdirectory_count = 0u32;

        for name in subdirectories {
            let child_path = task.path.join(&name);
            let boundary = if policy.should_skip(&child_path) {
                Some(BoundaryReason::ConfiguredSkip)
            } else if !policy.cross_mount_points && settings.mount_points.contains(&child_path) {
                Some(BoundaryReason::MountPoint)
            } else if !settings.graft_points.is_empty() && settings.graft_points.contains(&child_path) {
                Some(BoundaryReason::OtherRoot)
            } else {
                None
            };
            if let Some(reason) = boundary {
                if show_boundaries {
                    boundary_children
                        .push((ChildDirectory { id: context.allocator.allocate(), name }, reason));
                }
                continue;
            }
            if !within_depth_limit {
                depth_limited_subdirectory_count += 1;
                continue;
            }
            if materialize {
                let id = context.allocator.allocate();
                materialized_children.push(ChildDirectory { id, name });
                child_tasks.push(ScanTask::MeasureDirectory(MeasureDirectoryTask {
                    target: NodeRef { id, generation: 0 },
                    path: child_path,
                    depth_from_origin: child_depth_from_origin,
                    tree_depth: child_tree_depth,
                    describes_target: true,
                    materialize_children: true,
                    ignore_density: false,
                }));
            } else {
                anonymous_subdirectory_count += 1;
                child_tasks.push(ScanTask::MeasureDirectory(MeasureDirectoryTask {
                    target: task.target,
                    path: child_path,
                    depth_from_origin: child_depth_from_origin,
                    tree_depth: child_tree_depth,
                    describes_target: false,
                    materialize_children: false,
                    ignore_density: false,
                }));
            }
        }

        // The event must be visible to the tree owner before any child event can arrive.
        context.emit(ScanEvent::DirectoryMeasured(DirectoryMeasurement {
            target: task.target,
            attribution,
            own_size,
            file_count,
            entry_count,
            materialized_children,
            boundary_children,
            anonymous_subdirectory_count,
            depth_limited_subdirectory_count,
            dense,
            problem: None,
        }));
        context.queue.push_many(child_tasks);
    }

    // ----------------------------------------------------------------------------------
    // Listing the files of one directory
    // ----------------------------------------------------------------------------------

    fn list_files(&self, task: ListFilesTask) {
        let context = &*self.context;
        let mut heaviest = HeaviestFiles::new(task.limit, task.size_mode);
        let mut total_file_count = 0u64;

        let read_result = context.reader.read_directory(&task.path, &mut |entry| {
            let (kind, prefetched) = match entry.kind_hint() {
                Some(kind) => (kind, None),
                None => match entry.metadata() {
                    Ok(metadata) => (metadata.kind, Some(metadata)),
                    Err(_) => return Visit::Continue,
                },
            };
            if kind.is_directory() {
                return Visit::Continue;
            }
            let metadata = match prefetched {
                Some(metadata) => metadata,
                None => match entry.metadata() {
                    Ok(metadata) => metadata,
                    Err(_) => return Visit::Continue,
                },
            };
            total_file_count += 1;
            heaviest.offer(HeavyFile { path: PathBuf::from(entry.file_name()), size: metadata.size, kind });
            Visit::Continue
        });

        let problem = read_result.err().map(|error| error.to_access_problem());
        let kept = heaviest.len() as u64;
        let files = heaviest
            .into_sorted_vec()
            .into_iter()
            .map(|file| ListedFile { name: file.path.into_os_string(), size: file.size, kind: file.kind })
            .collect();
        context.emit(ScanEvent::FilesListed {
            node: task.node,
            request: task.request,
            listing: FileListing { files, total_file_count, truncated: total_file_count > kept },
            problem,
        });
    }

    // ----------------------------------------------------------------------------------
    // Heaviest files of a subtree
    // ----------------------------------------------------------------------------------

    fn collect_heaviest_files(&self, task: CollectHeaviestFilesTask) {
        let context = &*self.context;
        let settings = context.settings();
        let policy = &settings.policy;
        let floor = task.floor.load(Ordering::Relaxed);
        let mut heaviest = HeaviestFiles::new(task.limit, task.size_mode);
        let mut files_seen = 0u64;
        let mut subdirectories: Vec<OsString> = Vec::new();

        let _ = context.reader.read_directory(&task.path, &mut |entry| {
            let (kind, prefetched) = match entry.kind_hint() {
                Some(kind) => (kind, None),
                None => match entry.metadata() {
                    Ok(metadata) => (metadata.kind, Some(metadata)),
                    Err(_) => return Visit::Continue,
                },
            };
            if kind.is_directory() {
                subdirectories.push(entry.file_name().to_os_string());
                return Visit::Continue;
            }
            let metadata = match prefetched {
                Some(metadata) => metadata,
                None => match entry.metadata() {
                    Ok(metadata) => metadata,
                    Err(_) => return Visit::Continue,
                },
            };
            files_seen += 1;
            if metadata.size.select(task.size_mode).as_u64() <= floor {
                return Visit::Continue;
            }
            heaviest.offer(HeavyFile { path: task.path.join(entry.file_name()), size: metadata.size, kind });
            Visit::Continue
        });

        let child_depth = task.tree_depth.saturating_add(1);
        let mut child_tasks = Vec::new();
        if child_depth <= policy.max_depth {
            for name in subdirectories {
                let child_path = task.path.join(name);
                if policy.should_skip(&child_path)
                    || (!policy.cross_mount_points && settings.mount_points.contains(&child_path))
                {
                    continue;
                }
                child_tasks.push(ScanTask::CollectHeaviestFiles(CollectHeaviestFilesTask {
                    query: task.query,
                    path: child_path,
                    limit: task.limit,
                    size_mode: task.size_mode,
                    tree_depth: child_depth,
                    floor: Arc::clone(&task.floor),
                }));
            }
        }
        context.emit(ScanEvent::HeaviestFilesProgress {
            query: task.query,
            files: heaviest.into_sorted_vec(),
            files_seen,
            spawned: child_tasks.len() as u32,
        });
        context.queue.push_many(child_tasks);
    }

    // ----------------------------------------------------------------------------------
    // Discovery of cleaning targets
    // ----------------------------------------------------------------------------------

    fn discover(&self, task: DiscoverTask) {
        let context = &*self.context;
        let rules = &task.rules;
        let mut matches: Vec<(OsString, usize)> = Vec::new();
        let mut subdirectories: Vec<OsString> = Vec::new();

        let _ = context.reader.read_directory(&task.path, &mut |entry| {
            let kind = match entry.kind_hint() {
                Some(kind) => kind,
                None => match entry.metadata() {
                    Ok(metadata) => metadata.kind,
                    Err(_) => return Visit::Continue,
                },
            };
            if !kind.is_directory() {
                return Visit::Continue;
            }
            let name = entry.file_name();
            if let Some(rule) = rules.matching_rule(name) {
                let index = rules.rule_index(rule);
                matches.push((name.to_os_string(), index));
            } else if rules.should_descend_into(name) {
                subdirectories.push(name.to_os_string());
            }
            Visit::Continue
        });

        let mut found = Vec::new();
        for (name, rule_index) in matches {
            let Some(rule) = rules.rule_at(rule_index) else {
                continue;
            };
            let candidate_path = task.path.join(&name);
            if rules.is_excluded(&candidate_path) {
                continue;
            }
            if rules.verify(rule, &candidate_path, &task.path, &*context.probe) {
                found.push(DiscoveredTarget { category: rule.category, path: candidate_path });
            } else if rules.should_descend_into(&name) {
                subdirectories.push(name);
            }
        }

        let child_depth = task.depth.saturating_add(1);
        let mut child_tasks = Vec::new();
        if child_depth <= rules.max_depth {
            for name in subdirectories {
                let child_path = task.path.join(name);
                if rules.is_excluded(&child_path) {
                    continue;
                }
                child_tasks.push(ScanTask::Discover(DiscoverTask {
                    search: task.search,
                    path: child_path,
                    depth: child_depth,
                    rules: Arc::clone(&task.rules),
                }));
            }
        }
        context.emit(ScanEvent::DiscoveryProgress {
            search: task.search,
            found,
            spawned: child_tasks.len() as u32,
        });
        context.queue.push_many(child_tasks);
    }
}
