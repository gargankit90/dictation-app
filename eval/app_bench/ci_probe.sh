#!/usr/bin/env bash
# Can a GitHub-hosted macOS runner run this bench instead of a local VM? Writes a plain-text report
# (default probe.txt); never fails on a finding. No keys, no dataset audio or text: the end-to-end clip is
# a sentence spoken by macOS's own `say`.
#
#   ./ci_probe.sh [report.txt]
#
# 1 GUI session (console user, WindowServer, Dock; TextEdit through Accessibility)
# 2 SIP, the TCC databases, and whether Accessibility / Microphone can be granted without a person
# 3 BlackHole 2ch installed with Homebrew, then a loopback: play into it, record from it
# 4 CPU, memory, GPU and Metal
# 5 one app end to end (Handy 0.10.1, Whisper Small, push-to-talk) on one `say` clip
set -uo pipefail
cd "$(dirname "$0")"
OUT=$(cd "$(dirname "${1:-probe.txt}")" && pwd)/$(basename "${1:-probe.txt}")
: >"$OUT"
SENTENCE="Please send the quarterly report to the finance team before Friday afternoon."
HANDY_DMG=https://github.com/cjpais/Handy/releases/download/v0.10.1/Handy_0.10.1_aarch64.dmg
HANDY_SHA=7cdef5de7d1bfcea70813320b897540cbead2190f5280db6d9760a59e5995463
SMALL_URL=https://blob.handy.computer/ggml-small.bin
SMALL_SHA=1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b
HANDY_DIR="$HOME/Library/Application Support/com.pais.handy"
USER_TCC="$HOME/Library/Application Support/com.apple.TCC/TCC.db"
SYS_TCC="/Library/Application Support/com.apple.TCC/TCC.db"

r() { echo "$*" | tee -a "$OUT"; }
section() { r ""; r "## $*"; }
# run <seconds> <cmd...>: output into the report, cut off after <seconds>.
run() {
  local limit=$1; shift
  perl -e 'alarm shift; exec @ARGV' "$limit" "$@" 2>&1 | sed 's/^/    /' | tee -a "$OUT"
  return "${PIPESTATUS[0]}"
}
tcc_grant() {  # tcc_grant <db> <service> <bundle id>: one allowed row, the way System Settings writes it
  sudo sqlite3 "$1" "INSERT OR REPLACE INTO access (service, client, client_type, auth_value, auth_reason, auth_version, indirect_object_identifier, flags, last_modified) VALUES ('$2', '$3', 0, 2, 3, 1, 'UNUSED', 0, CAST(strftime('%s','now') AS INTEGER));"
}

section "4 Machine"
run 20 sw_vers
run 10 sysctl -n machdep.cpu.brand_string hw.ncpu hw.memsize
run 60 system_profiler SPHardwareDataType SPDisplaysDataType

section "1 GUI session"
r "console user: $(stat -f %Su /dev/console)  runner user: $(id -un)"
run 5 pgrep -lx WindowServer
run 5 pgrep -lx Dock
run 5 pgrep -lx loginwindow
r "brew: $(command -v brew)  uv: $(command -v uv || echo none)"
command -v uv >/dev/null || run 300 brew install uv
mkdir -p "$HOME/bench"

section "2 SIP and TCC"
run 10 csrutil status
r "user TCC.db readable: $(sqlite3 "$USER_TCC" 'select count(*) from access' 2>&1)"
r "system TCC.db readable (sudo): $(sudo sqlite3 "$SYS_TCC" 'select count(*) from access' 2>&1)"
r "Accessibility clients already allowed (system db):"
run 10 sudo sqlite3 "$SYS_TCC" "select client from access where service='kTCCServiceAccessibility' and auth_value=2"
r "Microphone clients already allowed (user db):"
run 10 sqlite3 "$USER_TCC" "select client from access where service='kTCCServiceMicrophone' and auth_value=2"
r "grant Accessibility to com.pais.handy (system db): $(tcc_grant "$SYS_TCC" kTCCServiceAccessibility com.pais.handy 2>&1 && echo inserted)"
r "grant Microphone to com.pais.handy (user db): $(tcc_grant "$USER_TCC" kTCCServiceMicrophone com.pais.handy 2>&1 && echo inserted)"
r "grant Microphone to com.pais.handy (system db): $(tcc_grant "$SYS_TCC" kTCCServiceMicrophone com.pais.handy 2>&1 && echo inserted)"
r "tccutil exists: $(command -v tccutil) (it can only reset, not grant)"

section "1b TextEdit through Accessibility (this process)"
# What bench.sh vm-setup does: no Mac-wide automatic corrections.
for k in NSAutomaticCapitalizationEnabled NSAutomaticPeriodSubstitutionEnabled NSAutomaticSpellingCorrectionEnabled \
  NSAutomaticQuoteSubstitutionEnabled NSAutomaticDashSubstitutionEnabled NSAutomaticTextReplacementEnabled \
  NSAutomaticInlinePredictionEnabled NSAutomaticTextCompletionEnabled; do defaults write -g "$k" -bool false; done
run 120 uv run --quiet ci_probe.py ax

section "3 Virtual audio device"
run 600 brew install --cask blackhole-2ch
run 30 sudo killall coreaudiod
sleep 5
run 30 say -o "$HOME/bench/probe.aiff" "$SENTENCE"
run 30 afconvert -f WAVE -d LEI16@16000 "$HOME/bench/probe.aiff" "$HOME/bench/probe.wav"
run 120 uv run --quiet ci_probe.py audio --wav "$HOME/bench/probe.wav"
r "system output stays: $(system_profiler SPAudioDataType 2>/dev/null | awk '/Default Output Device: Yes/{print prev} {prev=$0}' | xargs)"

section "4b Metal"
run 120 uv run --quiet ci_probe.py metal

section "5 Handy end to end"
(
  set -e
  curl -fsSL -o /tmp/handy.dmg "$HANDY_DMG"
  echo "$HANDY_SHA  /tmp/handy.dmg" | shasum -a 256 -c -
  mnt=$(hdiutil attach -nobrowse -readonly /tmp/handy.dmg | awk -F'\t' '/\/Volumes\// {print $NF}' | tail -1)
  ditto "$mnt/Handy.app" /Applications/Handy.app
  hdiutil detach -quiet "$mnt"
  mkdir -p "$HANDY_DIR/models"
  curl -fsSL -o "$HANDY_DIR/models/ggml-small.bin" "$SMALL_URL"
  echo "$SMALL_SHA  $HANDY_DIR/models/ggml-small.bin" | shasum -a 256 -c -
  # Choosing a model completes Handy's onboarding (settings.rs); push-to-talk with its default key.
  printf '{"settings": {"selected_model": "small", "push_to_talk": true, "selected_language": "en"}}\n' \
    >"$HANDY_DIR/settings_store.json"
) 2>&1 | sed 's/^/    /' | tee -a "$OUT"
open -g /Applications/Handy.app; sleep 15
run 5 pgrep -lx handy
run 5 pgrep -lix handy
run 400 uv run --quiet ci_probe.py e2e --app handy --wav "$HOME/bench/probe.wav" --text "$SENTENCE"
r "Handy log tail:"
run 5 sh -c "tail -n 25 \"\$HOME/Library/Logs/com.pais.handy/\"*.log 2>/dev/null | cut -c1-200"

section "done"
r "report: $OUT"
exit 0
