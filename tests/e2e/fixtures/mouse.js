// Moves the mouse pointer with a real mouse event and says where the system
// then puts it (JXA): `osascript -l JavaScript mouse.js <x> <y>` prints
// "at <x> <y>" (rounded) for the pointer's position read back from the
// window server after the move. For M5.9 criterion 6 (#173): if the window
// server's input stalls (an event tap holds it up), the move never lands and
// the position read back is the old one. Needs Accessibility for the process
// that runs it (Terminal in the VM), else the event is dropped silently, so
// it fails when the process isn't trusted.
ObjC.import("ApplicationServices");

function run(argv) {
  const x = Number(argv[0]);
  const y = Number(argv[1]);
  if (!(x >= 0) || !(y >= 0)) {
    throw new Error("usage: mouse.js <x> <y>");
  }
  if (!$.AXIsProcessTrusted()) {
    throw new Error("untrusted: this process has no Accessibility permission");
  }
  const move = $.CGEventCreateMouseEvent(null, $.kCGEventMouseMoved, { x: x, y: y }, 0);
  $.CGEventPost($.kCGHIDEventTap, move);
  delay(0.5);
  const at = $.CGEventGetLocation($.CGEventCreate(null));
  return "at " + Math.round(at.x) + " " + Math.round(at.y);
}
