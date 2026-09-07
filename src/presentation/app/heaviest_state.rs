//! State of the "heaviest files below this directory" view.

use ratatui::widgets::TableState;

use crate::domain::storage::{HeavyFile, NodeId, QueryId};

#[derive(Debug, Default)]
pub struct HeaviestState {
    pub query: Option<QueryId>,
    pub origin: Option<NodeId>,
    pub rows: Vec<HeavyFile>,
    pub selected: usize,
    pub limit: usize,
    pub table_state: TableState,
}

impl HeaviestState {
    pub fn new(limit: usize) -> Self {
        Self { limit, ..Self::default() }
    }

    pub fn selected_file(&self) -> Option<&HeavyFile> {
        self.rows.get(self.selected)
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.rows.is_empty() {
            self.selected = 0;
            self.table_state.select(None);
            return;
        }
        let target = (self.selected as isize + delta).clamp(0, self.rows.len() as isize - 1);
        self.selected = target as usize;
        self.table_state.select(Some(self.selected));
    }

    pub fn clamp_selection(&mut self) {
        if self.rows.is_empty() {
            self.selected = 0;
            self.table_state.select(None);
        } else {
            self.selected = self.selected.min(self.rows.len() - 1);
            self.table_state.select(Some(self.selected));
        }
    }
}
