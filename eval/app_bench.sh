#!/usr/bin/env bash
# BENCH1: the 151 Voice Arena clips through Vocet and other Mac dictation apps, in a Lume VM of its own.
# Host side; eval/app_bench.py runs in the VM's Terminal. See eval/reports/bench1.md for the order.
#
#   eval/app_bench.sh fetch                     # host: the 151 clips into eval/clips/ (git-ignored)
#   eval/app_bench.sh lock | unlock             # hold / release dictation-vm-2's e2e lock for the bench
#   eval/app_bench.sh vm-create                 # clone dictation-vm-golden -> dictation-vm-bench (once)
#   eval/app_bench.sh vm-start [--timed] | vm-stop
#   eval/app_bench.sh vm-setup                  # uv, the harness, the clips, BlackHole as input, no autocorrect
#   eval/app_bench.sh install <App.app|App.dmg|App.zip> [/Applications]   # host -> VM
#   eval/app_bench.sh run <label> <app> <mode> [harness args...]          # VM, timed; results back to the host
#   eval/app_bench.sh check <app> [harness args...]                       # harness preflight in the VM
#   eval/app_bench.sh meta <label> key=value...  # how the app was set up (meta.json, aggregates only)
#   eval/app_bench.sh score <label>             # eval/judge.py (Claude) -> scored.csv
#   eval/app_bench.sh summary                   # one row per run (eval/app_bench.py summarize)
#   eval/app_bench.sh ssh <cmd> | cua <tool> '<json>' | screenshot <png>
#
# Runs go to eval/work/bench1/<label>/ (git-ignored: transcripts of CC BY-NC clips). Only the VM named
# dictation-vm-bench is ever used: never the golden image, never dictation-vm or dictation-vm-2 themselves.
# The bench VM stands in for dictation-vm-2 while it holds that VM's e2e lock (the Lead, 2026-11-21), so a
# Tester's run says "VM busy" instead of resetting a VM the bench is using.
set -euo pipefail

HERE=$(cd "$(dirname "$0")" && pwd)
VM=dictation-vm-bench
GOLDEN=dictation-vm-golden
LOCKED_VM=dictation-vm-2
LOCK_DIR=${E2E_LOCK_DIR:-$HOME/.cache/dictation-e2e}
SSH_KEY=${SSH_KEY:-$HOME/.ssh/dictation_vm_ed25519}
VM_USER=lume
VM_DIR=/Users/$VM_USER/bench
CLIPS=$HERE/clips
WORK=$HERE/work/bench1
UV=/Users/$VM_USER/.local/bin/uv

die() { echo "app_bench: $*" >&2; exit 1; }
log() { echo "app_bench: $*" >&2; }

vm_ip() { lume get "$VM" -f json 2>/dev/null | jq -r 'if type=="array" then .[0] else . end | .ipAddress // empty'; }
vm() { ssh -i "$SSH_KEY" -o HostKeyAlias="$VM" -o BatchMode=yes -o ConnectTimeout=5 "$VM_USER@$(vm_ip)" "$@"; }
known_host() {
  local known=$HOME/.ssh/known_hosts
  ssh-keygen -F "$VM" -f "$known" >/dev/null 2>&1 && return 0
  grep -q '^dictation-vm ' "$known" || die "no host key for dictation-vm to copy (clones keep the golden key)"
  sed -n 's/^dictation-vm /'"$VM"' /p' "$known" >>"$known"
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
  printf '#!/bin/zsh\n( %s ) >%s 2>&1\nst=$?\necho $st >%s.tmp && mv -f %s.tmp %s.status\n' \
    "$cmd" "$logf" "$logf.status" "$logf.status" "$logf.status" | vm "cat >'$logf.command' && chmod +x '$logf.command'"
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
    uv run --quiet "$HERE/fetch_dataset.py" "$CLIPS" "$@"
    ;;
  lock)
    # The same lock tests/e2e/run.sh takes (a symlink whose target is "<pid> <time> <note>"), held by a
    # sleeper process so it outlives this command; `unlock` ends the sleeper and removes the lock.
    mkdir -p "$LOCK_DIR"
    lock=$LOCK_DIR/$LOCKED_VM.lock
    if [ -L "$lock" ]; then
      die "$LOCKED_VM is locked already: $(readlink "$lock")"
    fi
    nohup sleep 172800 >/dev/null 2>&1 &
    holder=$!
    ln -s "$holder $(date '+%F %T') BENCH1 app bench ($VM)" "$lock" || { kill "$holder"; die "lost the race for $lock"; }
    echo "$holder" >"$LOCK_DIR/bench-holder.pid"
    log "holding $LOCKED_VM's lock (pid $holder)"
    ;;
  unlock)
    lock=$LOCK_DIR/$LOCKED_VM.lock
    holder=$(cat "$LOCK_DIR/bench-holder.pid" 2>/dev/null || true)
    [ -n "$holder" ] || die "the bench holds no lock"
    if [ "$(readlink "$lock" 2>/dev/null | cut -d' ' -f1)" = "$holder" ]; then rm -f "$lock"; fi
    kill "$holder" 2>/dev/null || true
    rm -f "$LOCK_DIR/bench-holder.pid"
    log "released $LOCKED_VM's lock"
    ;;
  vm-create)
    lume ls | grep -q "^$VM " && die "$VM exists already"
    lume clone "$GOLDEN" "$VM"
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
    tar -C "$HERE" -cf - app_bench.py | vm "tar -C $VM_DIR -xf -"
    # Mac-wide and TextEdit substitutions off, so nothing changes a transcript on its way into TextEdit.
    vm 'for k in NSAutomaticCapitalizationEnabled NSAutomaticPeriodSubstitutionEnabled NSAutomaticSpellingCorrectionEnabled NSAutomaticQuoteSubstitutionEnabled NSAutomaticDashSubstitutionEnabled NSAutomaticTextReplacementEnabled NSAutomaticInlinePredictionEnabled NSAutomaticTextCompletionEnabled WebAutomaticSpellingCorrectionEnabled; do defaults write -g $k -bool false; done
        for k in SmartQuotes SmartDashes TextReplacement SmartLinks DataDetectors CheckSpellingWhileTyping CheckGrammarWithSpelling CorrectSpellingAutomatically SmartCopyPaste; do defaults write com.apple.TextEdit $k -bool false; done
        defaults write com.apple.TextEdit RichText -int 0'
    # Warm uv's environment (and Python) once, then make BlackHole 2ch the default input.
    vm_terminal "cd $VM_DIR && $UV run app_bench.py dry-run && $UV run app_bench.py set-input 'BlackHole 2ch'" "$VM_DIR/setup.log"
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
    vm_terminal "cd $VM_DIR && $UV run app_bench.py run --app $app --clips $VM_DIR/clips -o $VM_DIR/out/check.csv --check $*" "$VM_DIR/check.log"
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
    vm_terminal "cd $VM_DIR && $UV run app_bench.py run --app $app --mode $mode --clips $VM_DIR/clips -o $VM_DIR/out/$label.csv $*" "$VM_DIR/out/$label.log"
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
  summary)
    python3 "$HERE/app_bench.py" summarize "$WORK"/*/ "$@"
    ;;
  ssh)
    vm "$@"
    ;;
  cua)
    tool=${1:?tool}; args=${2:-'{}'}
    vm "~/.local/bin/cua-driver call $tool '$args'"
    ;;
  screenshot)
    vm "~/.local/bin/cua-driver call get_desktop_state '{}'" | jq -r '.screenshot_png_b64 // empty' | base64 -d >"${1:?png path}"
    ;;
  *)
    sed -n '2,17p' "$0"; exit 2 ;;
esac
