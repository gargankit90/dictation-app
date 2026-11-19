# On-device LLM cleanup: eval

An experiment for #295, not product code and not for `main`. It runs small
language models on the Mac (llama.cpp, Metal) over the raw drafts that Vocet's
speech engines produced on the Voice Arena public set (PR #323), and scores the
cleaned text with the same judges. Results and the recommendation:
[REPORT.md](REPORT.md).

No VM, no computer use, no network at run time and no keys: cleanup is text in,
text out, and the drafts already exist in `../voicearena/results/`.

## Files

| File | What it does |
| --- | --- |
| `split.csv` | tune / check halves by speaker (sha256 of `speaker_id`, even first hex digit = tune), shared with the Groq cleanup eval: 76 clips (37 speakers) / 75 clips (34 speakers) |
| `prompts.py` | every prompt version (v1 Blurt-based, v2, v3, and the two fine-tunes' own prompts `s1`, `bv`) |
| `guard.py` | the fallback guard: raw text is pasted when the reply is empty, an echo, assistant-style, much longer or shorter, or drops a number, an email/URL/dotted name or a capitalised name |
| `clean.py` | runs one model over one engine's drafts through `llama-server`: latency per clip, load time, peak memory |
| `run_models.sh` | the model list (names and GGUF files) and a loop over it |
| `screen.py` | the cheap check before judging: match with the intended text, CER/WER, "broke" count |
| `judge.sh` | both judges over run folders, with one shared answer cache per judge (`judge_cache/`) |
| `apply_guard.py` | re-applies a different guard (none, or stricter) to stored replies, without rerunning the model |
| `audit.py` | lost negations, changed identifiers, invented text, per run (`audit.csv`) |
| `screen_vs_judge.py` | how well each screen measure ranks runs the way the judges do (Spearman) |
| `summarize.py` | the tables: both judges, both halves, intervals, fallbacks, latency, memory, fixed/harmed clips (`harms.csv`) |
| `runs/<engine>/<model>__<prompt>[__guard]/` | `results.csv` (raw, reply, pasted text, fallback reason, seconds), `meta.json`, `server.log`, `scored.csv` (Claude), `scored_codex.csv` (Codex), `harms.csv`, `audit.csv` |

The committed CSVs contain the engines' drafts and the models' replies, never
the dataset's reference texts (CC BY-NC 4.0); join on `clip_id` with
`../voicearena/clips/manifest.csv` after `../voicearena/bench.sh fetch`.

## Reproducing

```sh
brew install llama.cpp                      # 1.0.0 (build 12359) was used
mkdir -p ~/vocet-cleanup-models             # models live outside the repo
# download each GGUF named in run_models.sh from Hugging Face (repos in REPORT.md) and check its sha256
eval/voicearena/bench.sh fetch              # clips/manifest.csv, for the references
cd eval/local-cleanup
./run_models.sh parakeet v1                 # round 1: every model, prompt v1
uv run screen.py runs/parakeet/*__v1        # screen
./judge.sh runs/parakeet/<model>__v1        # judge the best ones (claude -p and codex exec, signed in)
uv run summarize.py runs/parakeet/raw runs/parakeet/<model>__v1 ...
```

Models are treated as untrusted data: each file's sha256 is checked against
the Hugging Face LFS hash, and it is only loaded by llama.cpp.
