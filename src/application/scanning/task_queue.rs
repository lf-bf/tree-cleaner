//! Focus-aware work queue shared by the scanner threads.
//!
//! Three lanes: urgent interactive requests (FIFO), work under the directory the user is
//! looking at, and everything else. The two directory lanes are stacks, which gives a
//! depth-first walk: one branch is followed to its leaves before the next one starts, so
//! the frontier stays small and sizes of the focused branch settle quickly.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::Duration;

use parking_lot::{Condvar, Mutex};

use super::scan_task::ScanTask;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QueueDepths {
    pub urgent: usize,
    pub focused: usize,
    pub background: usize,
}

impl QueueDepths {
    pub const fn total(self) -> usize {
        self.urgent + self.focused + self.background
    }
}

#[derive(Default)]
struct QueueState {
    urgent: VecDeque<ScanTask>,
    focused: Vec<ScanTask>,
    background: Vec<ScanTask>,
    focus: Option<PathBuf>,
    background_paused: bool,
    shutting_down: bool,
}

impl QueueState {
    fn is_focused(&self, path: &Path) -> bool {
        self.focus.as_deref().is_some_and(|focus| path.starts_with(focus))
    }

    fn push(&mut self, task: ScanTask) {
        if task.is_urgent() {
            self.urgent.push_back(task);
        } else if self.is_focused(task.path()) {
            self.focused.push(task);
        } else {
            self.background.push(task);
        }
    }

    fn pop(&mut self) -> Option<ScanTask> {
        if let Some(task) = self.urgent.pop_front() {
            return Some(task);
        }
        if let Some(task) = self.focused.pop() {
            return Some(task);
        }
        if self.background_paused {
            return None;
        }
        self.background.pop()
    }

    fn has_runnable_work(&self) -> bool {
        !self.urgent.is_empty()
            || !self.focused.is_empty()
            || (!self.background_paused && !self.background.is_empty())
    }
}

#[derive(Default)]
pub struct TaskQueue {
    state: Mutex<QueueState>,
    work_available: Condvar,
}

impl TaskQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&self, task: ScanTask) {
        let mut state = self.state.lock();
        state.push(task);
        drop(state);
        self.work_available.notify_one();
    }

    pub fn push_many(&self, tasks: impl IntoIterator<Item = ScanTask>) {
        let mut state = self.state.lock();
        let mut pushed = 0usize;
        for task in tasks {
            state.push(task);
            pushed += 1;
        }
        drop(state);
        if pushed > 1 {
            self.work_available.notify_all();
        } else if pushed == 1 {
            self.work_available.notify_one();
        }
    }

    /// Blocks until a task is available. Returns `None` after [`Self::shutdown`], or when
    /// `idle_timeout` passes with no work so the caller can re-check its own state.
    pub fn pop(&self, idle_timeout: Duration) -> Option<ScanTask> {
        let mut state = self.state.lock();
        loop {
            if state.shutting_down {
                return None;
            }
            if let Some(task) = state.pop() {
                return Some(task);
            }
            if self.work_available.wait_for(&mut state, idle_timeout).timed_out() {
                return None;
            }
        }
    }

    /// Moves every queued task into the lane matching the new focus.
    pub fn set_focus(&self, focus: Option<PathBuf>) {
        let mut state = self.state.lock();
        if state.focus == focus {
            return;
        }
        state.focus = focus;
        let previously_focused = std::mem::take(&mut state.focused);
        let previously_background = std::mem::take(&mut state.background);
        for task in previously_focused.into_iter().chain(previously_background) {
            state.push(task);
        }
        drop(state);
        self.work_available.notify_all();
    }

    pub fn focus(&self) -> Option<PathBuf> {
        self.state.lock().focus.clone()
    }

    pub fn set_background_paused(&self, paused: bool) {
        let mut state = self.state.lock();
        state.background_paused = paused;
        drop(state);
        self.work_available.notify_all();
    }

    pub fn is_background_paused(&self) -> bool {
        self.state.lock().background_paused
    }

    /// Drops every queued task for which `should_discard` returns true. Running tasks are
    /// unaffected; their events are rejected later by the tree owner.
    pub fn discard_matching(&self, should_discard: &dyn Fn(&ScanTask) -> bool) -> usize {
        let mut state = self.state.lock();
        let before = state.focused.len() + state.background.len() + state.urgent.len();
        state.focused.retain(|task| !should_discard(task));
        state.background.retain(|task| !should_discard(task));
        state.urgent.retain(|task| !should_discard(task));
        before - (state.focused.len() + state.background.len() + state.urgent.len())
    }

    /// Drops queued measurement tasks below `path` (used before a rescan or a deletion).
    pub fn discard_measurements_under(&self, path: &Path) -> usize {
        self.discard_matching(&|task| {
            matches!(task, ScanTask::MeasureDirectory(_)) && task.path().starts_with(path)
        })
    }

    pub fn is_shut_down(&self) -> bool {
        self.state.lock().shutting_down
    }

    pub fn clear(&self) {
        let mut state = self.state.lock();
        state.urgent.clear();
        state.focused.clear();
        state.background.clear();
    }

    pub fn depths(&self) -> QueueDepths {
        let state = self.state.lock();
        QueueDepths {
            urgent: state.urgent.len(),
            focused: state.focused.len(),
            background: state.background.len(),
        }
    }

    pub fn has_runnable_work(&self) -> bool {
        self.state.lock().has_runnable_work()
    }

    pub fn shutdown(&self) {
        let mut state = self.state.lock();
        state.shutting_down = true;
        state.urgent.clear();
        state.focused.clear();
        state.background.clear();
        drop(state);
        self.work_available.notify_all();
    }
}
