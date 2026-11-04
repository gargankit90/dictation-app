// Holds Ctrl+Shift+A as a person does (JXA), for a combination hotkey:
// `osascript -l JavaScript combo.js <hold seconds> [<release gap seconds>]`.
// Left Control goes down, then Left Shift, then A (down for <hold> seconds,
// then up); <gap> seconds after A comes up (default 0.05), Shift and then
// Control come up. Every event carries the modifier masks and device bits of
// the keys down, like post-key.js. The modifiers are released however the
// script ends (a `finally`). Needs Accessibility for the process that runs
// it (Terminal in the VM): it fails when that process isn't trusted.
// From the K1 run 2 report (#239).
ObjC.import("ApplicationServices");

function post(code, down, flags, type) {
  const event = $.CGEventCreateKeyboardEvent(null, code, down);
  if (type !== undefined) $.CGEventSetType(event, type);
  $.CGEventSetFlags(event, flags);
  $.CGEventPost($.kCGHIDEventTap, event);
}

function run(argv) {
  const hold = Number(argv[0]);
  const gap = argv.length > 1 ? Number(argv[1]) : 0.05;
  if (!(hold >= 0) || !(gap >= 0)) {
    throw new Error("usage: combo.js <hold seconds> [<release gap seconds>]");
  }
  if (!$.AXIsProcessTrusted()) {
    throw new Error("untrusted: this process has no Accessibility permission");
  }
  const CTRL = 0x40000 | 0x1;
  const SHIFT = 0x20000 | 0x2;
  let ctrl = false;
  let shift = false;
  try {
    post(59, true, CTRL, $.kCGEventFlagsChanged);
    ctrl = true;
    delay(0.05);
    post(56, true, CTRL | SHIFT, $.kCGEventFlagsChanged);
    shift = true;
    delay(0.05);
    post(0, true, CTRL | SHIFT);
    delay(hold);
    post(0, false, CTRL | SHIFT);
    delay(gap);
  } finally {
    if (shift) post(56, false, ctrl ? CTRL : 0, $.kCGEventFlagsChanged);
    if (ctrl) post(59, false, 0, $.kCGEventFlagsChanged);
  }
  return "posted ctrl+shift+a";
}
