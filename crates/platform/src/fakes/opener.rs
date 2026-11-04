//! A recording [`Opener`] that launches nothing.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use dictation_ports::Opener;

use super::lock;

#[derive(Default)]
struct State {
    urls: Vec<String>,
    folders: Vec<PathBuf>,
    fail: bool,
}

/// Records what would have been opened. Clones share state.
#[derive(Clone, Default)]
pub struct FakeOpener(Arc<Mutex<State>>);

impl FakeOpener {
    /// An opener that has opened nothing and succeeds.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Makes every later call fail, as a missing launcher would.
    pub fn fail_from_now_on(&self) {
        lock(&self.0).fail = true;
    }

    /// The addresses opened so far, oldest first.
    #[must_use]
    pub fn urls(&self) -> Vec<String> {
        lock(&self.0).urls.clone()
    }

    /// The folders revealed so far, oldest first.
    #[must_use]
    pub fn folders(&self) -> Vec<PathBuf> {
        lock(&self.0).folders.clone()
    }
}

impl Opener for FakeOpener {
    fn open_url(&self, url: &str) -> Result<(), String> {
        let mut state = lock(&self.0);
        state.urls.push(url.to_owned());
        if state.fail {
            return Err("no launcher".into());
        }
        Ok(())
    }

    fn reveal_folder(&self, dir: &Path) -> Result<(), String> {
        let mut state = lock(&self.0);
        state.folders.push(dir.to_owned());
        if state.fail {
            return Err("no launcher".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_records_in_order_and_can_fail() {
        let opener = FakeOpener::new();
        opener.open_url("https://a").unwrap();
        opener.reveal_folder(Path::new("/x")).unwrap();
        opener.fail_from_now_on();
        assert!(opener.open_url("https://b").is_err());
        assert_eq!(opener.urls(), ["https://a", "https://b"]);
        assert_eq!(opener.folders(), [PathBuf::from("/x")]);
    }
}
