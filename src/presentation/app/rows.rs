//! Builds the rows each screen shows from the current state of the tree.

use std::path::PathBuf;

use super::App;
use super::cleaner_state::CleanerRow;
use super::explorer_state::{ExplorerRow, RowBadge, RowKey};
use crate::domain::cleaning::CleaningCategory;
use crate::domain::storage::{ByteSize, EntryKind};

impl App {
    pub(super) fn rebuild_explorer_rows(&mut self) {
        let Some(current) = self.explorer.current else {
            self.explorer.rows.clear();
            return;
        };
        let coordinator = &self.services.coordinator;
        let tree = coordinator.tree();
        let mode = coordinator.size_mode();
        let Some(current_node) = tree.node(current) else {
            self.explorer.rows.clear();
            self.explorer.current = None;
            return;
        };
        let current_path = tree.path_of(current);
        let total = current_node.total_size().select(mode);
        let filter = self.explorer.filter.as_ref().map(|filter| filter.to_lowercase());

        let mut rows: Vec<ExplorerRow> = Vec::with_capacity(current_node.children().len() + 32);
        for child_id in current_node.children() {
            let Some(child) = tree.node(*child_id) else {
                continue;
            };
            let name = child.display_name();
            if let Some(filter) = &filter {
                if !name.to_lowercase().contains(filter) {
                    continue;
                }
            }
            let size = child.total_size().select(mode);
            let path = current_path.join(child.name());
            rows.push(ExplorerRow {
                key: RowKey::Directory(*child_id),
                marked: self.marks.contains(&path),
                path,
                name,
                size,
                share: size.ratio_of(total),
                items: Some(child.total_item_count()),
                badge: RowBadge::from_flags(child.flags(), child.is_complete(), !child.children().is_empty()),
                is_directory: true,
                kind: EntryKind::Directory,
            });
        }
        if self.explorer.show_files {
            if let Some(listing) = coordinator.file_listing(current).and_then(|state| state.listing.as_ref())
            {
                for file in &listing.files {
                    let name = file.name.to_string_lossy().into_owned();
                    if let Some(filter) = &filter {
                        if !name.to_lowercase().contains(filter) {
                            continue;
                        }
                    }
                    let size = file.size.select(mode);
                    let path = current_path.join(&file.name);
                    rows.push(ExplorerRow {
                        key: RowKey::File(file.name.clone()),
                        marked: self.marks.contains(&path),
                        path,
                        name,
                        size,
                        share: size.ratio_of(total),
                        items: None,
                        badge: RowBadge::File,
                        is_directory: false,
                        kind: file.kind,
                    });
                }
            }
        }
        rows.sort_by(|left, right| {
            right
                .size
                .cmp(&left.size)
                .then_with(|| right.is_directory.cmp(&left.is_directory))
                .then_with(|| left.name.cmp(&right.name))
        });
        self.explorer.unlimited_row_count = rows.len();
        rows.truncate(self.explorer.row_limit);
        self.explorer.rows = rows;
        self.explorer.resync_selection();
    }

    pub(super) fn rebuild_dashboard_rows(&mut self) {
        let Some(root) = self.dashboard.root_node else {
            self.dashboard.root_rows.clear();
            return;
        };
        let coordinator = &self.services.coordinator;
        let mode = coordinator.size_mode();
        self.dashboard.root_rows = coordinator.tree().children_sorted_by_size(root, mode);
        if self.dashboard.root_rows.is_empty() {
            self.dashboard.root_table_state.select(None);
        } else {
            self.dashboard.selected_root_row =
                self.dashboard.selected_root_row.min(self.dashboard.root_rows.len() - 1);
            self.dashboard.root_table_state.select(Some(self.dashboard.selected_root_row));
        }
    }

    pub(super) fn rebuild_heaviest_rows(&mut self) {
        let Some(query) = self.heaviest.query else {
            return;
        };
        if let Some(state) = self.services.coordinator.heaviest_query(query) {
            self.heaviest.rows = state.snapshot();
        }
        self.heaviest.clamp_selection();
    }

    pub(super) fn rebuild_cleaner_rows(&mut self) {
        let candidates = self.services.cleaning.candidates();
        let mut rows: Vec<CleanerRow> = Vec::with_capacity(candidates.len() + 16);
        for category in CleaningCategory::ALL {
            let mut members: Vec<_> =
                candidates.iter().filter(|candidate| candidate.category == category).collect();
            if members.is_empty() {
                continue;
            }
            members.sort_by_key(|candidate| std::cmp::Reverse(candidate.size));
            rows.push(CleanerRow::Category(category));
            if !self.cleaner.collapsed.contains(&category) {
                rows.extend(members.into_iter().map(|candidate| CleanerRow::Candidate(candidate.id)));
            }
        }
        self.cleaner.rows = rows;
        self.cleaner.clamp_selection();
    }

    /// Size of everything selected in the cleaner, for the header line.
    pub fn cleaner_selected_total(&self) -> (usize, ByteSize) {
        let mut count = 0usize;
        let mut total = ByteSize::ZERO;
        for candidate in self.services.cleaning.candidates() {
            if candidate.is_actionable() {
                count += 1;
                total += candidate.size.unwrap_or(ByteSize::ZERO);
            }
        }
        (count, total)
    }

    /// Path of the row under the cursor in the explorer, if any.
    pub fn selected_explorer_path(&self) -> Option<PathBuf> {
        self.explorer.selected_row().map(|row| row.path.clone())
    }
}
