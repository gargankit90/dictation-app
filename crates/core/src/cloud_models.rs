//! The two cloud models the app uses through `OpenRouter` (Q2). A model
//! that shuts down (`openai/gpt-4o-mini-transcribe` does on 2027-02-26) is
//! replaced by editing these constants; the planned replacement is
//! `mistralai/voxtral-mini-3b-2507` (84.1 ZER with GLM cleanup). There is no
//! automatic fallback between models. Settings names them
//! (`src/settings/modelRow.ts`).

/// The speech model. `OpenRouter` has no zero-retention route for `OpenAI`'s
/// own speech models (`OpenAI` doesn't train on API data), so its requests
/// carry [`PrivacyFilter::DenyDataCollection`].
pub const SPEECH_MODEL: &str = "openai/gpt-4o-mini-transcribe";

/// The cleanup model: GLM, which reasons at max by default, so requests ask
/// for low effort; they carry [`PrivacyFilter::ZeroDataRetention`].
pub const CLEANUP_MODEL: &str = "z-ai/glm-5.3-flash";

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
