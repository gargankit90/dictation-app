//! Deliberately failing probe for PR #145; never merge.
#[test]
fn ci_fail_probe() {
    assert_eq!(1 + 1, 3, "deliberate failure to prove CI fails");
}
