# Helpers for the scenarios on the local provider (T4a): the app's real
# engine, keychain key, HTTP client and JSON parsing, talking to
# fixtures/local-provider.pl on the VM's loopback instead of Groq. Sourced by
# run.sh with the scenarios (every scenarios/*.sh), but defines no scenario.
#
# The test build is pointed at the provider with --debug-provider-url (debug
# builds only, loopback only; a release build ignores it). The provider is
# controlled through files in VM_PROVIDER_DIR (see local-provider.pl): the
# hash of the one key it accepts, a gate that holds replies, the reply text,
# and an events file that says what it saw (requests, their key's verdict,
# held replies, a client hanging up). The keys are made up: lowercase letters
# and digits, so ax.js can type them into Settings like a person.

PROVIDER_PORT=${E2E_PROVIDER_PORT:-18731}
PROVIDER_URL="http://127.0.0.1:$PROVIDER_PORT/v1"
VM_PROVIDER_DIR="/Users/$VM_USER/e2e-provider"
VM_PROVIDER="$VM_FIXTURES/local-provider.pl"
VM_KEYS="$VM_FIXTURES/keys.js"
# Made up, never real; the provider accepts the good one only.
PROVIDER_GOOD_KEY=t4agoodkey7f3a9c2e
PROVIDER_BAD_KEY=t4abadkey51d0e8b4
# What the provider answers unless told otherwise (local-provider.pl).
PROVIDER_TEXT="hello world"
# On the clipboard before a dictation; must be there again after it.
PROVIDER_SENTINEL="t4a clipboard sentinel"

# provider_start <accepted key>: a fresh provider (no events, no gate, the
# default reply) that accepts <key>. Only the key's SHA-256 is written in the
# VM. Waits (up to 10 s) until it answers.
provider_start() {
  local i
  provider_stop
  vm "rm -rf $(q "$VM_PROVIDER_DIR") && mkdir -p $(q "$VM_PROVIDER_DIR")"
  printf '%s' "$1" | vm "shasum -a 256 | cut -d' ' -f1 >$(q "$VM_PROVIDER_DIR/accept-key.sha256")"
  vm "nohup perl $(q "$VM_PROVIDER" "$VM_PROVIDER_DIR" "$PROVIDER_PORT") >$(q "$VM_PROVIDER_DIR/server.log") 2>&1 </dev/null &"
  for i in $(seq 1 20); do
    if [ "$(vm "curl -s -m 2 http://127.0.0.1:$PROVIDER_PORT/health" 2>/dev/null)" = '"ok"' ]; then
      log "  the local provider is up on port $PROVIDER_PORT"
      return 0
    fi
    sleep 0.5
  done
  log "  the local provider didn't answer: $(vm "cat $(q "$VM_PROVIDER_DIR/server.log")" 2>&1)"
  return 1
}

# Stops the provider and any reply it still holds. True once none runs.
provider_stop() {
  vm "pkill -f '[l]ocal-provider.pl'; i=0; while pgrep -f '[l]ocal-provider.pl' >/dev/null && [ \$i -lt 20 ]; do sleep 0.25; i=\$((i + 1)); done; ! pgrep -f '[l]ocal-provider.pl' >/dev/null"
}

# Holds every reply from now on, once its request has been read whole.
provider_gate_close() {
  vm "touch $(q "$VM_PROVIDER_DIR/gate")"
}

# Lets every held reply go.
provider_release() {
  vm "rm -f $(q "$VM_PROVIDER_DIR/gate")"
}

# provider_reply <base64 of UTF-8 text>: what the next replies answer with,
# byte for byte.
provider_reply() {
  printf '%s' "$1" | vm "base64 -D >$(q "$VM_PROVIDER_DIR/reply-text")"
}

# The provider's events so far (local-provider.pl). Fails if they can't be
# read (the provider writes its first line when it starts), so a broken
# read is never taken for "no requests".
provider_events() {
  vm "cat $(q "$VM_PROVIDER_DIR/events")"
}

# How many requests the provider has had; fails if its events can't be read.
provider_requests() {
  local events
  events=$(provider_events) || return 1
  printf '%s\n' "$events" | grep -c '^request ' || true
}

# provider_requests_are <n>: exactly <n> requests so far.
provider_requests_are() {
  local got
  if ! got=$(provider_requests); then
    log "  could not read the provider's events"
    return 1
  fi
  [ "$got" = "$1" ] && return 0
  log "  the provider had $got requests, not $1: $(provider_events | paste -sd '|' -)"
  return 1
}

# provider_wait_event <extended regex> [seconds]: until an event line
# matches (default 15 s).
provider_wait_event() {
  local pattern=$1 timeout=${2:-15} i
  for i in $(seq 1 $((timeout * 2))); do
    provider_events | grep -Eq -- "$pattern" && return 0
    sleep 0.5
  done
  log "  no provider event matching [$pattern]: $(provider_events | paste -sd '|' -)"
  return 1
}

# provider_request_has <n> <text>: request <n>'s line has <text>
# (auth=good, language=de, ...).
provider_request_has() {
  local line
  line=$(provider_events | grep -E "^request n=$1 " || true)
  case " $line " in
    *" $2 "*) return 0 ;;
  esac
  log "  request $1 isn't [$2]: [${line:-none}]"
  return 1
}

# provider_key_store <key>: saves <key> as the Groq key in the VM's
# keychain, through the app (_groq_helpers.sh), so the app reads it with no
# prompt. provider_cleanup removes it.
provider_key_store() {
  groq_key_store "$1"
}

# The first launch of a scenario on the local provider: the real engine
# (--transcriber settings) pointed at it, recording the tone. Waits until the
# engine has been built from the saved key.
launch_on_provider() {
  E2E_TRANSCRIBER=settings launch --debug-audio-file "$VM_TONE" \
    --debug-provider-url "$PROVIDER_URL" "$@"
  wait_for_log 'test hook: the provider is reached at a local URL' &&
    wait_for_log 'engine rebuilt'
}

# The cleanup hook (KEY_CLEANUP_HOOK) of every provider scenario, run once the
# app is gone, however the scenario ended: no reply held, the provider
# stopped, the key out of the keychain, the default settings. A step that
# fails fails the cleanup.
provider_cleanup() {
  local ok=0
  provider_release || ok=1
  provider_stop || {
    log "  cleanup: the local provider is still running"
    ok=1
  }
  if [ "$GROQ_KEY_STORED" = 1 ] && ! groq_key_remove; then
    ok=1
  fi
  settings_reset || ok=1
  return "$ok"
}

# The pid of the one running copy of the app.
app_pid() {
  vm "pgrep -x $(q "$APP_NAME")" | tr -d ' '
}

# The phases logged after the first <n>, space-separated.
phases_since() {
  logged_phases | tail -n +$(($1 + 1)) | paste -sd ' ' -
}

# How many phases are logged so far (a mark for phases_since).
phase_mark() {
  logged_phases | wc -l | tr -d ' '
}

# --- Exact text, as base64 of its UTF-8 ------------------------------------------
#
# Every comparison is of the exact bytes, encoded, so no whitespace or
# newline is lost on the way back (a $(...) drops trailing newlines). Texts
# are written as ASCII with \n, \t and \u{hex} escapes.

# esc_b64 <escaped text>: the base64 of its UTF-8.
esc_b64() {
  perl -CS -e 'my $s = shift; $s =~ s/\\u\{([0-9a-fA-F]+)\}/chr(hex $1)/ge; $s =~ s/\\n/\n/g; $s =~ s/\\t/\t/g; print $s' -- "$1" |
    base64 | tr -d '\n'
}

# esc_chars <escaped text>: how many characters (code points) it has.
esc_chars() {
  perl -CS -e 'my $s = shift; $s =~ s/\\u\{([0-9a-fA-F]+)\}/chr(hex $1)/ge; $s =~ s/\\n/\n/g; $s =~ s/\\t/\t/g; print length $s' -- "$1"
}

# b64_shown <base64>: the text, escaped back into one printable line, for a
# log line.
b64_shown() {
  printf '%s' "$1" | base64 --decode | perl -CSD -pe 's/([^\x20-\x7e])/sprintf("\\u{%x}", ord $1)/ge'
}

# Runs JavaScript (JXA) in the VM: the ssh session may control TextEdit.
vm_jxa() {
  printf '%s\n' "$1" | vm "osascript -l JavaScript -"
}

# textedit_b64 [document name]: the document's whole text (default: the
# front one), as base64 of its UTF-8.
textedit_b64() {
  local doc='te.documents[0]'
  [ $# -eq 0 ] || doc="te.documents.byName($(printf '%s' "$1" | jq -Rs .))"
  vm_jxa "ObjC.import('Foundation');
    const te = Application('TextEdit');
    const t = $doc.text();
    \$(t).dataUsingEncoding(\$.NSUTF8StringEncoding).base64EncodedStringWithOptions(0).js"
}

# textedit_set_b64 <base64 of UTF-8>: the front document's text becomes it.
# Not named textedit_set: e2e_hotkey_shortcuts.sh defines that name for plain
# text, and run.sh sources every scenario file into one shell, so the later
# definition silently won and this scenario seeded base64 (#185).
textedit_set_b64() {
  vm_jxa "ObjC.import('Foundation');
    const data = \$.NSData.alloc.initWithBase64EncodedStringOptions('$1', 0);
    Application('TextEdit').documents[0].text = \$.NSString.alloc.initWithDataEncoding(data, \$.NSUTF8StringEncoding).js;
    'set'" >/dev/null
}

# textedit_seeded <base64>: the front document holds exactly that text, the
# setup of a scenario. If not, says "setup: document not as expected", so a
# failed seeding is never mistaken for a paste failure.
textedit_seeded() {
  textedit_is "$1" || {
    log "  setup: document not as expected"
    return 1
  }
}

# textedit_is <base64> [document name]: the document holds exactly that text.
textedit_is() {
  local want=$1 got
  shift
  got=$(textedit_b64 "$@") || {
    log "  could not read TextEdit's text"
    return 1
  }
  [ "$got" = "$want" ] && return 0
  log "  TextEdit holds [$(b64_shown "$got")]"
  log "  expected       [$(b64_shown "$want")]"
  return 1
}

# keys <key>...: real key presses (fixtures/keys.js) into TextEdit, from
# Terminal (Accessibility is Terminal's), which first puts TextEdit back in
# front. Prints "pressed <n>".
keys() {
  local args=() arg
  for arg in "$@"; do args+=("$(q "$arg")"); done
  vm_terminal "open -a TextEdit; sleep 1; osascript -l JavaScript $(q "$VM_KEYS") ${args[*]}"
}

# Puts TextEdit (its front document) back in front, activated twice as in
# textedit_fresh so its window is key; checks it (up to 10 s).
textedit_front() {
  local i
  for i in $(seq 1 10); do
    vm_osascript 'tell application "TextEdit" to activate' >/dev/null || true
    sleep 0.5
    if [ "$(frontmost_app)" = TextEdit ]; then
      vm_osascript 'tell application "TextEdit" to activate' >/dev/null
      sleep 1
      return 0
    fi
  done
  log "  TextEdit didn't come to the front: [$(frontmost_app)] is"
  return 1
}

# The frontmost app's name.
frontmost_app() {
  vm_osascript 'tell application "System Events" to get name of first process whose frontmost is true'
}
