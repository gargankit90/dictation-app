# CLAUDE.md

@AGENTS.md

## Notes for Claude Code

- As Developer, follow `.claude/agents/developer.md`. As coordinator, start
  workers through Orca Orchestration (`orca skills get orchestration` prints
  the guide) and give each worker its role file and milestone spec.
- `.claude/settings.json` holds the git guard hook and deny rules. They load
  when a session starts; don't try to work around a block. Ask the Lead.
- `gh pr merge` is blocked for Claude. The Code Reviewer (Codex) merges.
- As Tester, follow `docs/agents/tester.md`; you run unattended, so stay
  inside its "may edit" list and time box.
- The `cua-vm` MCP server drives `dictation-vm` (Developers) and `cua-vm-2`
  drives `dictation-vm-2` (Testers). Use them only for tests, never the
  owner's screen.

- GitHub rulesets `main-public`, `immutable-tags` and
  `all-branches-no-force-no-delete` enforce push protection for every account,
  with no bypass. Never push to the default branch, force push or skip hooks.
- The unchanged pre-push guard runs the gate; no hook reinstall is needed
  for G1. Claude's command hook matches plain merge, destructive settings
  and guard-write commands, fails open on unreadable/unparsable input and
  heredocs, and does not read command files. Static push deny rules and
  hook-bypass checks are removed; Edit/Write deny rules remain.
- A deliberately disguised command can pass the hook. This accepted
  trade-off keeps ordinary scripts usable. The daily read-only ruleset
  workflow detects visible policy changes afterwards, but GitHub hides
  bypass actors from read-only callers. The Lead checks those locally with
  `python3 scripts/check-rulesets.py --require-bypass` and the admin token.
  Only the owner changes guards.
