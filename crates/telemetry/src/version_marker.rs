//! Notices an update: the version of the last launch, kept in one file.

use std::path::Path;

use dictation_ports::Version;

/// The file's name in the data directory.
pub const FILE: &str = "last-version";

/// Records `current` as the running version and returns the version of the
/// previous launch if it was a different one (the app was updated).
/// `None` on a first launch, on the same version, or if the file didn't hold
/// a version.
#[must_use]
pub fn updated_from(dir: &Path, current: &Version) -> Option<Version> {
    let file = dir.join(FILE);
    let before = std::fs::read_to_string(&file)
        .ok()
        .and_then(|text| Version::new(text.trim()));
    if before.as_ref() != Some(current) {
        let written =
            std::fs::create_dir_all(dir).and_then(|()| std::fs::write(&file, current.as_str()));
        if let Err(error) = written {
            tracing::debug!(%error, "last version not saved");
        }
    }
    before.filter(|before| before != current)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(text: &str) -> Version {
        Version::new(text).unwrap()
    }

    #[test]
    fn an_update_is_reported_once() {
        let dir = std::env::temp_dir().join(format!("dictation-ver-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(updated_from(&dir, &version("0.1.0")), None, "first launch");
        assert_eq!(updated_from(&dir, &version("0.1.0")), None, "same version");
        assert_eq!(
            updated_from(&dir, &version("0.2.0")),
            Some(version("0.1.0"))
        );
        assert_eq!(updated_from(&dir, &version("0.2.0")), None, "already told");
        std::fs::write(dir.join(FILE), "/Users/someone/secret").unwrap();
        assert_eq!(updated_from(&dir, &version("0.3.0")), None, "not a version");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
