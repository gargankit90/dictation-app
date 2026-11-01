// Types lowercase ASCII letters and spaces into the frontmost app with real
// key events (JXA): `osascript -l JavaScript type-keys.js "hello"`. The key
// codes are the US (ANSI) positions, so the US or ABC layout must be
// selected. Each event's modifier flags are set to none explicitly: an event
// made with no source takes the session's current flags, which a synthetic
// ⌘V posted earlier can leave holding Command (seen in the VM: "hello"
// typed as ⌘H ⌘E ⌘L…). Needs Accessibility for the process that runs it (Terminal in
// the VM), else the events are dropped silently, so it checks first.
ObjC.import("ApplicationServices");

const KEY_CODES = {
  a: 0,
  s: 1,
  d: 2,
  f: 3,
  h: 4,
  g: 5,
  z: 6,
  x: 7,
  c: 8,
  v: 9,
  b: 11,
  q: 12,
  w: 13,
  e: 14,
  r: 15,
  y: 16,
  t: 17,
  o: 31,
  u: 32,
  i: 34,
  p: 35,
  l: 37,
  j: 38,
  k: 40,
  n: 45,
  m: 46,
  " ": 49,
};

function run(argv) {
  if (!$.AXIsProcessTrusted()) {
    throw new Error("untrusted: this process has no Accessibility permission");
  }
  const text = argv[0] || "";
  for (const c of text) {
    const code = KEY_CODES[c];
    if (code === undefined) {
      throw new Error("type-keys.js types only a-z and space");
    }
    for (const down of [true, false]) {
      const event = $.CGEventCreateKeyboardEvent(null, code, down);
      $.CGEventSetFlags(event, 0);
      $.CGEventPost($.kCGHIDEventTap, event);
      delay(0.02);
    }
  }
  return "typed " + text.length;
}
