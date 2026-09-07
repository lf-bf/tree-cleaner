//! State of the dashboard: volumes and the breakdown of the system root.

use std::time::Instant;

use ratatui::widgets::TableState;

use crate::domain::storage::{NodeId, Volume};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DashboardPanel {
    Volumes,
    RootBreakdown,
}

#[derive(Debug)]
pub struct DashboardState {
    pub volumes: Vec<Volume>,
    pub selected_volume: usize,
    pub selected_root_row: usize,
    pub focus: DashboardPanel,
    pub root_node: Option<NodeId>,
    pub root_rows: Vec<NodeId>,
    pub root_table_state: TableState,
    pub refreshed_at: Instant,
}

impl DashboardState {
    pub fn new(volumes: Vec<Volume>) -> Self {
        Self {
            volumes,
            selected_volume: 0,
            selected_root_row: 0,
            focus: DashboardPanel::RootBreakdown,
            root_node: None,
            root_rows: Vec::new(),
            root_table_state: TableState::default(),
            refreshed_at: Instant::now(),
        }
    }

    pub fn selected_volume(&self) -> Option<&Volume> {
        self.volumes.get(self.selected_volume)
    }

    pub fn selected_root_child(&self) -> Option<NodeId> {
        self.root_rows.get(self.selected_root_row).copied()
    }

    pub fn move_selection(&mut self, delta: isize) {
        match self.focus {
            DashboardPanel::Volumes => {
                if !self.volumes.is_empty() {
                    let target =
                        (self.selected_volume as isize + delta).clamp(0, self.volumes.len() as isize - 1);
                    self.selected_volume = target as usize;
                }
            }
            DashboardPanel::RootBreakdown => {
                if !self.root_rows.is_empty() {
                    let target =
                        (self.selected_root_row as isize + delta).clamp(0, self.root_rows.len() as isize - 1);
                    self.selected_root_row = target as usize;
                    self.root_table_state.select(Some(self.selected_root_row));
                }
            }
        }
    }

    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            DashboardPanel::Volumes => DashboardPanel::RootBreakdown,
            DashboardPanel::RootBreakdown => DashboardPanel::Volumes,
        };
    }
}
