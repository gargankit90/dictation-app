//! The cloud models the app uses through `OpenRouter` (Q2): data, not code
//! paths, so a model that shuts down (`openai/gpt-4o-mini-transcribe` does on
//! 2027-02-26) is replaced by editing this table. The planned replacement is
//! `mistralai/voxtral-mini-3b-2507` (84.1 ZER with GLM cleanup); there is no
//! automatic fallback between models.
//!
//! Facts come from `OpenRouter`'s public model and endpoint lists and the
//! vendors' docs, read 2026-11-20 (the owner's `model-features.md`); scores
//! from the Voice Arena evals (PRs #327, #332). Prices are notes for people,
//! never used in code.

/// What a model does in a dictation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    /// Audio in, text out (`/audio/transcriptions`).
    Speech,
    /// Text in, cleaned text out (`/chat/completions`).
    Cleanup,
}

/// The `OpenRouter` provider filter every request to a model carries. A
/// request is never sent without it; if no provider matches, the request
/// fails (closed) rather than going to one that keeps or trains on data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrivacyFilter {
    /// `provider: {data_collection: "deny"}`: only providers that don't
    /// store requests to train on them.
    DenyDataCollection,
    /// `provider: {zdr: true, data_collection: "deny"}`: only providers
    /// with zero data retention, which also don't train.
    ZeroDataRetention,
}

impl PrivacyFilter {
    /// Whether `zdr: true` is sent.
    #[must_use]
    pub fn zdr(self) -> bool {
        self == Self::ZeroDataRetention
    }
}

/// One cloud model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CloudModel {
    /// The `OpenRouter` model id, sent as `model`.
    pub id: &'static str,
    /// Speech or cleanup.
    pub role: Role,
    /// What Settings shows.
    pub display_name: &'static str,
    /// The price, for people (not used in code).
    pub price_note: &'static str,
    /// The languages, in words.
    pub languages: &'static str,
    /// The day the model stops working (ISO 8601), if announced.
    pub shutdown: Option<&'static str>,
    /// Whether `OpenRouter` forwards a speech `prompt` to the model's
    /// providers (`OpenAI`, Azure, Groq, Together do; Mistral and `DeepInfra`
    /// ignore it). Speech only; the app sends no prompt to a model without.
    pub prompt: bool,
    /// The provider filter on every request.
    pub privacy: PrivacyFilter,
    /// Whether the request asks for low reasoning effort (models that
    /// reason by default, such as GLM, which defaults to max).
    pub low_reasoning: bool,
}

/// The speech model a new user gets.
pub const DEFAULT_SPEECH: &str = "openai/gpt-4o-mini-transcribe";

/// The cleanup model.
pub const DEFAULT_CLEANUP: &str = "z-ai/glm-5.3-flash";

/// Every model, speech first.
pub const CATALOG: &[CloudModel] = &[
    CloudModel {
        id: DEFAULT_SPEECH,
        role: Role::Speech,
        display_name: "GPT-4o mini Transcribe (OpenAI)",
        price_note: "$1.25 per million audio input tokens, $5 per million output tokens; with \
                     GLM cleanup about $0.68 per 1,000 dictations",
        languages: "Many; OpenAI doesn't publish the list",
        shutdown: Some("2027-02-26"),
        prompt: true,
        // OpenRouter has no zero-retention route for OpenAI's own speech
        // models; OpenAI doesn't train on API data.
        privacy: PrivacyFilter::DenyDataCollection,
        low_reasoning: false,
    },
    CloudModel {
        id: DEFAULT_CLEANUP,
        role: Role::Cleanup,
        display_name: "GLM 5.3 Flash (Z.ai)",
        price_note: "$0.04 to $0.15 per million input tokens, $0.08 to $0.50 per million output \
                     tokens, by provider",
        languages: "Tagged English and Chinese; others unverified",
        shutdown: None,
        prompt: false,
        privacy: PrivacyFilter::ZeroDataRetention,
        low_reasoning: true,
    },
];

/// The model with `id`, if the catalog has it.
#[must_use]
pub fn find(id: &str) -> Option<&'static CloudModel> {
    CATALOG.iter().find(|model| model.id == id)
}

/// The speech model with `id`, if the catalog has it as one.
#[must_use]
pub fn speech(id: &str) -> Option<&'static CloudModel> {
    find(id).filter(|model| model.role == Role::Speech)
}

/// The default speech model's entry.
#[must_use]
pub fn default_speech() -> &'static CloudModel {
    // The table always holds it (tested).
    speech(DEFAULT_SPEECH).unwrap_or(&CATALOG[0])
}

/// The cleanup model's entry.
#[must_use]
pub fn default_cleanup() -> &'static CloudModel {
    find(DEFAULT_CLEANUP).unwrap_or(&CATALOG[CATALOG.len() - 1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn criterion_1_defaults_are_the_owners_choice() {
        let speech = default_speech();
        assert_eq!(speech.id, "openai/gpt-4o-mini-transcribe");
        assert_eq!(speech.role, Role::Speech);
        assert_eq!(speech.shutdown, Some("2027-02-26"));
        assert!(speech.prompt, "OpenRouter forwards prompts to OpenAI");
        assert_eq!(speech.privacy, PrivacyFilter::DenyDataCollection);

        let cleanup = default_cleanup();
        assert_eq!(cleanup.id, "z-ai/glm-5.3-flash");
        assert_eq!(cleanup.role, Role::Cleanup);
        assert_eq!(cleanup.privacy, PrivacyFilter::ZeroDataRetention);
        assert!(cleanup.privacy.zdr());
        assert!(cleanup.low_reasoning, "GLM reasons at max by default");
    }

    #[test]
    fn ids_are_unique_and_every_model_has_a_filter() {
        for (i, model) in CATALOG.iter().enumerate() {
            assert!(
                CATALOG[i + 1..].iter().all(|other| other.id != model.id),
                "{}",
                model.id
            );
            assert!(model.id.contains('/'), "{}", model.id);
            assert!(!model.display_name.is_empty() && !model.price_note.is_empty());
            if model.role == Role::Cleanup {
                assert!(!model.prompt, "prompt support is a speech field");
            }
        }
        assert_eq!(speech(DEFAULT_CLEANUP), None, "not a speech model");
        assert_eq!(find("groq/whisper"), None);
    }
}
