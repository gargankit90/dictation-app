// Posts real key presses (down, then up) into the frontmost app (JXA), for
// the keys type-keys.js doesn't type: Escape and the arrows, alone or with
// modifiers. `osascript -l JavaScript keys.js <key>[*<count>] ...`, e.g.
// `keys.js cmd+up right*13 shift+right*10` or `keys.js right-option+escape`.
//
// Keys: escape, left, right, up, down. Modifiers, joined with "+": shift,
// cmd, right-option. Each event's flags are set explicitly (the modifier's
// mask and its device bit, as in post-key.js, or none), never left to the
// session's state; a modifier here is only a flag on the key's own events,
// so nothing can stay held when the script dies. `right-option+escape` is
// Escape as the keyboard sends it while the dictation hotkey (Right Option)
// is held. Needs Accessibility for the process that runs it (Terminal in the
// VM), else the events are dropped silently, so it checks first.
ObjC.import("ApplicationServices");

const CODES = { escape: 53, left: 123, right: 124, down: 125, up: 126 };
const MODIFIERS = {
  shift: 0x20000 | 0x2,
  cmd: 0x100000 | 0x8,
  "right-option": 0x80000 | 0x40,
};

function parse(arg) {
  const [combo, times] = arg.split("*");
  const parts = combo.split("+");
  const key = parts.pop();
  const code = CODES[key];
  if (code === undefined) throw new Error("unknown key " + JSON.stringify(key));
  let flags = 0;
  for (const name of parts) {
    if (MODIFIERS[name] === undefined) throw new Error("unknown modifier " + JSON.stringify(name));
    flags |= MODIFIERS[name];
  }
  const count = times === undefined ? 1 : Number(times);
  if (!Number.isInteger(count) || count < 1) throw new Error("bad count in " + JSON.stringify(arg));
  return { code: code, flags: flags, count: count };
}

function run(argv) {
  if (argv.length === 0) throw new Error("usage: keys.js <key>[*<count>] ...");
  const presses = argv.map(parse);
  if (!$.AXIsProcessTrusted()) {
    throw new Error("untrusted: this process has no Accessibility permission");
  }
  let posted = 0;
  for (const press of presses) {
    for (let i = 0; i < press.count; i++) {
      for (const down of [true, false]) {
        const event = $.CGEventCreateKeyboardEvent(null, press.code, down);
        $.CGEventSetFlags(event, press.flags);
        $.CGEventPost($.kCGHIDEventTap, event);
        delay(0.03);
      }
      posted++;
    }
  }
  return "pressed " + posted;
}
