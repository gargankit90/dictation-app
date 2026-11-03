//! The random install ID: the PostHog `distinct_id`.
//!
//! 16 random bytes from the OS, written as a version-4 UUID. Not derived
//! from the hardware, the user or the machine. Stored in one file; deleted
//! when sharing is turned off, so turning it on again starts a new ID.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// The file's name in the data directory.
pub const FILE: &str = "install-id";

/// A new random ID.
///
/// # Errors
///
/// The OS random source failing.
pub fn generate() -> Result<String, getrandom::Error> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes)?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    ))
}

/// Whether `text` is shaped like an ID this module writes.
fn is_valid(text: &str) -> bool {
    text.len() == 36
        && text.chars().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit() && !c.is_ascii_uppercase(),
        })
}

/// Where the ID lives in `dir`.
fn path(dir: &Path) -> PathBuf {
    dir.join(FILE)
}

/// The stored ID, or a new one stored now. A file that isn't an ID
/// (edited, cut short) is replaced. `None` if there is no random source or
/// the file can't be written: no ID means nothing is sent.
#[must_use]
pub fn load_or_create(dir: &Path) -> Option<String> {
    let file = path(dir);
    match std::fs::read_to_string(&file) {
        Ok(text) if is_valid(text.trim()) => return Some(text.trim().to_owned()),
        Ok(_) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => {
            tracing::debug!(%error, "install ID unreadable");
        }
    }
    let id = generate().ok()?;
    std::fs::create_dir_all(dir).ok()?;
    std::fs::write(&file, &id).ok()?;
    Some(id)
}

/// Deletes the stored ID; a missing file is fine.
pub fn forget(dir: &Path) {
    if let Err(error) = std::fs::remove_file(path(dir)) {
        if error.kind() != ErrorKind::NotFound {
            tracing::debug!(%error, "install ID not deleted");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dictation-id-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn ids_are_random_version_4_uuids() {
        let (a, b) = (generate().unwrap(), generate().unwrap());
        assert_ne!(a, b);
        assert!(is_valid(&a), "{a}");
        assert_eq!(&a[14..15], "4");
        assert!(matches!(&a[19..20], "8" | "9" | "a" | "b"), "{a}");
    }

    #[test]
    fn the_id_is_stored_and_reused() {
        let dir = temp("reuse");
        let first = load_or_create(&dir).unwrap();
        assert_eq!(load_or_create(&dir).unwrap(), first);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_forgotten_id_is_replaced_by_a_new_one() {
        let dir = temp("forget");
        let first = load_or_create(&dir).unwrap();
        forget(&dir);
        forget(&dir);
        assert!(!path(&dir).exists());
        assert_ne!(load_or_create(&dir).unwrap(), first);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_damaged_file_is_replaced() {
        let dir = temp("damaged");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(path(&dir), "somebody@example.com").unwrap();
        let id = load_or_create(&dir).unwrap();
        assert!(is_valid(&id));
        assert_eq!(std::fs::read_to_string(path(&dir)).unwrap(), id);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
