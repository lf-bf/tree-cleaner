//! State of the directory explorer and the rows it shows.

use std::ffi::OsString;
use std::path::PathBuf;

use ratatui::widgets::TableState;

use crate::domain::storage::{ByteSize, EntryKind, NodeFlags, NodeId};

/// Identity of a row that survives re-sorting while sizes change.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RowKey {
    Directory(NodeId),
    File(OsString),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowBadge {
    Scanning,
    Complete,
    Dense,
    Denied,
    Failed,
    Boundary,
    Lazy,
    File,
}

impl RowBadge {
    pub fn from_flags(flags: NodeFlags, complete: bool, has_children: bool) -> Self {
        if flags.boundary {
            Self::Boundary
        } else if flags.access_denied {
            Self::Denied
        } else if flags.read_failed {
            Self::Failed
        } else if flags.dense {
            Self::Dense
        } else if !complete {
            Self::Scanning
        } else if flags.has_unmaterialized_children && !has_children {
            Self::Lazy
        } else {
            Self::Complete
        }
    }
}

#[derive(Clone, Debug)]
pub struct ExplorerRow {
    pub key: RowKey,
    pub name: String,
    pub path: PathBuf,
    pub size: ByteSize,
    pub share: f64,
    pub items: Option<u64>,
    pub badge: RowBadge,
    pub is_directory: bool,
    pub kind: EntryKind,
    pub marked: bool,
}

#[derive(Debug)]
pub struct ExplorerState {
    pub current: Option<NodeId>,
    pub rows: Vec<ExplorerRow>,
    pub selected: usize,
    pub selected_key: Option<RowKey>,
    pub row_limit: usize,
    pub show_files: bool,
    pub filter: Option<String>,
    pub table_state: TableState,
    /// Total number of rows before the limit was applied.
    pub unlimited_row_count: usize,
}

impl ExplorerState {
    pub fn new(row_limit: usize) -> Self {
        Self {
            current: None,
            rows: Vec::new(),
            selected: 0,
            selected_key: None,
            row_limit,
            show_files: true,
            filter: None,
            table_state: TableState::default(),
            unlimited_row_count: 0,
        }
    }

    pub fn selected_row(&self) -> Option<&ExplorerRow> {
        self.rows.get(self.selected)
    }

    pub fn select(&mut self, index: usize) {
        if self.rows.is_empty() {
            self.selected = 0;
            self.selected_key = None;
        } else {
            self.selected = index.min(self.rows.len() - 1);
            self.selected_key = Some(self.rows[self.selected].key.clone());
        }
        self.table_state.select(Some(self.selected));
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.rows.is_empty() {
            return;
        }
        let target = (self.selected as isize + delta).clamp(0, self.rows.len() as isize - 1);
        self.select(target as usize);
    }

    /// After rows were rebuilt, keeps the cursor on the same item when it still exists.
    pub fn resync_selection(&mut self) {
        if let Some(key) = &self.selected_key {
            if let Some(index) = self.rows.iter().position(|row| &row.key == key) {
                self.selected = index;
                self.table_state.select(Some(index));
                return;
            }
        }
        if self.rows.is_empty() {
            self.selected = 0;
            self.table_state.select(None);
        } else {
            self.selected = self.selected.min(self.rows.len() - 1);
            self.selected_key = Some(self.rows[self.selected].key.clone());
            self.table_state.select(Some(self.selected));
        }
    }
}
