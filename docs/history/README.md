# History: how the agents built this

Every change to dictation-app was written by AI agents and went through a pull
request. These folders are the paper trail, kept as written:

- `milestones/<id>.md`: the spec, with numbered acceptance criteria tagged
  `[auto]` or `[manual]`. Written by the Lead before any code.
- `test-reports/<id>-run<k>.md`: one Tester run, criterion by criterion, with
  PASS or FAIL. Screenshots and VM evidence sit next to the report.
- `reviews/<id>-round<k>.md`: one Code Review round, with findings ranked
  blocker, major or minor.

## How they connect

A milestone id (say `M5.9`) names the spec, `milestones/M5.9.md`. The pull
request opened for it gets a Tester run
(`test-reports/M5.9-run1.md`) and a review round (`reviews/M5.9-round1.md`)
in parallel. A FAIL or CHANGES REQUESTED means the Developer fixes and a
second run or round follows (`-run2`, `-round2`). Each report states its PR
number at the top; the merge commit on `main` names the same PR.

## Read these first

| Story | Read |
| ----- | ---- |
| A Mac froze when Accessibility was switched off (#173): spec, the test that mutates the fix, and the review | [spec](milestones/M5.9.md), [test run](test-reports/M5.9-run1.md), [review](reviews/M5.9-round1.md) |
| The review caught a proxy that would have carried audio and the API key (PR #157) | [review round 1](reviews/T4a-round1.md), [round 2](reviews/T4a-round2.md), [spec](milestones/T4a.md) |
| The push guard, simplified in three review rounds (G1) | [spec](milestones/G1.md), [round 1](reviews/G1-round1.md), [round 2](reviews/G1-round2.md), [round 3](reviews/G1-round3.md) |
| A test seeded the wrong text because a shell function was shadowed (T7) | [spec](milestones/T7.md), [test run](test-reports/T7-run1.md), [review](reviews/T7-round1.md) |
| The Tester finds that a release test cannot tell two checks apart (R1) | [test run](test-reports/R1-run1.md) |
| A spec for this very move (D3) | [spec](milestones/D3.md) |

## The team

A Developer, a Tester and a Code Reviewer, each a separate agent with its own
role file, supervised by a Lead and the owner: see [AGENTS.md](../../AGENTS.md).
