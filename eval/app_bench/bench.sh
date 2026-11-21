#!/usr/bin/env bash
# Host side of the bench: a Lume macOS VM of its own, harness.py in the VM's Terminal, results back here.
# See README.md for the one-time VM image and the order of steps.
#
#   ./bench.sh fetch                     # the clips from their original source into clips/ (git-ignored)
#   ./bench.sh vm-create                 # clone $BENCH_BASE_IMAGE -> $BENCH_VM (once; 8 GB, 4 CPUs)
#   ./bench.sh vm-start [--timed] | vm-stop     # --timed refuses while any other VM runs
#   ./bench.sh vm-setup                  # uv, the harness, the clips, BlackHole as input, no autocorrect
#   ./bench.sh install <App.app|App.dmg|App.zip> [/Applications]   # host -> VM
#   ./bench.sh check <app> [harness args...]                       # the harness preflight in the VM
#   ./bench.sh run <label> <app> <mode> [harness args...]          # timed run; results to work/<label>/
#   ./bench.sh meta <label> key=value...  # how the app was set up (work/<label>/meta.json)
#   ./bench.sh score <label>             # judge.py (Claude) -> work/<label>/scored.csv
#   ./bench.sh publish                   # aggregates only -> results/ (summary.csv, summary.md, per-run setup)
#   ./bench.sh lock | unlock             # hold / release $BENCH_LOCK_FILE, if you share VMs with other jobs
#   ./bench.sh ssh <cmd> | screenshot <png>
#
# Settings (environment): BENCH_VM (default bench-vm), BENCH_BASE_IMAGE (the VM to clone for vm-create),
# BENCH_SSH_KEY (default ~/.ssh/id_ed25519), BENCH_VM_USER (default lume), BENCH_HOSTKEY_FROM (an ssh
# known_hosts alias whose key the clone inherits), BENCH_LOCK_FILE (optional), BENCH_EXTRA_FILES (more files
# for vm-setup to copy next to harness.py). work/ holds transcripts of CC BY-NC clips and is git-ignored.
set -euo pipefail

HERE=$(cd "$(dirname "$0")" && pwd)
VM=${BENCH_VM:-bench-vm}
BASE=${BENCH_BASE_IMAGE:-}
LOCK_FILE=${BENCH_LOCK_FILE:-}
SSH_KEY=${BENCH_SSH_KEY:-$HOME/.ssh/id_ed25519}
VM_USER=${BENCH_VM_USER:-lume}
VM_DIR=/Users/$VM_USER/bench
CLIPS=$HERE/clips
WORK=$HERE/work
RESULTS=$HERE/results
UV=/Users/$VM_USER/.local/bin/uv
[ -z "$BASE" ] || [ "$VM" != "$BASE" ] || { echo "bench: BENCH_VM must not be the base image" >&2; exit 1; }

die() { echo "bench: $*" >&2; exit 1; }
log() { echo "bench: $*" >&2; }

vm_ip() { lume get "$VM" -f json 2>/dev/null | jq -r 'if type=="array" then .[0] else . end | .ipAddress // empty'; }
vm() { ssh -i "$SSH_KEY" -o HostKeyAlias="$VM" -o BatchMode=yes -o ConnectTimeout=5 "$VM_USER@$(vm_ip)" "$@"; }
known_host() {
  local known=$HOME/.ssh/known_hosts
  ssh-keygen -F "$VM" -f "$known" >/dev/null 2>&1 && return 0
  local from=${BENCH_HOSTKEY_FROM:-}
  [ -n "$from" ] || return 0   # first connection then asks; BatchMode fails, so add the key once by hand
  grep -q "^$from " "$known" || die "no known_hosts entry for $from to copy (clones keep their base's host key)"
  sed -n "s/^$from /$VM /p" "$known" >>"$known"
}
running_vms() { lume ls 2>/dev/null | awk '$0 ~ /running/ {print $1}'; }

# Host load, for the report: load averages and the free-memory share macOS reports.
host_load() {
  printf '%s load %s; memory free %s\n' "$(date -u +%FT%TZ)" \
    "$(sysctl -n vm.loadavg | tr -d '{}' | xargs)" \
    "$(memory_pressure 2>/dev/null | awk -F': ' '/free percentage/ {print $2}')"
}

# Runs a command in the VM's GUI session (Terminal has Accessibility; the ssh session has none), output to
# <log>, exit status to <log>.status; returns at once.
vm_terminal() {
  local cmd=$1 logf=$2
  vm "rm -f '$logf' '$logf.status' '$logf.status.tmp'"
  printf '#!/bin/zsh\n( %s ) >%s 2>&1\nst=$?\necho $st >%s.status.tmp && mv -f %s.status.tmp %s.status\n' \
    "$cmd" "$logf" "$logf" "$logf" "$logf" | vm "cat >'$logf.command' && chmod +x '$logf.command'"
  vm "open -a Terminal '$logf.command'"
}
vm_terminal_wait() {
  local logf=$1 every=${2:-30}
  until vm "test -f '$logf.status'" 2>/dev/null; do
    vm "tail -n 1 '$logf'" 2>/dev/null | cut -c1-160 || true
    sleep "$every"
  done
  vm "cat '$logf.status'"
}

cmd=${1:-}; shift || true
case "$cmd" in
  fetch)
    uv run --quiet "$HERE/fetch_clips.py" "$CLIPS" "$@"
    ;;
  lock)
    # A symlink whose target is "<pid> <time> <note>", held by a sleeper process so it outlives this
    # command (the format tests/e2e of the project this came from uses, so its runs see the VM as busy).
    [ -n "$LOCK_FILE" ] || die "BENCH_LOCK_FILE is not set"
    mkdir -p "$(dirname "$LOCK_FILE")"
    [ ! -L "$LOCK_FILE" ] || die "locked already: $(readlink "$LOCK_FILE")"
    nohup sleep 172800 >/dev/null 2>&1 &
    holder=$!
    ln -s "$holder $(date '+%F %T') bench ($VM)" "$LOCK_FILE" || { kill "$holder"; die "lost the race for $LOCK_FILE"; }
    echo "$holder" >"$LOCK_FILE.bench-holder"
    log "holding $LOCK_FILE (pid $holder)"
    ;;
  unlock)
    [ -n "$LOCK_FILE" ] || die "BENCH_LOCK_FILE is not set"
    holder=$(cat "$LOCK_FILE.bench-holder" 2>/dev/null || true)
    [ -n "$holder" ] || die "the bench holds no lock"
    if [ "$(readlink "$LOCK_FILE" 2>/dev/null | cut -d' ' -f1)" = "$holder" ]; then rm -f "$LOCK_FILE"; fi
    kill "$holder" 2>/dev/null || true
    rm -f "$LOCK_FILE.bench-holder"
    log "released $LOCK_FILE"
    ;;
  vm-create)
    [ -n "$BASE" ] || die "set BENCH_BASE_IMAGE to the VM to clone (README: the VM image)"
    lume ls | grep -q "^$VM " && die "$VM exists already"
    lume clone "$BASE" "$VM"
    lume set "$VM" --memory 8GB --cpu 4
    ;;
  vm-start)
    others=$(running_vms | grep -vx "$VM" || true)
    if [ "${1:-}" = --timed ] && [ -n "$others" ]; then
      die "timed runs need the only running VM; running: $(echo "$others" | xargs)"
    fi
    [ "$(echo "$others" | grep -c . || true)" -lt 2 ] || die "two other macOS VMs are running (Apple's limit)"
    lume ls | grep -q "^$VM " || die "no $VM: $0 vm-create"
    lume run "$VM" --no-display --detach >/dev/null 2>&1 || true
    for _ in $(seq 1 90); do [ -n "$(vm_ip)" ] && break; sleep 2; done
    known_host
    for _ in $(seq 1 60); do vm true 2>/dev/null && break; sleep 2; done
    vm "sw_vers -productVersion" && log "$VM is up at $(vm_ip)"
    ;;
  vm-stop)
    vm "osascript -e 'tell application \"System Events\" to shut down'" 2>/dev/null || lume stop "$VM"
    ;;
  vm-setup)
    [ -f "$CLIPS/manifest.csv" ] || die "no clips: $0 fetch"
    vm "test -x $UV || curl -LsSf https://astral.sh/uv/install.sh | sh" >/dev/null
    vm "mkdir -p $VM_DIR/out && rm -rf $VM_DIR/clips && mkdir -p $VM_DIR/clips"
    tar -C "$CLIPS" -cf - . | vm "tar -C $VM_DIR/clips -xf -"
    tar -C "$HERE" -cf - harness.py | vm "tar -C $VM_DIR -xf -"
    for f in ${BENCH_EXTRA_FILES:-}; do tar -C "$(dirname "$f")" -cf - "$(basename "$f")" | vm "tar -C $VM_DIR -xf -"; done
    # Mac-wide substitutions off, so nothing changes a typed transcript in TextEdit (TextEdit's own settings
    # live in its sandbox container, which an ssh session can't write; the document is a .txt, plain text).
    vm 'for k in NSAutomaticCapitalizationEnabled NSAutomaticPeriodSubstitutionEnabled NSAutomaticSpellingCorrectionEnabled NSAutomaticQuoteSubstitutionEnabled NSAutomaticDashSubstitutionEnabled NSAutomaticTextReplacementEnabled NSAutomaticInlinePredictionEnabled NSAutomaticTextCompletionEnabled WebAutomaticSpellingCorrectionEnabled; do defaults write -g $k -bool false; done
'
    # Warm uv's environment (and Python) once, then make BlackHole 2ch the default input.
    vm_terminal "cd $VM_DIR && $UV run harness.py dry-run && $UV run harness.py set-input 'BlackHole 2ch'" "$VM_DIR/setup.log"
    st=$(vm_terminal_wait "$VM_DIR/setup.log" 10)
    vm "cat $VM_DIR/setup.log | tail -5"
    [ "$st" = 0 ] || die "vm-setup failed ($st)"
    ;;
  install)
    src=${1:?App.app, App.dmg or App.zip}; dest=${2:-/Applications}
    name=$(basename "$src")
    case "$name" in
      *.app) tar -C "$(dirname "$src")" -cf - "$name" | vm "rm -rf '$dest/$name' && tar -C '$dest' -xf -" ;;
      *.zip) cat "$src" | vm "cd /tmp && rm -rf benchzip && mkdir benchzip && cat >benchzip/a.zip && ditto -x -k benchzip/a.zip benchzip && app=\$(ls -d benchzip/*.app | head -1) && rm -rf '$dest'/\$(basename \"\$app\") && ditto \"\$app\" '$dest'/\$(basename \"\$app\") && echo \$(basename \"\$app\")" ;;
      *.dmg) cat "$src" | vm "cat >/tmp/bench.dmg && mnt=\$(hdiutil attach -nobrowse -readonly /tmp/bench.dmg | awk -F'\t' '/\\/Volumes\\// {print \$NF}' | tail -1) && app=\$(ls -d \"\$mnt\"/*.app | head -1) && rm -rf '$dest'/\$(basename \"\$app\") && ditto \"\$app\" '$dest'/\$(basename \"\$app\") && hdiutil detach -quiet \"\$mnt\" && rm /tmp/bench.dmg && echo \$(basename \"\$app\")" ;;
      *) die "not an .app, .dmg or .zip: $src" ;;
    esac
    vm "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f '$dest'/*.app >/dev/null 2>&1; true"
    ;;
  check)
    app=${1:?app}; shift
    vm_terminal "cd $VM_DIR && $UV run harness.py run --app $app --clips $VM_DIR/clips -o $VM_DIR/out/check.csv --check $*" "$VM_DIR/check.log"
    st=$(vm_terminal_wait "$VM_DIR/check.log" 5)
    vm "cat $VM_DIR/check.log"
    [ "$st" = 0 ]
    ;;
  run)
    label=${1:?label}; app=${2:?app}; mode=${3:?mode}; shift 3
    out=$WORK/$label; mkdir -p "$out"
    tag=$(echo "$*" | grep -q -- '--limit' && echo trial || echo full)
    echo "start $(host_load)" >>"$out/host_load.txt"
    vm "echo start \$(date -u +%FT%TZ) load \$(sysctl -n vm.loadavg | tr -d '{}' | xargs)" >>"$out/vm_load.txt"
    vm_terminal "cd $VM_DIR && $UV run harness.py run --app $app --mode $mode --clips $VM_DIR/clips -o $VM_DIR/out/$label.csv $*" "$VM_DIR/out/$label.log"
    log "running $label ($tag) in $VM's Terminal"
    st=$(vm_terminal_wait "$VM_DIR/out/$label.log" 60)
    echo "end $(host_load)" >>"$out/host_load.txt"
    vm "echo end \$(date -u +%FT%TZ) load \$(sysctl -n vm.loadavg | tr -d '{}' | xargs)" >>"$out/vm_load.txt"
    vm "cat $VM_DIR/out/$label.log" >"$out/harness.log"
    vm "cat $VM_DIR/out/$label.csv" >"$out/results.csv" 2>/dev/null || true
    tail -n 2 "$out/harness.log"
    log "harness exit status $st; $out"
    [ "$st" = 0 ]
    ;;
  meta)
    label=${1:?label}; shift
    out=$WORK/$label; mkdir -p "$out"
    python3 -I - "$out/meta.json" "$@" <<'PY'
import json, pathlib, sys
p = pathlib.Path(sys.argv[1])
m = json.loads(p.read_text()) if p.exists() else {}
for kv in sys.argv[2:]:
    k, _, v = kv.partition("=")
    m[k] = v
p.write_text(json.dumps(m, indent=1, sort_keys=True) + "\n")
print(json.dumps(m, sort_keys=True))
PY
    ;;
  score)
    label=${1:?label}
    out=$WORK/$label
    uv run --quiet "$HERE/judge.py" --judge claude "$out/results.csv" "$CLIPS/manifest.csv" -o "$out/scored.csv" --label "$label"
    ;;
  publish)
    # Aggregates only: the summary table and, per run, how the app was set up and the load during the run.
    mkdir -p "$RESULTS"
    runs=()
    for run in "$WORK"/*/; do
      [ -f "$run/results.csv" ] || continue
      runs+=("$run")
      r=$(basename "$run"); mkdir -p "$RESULTS/$r"
      for f in meta.json host_load.txt vm_load.txt; do [ ! -f "$run/$f" ] || cp "$run/$f" "$RESULTS/$r/"; done
    done
    [ "${#runs[@]}" -gt 0 ] || die "no runs in $WORK"
    python3 "$HERE/harness.py" summarize "${runs[@]}" --format csv >"$RESULTS/summary.csv"
    python3 "$HERE/harness.py" summarize "${runs[@]}" --format md >"$RESULTS/summary.md"
    cat "$RESULTS/summary.md"
    ;;
  ssh)
    vm "$@"
    ;;
  screenshot)
    # Needs cua-driver (github.com/trycua) running in the VM; optional.
    vm "~/.local/bin/cua-driver call get_desktop_state '{}'" | jq -r '.screenshot_png_b64 // empty' | base64 -d >"${1:?png path}"
    ;;
  *)
    sed -n '2,23p' "$0"; exit 2 ;;
esac
