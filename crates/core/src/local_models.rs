//! The on-device models the app offers (L1, Q4): data, not code paths, so
//! the owner's evaluations can change the list or the default by editing
//! these tables.
//!
//! Every entry is a single GGUF file with a fixed size and SHA-256; the
//! model manager refuses a file whose size or hash differs. There are two
//! roles:
//!
//! - **Speech** ([`CATALOGUE`]), run by transcribe.cpp. OrukQwen 1.7B is the
//!   only one (Q4, owner decision 2026-11-20, after PR #331's screen).
//! - **Cleanup** ([`CLEANUP_CATALOGUE`]), a small chat model run by
//!   llama.cpp's `llama-server` helper that turns the raw transcript into
//!   send-ready text (Gemma 4 E2B, PR #326's winner).

/// What a model does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    /// Speech to text (transcribe.cpp).
    Speech,
    /// Raw transcript to cleaned text (llama.cpp).
    Cleanup,
}

/// One model the user can download.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalModel {
    /// Stable id (speech models are saved in the settings as
    /// `local_model`); never reuse one for different weights.
    pub id: &'static str,
    /// What it does.
    pub role: Role,
    /// What Settings shows.
    pub name: &'static str,
    /// The languages it handles, as ISO 639 codes.
    pub languages: &'static [&'static str],
    /// The languages in words ("English only").
    pub languages_label: &'static str,
    /// The download's exact size.
    pub size_bytes: u64,
    /// Where it is downloaded from: a Hugging Face `resolve` URL.
    pub url: &'static str,
    /// The file's SHA-256, lowercase hex.
    pub sha256: &'static str,
    /// The name it is stored under in the models folder.
    pub file_name: &'static str,
    /// The weights' licence, for the notice.
    pub licence: &'static str,
}

/// OrukQwen ASR 1.7B, `Q4_K_M`: Qwen3-ASR 1.7B fine-tuned for English by
/// oruk (Apache-2.0, <https://huggingface.co/oruk/orukqwen-asr-1.7b>).
///
/// oruk publishes only a Transformers checkpoint. This GGUF is the exact
/// file PR #331 evaluated: `eval/asr-screen/prepare_oruk.py` on the
/// checkpoint at revision `a526248c`, then transcribe.cpp `db678a03`'s
/// `convert-qwen3_asr.py` and `transcribe-quantize --quant Q4_K_M`. It was
/// reproduced byte for byte on 2026-11-20 (same SHA-256) and is hosted by
/// the owner, with oruk's licence and a notice. The URL is pinned to a
/// commit once the upload lands.
pub const ORUKQWEN_1_7B: LocalModel = LocalModel {
    id: "orukqwen-asr-1.7b-q4_k_m",
    role: Role::Speech,
    name: "OrukQwen 1.7B",
    languages: &["en"],
    languages_label: "English only",
    size_bytes: 1_319_830_656,
    url: "https://huggingface.co/vocetlabs/orukqwen-asr-1.7b-gguf/resolve/main/orukqwen-asr-1.7b-Q4_K_M.gguf",
    sha256: "057471662f060800a6374708ad679d4abe51775e52ca591a88ea4ade78b273bb",
    file_name: "orukqwen-asr-1.7b-Q4_K_M.gguf",
    licence: "Apache-2.0 (oruk; based on Qwen3-ASR by Qwen)",
};

/// Gemma 4 E2B instruct, `Q4_K_M` (Apache-2.0, Google), as quantised by
/// Unsloth: the exact file PR #326 evaluated (its `meta.json` records this
/// size and SHA-256), pinned to the repository's commit of 2026-07-17.
/// Size and SHA-256 match Hugging Face's `x-linked-size` and
/// `x-linked-etag` for that commit (checked 2026-11-20).
pub const GEMMA_4_E2B: LocalModel = LocalModel {
    id: "gemma-4-e2b-it-q4_k_m",
    role: Role::Cleanup,
    name: "Gemma 4 E2B",
    languages: &["en"],
    languages_label: "English",
    size_bytes: 3_106_738_272,
    url: "https://huggingface.co/unsloth/gemma-4-E2B-it-GGUF/resolve/0314792d7f1f7e229411f620751375812bb9faf2/gemma-4-E2B-it-Q4_K_M.gguf",
    sha256: "740185b21d22ceb83a11c3aa62ad5842ef32c70f6096d756bbee85a1e4ec34b8",
    file_name: "gemma-4-E2B-it-Q4_K_M.gguf",
    licence: "Apache-2.0 (Google)",
};

/// Every speech model offered, in the order Settings shows them. The first
/// is the default.
pub const CATALOGUE: &[LocalModel] = &[ORUKQWEN_1_7B];

/// Every cleanup model offered. The first is the default.
pub const CLEANUP_CATALOGUE: &[LocalModel] = &[GEMMA_4_E2B];

/// The speech model a new user gets: the first entry.
pub const DEFAULT_MODEL: &str = CATALOGUE[0].id;

/// The cleanup model used with on-device speech.
pub const DEFAULT_CLEANUP_MODEL: &str = CLEANUP_CATALOGUE[0].id;

/// The speech model with `id`, if the catalogue still has it.
#[must_use]
pub fn find(id: &str) -> Option<&'static LocalModel> {
    CATALOGUE.iter().find(|model| model.id == id)
}

/// The speech model to run for a saved `id`: that model, or the default
/// when the catalogue no longer has it (a file saved before Q4 removed
/// Parakeet and Qwen3-ASR). No migration: the file keeps the old id until
/// the user next saves.
#[must_use]
pub fn resolve(id: &str) -> &'static LocalModel {
    find(id).unwrap_or(&ORUKQWEN_1_7B)
}

/// The cleanup model with `id`, if the catalogue has it.
#[must_use]
pub fn find_cleanup(id: &str) -> Option<&'static LocalModel> {
    CLEANUP_CATALOGUE.iter().find(|model| model.id == id)
}

/// Every downloadable model, speech first, in the order Settings shows
/// them.
pub fn all() -> impl Iterator<Item = &'static LocalModel> {
    CATALOGUE.iter().chain(CLEANUP_CATALOGUE)
}

/// The model with `id` in either catalogue.
#[must_use]
pub fn find_any(id: &str) -> Option<&'static LocalModel> {
    all().find(|model| model.id == id)
}

impl LocalModel {
    /// Whether it handles `language` (an ISO code).
    #[must_use]
    pub fn speaks(&self, language: &str) -> bool {
        self.languages.contains(&language)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn criterion_1_orukqwen_is_the_only_speech_model_and_the_default() {
        let ids: Vec<_> = CATALOGUE.iter().map(|m| m.id).collect();
        assert_eq!(ids, ["orukqwen-asr-1.7b-q4_k_m"]);
        assert_eq!(DEFAULT_MODEL, "orukqwen-asr-1.7b-q4_k_m");
        assert_eq!(find(DEFAULT_MODEL), Some(&ORUKQWEN_1_7B));
        // The models Q4 removed are gone; a saved id of theirs finds nothing.
        assert_eq!(find("parakeet-unified-en-0.6b-q4_k_m"), None);
        assert_eq!(find("qwen3-asr-0.6b-q4_k_m"), None);
        // A cleanup model is not a speech model.
        assert_eq!(find(DEFAULT_CLEANUP_MODEL), None);
    }

    #[test]
    fn criterion_1_hashes_and_sizes_are_the_evaluated_files() {
        // PR #331's final GGUF, reproduced on 2026-11-20.
        assert_eq!(
            ORUKQWEN_1_7B.sha256,
            "057471662f060800a6374708ad679d4abe51775e52ca591a88ea4ade78b273bb"
        );
        assert_eq!(ORUKQWEN_1_7B.size_bytes, 1_319_830_656);
        // PR #326's meta.json for gemma4-e2b.
        assert_eq!(
            GEMMA_4_E2B.sha256,
            "740185b21d22ceb83a11c3aa62ad5842ef32c70f6096d756bbee85a1e4ec34b8"
        );
        assert_eq!(GEMMA_4_E2B.size_bytes, 3_106_738_272);
    }

    #[test]
    fn gemma_is_the_cleanup_default() {
        assert_eq!(DEFAULT_CLEANUP_MODEL, "gemma-4-e2b-it-q4_k_m");
        assert_eq!(find_cleanup(DEFAULT_CLEANUP_MODEL), Some(&GEMMA_4_E2B));
        assert_eq!(find_cleanup(DEFAULT_MODEL), None);
        assert_eq!(find_any(DEFAULT_CLEANUP_MODEL), Some(&GEMMA_4_E2B));
        assert_eq!(find_any(DEFAULT_MODEL), Some(&ORUKQWEN_1_7B));
        let ids: Vec<_> = all().map(|m| m.id).collect();
        assert_eq!(ids, [DEFAULT_MODEL, DEFAULT_CLEANUP_MODEL]);
    }

    #[test]
    fn roles_match_their_catalogue() {
        assert!(CATALOGUE.iter().all(|m| m.role == Role::Speech));
        assert!(CLEANUP_CATALOGUE.iter().all(|m| m.role == Role::Cleanup));
    }

    #[test]
    fn criterion_1_every_entry_is_checkable() {
        let mut ids = HashSet::new();
        let mut files = HashSet::new();
        for model in all() {
            assert!(ids.insert(model.id), "duplicate id {}", model.id);
            assert!(files.insert(model.file_name), "duplicate file");
            assert_eq!(model.sha256.len(), 64, "{}", model.id);
            assert!(
                model
                    .sha256
                    .chars()
                    .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
                "{}",
                model.id
            );
            // A Hugging Face resolve URL ending in the stored name.
            let rest = model.url.strip_prefix("https://huggingface.co/").unwrap();
            let (repo, after) = rest.split_once("/resolve/").unwrap();
            assert_eq!(repo.matches('/').count(), 1, "{}", model.id);
            let (revision, file) = after.split_once('/').unwrap();
            assert_eq!(file, model.file_name);
            // Pinned to a commit, except OrukQwen until the owner's upload
            // has one (the Lead pins it before release).
            if model.id != ORUKQWEN_1_7B.id {
                assert_eq!(revision.len(), 40, "{}", model.id);
                assert!(revision.chars().all(|c| c.is_ascii_hexdigit()));
            }
            assert_eq!(
                std::path::Path::new(model.file_name).extension(),
                Some("gguf".as_ref())
            );
            assert!(model.size_bytes > 100_000_000, "{}", model.id);
            assert!(!model.languages.is_empty() && !model.languages_label.is_empty());
            assert_ne!(model.licence, "");
            // The id is a valid settings value.
            assert!(model.id.len() <= crate::settings::MAX_MODEL_LEN);
            assert!(!model.id.contains(char::is_whitespace));
        }
    }

    #[test]
    fn a_settings_file_with_a_removed_model_still_loads() {
        // Q4 removed Parakeet; a file that saved it must not break loading.
        // The id loads as it is, runs as OrukQwen, and every other
        // preference survives.
        let json = r#"{"version": 2, "transcription": "local",
            "local_model": "parakeet-unified-en-0.6b-q4_k_m", "hotkey": "RightCmd"}"#;
        let settings = crate::settings::from_json(json).unwrap();
        assert_eq!(settings.local_model, "parakeet-unified-en-0.6b-q4_k_m");
        assert_eq!(settings.hotkey, "RightCmd");
        assert_eq!(find(&settings.local_model), None);
        assert_eq!(resolve(&settings.local_model), &ORUKQWEN_1_7B);
        assert_eq!(resolve(DEFAULT_MODEL), &ORUKQWEN_1_7B);
        // A fresh install gets OrukQwen.
        assert_eq!(
            crate::settings::Settings::default().local_model,
            DEFAULT_MODEL
        );
    }

    #[test]
    fn orukqwen_is_english_only() {
        assert!(ORUKQWEN_1_7B.speaks("en") && !ORUKQWEN_1_7B.speaks("hi"));
        assert_eq!(ORUKQWEN_1_7B.languages_label, "English only");
    }
}
