// A window with one text field in front, for e2e_password_field_not_read
// (M5.8 #2, #135). JXA, run from Terminal (the GUI session):
//
//   osascript -l JavaScript password-field.js <secure|plain> <dir>
//
// The field holds "hello" with the cursor after it, focused in the key
// window of the front app, and has an Edit menu so ⌘V pastes into it.
// `secure` makes the field answer AXSecureTextField to AXSubrole, as a
// password field does over Accessibility, without turning on Secure Input
// (an NSSecureTextField would, and then the app's Secure Input check stops
// the paste before the subrole is ever asked; e2e_secure_input_blocks_paste
// covers that). `plain` is the control: the same field with AppKit's own
// answer (no subrole), which the app reads.
//
// In <dir> it writes `pid` (its own pid, so the caller stops only this
// process), `ready` once the field is focused with the cursor at its end,
// and `text` (the field's text, UTF-8, rewritten every 0.2 s). It quits when
// <dir>/stop appears, or after 120 s.
ObjC.import("Cocoa");

const START_TEXT = "hello";
const LIFETIME_SECONDS = 120;

function write(path, text) {
  $(text).writeToFileAtomicallyEncodingError($(path), true, $.NSUTF8StringEncoding, null);
}

function exists(path) {
  return $.NSFileManager.defaultManager.fileExistsAtPath($(path));
}

function editMenu() {
  const item = $.NSMenuItem.alloc.init;
  const menu = $.NSMenu.alloc.initWithTitle($("Edit"));
  menu.addItemWithTitleActionKeyEquivalent($("Paste"), "paste:", $("v"));
  menu.addItemWithTitleActionKeyEquivalent($("Select All"), "selectAll:", $("a"));
  item.submenu = menu;
  return item;
}

function run(argv) {
  const [kind, dir] = argv;
  if ((kind !== "secure" && kind !== "plain") || !dir) {
    throw new Error("usage: password-field.js <secure|plain> <dir>");
  }
  write(`${dir}/pid`, String($.NSProcessInfo.processInfo.processIdentifier));

  let fieldClass = $.NSTextField;
  if (kind === "secure") {
    ObjC.registerSubclass({
      name: "E2ESecureSubroleField",
      superclass: "NSTextField",
      methods: {
        accessibilitySubrole: {
          types: ["id", []],
          implementation: function () {
            return $("AXSecureTextField");
          },
        },
      },
    });
    fieldClass = $.E2ESecureSubroleField;
  }

  const app = $.NSApplication.sharedApplication;
  app.setActivationPolicy($.NSApplicationActivationPolicyRegular);
  const menuBar = $.NSMenu.alloc.init;
  menuBar.addItem($.NSMenuItem.alloc.init); // the app menu
  menuBar.addItem(editMenu());
  app.mainMenu = menuBar;

  const window = $.NSWindow.alloc.initWithContentRectStyleMaskBackingDefer(
    $.NSMakeRect(200, 300, 420, 120),
    $.NSWindowStyleMaskTitled,
    $.NSBackingStoreBuffered,
    false,
  );
  window.title = $(`e2e ${kind} field`);
  const field = fieldClass.alloc.initWithFrame($.NSMakeRect(20, 40, 380, 30));
  field.stringValue = $(START_TEXT);
  window.contentView.addSubview(field);
  window.makeKeyAndOrderFront(null);
  app.activateIgnoringOtherApps(true);

  const started = Date.now();
  let placed = false;
  $.NSTimer.scheduledTimerWithTimeIntervalRepeatsBlock(0.2, true, () => {
    if (exists(`${dir}/stop`) || Date.now() - started > LIFETIME_SECONDS * 1000) {
      app.terminate(null);
      return;
    }
    if (!placed && window.isKeyWindow) {
      window.makeFirstResponder(field);
      const editor = field.currentEditor;
      if (!editor.isNil()) {
        // Focusing a field selects all of it; put the cursor at the end.
        editor.selectedRange = $.NSMakeRange(START_TEXT.length, 0);
        placed = true;
        write(`${dir}/ready`, "ready");
      }
    }
    if (!placed && !app.isActive) {
      app.activateIgnoringOtherApps(true);
    }
    const editor = field.currentEditor;
    write(`${dir}/text`, editor.isNil() ? field.stringValue.js : editor.string.js);
  });
  app.run;
}
