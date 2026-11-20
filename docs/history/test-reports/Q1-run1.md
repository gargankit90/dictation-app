# Q1 test report, run 1

PR: #335 at be7e502
Result: PASS

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | No audio or keys under `eval/`; `.gitignore` covers clips | `git ls-files eval` filtered by audio extensions (flac, wav, mp3, m4a, ogg, opus, aac, webm, mp4, aiff); `git grep` for sk-, gsk_, sk-or-v1-, AIza and `api_key = "..."` patterns | PASS | 33 tracked files (16 py, 6 md, 3 csv, 4 txt, 2 sh, 1 json, 1 .gitignore); no audio, no key match. `eval/.gitignore` covers `clips/`, `work/`, `.judge-cache/`, `*.flac`, `*.wav`, `*.mp3`, `*.m4a`, `*.parquet`. Also covered by `test_hygiene.py`. |
| 2 | `uv run pytest eval` passes offline | `uv run pytest eval -q` | PASS | 62 passed in 18.4 s on a fresh venv (under the 30 s limit for the gate). I did not time the whole gate; CI is the authority for that. |
| 3 | `eval/regress.sh --dry-run` prints the plan and cost estimate without network | `OPENROUTER_API_KEY` unset, `eval/regress.sh --dry-run` | PASS | Finished in 0.2 s. It printed 40 clips, models, prompt sha, estimated cost ($0.0216 speech + $0.0049 cleanup), baseline ZER 57.5% and the fail threshold. It says the key is not set and the manifest is missing, which is consistent with no network. |
| 4 | e2e results include key-up-to-paste p50/p95 | `E2E_VM=dictation-vm-2 tests/e2e/run.sh e2e_latency` | PASS | 20 dictations (mock engine). `results.txt` has `LATENCY key-up to paste, 20 dictations (mock engine): p50 12 ms, p95 35 ms`. Total: 1 passed, 0 failed, 0 skipped. |
| 5 | Lead runs one real `regress.sh` | none | not tested | Manual, the Lead's. |

## Dataset text (CC BY-NC) check

Not committed beyond fragments:
- `split.csv`, `regress/baseline.csv`, `summary.csv` and `regress/clips.txt` hold only clip ids, speaker ids, halves, domains, edit counts and scores. They hold no clip text.
- `eval/reports/*.md` quote short fragments of draft or intended text, mostly one sentence or less, for example the `cleanup.md` failure examples for dict_0235, dict_0335, dict_0350, dict_0621, dict_0414, dict_0346, dict_0474, dict_0399 and dict_0539. These look like the "tiny illustrative fragments" allowed.
- I found no full-corpus or per-clip reference lists, drafts or transcripts.
- Minor, not blocking: the two longest quotes are the dict_0235 and dict_0335 examples, each about two sentences.

## Gate
Not re-run: CI covers it. Python tests: 62 passed offline.

## Untested behaviour found
- `regress.sh --dry-run` was not run inside a network-blocked sandbox. It finished in 0.2 s with no key set, so no request was made. None added.
- e2e note: a fresh worktree needs `npm ci` before `tests/e2e/run.sh`, otherwise the build fails with `tsc: command not found`. This is an environment issue, not a PR defect.
- I ran on `dictation-vm-2` (the Tester VM, the runner's allowed VM). The runner accepts only `dictation-vm` or `dictation-vm-2`, so `dictation-vm-eval-2` could not be used.

## Failures
None.
