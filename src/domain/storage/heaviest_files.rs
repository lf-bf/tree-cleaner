//! Bounded collection of the largest files seen so far.

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::path::PathBuf;

use super::byte_size::{ByteSize, MeasuredSize, SizeMode};
use super::entry_kind::EntryKind;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeavyFile {
    pub path: PathBuf,
    pub size: MeasuredSize,
    pub kind: EntryKind,
}

impl HeavyFile {
    fn ranking_size(&self, mode: SizeMode) -> ByteSize {
        self.size.select(mode)
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Ranked {
    size: ByteSize,
    sequence: u64,
    file: HeavyFile,
}

impl PartialOrd for Ranked {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Ranked {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.size.cmp(&other.size).then_with(|| other.sequence.cmp(&self.sequence))
    }
}

/// Keeps only the `limit` largest files offered to it, in O(log limit) per offer.
#[derive(Debug)]
pub struct HeaviestFiles {
    limit: usize,
    mode: SizeMode,
    heap: BinaryHeap<Reverse<Ranked>>,
    sequence: u64,
    offered: u64,
}

impl HeaviestFiles {
    pub fn new(limit: usize, mode: SizeMode) -> Self {
        Self {
            limit: limit.max(1),
            mode,
            heap: BinaryHeap::with_capacity(limit.max(1) + 1),
            sequence: 0,
            offered: 0,
        }
    }

    pub fn offer(&mut self, file: HeavyFile) {
        self.offered += 1;
        let size = file.ranking_size(self.mode);
        if self.heap.len() >= self.limit {
            let smallest_kept = self.heap.peek().map(|entry| entry.0.size).unwrap_or(ByteSize::ZERO);
            if size <= smallest_kept {
                return;
            }
            self.heap.pop();
        }
        self.sequence += 1;
        self.heap.push(Reverse(Ranked { size, sequence: self.sequence, file }));
    }

    pub fn merge(&mut self, files: impl IntoIterator<Item = HeavyFile>) {
        for file in files {
            self.offer(file);
        }
    }

    pub fn len(&self) -> usize {
        self.heap.len()
    }

    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }

    /// How many files were offered in total, including the ones that were dropped.
    pub const fn offered(&self) -> u64 {
        self.offered
    }

    pub const fn limit(&self) -> usize {
        self.limit
    }

    pub fn is_full(&self) -> bool {
        self.heap.len() >= self.limit
    }

    /// Size of the smallest file still kept. Anything smaller can be discarded upstream
    /// once the collection is full.
    pub fn smallest_kept(&self) -> Option<ByteSize> {
        self.heap.peek().map(|entry| entry.0.size)
    }

    /// Largest first.
    pub fn into_sorted_vec(self) -> Vec<HeavyFile> {
        let mut ranked: Vec<Ranked> = self.heap.into_iter().map(|entry| entry.0).collect();
        ranked.sort_by(|left, right| right.cmp(left));
        ranked.into_iter().map(|entry| entry.file).collect()
    }

    /// Largest first, without consuming the collection.
    pub fn sorted_snapshot(&self) -> Vec<HeavyFile> {
        let mut ranked: Vec<&Ranked> = self.heap.iter().map(|entry| &entry.0).collect();
        ranked.sort_by(|left, right| right.cmp(left));
        ranked.into_iter().map(|entry| entry.file.clone()).collect()
    }
}
