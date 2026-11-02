# Ending test-runner.sh's fixture processes however the run ends (#164).
# Sourced with $tmp set to the run's directory; finish_run is its EXIT trap.

# The fixture processes: every process whose command line holds $tmp/ (the
# longsleep job, a Terminal job still running, a stub) and their
# descendants (a stub's /bin/sleep), parents first.
fixture_pids() {
  local found pid next
  found=$(pgrep -f "$tmp/" || true)
  while [ -n "$found" ]; do
    printf '%s\n' $found
    next=""
    for pid in $found; do next="$next $(pgrep -P "$pid" || true)"; done
    found=$(echo $next)
  done
}

# A TERM first, as the VM runner sends (a held-key job answers it by
# releasing the key, which starts one more stub), then KILL for what is
# left, the tree found before the TERM included: an orphaned /bin/sleep no
# longer has a parent to be found by.
end_fixtures() {
  local before left i
  before=$(fixture_pids)
  pkill -f "$tmp/" 2>/dev/null || true
  for i in 1 2 3 4 5; do
    left=$(fixture_pids)
    [ -n "$left" ] || break
    sleep 0.2
  done
  for i in 1 2 3; do
    left=$(fixture_pids)
    # shellcheck disable=SC2086 # pids, one per word
    kill -9 $before $left 2>/dev/null || true
    before=""
    [ -n "$(fixture_pids)" ] || break
  done
}

# No fixture process outlives the run. KEEP=1 keeps the files for a look
# and prints where; never the processes.
finish_run() {
  end_fixtures
  if [ "${KEEP:-0}" = 1 ]; then
    echo "cases kept in $tmp" >&2
  else
    rm -rf "$tmp"
  fi
}
