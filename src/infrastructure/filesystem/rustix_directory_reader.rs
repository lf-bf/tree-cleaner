//! Directory reader built on raw system calls.
//!
//! Every entry is stat'ed relative to the open directory descriptor (`fstatat`), so the
//! kernel never re-resolves the full path and no `PathBuf` is allocated per file. In
//! theory the cheapest approach; in practice, measured on macOS/APFS, `lstat` on the full
//! path (the `std` reader) finished a warm 238k-file scan about 1.5x faster with less
//! system time. It is kept as an alternative (`--reader rustix`) for comparison and for
//! Linux, where the standard library already uses `fstatat` internally.

use std::ffi::OsStr;
use std::os::fd::{AsFd, BorrowedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use rustix::fs::{AtFlags, Dir, DirEntry, FileType, Mode, OFlags, RawMode, openat, statat};
use rustix::io::Errno;

use crate::domain::ports::{DirectoryEntryAccess, DirectoryReadError, DirectoryReader, Visit};
use crate::domain::storage::{ByteSize, EntryKind, FileMetadata, MeasuredSize};

#[derive(Debug, Default)]
pub struct RustixDirectoryReader;

impl RustixDirectoryReader {
    pub fn new() -> Self {
        Self
    }
}

struct RustixEntry<'a> {
    directory: BorrowedFd<'a>,
    entry: &'a DirEntry,
}

impl DirectoryEntryAccess for RustixEntry<'_> {
    fn file_name(&self) -> &OsStr {
        OsStr::from_bytes(self.entry.file_name().to_bytes())
    }

    fn kind_hint(&self) -> Option<EntryKind> {
        match self.entry.file_type() {
            FileType::Unknown => None,
            other => Some(kind_from_file_type(other)),
        }
    }

    fn metadata(&self) -> Result<FileMetadata, DirectoryReadError> {
        let stat = statat(self.directory, self.entry.file_name(), AtFlags::SYMLINK_NOFOLLOW)
            .map_err(errno_to_error)?;
        Ok(metadata_from_stat(&stat))
    }
}

impl DirectoryReader for RustixDirectoryReader {
    fn read_directory(
        &self,
        path: &Path,
        visitor: &mut dyn FnMut(&dyn DirectoryEntryAccess) -> Visit,
    ) -> Result<(), DirectoryReadError> {
        let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC | OFlags::NOFOLLOW;
        let directory = openat(rustix::fs::CWD, path, flags, Mode::empty()).map_err(errno_to_error)?;
        let stream = Dir::read_from(&directory).map_err(errno_to_error)?;
        for entry in stream {
            let entry = entry.map_err(errno_to_error)?;
            let name = entry.file_name().to_bytes();
            if name == b"." || name == b".." {
                continue;
            }
            let access = RustixEntry { directory: directory.as_fd(), entry: &entry };
            if visitor(&access) == Visit::Stop {
                break;
            }
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        "rustix"
    }
}

fn kind_from_file_type(file_type: FileType) -> EntryKind {
    match file_type {
        FileType::Directory => EntryKind::Directory,
        FileType::RegularFile => EntryKind::RegularFile,
        FileType::Symlink => EntryKind::Symlink,
        _ => EntryKind::Other,
    }
}

#[allow(clippy::unnecessary_cast)]
fn metadata_from_stat(stat: &rustix::fs::Stat) -> FileMetadata {
    let kind = kind_from_file_type(FileType::from_raw_mode(stat.st_mode as RawMode));
    let blocks = u64::try_from(stat.st_blocks as i64).unwrap_or(0);
    let apparent = u64::try_from(stat.st_size as i64).unwrap_or(0);
    FileMetadata {
        kind,
        size: MeasuredSize::new(ByteSize::new(blocks.saturating_mul(512)), ByteSize::new(apparent)),
        hard_link_count: stat.st_nlink as u64,
        inode: stat.st_ino as u64,
        device: stat.st_dev as u64,
    }
}

fn errno_to_error(errno: Errno) -> DirectoryReadError {
    match errno {
        Errno::ACCESS | Errno::PERM => DirectoryReadError::PermissionDenied,
        Errno::NOENT => DirectoryReadError::NotFound,
        Errno::NOTDIR => DirectoryReadError::NotADirectory,
        other => DirectoryReadError::Other(other.to_string()),
    }
}
