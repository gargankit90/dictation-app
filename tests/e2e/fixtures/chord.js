// Posts a shortcut on a modifier key as a person types it (JXA): the
// modifier goes down, `before` seconds later the key is pressed (down, up)
// with the modifier's flags, `after` seconds later the modifier comes up.
// `osascript -l JavaScript chord.js <modifier> <key> <before> <after>`, e.g.
// `chord.js right-command c 0.5 0.2` is right Command+C held about 0.7 s, as
// in the owner's report on #172.
//
// <modifier> is left-command, right-command, left-option or right-option;
// <key> is a, c, v or x. The modifier is a flagsChanged event for its keycode
// whose flags carry its mask and device bit (as in post-key.js); the key's
// events carry the same flags. The modifier is posted up however the script
// ends after its down (a `finally`), so nothing stays held. Needs
// Accessibility for the process that runs it (Terminal in the VM), else the
// events are dropped silently, so it fails when the process isn't trusted.
ObjC.import("ApplicationServices");

const MODIFIERS = {
  "left-command": { code: 55, mask: 0x100000, device: 0x8 },
  "right-command": { code: 54, mask: 0x100000, device: 0x10 },
  "left-option": { code: 58, mask: 0x80000, device: 0x20 },
  "right-option": { code: 61, mask: 0x80000, device: 0x40 },
};
// ANSI (US layout) key codes.
const KEYS = { a: 0, x: 7, c: 8, v: 9 };

function post(code, down, flags, type) {
  const event = $.CGEventCreateKeyboardEvent(null, code, down);
  if (type !== undefined) $.CGEventSetType(event, type);
  $.CGEventSetFlags(event, flags);
  $.CGEventPost($.kCGHIDEventTap, event);
}

function run(argv) {
  const usage =
    "usage: chord.js <" +
    Object.keys(MODIFIERS).join("|") +
    "> <" +
    Object.keys(KEYS).join("|") +
    "> <before seconds> <after seconds>";
  const modifier = MODIFIERS[argv[0]];
  const key = KEYS[argv[1]];
  const before = Number(argv[2]);
  const after = Number(argv[3]);
  if (modifier === undefined || key === undefined || !(before >= 0) || !(after >= 0)) {
    throw new Error(usage);
  }
  if (!$.AXIsProcessTrusted()) {
    throw new Error("untrusted: this process has no Accessibility permission");
  }
  const flags = modifier.mask | modifier.device;
  post(modifier.code, true, flags, $.kCGEventFlagsChanged);
  try {
    delay(before);
    post(key, true, flags);
    delay(0.03);
    post(key, false, flags);
    delay(after);
  } finally {
    post(modifier.code, false, 0, $.kCGEventFlagsChanged);
  }
  return "posted " + argv[0] + "+" + argv[1];
}
