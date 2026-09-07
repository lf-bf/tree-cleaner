//! State of the cleaner screen.

use std::collections::HashSet;

use ratatui::widgets::TableState;

use crate::domain::cleaning::{CandidateId, CleaningCategory, CleaningReport};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CleanerRow {
    Category(CleaningCategory),
    Candidate(CandidateId),
}

#[derive(Debug, Default)]
pub struct CleanerState {
    pub rows: Vec<CleanerRow>,
    pub selected: usize,
    pub collapsed: HashSet<CleaningCategory>,
    pub table_state: TableState,
    pub last_report: Option<CleaningReport>,
    pub running: bool,
}

impl CleanerState {
    pub fn selected_row(&self) -> Option<&CleanerRow> {
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
