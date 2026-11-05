# G1: simple Claude safety hook

Owner approved the guard changes on 2026-11-05. Codex implements this milestone;
a Claude reviewer reviews it. Branch: `g1-simple-guard`.

1. [auto] `guard-git.sh` is about 30 lines of simple matching, fails open on
   unreadable input, unparsable commands and heredocs, never executes the
   inspected command and never reads files named in it.
2. [auto] Block plain `gh pr merge`, REST/GraphQL merges, `gh repo delete`,
   `gh repo edit`, `gh release delete`, non-GET repository policy/ref API
   requests and matching GraphQL mutations, and shell writes to real guard
   paths: `.claude/hooks/`, `.claude/settings*.json`, `scripts/guard-push.sh`,
   `scripts/pre-push`, `scripts/install-hooks.sh`, `lefthook*.yml`, `.git/hooks`.
3. [auto] Allow hook-bypass flags/settings, settings.json outside `.claude`,
   read-only references to guards (cat, grep, sed -n, Python), and multiline
   scripts. The rule against skipping hooks remains in AGENTS.md.
4. [auto] Remove the four static Bash git-push deny rules; retain the merge
   deny and every Edit/Write deny rule.
5. [auto] Table-driven must-block/must-pass guard tests include the former
   false positives and quoted commit-message data; the suite takes under 5 s.
6. [auto] `scripts/guard-push.sh`, `scripts/pre-push` and
   `scripts/install-hooks.sh` are unchanged; no reinstall is needed.
7. [auto] A daily hosted workflow with no secrets and a read-only token
   compares the three rulesets against a checked-in policy snapshot and
   passes `scripts/check-workflows.sh`. Disclose API visibility limitations.
   Per the Lead's clarification, compare visible fields and warn when bypass
   actors are hidden; never treat omission as an empty bypass list. Provide
   an optional local admin-token check for the bypass list.
8. [auto] AGENTS.md, CLAUDE.md and docs/ENGINEERING.md describe the accepted
   trade-off: deliberately disguised commands can bypass the hook; the
   ruleset audit catches visible policy drift afterwards, with bypass-actor
   changes requiring the local admin-token check.
