//! The on-device speech-to-text models the app offers (L1): data, not code
//! paths, so the owner's evaluations can change the list or the default by
//! editing this table.
//!
//! Every entry is a single GGUF file for transcribe.cpp, published by its
//! authors on Hugging Face (`handy-computer/*-gguf`) and pinned to a commit,
//! so the bytes behind a URL can't change under us. Sizes and SHA-256
//! values were checked against Hugging Face's own `x-linked-size` and
//! `x-linked-etag` headers for those commits on 2026-11-03 (they also match
//! transcribe.cpp's `catalog/*.json` and Handy's `catalog.json`). The model
//! manager refuses a file whose size or hash differs.

/// One model the user can download.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalModel {
    /// Stable id saved in the settings (`local_model`); never reuse one for
    /// different weights.
    pub id: &'static str,
    /// What the picker shows.
    pub name: &'static str,
    /// The languages it transcribes, as ISO 639 codes.
    pub languages: &'static [&'static str],
    /// The languages in words, for the picker ("English only").
    pub languages_label: &'static str,
    /// The download's exact size.
    pub size_bytes: u64,
    /// Where it is downloaded from: a Hugging Face `resolve` URL at a pinned
    /// commit.
    pub url: &'static str,
    /// The file's SHA-256, lowercase hex.
    pub sha256: &'static str,
    /// The name it is stored under in the models folder.
    pub file_name: &'static str,
    /// The weights' licence, for the notice.
    pub licence: &'static str,
}

/// Every model offered, in the order the picker shows them. The first is
/// the default.
pub const CATALOGUE: &[LocalModel] = &[
    LocalModel {
        id: "parakeet-unified-en-0.6b-q4_k_m",
        name: "Parakeet Unified 0.6B",
        languages: &["en"],
        languages_label: "English only",
        size_bytes: 477_274_496,
        url: "https://huggingface.co/handy-computer/parakeet-unified-en-0.6b-gguf/resolve/7e948f21b7bdbac698d3318db9d350f1096f3b6c/parakeet-unified-en-0.6b-Q4_K_M.gguf",
        sha256: "a8bf3de2b393bd14ead5a858c3748d5e3b07a20fdeabdd3b498fba4f463fa929",
        file_name: "parakeet-unified-en-0.6b-Q4_K_M.gguf",
        licence: "CC-BY-4.0 (NVIDIA)",
    },
    LocalModel {
        id: "qwen3-asr-0.6b-q4_k_m",
        name: "Qwen3-ASR 0.6B",
        languages: &[
            "zh", "en", "yue", "ar", "de", "fr", "es", "pt", "id", "it", "ko", "ru", "th", "vi",
            "ja", "tr", "hi", "ms", "nl", "sv", "da", "fi", "pl", "cs", "fil", "fa", "el", "ro",
            "hu", "mk",
        ],
        languages_label: "30 languages, including English and Hindi",
        size_bytes: 589_560_480,
        url: "https://huggingface.co/handy-computer/Qwen3-ASR-0.6B-gguf/resolve/e4e16599b900eb0cb36e524514756bb92eb092b7/Qwen3-ASR-0.6B-Q4_K_M.gguf",
        sha256: "5b58f32a58ffa2c8783e0b0963485623e286e6272d953dfc9e28bc3447dee0c0",
        file_name: "Qwen3-ASR-0.6B-Q4_K_M.gguf",
        licence: "Apache-2.0 (Qwen)",
    },
];

/// The id a new user gets: the first entry.
pub const DEFAULT_MODEL: &str = CATALOGUE[0].id;

/// The entry with `id`, if the catalogue still has it.
#[must_use]
pub fn find(id: &str) -> Option<&'static LocalModel> {
    CATALOGUE.iter().find(|model| model.id == id)
}

impl LocalModel {
    /// Whether it transcribes `language` (an ISO code).
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
    fn criterion_2_exactly_the_two_chosen_models_with_parakeet_first() {
        let ids: Vec<_> = CATALOGUE.iter().map(|m| m.id).collect();
        assert_eq!(
            ids,
            ["parakeet-unified-en-0.6b-q4_k_m", "qwen3-asr-0.6b-q4_k_m"]
        );
        assert_eq!(DEFAULT_MODEL, "parakeet-unified-en-0.6b-q4_k_m");
        assert_eq!(find(DEFAULT_MODEL), CATALOGUE.first());
        assert_eq!(find("whisper-1"), None);
    }

    #[test]
    fn criterion_2_every_entry_is_pinned_and_checkable() {
        let mut ids = HashSet::new();
        let mut files = HashSet::new();
        for model in CATALOGUE {
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
            // A resolve URL at a 40-hex commit, ending in the stored name.
            let rest = model
                .url
                .strip_prefix("https://huggingface.co/handy-computer/")
                .unwrap();
            let (_, after) = rest.split_once("/resolve/").unwrap();
            let (revision, file) = after.split_once('/').unwrap();
            assert_eq!(revision.len(), 40, "{}", model.id);
            assert!(revision.chars().all(|c| c.is_ascii_hexdigit()));
            assert_eq!(file, model.file_name);
            assert_eq!(
                std::path::Path::new(model.file_name).extension(),
                Some("gguf".as_ref())
            );
            assert!(model.size_bytes > 100_000_000, "{}", model.id);
            assert!(!model.languages.is_empty() && !model.languages_label.is_empty());
            // The id is a valid settings value.
            assert!(model.id.len() <= crate::settings::MAX_MODEL_LEN);
            assert!(!model.id.contains(char::is_whitespace));
        }
    }

    #[test]
    fn parakeet_is_english_only_and_qwen_has_hindi() {
        let parakeet = find("parakeet-unified-en-0.6b-q4_k_m").unwrap();
        assert!(parakeet.speaks("en") && !parakeet.speaks("hi"));
        assert_eq!(parakeet.languages_label, "English only");
        let qwen = find("qwen3-asr-0.6b-q4_k_m").unwrap();
        assert!(qwen.speaks("hi") && qwen.speaks("en"));
        assert_eq!(qwen.languages.len(), 30);
    }
}
