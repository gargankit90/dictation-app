# Groq LLM cleanup after Whisper: Voice Arena eval

Experiment, not for `main`. Code: branch `exp/groq-cleanup` (draft PR
"experiment: Groq LLM cleanup (do not merge)"). Harness, timing, judge and
scoring: `eval/voicearena/` (same as every other run there).

## Tune / check split

`split.csv` (`clip_id,speaker_id,half`) is fixed for all cleanup evals
(Lead, 2026-11-19): **tune** = speakers whose `sha256(speaker_id)` (the
decimal string in the dataset) starts with an even hex digit, **check** =
odd. 76 clips / 37 speakers in tune, 75 clips / 34 speakers in check.
Prompt changes are made only from tune-half failures; every version is
reported on both halves.
