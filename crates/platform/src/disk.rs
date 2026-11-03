//! Free disk space, checked before a model download starts (L1).

use std::io;
use std::path::Path;

/// Bytes available to this user on the volume holding `path`, or, if `path`
/// doesn't exist yet, on the volume of its nearest existing parent.
///
/// On macOS this is `statvfs`'s `f_bavail`: it leaves out space APFS could
/// purge on demand, so it errs on the side of "not enough".
///
/// # Errors
///
/// If no part of `path` exists or the OS can't say.
pub fn available_space(path: &Path) -> io::Result<u64> {
    let existing = path
        .ancestors()
        .find(|dir| dir.exists())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no existing folder"))?;
    available(existing)
}

#[cfg(unix)]
fn available(path: &Path) -> io::Result<u64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let c_path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path holds a NUL byte"))?;
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: `c_path` is a valid NUL-terminated string that outlives the
    // call, and `stats` points to writable memory of the right type, which
    // statvfs fills completely when it returns 0.
    let status = unsafe { libc::statvfs(c_path.as_ptr(), stats.as_mut_ptr()) };
    if status != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: statvfs returned 0, so it initialised `stats`.
    let stats = unsafe { stats.assume_init() };
    // The field widths differ between platforms; both fit in u64.
    #[allow(
        clippy::useless_conversion,
        reason = "u32 on some targets, u64 on others"
    )]
    let (blocks, size) = (u64::from(stats.f_bavail), u64::from(stats.f_frsize));
    Ok(blocks.saturating_mul(size))
}

#[cfg(not(unix))]
fn available(_path: &Path) -> io::Result<u64> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "free space isn't known on this OS yet",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_temp_folder_has_some_space() {
        let free = available_space(&std::env::temp_dir()).unwrap();
        assert!(free > 0);
    }

    #[test]
    fn a_missing_folder_is_measured_at_its_nearest_parent() {
        let missing = std::env::temp_dir().join("l1-no-such-dir/models");
        assert!(available_space(&missing).unwrap() > 0);
    }

    #[test]
    fn a_relative_path_with_no_existing_part_is_an_error() {
        assert!(available_space(Path::new("")).is_err());
    }
}
