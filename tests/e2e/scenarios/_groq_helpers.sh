# Helpers shared by e2e_real_groq and e2e_error_bad_key (M4.4). Sourced by
# run.sh with the scenarios (every scenarios/*.sh), but defines no scenario.
#
# The app reads its API key from the login keychain, so a test puts one
# there, the way the Settings window would, and removes it again. The key
# travels to the VM on the stdin of `security -i`, so it never appears in a
# process list on the host or in the VM, or in any file or log. It stays in
# the VM's keychain only while a scenario runs (and the VM is thrown away at
# the next reset anyway).

GROQ_KEYCHAIN_SERVICE=com.dictationapp.desktop
GROQ_KEYCHAIN_ACCOUNT=groq-api-key

# groq_key_store <key>: saves <key> as the Groq key in the VM's login
# keychain. -A lets the app read it without the "allow access" prompt nobody
# is there to click; -U replaces an earlier one.
groq_key_store() {
  local key=$1 out
  # The key goes into a `security -i` command line, so it must be plain.
  case $key in
    '' | *[!A-Za-z0-9_-]*) die "the API key is empty or has characters the keychain command can't carry" ;;
  esac
  if ! out=$(printf 'add-generic-password -a %s -s %s -w %s -A -U\n' \
    "$GROQ_KEYCHAIN_ACCOUNT" "$GROQ_KEYCHAIN_SERVICE" "$key" |
    vm "security -i" 2>&1); then
    die "could not store the key in the VM's keychain: ${out//"$key"/<key>}"
  fi
}

# Removes the Groq key from the VM's login keychain (none is fine).
groq_key_remove() {
  vm "security delete-generic-password -a $(q "$GROQ_KEYCHAIN_ACCOUNT") -s $(q "$GROQ_KEYCHAIN_SERVICE") >/dev/null 2>&1; true"
}

# Opens an empty TextEdit document in front, to paste into. Needs the VM's
# golden image to allow osascript to control TextEdit (M4.3).
groq_textedit_new() {
  vm "osascript -e 'tell application \"TextEdit\"' -e 'activate' -e 'close every document saving no' -e 'make new document' -e 'end tell'"
}

# The front TextEdit document's text.
groq_textedit_text() {
  vm "osascript -e 'tell application \"TextEdit\" to get text of document 1'"
}

# Quits TextEdit without saving.
groq_textedit_quit() {
  vm "osascript -e 'tell application \"TextEdit\" to quit saving no' >/dev/null 2>&1; true"
}

# Lower case, letters and digits only, one space between words.
groq_words() {
  printf '%s' "$1" | tr '[:upper:]' '[:lower:]' | tr -cs 'a-z0-9' ' ' | sed 's/^ //; s/ $//'
}

# groq_fuzzy_match <expected> <actual> [percent]: whether at least <percent>
# (default 80) of the expected words appear in the actual text, ignoring case
# and punctuation. A speech recogniser may drop or change a word, so an exact
# match would be flaky.
groq_fuzzy_match() {
  local expected actual word found=0 total=0 percent=${3:-80}
  expected=$(groq_words "$1")
  actual=" $(groq_words "$2") "
  for word in $expected; do
    total=$((total + 1))
    case $actual in
      *" $word "*) found=$((found + 1)) ;;
    esac
  done
  log "  matched $found of $total expected words"
  [ "$total" -gt 0 ] && [ $((found * 100)) -ge $((total * percent)) ]
}

# True if the app's log has no line matching the extended regex <pattern>
# (case-insensitive).
groq_log_lacks() {
  ! app_log | grep -qiE -- "$1"
}

# True if the app's log doesn't hold $GROQ_API_KEY. The key reaches grep
# through a pipe (process substitution), not an argument or a file.
groq_log_lacks_key() {
  ! app_log | grep -qF -f <(printf '%s' "$GROQ_API_KEY")
}

# Waits up to 20 s for the log to show more than <count> of the lines <pattern>
# matches (extended regex), for a command that repeats.
groq_wait_for_more() {
  local pattern=$1 count=$2 i
  for i in $(seq 1 20); do
    if [ "$(app_log | grep -cE -- "$pattern" || true)" -gt "$count" ]; then
      return 0
    fi
    sleep 1
  done
  return 1
}

# groq_dictate <seconds>: a press, <seconds> later a release, and then waits
# for the pipeline to get back to Idle (up to a minute, for the retries).
# Both are second launches of the running app, which has its audio file
# from its first launch's --debug-audio-file.
groq_dictate() {
  local hold=$1 pressed idle i
  pressed=$(app_log | grep -c 'event=Pressed from="a second launch" delivered=true' || true)
  idle=$(logged_phases | grep -c '^Idle$' || true)
  launch --press
  groq_wait_for_more 'event=Pressed from="a second launch" delivered=true' "$pressed" || return 1
  sleep "$hold"
  launch --release
  for i in $(seq 1 60); do
    if [ "$(logged_phases | grep -c '^Idle$' || true)" -gt "$idle" ]; then
      return 0
    fi
    sleep 1
  done
  return 1
}
