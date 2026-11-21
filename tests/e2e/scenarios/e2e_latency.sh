# Q1 criterion 4: key-up to paste time per dictation. Twenty dictations on the mock engine (no network, no
# key), so the numbers are the pipeline's own overhead: the transcript, cleanup guard, cursor read and
# paste. Each dictation's time is the app log's `Transcribing` line (the key-up) to the next `Idle` line
# (the paste is done); tests/e2e/latency.py turns them into p50/p95, saved as $OUT_DIR/latency.csv and shown
# in the run's results.txt.

e2e_latency() {
  local cycles=20 i
  fresh_app_state
  launch_full "$VM_TONE"
  expect "the app starts and logs ready" wait_for_log 'INFO .*ready'
  textedit_fresh

  for i in $(seq 1 "$cycles"); do
    expect "dictation $i gets back to Idle" dictate_once 0.4
  done
  collect_log e2e_latency

  # A sanity bound, not a target: the mock engine answers at once, so seconds mean something is stuck.
  expect "all $cycles dictations have a key-up to paste time, and the p95 is under 5 s" \
    python3 tests/e2e/latency.py "$OUT_DIR/e2e_latency.app.log" --csv "$OUT_DIR/latency.csv" \
    --line "$OUT_DIR/latency.txt" --expect "$cycles" --p95-under 5000
  log "  $(cat "$OUT_DIR/latency.txt" 2>/dev/null || echo 'no latency line')"
  textedit_close
}
