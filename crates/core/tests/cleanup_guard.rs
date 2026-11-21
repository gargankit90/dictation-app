//! QT1: the cleanup guard as one table: what it refuses, and what it accepts
//! on purpose. A row is `(raw, reply, judgement)`. Self-corrections keep
//! both values (prompt v4); a reply that drops or rewrites a number, link or
//! name is refused; invented content is caught only by the length check.

use dictation_core::cleanup::Judgement::{
    Accepted, DroppedLink, DroppedName, DroppedNumber, Empty, TooLong,
};
use dictation_core::cleanup::{Judgement, judge, prompt};

const TABLE: &[(&str, &str, Judgement)] = &[
    // Tidied, nothing lost.
    (
        "um so send it to Priya by 5",
        "Send it to Priya by 5.",
        Accepted,
    ),
    (
        "the build is at example dot com slash a",
        "The build is at example.com/a.",
        Accepted,
    ),
    // Numbers.
    (
        "we need 3 of them by 5",
        "We need three of them.",
        DroppedNumber,
    ),
    ("call me at 555 0100", "Call me.", DroppedNumber),
    (
        "Print 20 copies, make that 30.",
        "Print 20 copies, make that 30.",
        Accepted,
    ),
    (
        "Print 20 copies, make that 30.",
        "Print 30 copies.",
        DroppedNumber,
    ),
    (
        "Print 20 copies, make that 30.",
        "Print 20 copies.",
        DroppedNumber,
    ),
    // Links and addresses.
    (
        "see https://example.org/x for it",
        "See the site for it.",
        DroppedLink,
    ),
    ("mail sam@example.com", "Mail Sam.", DroppedLink),
    (
        "see https://example.org/x for it",
        "See https://example.org/x for it.",
        Accepted,
    ),
    // Names.
    (
        "tell Priya Raman I called",
        "Tell her I called.",
        DroppedName,
    ),
    (
        "tell Priya Raman I called",
        "Tell Priya I called.",
        DroppedName,
    ),
    (
        "tell Priya Raman I called",
        "Tell Priya Raman I called.",
        Accepted,
    ),
    // Nothing, or far too much (an answer or an invented paragraph).
    ("send it today", "", Empty),
    ("send it today", "   \n", Empty),
    (
        "what is the capital of France",
        "The capital of France is Paris, a city on the Seine known for its museums, its food and its cafes.",
        TooLong,
    ),
];

#[test]
fn the_guard_matrix() {
    for (raw, reply, want) in TABLE {
        assert_eq!(judge(raw, reply, prompt()), *want, "{raw:?} -> {reply:?}");
    }
}
