# Where these prompts come from

Copied unchanged on 2026-11-19 from the Groq cleanup eval worker's worktree
(`groq-cleanup-eval`, branch `eval/groq-cleanup`, folder `eval/groq-cleanup/prompts/`):

| File | What it is | sha256 of the trimmed text (first 12) |
| --- | --- | --- |
| `v1-blurt-smart.txt` | Blurt's Smart rules as a chat prompt; the app's built-in prompt on `exp/groq-cleanup` (PR #324), MIT, credited in `NOTICE` there | `f2a6b565e9db` (the VM v1 run's `prompt_sha`) |
| `v3.txt` | v1 rewritten from v1's tune-half failures: keep meaning-carrying sentences, keep both values of a self-correction, spoken symbols in addresses as symbols, drop a phantom final "Thank you." | see `offline_cleanup.py` output |
| `v4.txt` | v3 + fix clearly misheard technical words and write code names as code | see `offline_cleanup.py` output |

The task brief calls the "fixes misheard technical words" prompt v3; in the source folder that text is
`v4.txt` (its `v3.txt` has no such rule). This eval uses the file names as they are.
