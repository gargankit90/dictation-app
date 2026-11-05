//! Downloading and keeping the on-device models (L1).
//!
//! [`ModelStore`] owns one folder (the app's `models/` folder in Application
//! Support). A model is stored as its catalogue `file_name`; while it
//! downloads it is `<file_name>.part`. A download:
//!
//! 1. checks the free space first (what is left to fetch, plus
//!    [`SPACE_HEADROOM`]), so a full disk is refused before a byte is sent;
//! 2. resumes a `.part` left by an interrupted or cancelled run with an HTTP
//!    `Range` request (a server that ignores it sends the whole file, which
//!    starts the part again);
//! 3. reports [`Progress`] as bytes arrive;
//! 4. checks the size and the SHA-256 of the whole part against the
//!    catalogue, and only then renames it to its final name. A mismatch
//!    deletes the part.
//!
//! A file under the final name can still change afterwards (disk damage, a
//! restored backup), so it is hashed again before its first use in each run
//! of the app: [`ModelStore::verified_path`] is what the engine loads. A
//! successful check is remembered for that file while its size and
//! modification time stay the same; a failed one deletes the file, so
//! Settings offers Download again. A download of a model whose final file
//! fails the check fetches it afresh.
//!
//! Errors are typed ([`DownloadError`]); none holds a URL query or a body.
//! Redirects are followed only to `https` URLs (Hugging Face sends the file
//! from its CDN), at most [`MAX_REDIRECTS`] times.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use std::time::SystemTime;

use dictation_core::local_models::LocalModel;
use dictation_core::redact;
use reqwest::{Client, StatusCode, header};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::io::AsyncWriteExt;
use tokio::sync::Notify;

/// Space kept free on top of the download itself, so finishing a model
/// never leaves the disk full.
pub const SPACE_HEADROOM: u64 = 200 * 1024 * 1024;

/// The most redirects one download follows.
pub const MAX_REDIRECTS: usize = 5;

/// How long connecting, or waiting for the next bytes, may take before the
/// download counts as failed (and can be resumed).
pub const STALL_TIMEOUT: Duration = Duration::from_secs(30);

/// Reports at most this often (in bytes), plus the last one.
const PROGRESS_STEP: u64 = 1024 * 1024;

/// Where a model is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelStatus {
    /// Nothing on disk.
    Missing,
    /// A download stopped part way; it resumes from `downloaded` bytes.
    Partial {
        /// Bytes already on disk.
        downloaded: u64,
    },
    /// Downloaded and verified.
    Ready,
}

/// What became of a part left by an earlier download, from the server's
/// answer, not from the bytes on disk.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Resume {
    /// There was no part, or the server hasn't answered yet.
    #[default]
    None,
    /// The server accepted the range (HTTP 206): the part was kept.
    Continued {
        /// The bytes that were kept.
        from: u64,
    },
    /// The server sent the whole file (HTTP 200): the part was thrown away.
    Restarted,
}

/// How far a download is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    /// Bytes on disk so far, including a resumed part.
    pub downloaded: u64,
    /// The model's full size.
    pub total: u64,
    /// What became of an earlier part.
    pub resume: Resume,
}

/// Why a download failed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DownloadError {
    /// Not enough free space for the rest of the download plus headroom.
    #[error("not enough disk space: {needed} bytes needed, {available} available")]
    NoSpace {
        /// Bytes the download still needs, with the headroom.
        needed: u64,
        /// Bytes free.
        available: u64,
    },
    /// The server couldn't be reached, answered with an error, or the
    /// connection broke. What was fetched is kept, so trying again resumes.
    #[error("network error: {}", redact::summary(.0))]
    Network(String),
    /// The file's size or SHA-256 isn't the catalogue's; it was deleted.
    #[error("the downloaded file doesn't match its checksum")]
    Checksum,
    /// [`DownloadCancel::cancel`] stopped it. What was fetched is kept.
    #[error("the download was cancelled")]
    Cancelled,
    /// The file being downloaded, or its folder, was removed meanwhile.
    /// Trying again starts over.
    #[error("the file being downloaded was removed")]
    Removed,
    /// The models folder couldn't be read or written.
    #[error("couldn't write the model to disk: {}", redact::summary(.0))]
    Disk(String),
}

impl DownloadError {
    /// What Settings shows: plain words and the next step.
    #[must_use]
    pub fn user_message(&self) -> String {
        match self {
            Self::NoSpace { needed, .. } => format!(
                "Not enough disk space. Free up about {} and try again.",
                megabytes(*needed)
            ),
            Self::Network(_) => "The download failed. Check your internet connection and try \
                 again; it continues where it stopped."
                .to_owned(),
            Self::Checksum => {
                "The downloaded file was damaged and has been removed. Try again.".to_owned()
            }
            Self::Cancelled => "Download cancelled.".to_owned(),
            Self::Removed => {
                "The download was removed from this Mac before it finished. Try again.".to_owned()
            }
            Self::Disk(_) => {
                "Couldn't save the model to disk. Check the free space and try again.".to_owned()
            }
        }
    }
}

/// `bytes` as whole megabytes ("477 MB"), rounded up.
#[must_use]
pub fn megabytes(bytes: u64) -> String {
    format!("{} MB", bytes.div_ceil(1_000_000))
}

/// Stops a running download. Clones share the flag.
#[derive(Debug, Clone, Default)]
pub struct DownloadCancel {
    inner: Arc<CancelInner>,
}

#[derive(Debug, Default)]
struct CancelInner {
    flag: AtomicBool,
    notify: Notify,
}

impl DownloadCancel {
    /// A token that isn't cancelled.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Cancels the download (now, or as soon as it starts).
    pub fn cancel(&self) {
        self.inner.flag.store(true, Ordering::SeqCst);
        self.inner.notify.notify_waiters();
    }

    /// Whether [`cancel`](Self::cancel) was called.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.inner.flag.load(Ordering::SeqCst)
    }

    async fn cancelled(&self) {
        loop {
            let notified = self.inner.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.is_cancelled() {
                return;
            }
            notified.await;
        }
    }
}

/// Free bytes on the volume holding a path.
pub type FreeSpace = Arc<dyn Fn(&Path) -> io::Result<u64> + Send + Sync>;

/// The models folder.
#[derive(Clone)]
pub struct ModelStore {
    dir: PathBuf,
    client: Client,
    free_space: FreeSpace,
    /// Final files whose SHA-256 was checked in this run, with what they
    /// looked like then. Shared by clones.
    verified: Arc<Mutex<HashMap<PathBuf, Stamp>>>,
}

/// What a file looked like when it was hashed: if either changes, it is
/// hashed again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stamp {
    len: u64,
    modified: Option<SystemTime>,
}

impl Stamp {
    fn of(path: &Path) -> Option<Self> {
        let meta = std::fs::metadata(path).ok()?;
        Some(Self {
            len: meta.len(),
            modified: meta.modified().ok(),
        })
    }
}

impl std::fmt::Debug for ModelStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModelStore")
            .field("dir", &self.dir)
            .finish_non_exhaustive()
    }
}

impl ModelStore {
    /// The store in `dir` (created when a download starts), asking
    /// `free_space` how much room the volume has.
    ///
    /// # Errors
    ///
    /// [`DownloadError::Network`] if the HTTP client can't be built.
    pub fn new(dir: PathBuf, free_space: FreeSpace) -> Result<Self, DownloadError> {
        let client = crate::http::download_client(STALL_TIMEOUT, MAX_REDIRECTS)
            .map_err(|error| DownloadError::Network(error.to_string()))?;
        Ok(Self {
            dir,
            client,
            free_space,
            verified: Arc::default(),
        })
    }

    /// The folder.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn final_path(&self, model: &LocalModel) -> PathBuf {
        self.dir.join(model.file_name)
    }

    fn part_path(&self, model: &LocalModel) -> PathBuf {
        self.dir.join(format!("{}.part", model.file_name))
    }

    /// Where `model` is now. A final file of the wrong size (changed on
    /// disk since it was verified) counts as missing.
    #[must_use]
    pub fn status(&self, model: &LocalModel) -> ModelStatus {
        if self.ready_path(model).is_some() {
            return ModelStatus::Ready;
        }
        match std::fs::metadata(self.part_path(model)) {
            Ok(meta) if meta.is_file() => ModelStatus::Partial {
                downloaded: meta.len(),
            },
            _ => ModelStatus::Missing,
        }
    }

    /// The final file, if one of the catalogue size is there. Cheap (no
    /// hashing), for showing the status; load only
    /// [`verified_path`](Self::verified_path).
    #[must_use]
    pub fn ready_path(&self, model: &LocalModel) -> Option<PathBuf> {
        let path = self.final_path(model);
        let meta = std::fs::metadata(&path).ok()?;
        (meta.is_file() && meta.len() == model.size_bytes).then_some(path)
    }

    /// The final file to load, once its SHA-256 matches the catalogue: hashed
    /// on the first call in this run (seconds for a model; call it from a
    /// blocking thread), then remembered while the file is unchanged.
    /// `Ok(None)` if it isn't downloaded.
    ///
    /// # Errors
    ///
    /// [`DownloadError::Checksum`] if the bytes changed since the download;
    /// the file has been deleted then. [`DownloadError::Disk`] if it can't be
    /// read or deleted.
    pub fn verified_path(&self, model: &LocalModel) -> Result<Option<PathBuf>, DownloadError> {
        let Some(path) = self.ready_path(model) else {
            return Ok(None);
        };
        let before = Stamp::of(&path);
        if before.is_some() && self.lock_verified().get(&path) == before.as_ref() {
            return Ok(Some(path));
        }
        let hashed = sha256_file(&path).map_err(|e| disk(&e))?;
        if hashed != model.sha256 {
            self.lock_verified().remove(&path);
            tracing::warn!(
                model = model.id,
                "an on-device model failed its checksum; deleted"
            );
            std::fs::remove_file(&path).map_err(|e| disk(&e))?;
            return Err(DownloadError::Checksum);
        }
        if let Some(stamp) = before {
            self.lock_verified().insert(path.clone(), stamp);
        }
        Ok(Some(path))
    }

    fn lock_verified(&self) -> std::sync::MutexGuard<'_, HashMap<PathBuf, Stamp>> {
        self.verified.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Deletes `model`, downloaded or partial. Deleting what isn't there is
    /// fine.
    ///
    /// # Errors
    ///
    /// [`DownloadError::Disk`] if a file can't be removed.
    pub fn delete(&self, model: &LocalModel) -> Result<(), DownloadError> {
        self.lock_verified().remove(&self.final_path(model));
        for path in [self.final_path(model), self.part_path(model)] {
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(disk(&error)),
            }
        }
        Ok(())
    }

    /// Downloads `model` from its catalogue URL; see the module docs.
    /// Returns the verified file. A model already downloaded is checked
    /// ([`verified_path`](Self::verified_path)) and returned; if it fails the
    /// check it is downloaded again.
    ///
    /// # Errors
    ///
    /// Every [`DownloadError`], as documented on each variant.
    pub async fn download(
        &self,
        model: &LocalModel,
        cancel: &DownloadCancel,
        on_progress: impl FnMut(Progress) + Send,
    ) -> Result<PathBuf, DownloadError> {
        self.download_from(model, model.url, cancel, on_progress)
            .await
    }

    /// [`download`](Self::download) from `url` instead of the catalogue's
    /// (tests point it at a local server).
    ///
    /// # Errors
    ///
    /// As [`download`](Self::download).
    pub async fn download_from(
        &self,
        model: &LocalModel,
        url: &str,
        cancel: &DownloadCancel,
        mut on_progress: impl FnMut(Progress) + Send,
    ) -> Result<PathBuf, DownloadError> {
        if self.ready_path(model).is_some() {
            let store = self.clone();
            let model_copy = *model;
            let checked = tokio::task::spawn_blocking(move || store.verified_path(&model_copy))
                .await
                .map_err(|_| DownloadError::Disk("the checksum task crashed".to_owned()))?;
            match checked {
                Ok(Some(path)) => return Ok(path),
                // Damaged and deleted, or gone meanwhile: download it again.
                Ok(None) | Err(DownloadError::Checksum) => {}
                Err(error) => return Err(error),
            }
        }
        if cancel.is_cancelled() {
            return Err(DownloadError::Cancelled);
        }
        tokio::fs::create_dir_all(&self.dir)
            .await
            .map_err(|e| disk(&e))?;
        let part = self.part_path(model);
        let total = model.size_bytes;
        let mut have = match tokio::fs::metadata(&part).await {
            Ok(meta) => meta.len(),
            Err(_) => 0,
        };
        if have > total {
            tokio::fs::remove_file(&part).await.map_err(|e| disk(&e))?;
            have = 0;
        }
        self.check_space(total - have)?;

        let mut resume = Resume::None;
        if have < total {
            (have, resume) = tokio::select! {
                () = cancel.cancelled() => return Err(DownloadError::Cancelled),
                fetched = self.fetch(url, &part, have, total, &mut on_progress) => fetched?,
            };
        }
        if have != total {
            return Err(DownloadError::Network(
                "the download ended early".to_owned(),
            ));
        }
        on_progress(Progress {
            downloaded: total,
            total,
            resume,
        });
        verify(&part, model.sha256).await?;
        let path = self.final_path(model);
        tokio::fs::rename(&part, &path)
            .await
            .map_err(|e| gone_or_disk(&e))?;
        if let Some(stamp) = Stamp::of(&path) {
            self.lock_verified().insert(path.clone(), stamp);
        }
        tracing::info!(model = model.id, "on-device model downloaded and verified");
        Ok(path)
    }

    fn check_space(&self, remaining: u64) -> Result<(), DownloadError> {
        let needed = remaining.saturating_add(SPACE_HEADROOM);
        let available = (self.free_space)(&self.dir).map_err(|e| disk(&e))?;
        if available < needed {
            return Err(DownloadError::NoSpace { needed, available });
        }
        Ok(())
    }

    /// Fetches from byte `have` into `part`; returns the bytes on disk and
    /// what became of the part.
    async fn fetch(
        &self,
        url: &str,
        part: &Path,
        mut have: u64,
        total: u64,
        on_progress: &mut (impl FnMut(Progress) + Send),
    ) -> Result<(u64, Resume), DownloadError> {
        let mut request = self.client.get(url);
        if have > 0 {
            request = request.header(header::RANGE, format!("bytes={have}-"));
        }
        let mut response = request.send().await.map_err(|e| network(&e))?;
        let status = response.status();
        let append = match status {
            StatusCode::PARTIAL_CONTENT if have > 0 => true,
            // The whole file: the server ignored the range, or there was none.
            StatusCode::OK => false,
            StatusCode::RANGE_NOT_SATISFIABLE => {
                // The part is longer than the server's file thinks; start over.
                tokio::fs::remove_file(part).await.map_err(|e| disk(&e))?;
                return Err(DownloadError::Network(
                    "the server refused to resume (HTTP 416)".to_owned(),
                ));
            }
            other => {
                return Err(DownloadError::Network(format!(
                    "the server answered HTTP {}",
                    other.as_u16()
                )));
            }
        };
        let resume = if append {
            Resume::Continued { from: have }
        } else if have > 0 {
            Resume::Restarted
        } else {
            Resume::None
        };
        if !append {
            have = 0;
        }
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(append)
            .truncate(!append)
            .open(part)
            .await
            .map_err(|e| disk(&e))?;
        // This line's text is what the e2e scenario waits for.
        tracing::info!(?resume, "model download answered");
        let mut reported = have;
        on_progress(Progress {
            downloaded: have,
            total,
            resume,
        });
        while let Some(chunk) = response.chunk().await.map_err(|e| network(&e))? {
            have += chunk.len() as u64;
            if have > total {
                drop(file);
                tokio::fs::remove_file(part).await.map_err(|e| disk(&e))?;
                return Err(DownloadError::Checksum);
            }
            file.write_all(&chunk).await.map_err(|e| disk(&e))?;
            if have - reported >= PROGRESS_STEP {
                // Writing to a removed file succeeds: without this the rest
                // of the model is fetched for nothing.
                if let Err(error) = tokio::fs::metadata(part).await {
                    return Err(gone_or_disk(&error));
                }
                reported = have;
                on_progress(Progress {
                    downloaded: have,
                    total,
                    resume,
                });
            }
        }
        file.flush().await.map_err(|e| disk(&e))?;
        file.sync_all().await.map_err(|e| disk(&e))?;
        Ok((have, resume))
    }
}

/// Hashes `part` on the blocking pool and deletes it if it isn't `expected`.
async fn verify(part: &Path, expected: &str) -> Result<(), DownloadError> {
    let path = part.to_owned();
    let hashed = tokio::task::spawn_blocking(move || sha256_file(&path))
        .await
        .map_err(|_| DownloadError::Disk("the checksum task crashed".to_owned()))?
        .map_err(|e| gone_or_disk(&e))?;
    if hashed != expected {
        tracing::warn!("a downloaded model failed its checksum; deleted");
        tokio::fs::remove_file(part).await.map_err(|e| disk(&e))?;
        return Err(DownloadError::Checksum);
    }
    Ok(())
}

/// The SHA-256 of the file at `path`, lowercase hex.
///
/// # Errors
///
/// If the file can't be read.
pub fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    io::copy(&mut file, &mut hasher)?;
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn disk(error: &io::Error) -> DownloadError {
    DownloadError::Disk(error.to_string())
}

/// For a file this download made: not there any more means someone removed it,
/// which free space has nothing to do with.
fn gone_or_disk(error: &io::Error) -> DownloadError {
    if error.kind() == io::ErrorKind::NotFound {
        DownloadError::Removed
    } else {
        disk(error)
    }
}

/// The error without its URL: a signed CDN URL carries tokens.
fn network(error: &reqwest::Error) -> DownloadError {
    let kind = if error.is_timeout() {
        "the connection timed out"
    } else if error.is_connect() {
        "couldn't connect"
    } else if error.is_redirect() {
        "too many or unsafe redirects"
    } else if error.is_body() || error.is_decode() {
        "the connection broke during the download"
    } else {
        "the request failed"
    };
    DownloadError::Network(kind.to_owned())
}

#[cfg(test)]
mod tests;
