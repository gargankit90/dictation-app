#!/usr/bin/env bash
# Voice Arena Dictation Bench (public set) through Vocet, inside the Lume VM dictation-vm-eval.
# One command per step and per engine; see README.md for the order and the one-time VM setup.
#
#   eval/voicearena/bench.sh fetch                    # host: download the 151 clips (git-ignored)
#   eval/voicearena/bench.sh start                    # host: start dictation-vm-eval (a clone of the golden image)
#   eval/voicearena/bench.sh push-harness             # host -> VM: harness + Vocet driver + clips
#   eval/voicearena/bench.sh install-app [Vocet.app]  # host -> VM: the app (default: this checkout's debug build)
#   eval/voicearena/bench.sh run <engine> [harness args...]   # VM: run the harness in Terminal, wait, copy results back
#   eval/voicearena/bench.sh score <engine>           # host: judge + score -> results/<engine>/scored.csv
#   eval/voicearena/bench.sh score-codex <engine>     # host: second judge (Codex) -> results/<engine>/scored_codex.csv
#   eval/voicearena/bench.sh report <engine>...       # host: summary.json + tables for REPORT.md
#   eval/voicearena/bench.sh cloud-sources <engine>   # Vocet Cloud only: rewrite vs verbatim per clip
#
# BENCH_VM=<name> picks the VM (default dictation-vm-eval; dictation-vm-eval-2 for a second run at once).
# <engine> is a label such as vocet_groq or vocet_assemblyai. Everything else (clips, harness timing,
# judge, prompt version) is fixed, so two engines' results are directly comparable.
# Never point this at dictation-vm, dictation-vm-2 or the golden image: the name is checked.
set -euo pipefail

HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../.." && pwd)
VM=${BENCH_VM:-dictation-vm-eval}
SSH_KEY=${SSH_KEY:-$HOME/.ssh/dictation_vm_ed25519}
VM_USER=lume
VM_DIR=/Users/$VM_USER/va
HARNESS_REPO=https://github.com/VoiceArenaAI/Dictation-harness
HARNESS_SHA=3c3329e046e7ea79f95bc14adccfe08eab6c9a58   # the harness commit this run used
CLIPS=$HERE/clips

die() { echo "bench: $*" >&2; exit 1; }
case "$VM" in
  dictation-vm-eval|dictation-vm-eval-*) ;;
  *) die "refusing VM '$VM': only dictation-vm-eval (never the golden image or the shared test VMs)" ;;
esac

vm_ip() { lume get "$VM" -f json 2>/dev/null | jq -r 'if type=="array" then .[0] else . end | .ipAddress // empty'; }
known_host() {
  local known=$HOME/.ssh/known_hosts
  ssh-keygen -F "$VM" -f "$known" >/dev/null 2>&1 && return 0
  grep -q '^dictation-vm ' "$known" || die "no host key for dictation-vm to copy"
  sed -n 's/^dictation-vm /'"$VM"' /p' "$known" >>"$known"
}
vm() { ssh -i "$SSH_KEY" -o HostKeyAlias="$VM" -o BatchMode=yes -o ConnectTimeout=5 "$VM_USER@$(vm_ip)" "$@"; }
# Runs a command in the VM's GUI session through Terminal.app (Accessibility, CoreAudio and the
# window server are only there), with its output in <log> and its exit status in <log>.status.
vm_terminal() {
  local cmd=$1 log=$2
  vm "rm -f '$log' '$log.status'"
  # A .command file opened with Terminal, as tests/e2e/lib_full.sh does (Terminal won't take
  # AppleEvents from an ssh session). The status goes through a temp file and a rename.
  printf '#!/bin/zsh\n( %s ) >%s 2>&1\nst=$?\necho $st >%s.tmp && mv -f %s.tmp %s.status\n' \
    "$cmd" "$log" "$log" "$log" "$log" | vm "cat >'$log.command' && chmod +x '$log.command'"
  vm "open -a Terminal '$log.command'"
}

cmd=${1:-}; shift || true
case "$cmd" in
  fetch)
    uv run --quiet "$HERE/fetch_dataset.py" "$CLIPS" "$@"
    ;;
  start)
    [ "$(lume ls 2>/dev/null | awk '$7=="running"' | grep -vc "^$VM ")" -lt 2 ] ||
      die "two other macOS VMs are running (Apple's limit); stop one first"
    lume ls | grep -q "^$VM " || die "$VM doesn't exist: lume clone dictation-vm-golden $VM && lume set $VM --memory 6GB --cpu 4"
    lume run "$VM" --detach >/dev/null 2>&1 || true
    for _ in $(seq 1 90); do [ -n "$(vm_ip)" ] && break; sleep 2; done
    known_host
    for _ in $(seq 1 60); do vm true 2>/dev/null && break; sleep 2; done
    vm "sw_vers -productVersion" && echo "$VM is up at $(vm_ip)"
    ;;
  push-harness)
    [ -f "$CLIPS/manifest.csv" ] || die "no clips: run '$0 fetch' first"
    # The VM has no git (no developer tools), so the pinned harness is cloned on the host and copied.
    h=$HERE/work/harness
    [ -d "$h/.git" ] || git clone -q "$HARNESS_REPO" "$h"
    git -C "$h" fetch -q origin && git -C "$h" -c advice.detachedHead=false checkout -q "$HARNESS_SHA"
    vm "rm -rf $VM_DIR/harness && mkdir -p $VM_DIR/harness"
    git -C "$h" archive "$HARNESS_SHA" | vm "tar -C $VM_DIR/harness -xf -"
    tar -C "$HERE" -cf - run_vocet_dictation.py | vm "tar -C $VM_DIR/harness -xf -"
    tar -C "$CLIPS" -cf - . | vm "rm -rf $VM_DIR/clips && mkdir -p $VM_DIR/clips && tar -C $VM_DIR/clips -xf -"
    vm "echo $HARNESS_SHA > $VM_DIR/harness/HARNESS_SHA; ls $VM_DIR/clips | wc -l; ls $VM_DIR/harness"
    ;;
  install-app)
    app=${1:-$REPO/target/debug/bundle/macos/Vocet.app}
    [ -d "$app" ] || die "no app at $app (npm run tauri build -- --debug --bundles app)"
    vm "pkill -x Vocet; mkdir -p ~/Applications && rm -rf ~/Applications/Vocet.app ~/Applications/dictation-app.app"
    tar -C "$(dirname "$app")" -cf - "$(basename "$app")" | vm "tar -C ~/Applications -xf -"
    vm "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f ~/Applications/Vocet.app; codesign --verify --deep ~/Applications/Vocet.app && echo installed"
    ;;
  run)
    engine=${1:?engine label}; shift
    out=$HERE/results/$engine; mkdir -p "$out"
    log=$VM_DIR/run_$engine.log
    # Same timing as every tool in the whitepaper: the harness defaults (pad 300 ms, start delay 0.5 s,
    # tail 4 s, settle 2 s, timeout 30 s); they are spelled out so a changed default can't slip in.
    args="--pad-ms 300 --start-delay 0.5 --tail 4 --settle 2 --timeout 30 $*"
    vm "open -g ~/Applications/Vocet.app" || true
    sleep 3
    vm_terminal "cd $VM_DIR/harness && caffeinate -dimsu /Users/lume/.local/bin/uv run run_vocet_dictation.py ../clips -o $VM_DIR/results_$engine.csv $args" "$log"
    echo "running in $VM's Terminal; log: $log"
    until vm "test -f $log.status" 2>/dev/null; do vm "tail -n 1 $log" 2>/dev/null | cut -c1-160 || true; sleep 30; done
    status=$(vm "cat $log.status")
    vm "cat $log" >"$out/harness.log"
    vm "test -f $VM_DIR/results_$engine.csv" && vm "cat $VM_DIR/results_$engine.csv" >"$out/results.csv" || true
    [ -f "$out/results.csv" ] && python3 -I "$HERE/strip_results.py" "$out/results.csv" "$out/harness_results.csv"
    echo "harness exit status $status; results in $out"
    [ "$status" = 0 ]
    ;;
  score)
    engine=${1:?engine label}
    out=$HERE/results/$engine
    uv run --quiet "$HERE/score_claude_cli.py" "$out/results.csv" "$CLIPS/manifest.csv" -o "$out/scored.csv" --label "$engine"
    ;;
  score-codex)
    engine=${1:?engine label}
    out=$HERE/results/$engine
    src=$out/results.csv
    [ -f "$src" ] || src=$out/harness_results.csv
    uv run --quiet "$HERE/score_codex_cli.py" "$src" "$CLIPS/manifest.csv" -o "$out/scored_codex.csv" --label "$engine"
    ;;
  report)
    uv run --quiet "$HERE/report.py" "$@"
    ;;
  check)
    # The harness's preflight checks, in the VM's GUI session.
    log=$VM_DIR/check.log
    vm "open -g ~/Applications/Vocet.app" || true
    vm_terminal "cd $VM_DIR/harness && /Users/lume/.local/bin/uv run run_vocet_dictation.py ../clips --check $*" "$log"
    for _ in $(seq 1 60); do vm "test -f $log.status" && break; sleep 5; done
    vm "tail -n 30 $log; echo exit=\$(cat $log.status)"
    ;;
  cloud-sources)
    # Vocet Cloud (AssemblyAI) only: rewrite vs verbatim fallback per clip, from Vocet's log
    # (those lines carry no text).
    engine=${1:?engine label}
    out=$HERE/results/$engine
    vm "grep -h 'assemblyai text' ~/Library/Logs/com.dictationapp.desktop/dictation-app.log*" >"$out/vocet_cloud_log_lines.txt"
    python3 -I "$HERE/vocet_cloud_sources.py" "$out/vocet_cloud_log_lines.txt" "$out/results.csv" "$out/vocet_cloud_sources.csv"
    ;;
  ssh)
    vm "$@"
    ;;
  screenshot)
    vm "~/.local/bin/cua-driver call get_desktop_state '{}'" | jq -r '.screenshot_png_b64 // empty' | base64 -d >"${1:?png path}"
    ;;
  *)
    sed -n '2,16p' "$0"; exit 2 ;;
esac
