//! Directory reader on top of the standard library. `read_dir` gives the entry kind for
//! free (`d_type`); `DirEntry::metadata` is one `lstat` per file (`fstatat` on Linux).
//! Measured on macOS/APFS this is the fastest reader, so it is the default.

use std::ffi::OsStr;
use std::fs::DirEntry;
use std::path::Path;

use crate::domain::ports::{DirectoryEntryAccess, DirectoryReadError, DirectoryReader, Visit};
use crate::domain::storage::{ByteSize, EntryKind, FileMetadata, MeasuredSize};

#[derive(Debug, Default)]
pub struct StdDirectoryReader;

impl StdDirectoryReader {
    pub fn new() -> Self {
        Self
    }
}

struct StdEntry<'a> {
    entry: &'a DirEntry,
    name: std::ffi::OsString,
}

impl DirectoryEntryAccess for StdEntry<'_> {
    fn file_name(&self) -> &OsStr {
        &self.name
    }

    fn kind_hint(&self) -> Option<EntryKind> {
        self.entry.file_type().ok().map(|file_type| {
            if file_type.is_dir() {
                EntryKind::Directory
            } else if file_type.is_file() {
                EntryKind::RegularFile
            } else if file_type.is_symlink() {
                EntryKind::Symlink
            } else {
                EntryKind::Other
            }
        })
    }

    fn metadata(&self) -> Result<FileMetadata, DirectoryReadError> {
        let metadata = self.entry.metadata().map_err(|error| DirectoryReadError::from_io(&error))?;
        Ok(metadata_from_std(&metadata))
    }
}

impl DirectoryReader for StdDirectoryReader {
    fn read_directory(
        &self,
        path: &Path,
        visitor: &mut dyn FnMut(&dyn DirectoryEntryAccess) -> Visit,
    ) -> Result<(), DirectoryReadError> {
        let entries = std::fs::read_dir(path).map_err(|error| DirectoryReadError::from_io(&error))?;
        for entry in entries {
            let entry = entry.map_err(|error| DirectoryReadError::from_io(&error))?;
            let access = StdEntry { name: entry.file_name(), entry: &entry };
            if visitor(&access) == Visit::Stop {
                break;
            }
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        "std"
    }
}

pub fn metadata_from_std(metadata: &std::fs::Metadata) -> FileMetadata {
    let file_type = metadata.file_type();
    let kind = if file_type.is_dir() {
        EntryKind::Directory
    } else if file_type.is_file() {
        EntryKind::RegularFile
    } else if file_type.is_symlink() {
        EntryKind::Symlink
    } else {
        EntryKind::Other
    };
    #[cfg(unix)]
    let (allocated, hard_link_count, inode, device) = {
        use std::os::unix::fs::MetadataExt;
        (metadata.blocks().saturating_mul(512), metadata.nlink(), metadata.ino(), metadata.dev())
    };
    #[cfg(not(unix))]
    let (allocated, hard_link_count, inode, device) = (metadata.len(), 1, 0, 0);
    FileMetadata {
        kind,
        size: MeasuredSize::new(ByteSize::new(allocated), ByteSize::new(metadata.len())),
        hard_link_count,
        inode,
        device,
    }
}
