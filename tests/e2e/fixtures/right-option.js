// Posts one real Right Option event (JXA): `osascript -l JavaScript
// right-option.js down|up`. The key is a modifier, so the HID system sees a
// flagsChanged event for keycode 61 (kVK_RightOption) whose flags carry the
// alternate mask (0x80000) and, while it is down, the right-option device
// bit NX_DEVICERALTKEYMASK (0x40). Needs Accessibility for the process that
// runs it (Terminal in the VM), else the event is dropped silently, so it
// says whether the process is trusted.
ObjC.import("ApplicationServices");

function run(argv) {
  const down = argv[0] === "down";
  if (!down && argv[0] !== "up") {
    return "usage: right-option.js down|up";
  }
  if (!$.AXIsProcessTrusted()) {
    return "untrusted: this process has no Accessibility permission";
  }
  const ALTERNATE = 0x80000;
  const RIGHT_ALT_DEVICE = 0x40;
  const event = $.CGEventCreateKeyboardEvent(null, 61, down);
  $.CGEventSetType(event, $.kCGEventFlagsChanged);
  $.CGEventSetFlags(event, down ? ALTERNATE | RIGHT_ALT_DEVICE : 0);
  $.CGEventPost($.kCGHIDEventTap, event);
  return "posted " + argv[0];
}
