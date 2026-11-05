# U8 criterion 1: the real onboarding at its minimum content size (680×520 less
# the title bar) with every text made pseudo-long, measured in real WebKit: the
# primary action and its reason stay usable and unclipped, the footer's parts
# don't overlap, nothing sticks out sideways, and the content scrolls to its
# end. The test page (src/onboarding/layoutHarness.tsx) runs in the VM's Safari
# on a fake backend, since the app's strings can't be swapped in the app, and
# prints its verdict, which is read through Accessibility.
#
# U9: the owner's Try it (a two-key shortcut, the failed-paste panel open) at the
# default and the minimum size, the text box's focus there, the selected language
# button in each state (criterion 4), and the language search box in setup and in
# Settings (criterion 7): see the checks in the test page.
#
# Pictures of each step, light and dark, go to the run's output.

LAYOUT_DIR="/Users/$VM_USER/onboarding-layout"
LAYOUT_STEPS="welcome speech speech-more speech-more-plain speech-key permissions try try-plain try-default ready settings-language"

layout_build() {
  local out=target/onboarding-layout
  npx vite build --config vite.layout.config.ts >/dev/null 2>&1 || return 1
  local step
  for step in $LAYOUT_STEPS; do
    sed "s/data-step=\"STEP\"/data-step=\"$step\"/" tests/e2e/fixtures/onboarding-layout.html >"$out/$step.html"
  done
  vm "rm -rf $(q "$LAYOUT_DIR") && mkdir -p $(q "$LAYOUT_DIR")"
  tar -C "$out" -cf - . | vm "tar -C $(q "$LAYOUT_DIR") -xf -"
}

layout_quit_safari() {
  vm "osascript -e 'tell application \"Safari\" to quit'" >/dev/null 2>&1 || true
}

# layout_shot <name>: Safari's window, as onboarding-layout-<name>.png.
layout_shot() {
  local window id pid png="$OUT_DIR/onboarding-layout-$1.png"
  window=$(vm "~/.local/bin/cua-driver call list_windows '{}'" |
    jq -c '[.windows[] | select(.app_name == "Safari" and .is_on_screen and (.title | contains("Onboarding layout")))] | first // empty')
  [ -n "$window" ] || return 0
  id=$(printf '%s' "$window" | jq -r .window_id)
  pid=$(printf '%s' "$window" | jq -r .pid)
  vm "rm -f /tmp/layout.png; ~/.local/bin/cua-driver call get_window_state '{\"pid\":$pid,\"window_id\":$id,\"include_accessibility_tree\":false,\"screenshot_out_file\":\"/tmp/layout.png\"}' >/dev/null" || true
  vm "base64 -i /tmp/layout.png" | base64 -d >"$png" || true
}

# layout_step <step>: opens the step's page; true if its verdict is OK. A
# failing verdict is logged in full.
layout_step() {
  local step=$1 said
  layout_quit_safari
  sleep 1
  vm "open -a Safari $(q "$LAYOUT_DIR/$step.html")" || return 1
  # After a VM reset, Safari's first launch takes a while to show a window.
  ax wait-window Safari "" 30 >/dev/null 2>&1 || true
  said=$(ax wait-text Safari "" "LAYOUT $step OK" 15 2>&1) && return 0
  log "  $step: $said"
  ax dump Safari 2>/dev/null | grep -a "LAYOUT" | head -14 | while IFS= read -r line; do
    log "  $line"
  done
  return 1
}

e2e_onboarding_layout() {
  local step mode
  fresh_app_state
  expect "the test page builds and reaches the VM" layout_build
  for mode in light dark; do
    expect "the appearance is $mode" obs_appearance "$mode"
    for step in $LAYOUT_STEPS; do
      expect "$mode, $step: usable with pseudo-long text at the minimum size" layout_step "$step"
      layout_shot "$step-$mode"
    done
  done
  obs_appearance light
  layout_quit_safari
  vm "rm -rf $(q "$LAYOUT_DIR")"
  collect_log e2e_onboarding_layout
}
