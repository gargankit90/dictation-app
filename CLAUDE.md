# CLAUDE.md

@AGENTS.md

## Notes for Claude Code

- As Developer, follow `.claude/agents/developer.md`. As coordinator, start
  workers through Orca Orchestration (`orca skills get orchestration` prints
  the guide) and give each worker its role file and milestone spec.
- `.claude/settings.json` holds the git guard hook and deny rules. They load
  when a session starts; don't try to work around a block. Ask the Lead.
- `gh pr merge` is blocked for Claude. The Code Reviewer (Antigravity) merges.
- As Tester, follow `docs/agents/tester.md`; you run unattended, so stay
  inside its "may edit" list and time box.
- The `cua-vm` MCP server drives the test VM. Use it only for tests, never the
  owner's screen.

- GitHub rulesets `main-public` and `immutable-tags` enforce default-branch
  and tag protection server-side for every account, with no bypass. Never
  push to the default branch, force push or skip hooks.
- The unchanged pre-push guard adds defence in depth and runs the gate.
  Claude's command hook keeps only role, hook bypass, guard edit and
  destructive settings checks; it does not inspect push commands or git
  aliases. Static push deny rules in settings.json remain for the owner to
  decide separately. Only the owner changes guards.
