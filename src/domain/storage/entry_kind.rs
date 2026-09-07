//! Classification of filesystem entries.

/// The kind of a directory entry, as far as the scanner cares.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EntryKind {
    Directory,
    RegularFile,
    /// Symbolic links are never followed; only the link itself is measured.
    Symlink,
    /// Sockets, FIFOs, devices and anything else without meaningful disk usage.
    Other,
}

impl EntryKind {
    pub const fn is_directory(self) -> bool {
        matches!(self, Self::Directory)
    }

    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Directory => "▸",
            Self::RegularFile => "·",
            Self::Symlink => "↪",
            Self::Other => "○",
        }
    }
}
