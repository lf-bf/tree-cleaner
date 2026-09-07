//! The complete list of mount points, including virtual and hidden ones.

use std::collections::HashSet;
use std::path::PathBuf;

/// Every mount point the kernel knows about.
pub fn mount_points() -> HashSet<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        macos::mount_points()
    }
    #[cfg(target_os = "linux")]
    {
        linux::mount_points()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        HashSet::new()
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::collections::HashSet;
    use std::ffi::CStr;
    use std::path::PathBuf;

    pub fn mount_points() -> HashSet<PathBuf> {
        let mut result = HashSet::new();
        let mut entries: *mut libc::statfs = std::ptr::null_mut();
        // SAFETY: `getmntinfo` allocates the array itself and returns the element count;
        // the memory belongs to libc and must not be freed by us.
        let count = unsafe { libc::getmntinfo(&mut entries, libc::MNT_NOWAIT) };
        if count <= 0 || entries.is_null() {
            return result;
        }
        for index in 0..count as usize {
            // SAFETY: `index` is below the count returned by the kernel.
            let entry = unsafe { &*entries.add(index) };
            // SAFETY: `f_mntonname` is a NUL terminated buffer filled by the kernel.
            let name = unsafe { CStr::from_ptr(entry.f_mntonname.as_ptr()) };
            result.insert(PathBuf::from(name.to_string_lossy().into_owned()));
        }
        result
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::collections::HashSet;
    use std::path::PathBuf;

    pub fn mount_points() -> HashSet<PathBuf> {
        let Ok(content) = std::fs::read_to_string("/proc/self/mounts") else {
            return HashSet::new();
        };
        content
            .lines()
            .filter_map(|line| line.split_whitespace().nth(1))
            .map(unescape_octal)
            .map(PathBuf::from)
            .collect()
    }

    /// `/proc/mounts` escapes spaces and friends as `\040`.
    fn unescape_octal(raw: &str) -> String {
        let bytes = raw.as_bytes();
        let mut result: Vec<u8> = Vec::with_capacity(bytes.len());
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] == b'\\' && index + 3 < bytes.len() {
                if let Ok(value) = u8::from_str_radix(&raw[index + 1..index + 4], 8) {
                    result.push(value);
                    index += 4;
                    continue;
                }
            }
            result.push(bytes[index]);
            index += 1;
        }
        String::from_utf8_lossy(&result).into_owned()
    }
}
