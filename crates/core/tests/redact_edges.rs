//! M5.3 tester edges for `redact`: keys registered on one thread are hidden
//! when text is cleaned on another, and a provider body can't smuggle a key
//! or a long reply through `summary`.

use dictation_core::redact::{
    KEY_PLACEHOLDER, MAX_SUMMARY_CHARS, register_secret, secrets, summary,
};

#[test]
fn a_key_registered_on_one_thread_is_hidden_on_another() {
    let odd = "tester-sentinel-no-known-shape-7731";
    std::thread::spawn(move || register_secret(odd))
        .join()
        .unwrap();
    let cleaned = std::thread::spawn(move || secrets(&format!("echoed {odd}!")).into_owned())
        .join()
        .unwrap();
    assert!(!cleaned.contains(odd), "{cleaned}");
    assert!(cleaned.contains(KEY_PLACEHOLDER));
}

#[test]
fn concurrent_registering_and_cleaning_never_leaks_or_panics() {
    let handles: Vec<_> = (0..8)
        .map(|n| {
            std::thread::spawn(move || {
                let key = format!("tester-concurrent-key-{n}-abcdef");
                for _ in 0..200 {
                    register_secret(&key);
                    assert!(!secrets(&format!("x {key} y")).contains(&key));
                }
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
}

#[test]
fn a_multiline_provider_body_with_a_key_is_one_short_clean_line() {
    let body = format!(
        "{{\n  \"error\": \"Incorrect API key provided: gsk_tester_sentinel_0123456789\",\n  \"detail\": \"{}\"\n}}",
        "word ".repeat(400)
    );
    let line = summary(&body);
    assert!(!line.contains("gsk_tester_sentinel"), "{line}");
    assert!(!line.contains('\n'));
    assert!(
        line.chars().count() <= MAX_SUMMARY_CHARS + 1,
        "{}",
        line.len()
    );
}

#[test]
fn a_bearer_token_split_by_newline_is_still_hidden() {
    let cleaned = secrets("Authorization: Bearer\n   tester-token-0001 next");
    assert!(!cleaned.contains("tester-token-0001"), "{cleaned}");
}
