//! The "Report a problem" text (E1, #209, `docs/privacy.md`).
//!
//! The report is built only from the typed fields of [`ReportInput`]. There
//! is no free-text slot: a transcript, a clipboard, an app name, a path, a
//! log line or a provider's answer has no way in, because nothing here takes
//! one. The few fields that are text (versions, the hotkey, a model id) are
//! checked against a narrow character set and a known-key filter
//! ([`field`]), and show `unknown` if they fail it.
//!
//! The app never sends the report. The user sees [`render`]'s exact text,
//! then saves it, copies it, or opens a pre-filled issue ([`issue_url`])
//! that they submit themselves.

use std::fmt::Write as _;

use crate::diagnostics::LoggedError;
use crate::redact;
use crate::settings::Provider;

/// How many recent errors the report lists.
pub const MAX_ERRORS: usize = 10;

/// The longest text field, in characters.
const MAX_FIELD_CHARS: usize = 40;

/// Where the issue form opens.
pub const ISSUES_URL: &str = "https://github.com/gargankit90/dictation-app/issues/new";

/// The state of an OS permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// Granted.
    Granted,
    /// Refused.
    Denied,
    /// Not asked, or the OS can't say.
    NotDetermined,
}

impl Access {
    fn as_str(self) -> &'static str {
        match self {
            Self::Granted => "granted",
            Self::Denied => "denied",
            Self::NotDetermined => "not determined",
        }
    }
}

/// Which engine transcribes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineInfo<'a> {
    /// A cloud provider (never its key, never the account).
    Cloud(Provider),
    /// A model on this Mac, by its catalogue id.
    Local(&'a str),
}

/// Everything the report may say. Adding a field here is adding it to the
/// report, so it is also where `docs/privacy.md` is checked.
#[derive(Debug, Clone, Copy)]
pub struct ReportInput<'a> {
    /// The app's version.
    pub app_version: &'a str,
    /// The macOS version, such as `15.1`.
    pub macos_version: &'a str,
    /// The CPU architecture, such as `aarch64`.
    pub arch: &'a str,
    /// The microphone permission.
    pub microphone: Access,
    /// The Accessibility permission.
    pub accessibility: Access,
    /// The dictation hotkey, as the settings write it.
    pub hotkey: &'a str,
    /// The engine.
    pub engine: EngineInfo<'a>,
    /// Whether the engine is ready (a key can be read, or a model is
    /// present); `None` if not checked.
    pub engine_ready: Option<bool>,
    /// The most recent errors, oldest first.
    pub errors: &'a [LoggedError],
    /// Whether anonymous usage events are on.
    pub analytics_on: bool,
}

/// `text` if it is short, plain and holds no key; otherwise `unknown`.
///
/// Plain means letters, digits, space and `. + - _ :` only, which keeps
/// out paths (`/`, `~`), user names in paths, quotes, newlines and anything
/// that looks like markup.
#[must_use]
pub fn field(text: &str) -> &str {
    let plain = !text.is_empty()
        && text.chars().count() <= MAX_FIELD_CHARS
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '.' | '+' | '-' | '_' | ':'))
        && redact::secrets(text) == text;
    if plain { text } else { "unknown" }
}

/// `seconds` since the Unix epoch as `2026-11-03T10:11:12Z` (UTC).
#[must_use]
pub fn utc(seconds: u64) -> String {
    let days = i64::try_from(seconds / 86_400).unwrap_or(i64::MAX / 2);
    let rest = seconds % 86_400;
    // Howard Hinnant's civil-from-days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3_600,
        rest % 3_600 / 60,
        rest % 60
    )
}

/// The report, as the user sees and saves it.
#[must_use]
pub fn render(input: &ReportInput<'_>) -> String {
    let mut out = String::new();
    // Writing to a String can't fail.
    let _ = writeln!(out, "Vocet diagnostics report");
    let _ = writeln!(
        out,
        "Built from a fixed list of fields. It holds no audio, transcripts, text you typed \
         or copied, API keys, app names, file paths or log lines."
    );
    let _ = writeln!(out);
    let _ = writeln!(out, "app_version: {}", field(input.app_version));
    let _ = writeln!(out, "macos_version: {}", field(input.macos_version));
    let _ = writeln!(out, "arch: {}", field(input.arch));
    let _ = writeln!(out, "microphone: {}", input.microphone.as_str());
    let _ = writeln!(out, "accessibility: {}", input.accessibility.as_str());
    let _ = writeln!(out, "dictation_key: {}", field(input.hotkey));
    let _ = match input.engine {
        EngineInfo::Cloud(provider) => writeln!(out, "engine: cloud ({provider})"),
        EngineInfo::Local(model) => writeln!(out, "engine: local ({})", field(model)),
    };
    let ready = match input.engine_ready {
        Some(true) => "yes",
        Some(false) => "no",
        None => "not checked",
    };
    let _ = writeln!(out, "engine_ready: {ready}");
    let _ = writeln!(
        out,
        "analytics: {}",
        if input.analytics_on { "on" } else { "off" }
    );
    let skip = input.errors.len().saturating_sub(MAX_ERRORS);
    let shown = &input.errors[skip..];
    if shown.is_empty() {
        let _ = writeln!(out, "recent_errors: none");
    } else {
        let _ = writeln!(out, "recent_errors:");
        for error in shown {
            let _ = writeln!(out, "  - {} {}", utc(error.at), error.code);
        }
    }
    out
}

/// The issue form's URL with the report as its body, for the browser. The
/// user reads and submits it there; the app sends nothing.
#[must_use]
pub fn issue_url(report: &str) -> String {
    let mut url = format!("{ISSUES_URL}?title=");
    url.push_str(&percent_encode("Problem report"));
    url.push_str("&body=");
    url.push_str(&percent_encode(report));
    url
}

fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::ErrorCode;

    fn input(errors: &[LoggedError]) -> ReportInput<'_> {
        ReportInput {
            app_version: "0.1.0",
            macos_version: "15.1",
            arch: "aarch64",
            microphone: Access::Denied,
            accessibility: Access::Granted,
            hotkey: "RightAlt",
            engine: EngineInfo::Cloud(Provider::Groq),
            engine_ready: Some(true),
            errors,
            analytics_on: true,
        }
    }

    #[test]
    fn the_report_lists_exactly_the_allowlisted_fields() {
        let errors = [LoggedError {
            code: ErrorCode::MicDenied,
            at: 1_793_700_672,
        }];
        let text = render(&input(&errors));
        let keys: Vec<&str> = text
            .lines()
            .skip(3)
            .filter(|line| !line.starts_with("  - "))
            .map(|line| line.split(':').next().unwrap_or(""))
            .collect();
        assert_eq!(
            keys,
            [
                "app_version",
                "macos_version",
                "arch",
                "microphone",
                "accessibility",
                "dictation_key",
                "engine",
                "engine_ready",
                "analytics",
                "recent_errors"
            ]
        );
        assert!(text.contains("microphone: denied"), "{text}");
        assert!(text.contains("engine: cloud (groq)"), "{text}");
        assert!(text.contains("  - 2026-11-03T"), "{text}");
        assert!(text.contains("mic_denied"), "{text}");
    }

    #[test]
    fn local_engines_show_the_model_id_and_no_errors_say_none() {
        let mut data = input(&[]);
        data.engine = EngineInfo::Local("whisper-small");
        data.engine_ready = None;
        data.analytics_on = false;
        let text = render(&data);
        assert!(text.contains("engine: local (whisper-small)"), "{text}");
        assert!(text.contains("engine_ready: not checked"), "{text}");
        assert!(text.contains("analytics: off"), "{text}");
        assert!(text.contains("recent_errors: none"), "{text}");
    }

    #[test]
    fn only_the_newest_errors_are_listed() {
        let errors: Vec<LoggedError> = (0..25)
            .map(|i| LoggedError {
                code: ErrorCode::NoSpeech,
                at: 1_000 + i,
            })
            .collect();
        let text = render(&input(&errors));
        assert_eq!(text.matches("no_speech").count(), MAX_ERRORS);
        assert!(text.contains(&utc(1_024)), "{text}");
        assert!(!text.contains(&utc(1_000)), "{text}");
    }

    #[test]
    fn text_fields_that_are_not_plain_show_unknown() {
        for bad in [
            "",
            "/Users/someone/models/x",
            "~/models",
            "gsk_dummy_NOT_A_REAL_KEY_0123456789",
            "sk-proj-NOTAREALKEY0123456789abcdef",
            "line\nbreak",
            "quote\"",
            "<script>",
            &"x".repeat(MAX_FIELD_CHARS + 1),
        ] {
            assert_eq!(field(bad), "unknown", "{bad:?}");
        }
        for good in ["0.1.0", "15.1", "aarch64", "RightAlt", "Cmd+Shift+Space"] {
            assert_eq!(field(good), good);
        }
    }

    #[test]
    fn times_are_utc() {
        assert_eq!(utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(utc(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(utc(1_793_700_672), "2026-11-03T10:11:12Z");
    }

    #[test]
    fn the_issue_url_carries_the_report_as_its_body() {
        let report = render(&input(&[]));
        let url = issue_url(&report);
        assert!(url.starts_with("https://github.com/gargankit90/dictation-app/issues/new?title="));
        let body = url.split("&body=").nth(1).unwrap();
        assert!(!body.contains(' ') && !body.contains('\n'));
        assert!(body.contains("app_version%3A%200.1.0"), "{body}");
        assert!(url.len() < 4_000, "{}", url.len());
    }
}
