// Reads or switches the keyboard input source (JXA, Carbon TIS):
//   osascript -l JavaScript input-source.js current
//   osascript -l JavaScript input-source.js select <id>    // enables it first if needed
//   osascript -l JavaScript input-source.js disable <id>
// `select` prints "selected <id>", plus " added" if it had to enable the
// source (so the caller knows to disable it again). Ids look like
// com.apple.keylayout.French. Run it in the GUI session (Terminal in the VM)
// so the switch reaches the apps the user sees. Throws on failure.
ObjC.import("Carbon");

function idOf(source) {
  return ObjC.castRefToObject($.TISGetInputSourceProperty(source, $.kTISPropertyInputSourceID)).js;
}

function isEnabled(source) {
  return ObjC.castRefToObject(
    $.TISGetInputSourceProperty(source, $.kTISPropertyInputSourceIsEnabled),
  ).js;
}

function current() {
  return idOf($.TISCopyCurrentKeyboardInputSource());
}

// The installed source with this id, enabled or not.
function find(id) {
  const all = ObjC.castRefToObject($.TISCreateInputSourceList(null, true));
  for (let i = 0; i < all.count; i++) {
    const source = all.objectAtIndex(i);
    if (idOf(source) === id) {
      return source;
    }
  }
  throw new Error("no input source " + id);
}

function run(argv) {
  const command = argv[0];
  if (command === "current") {
    return current();
  }
  if (command === "select") {
    const source = find(argv[1]);
    let added = "";
    if (!isEnabled(source)) {
      const enabled = $.TISEnableInputSource(source);
      if (enabled !== 0) {
        throw new Error("TISEnableInputSource " + argv[1] + ": " + enabled);
      }
      added = " added";
    }
    const selected = $.TISSelectInputSource(source);
    if (selected !== 0) {
      throw new Error("TISSelectInputSource " + argv[1] + ": " + selected);
    }
    // The switch is asynchronous: wait (up to 3 s) until it shows.
    for (let i = 0; i < 30 && current() !== argv[1]; i++) {
      delay(0.1);
    }
    if (current() !== argv[1]) {
      throw new Error("still on " + current() + " after selecting " + argv[1]);
    }
    return "selected " + argv[1] + added;
  }
  if (command === "disable") {
    const disabled = $.TISDisableInputSource(find(argv[1]));
    if (disabled !== 0) {
      throw new Error("TISDisableInputSource " + argv[1] + ": " + disabled);
    }
    return "disabled " + argv[1];
  }
  throw new Error("usage: input-source.js current | select <id> | disable <id>");
}
