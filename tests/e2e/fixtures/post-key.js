// Posts one real modifier key event (JXA), like right-option.js but for any
// modifier key: `osascript -l JavaScript post-key.js <key> down|up|state`.
// <key> is left-option, right-option, left-command, right-command,
// left-control, right-control, left-shift or right-shift. `state` says
// whether the system thinks that key is held ("<key> held" or "<key> up"),
// without posting anything or needing Accessibility.
//
// A modifier is a flagsChanged event for the key's keycode whose flags carry
// the modifier's mask and, while it is down, the key's own device bit (the
// NX_DEVICE* masks in IOLLEvent.h), which is how the hotkey tells left from
// right. Needs Accessibility for the process that runs it (Terminal in the
// VM), else the event is dropped silently, so it fails (exit status 1) when
// the process isn't trusted, as it does for a usage error.
ObjC.import("ApplicationServices");

const KEYS = {
  "left-control": { code: 59, mask: 0x40000, device: 0x1 },
  "right-control": { code: 62, mask: 0x40000, device: 0x2000 },
  "left-shift": { code: 56, mask: 0x20000, device: 0x2 },
  "right-shift": { code: 60, mask: 0x20000, device: 0x4 },
  "left-command": { code: 55, mask: 0x100000, device: 0x8 },
  "right-command": { code: 54, mask: 0x100000, device: 0x10 },
  "left-option": { code: 58, mask: 0x80000, device: 0x20 },
  "right-option": { code: 61, mask: 0x80000, device: 0x40 },
};

function run(argv) {
  const usage = "usage: post-key.js <" + Object.keys(KEYS).join("|") + "> down|up|state";
  const name = argv[0];
  const key = KEYS[name];
  if (key === undefined) {
    throw new Error(usage);
  }
  if (argv[1] === "state") {
    // kCGEventSourceStateHIDSystemState is 1.
    const flags = $.CGEventSourceFlagsState(1);
    return flags & key.device ? name + " held" : name + " up";
  }
  const down = argv[1] === "down";
  if (!down && argv[1] !== "up") {
    throw new Error(usage);
  }
  if (!$.AXIsProcessTrusted()) {
    throw new Error("untrusted: this process has no Accessibility permission");
  }
  const event = $.CGEventCreateKeyboardEvent(null, key.code, down);
  $.CGEventSetType(event, $.kCGEventFlagsChanged);
  $.CGEventSetFlags(event, down ? key.mask | key.device : 0);
  $.CGEventPost($.kCGHIDEventTap, event);
  return "posted " + argv[1];
}
