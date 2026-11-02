//! Deliberately failing probe for PR #145 (never merge). It fails only under
//! GitHub Actions, so the local pre-push gate still lets the branch be pushed.
#[test]
fn ci_fail_probe() {
    assert!(
        std::env::var_os("GITHUB_ACTIONS").is_none(),
        "deliberate failure to prove the CI job fails"
    );
}
