// Drives and reads an app's UI through the Accessibility API (JXA), for the
// e2e scenarios: the tray (menu-bar status item) and its menu, an app's main
// menu, and the fields and buttons of a window, including a WebView's.
//
//   osascript -l JavaScript ax.js <command> <app> [args...]
//
// <app> is a process id or an app's name ("Vocet", "Terminal"). It
// calls the AX functions directly, not System Events, so the only grant it
// needs is Accessibility for the process that runs it (Terminal in the VM);
// an ssh session has none. Each command prints what it found or did and
// exits 0; anything else (no such app, element or window, a deadline passed)
// throws, so osascript exits 1 with the reason on stderr.
//
// Commands:
//   trusted                          "trusted" or "untrusted" (no <app>)
//   tray-info <app>                  count=, title=, help= (the tooltip) and
//                                    description= of the app's first status item
//   tray-menu <app>                  opens the status item's menu and lists its
//                                    items, one per line, "-" for a separator,
//                                    "(off) " before a disabled one; closes it
//   tray-open <app> / tray-close <app>  open the menu and leave it open (for a
//                                    screenshot) / close it with Escape
//   tray-click <app> <item>          opens the status item's menu and presses
//                                    the item whose title starts with <item>
//   menu-click <app> <menu> <item>   the same in the app's main menu bar
//   menu-mark <app> <menu> <item>    "checked" or "unchecked"
//   windows <app>                    the app's on-screen windows: "<n> <title>..."
//   wait-window <app> <text> <secs>  until a window whose title has <text> is
//                                    on screen (any title if it has one window)
//   wait-no-window <app> <text> <secs>  until the app has no window on screen
//   field-get <app> <window> <field> a text field's value
//   field-set <app> <window> <field> <value>  types <value> into the field
//                                    (focus, empty it, the keys; lowercase
//                                    letters and digits only), as a person
//                                    would; prints "typed"
//   combo-pick <app> <window> <field> <search> <label>  chooses an option in
//                                    a searchable popup (the language
//                                    picker): focus, type <search>, press
//                                    Return, then checks the field shows
//                                    <label>; prints "picked"
//   secret-set <app> <window> <field> <value>  field-set for a password
//                                    field, whose value can't be read back:
//                                    checks only that it holds as many
//                                    characters; prints "typed"
//   press <app> <window> <button>    presses a button
//   radio-press <app> <window> <label>  presses a radio button by its label
//   check-get <app> <window> <box>   "checked" or "unchecked" (a checkbox,
//                                    matched like a field)
//   check-set <app> <window> <box> <on|off>  presses the checkbox if it
//                                    isn't already so, then waits (5 s) until
//                                    it is; prints "checked" or "unchecked"
//   wait-text <app> <window> <text> <secs>  until some element shows <text>
//   wait-no-text <app> <window> <text> <secs>  until no element shows <text>
//   close <app> <window>             presses the window's close button
//   alert-press <app> <text> <button> <secs>  until a window of the app shows
//                                    <text> (untitled alerts, such as
//                                    UserNotificationCenter's), then presses
//                                    <button> in that window; prints "pressed"
//   alert-count <app> <text>         how many of the app's windows show <text>
//   alert-dismiss <app> <text> <button...>  in every window showing <text>,
//                                    presses the first of <button...> it has;
//                                    prints how many it dismissed
//   overlay <app>                    what the overlay window shows: "hidden"
//                                    (no window titled "overlay" in AXWindows,
//                                    which lists only windows on screen) or
//                                    "shown: <text>", its WebView's text
//                                    (AXStaticText values, space-joined;
//                                    "shown: " alone for a blank page)
//   overlay-wait <app> <want> <secs>  until overlay prints <want> exactly
//                                    ("hidden", "shown: Recording", ...); prints it
//   overlay-watch <app> <secs>       samples overlay every 0.1 s for <secs> and
//                                    prints "<ms> <what overlay printed>" at the
//                                    start and at each change
//   dump <app> [window]              the element tree, for debugging a failure
//
// <window> is text in the window's title; if no title has it and the app has
// exactly one window, that one. <field> matches a text field's DOM id
// (AXDOMIdentifier, WebKit), title, description, placeholder or label.
ObjC.import("ApplicationServices");
ObjC.import("AppKit");

// Element and attribute name as `id`: the AX references are CF objects, and
// these signatures keep them as ObjC objects JXA can pass back in.
ObjC.bindFunction("AXUIElementCreateApplication", ["id", ["int"]]);
ObjC.bindFunction("AXUIElementCopyAttributeValue", ["int", ["id", "id", "id *"]]);
ObjC.bindFunction("AXUIElementSetAttributeValue", ["int", ["id", "id", "id"]]);
ObjC.bindFunction("AXUIElementPerformAction", ["int", ["id", "id"]]);
ObjC.bindFunction("AXUIElementSetMessagingTimeout", ["int", ["id", "float"]]);

const AX_CANNOT_COMPLETE = -25204;
const AX_ACTION_UNSUPPORTED = -25206;
const MAX_NODES = 4000;
const KEY_A = 0;
const KEY_DELETE = 51;
const KEY_RETURN = 36;
const KEY_ESCAPE = 53;
const COMMAND = 0x100000;
// The ANSI (US layout, as in the VM) key code of each character is its
// index here; "_" marks codes field-set doesn't type.
const KEY_LAYOUT = "asdfhgzxcv_bqweryt123465=97-80]ou[ip_lj'k;\\,/nm";

function fail(message) {
  throw new Error(message);
}

function get(element, name) {
  const ref = Ref();
  const err = $.AXUIElementCopyAttributeValue(element, $(name), ref);
  return err === 0 ? ref[0] : undefined;
}

function text(element, name) {
  const value = get(element, name);
  if (value === undefined) return "";
  const js = ObjC.unwrap(value);
  return typeof js === "string" ? js : "";
}

function children(element) {
  const kids = get(element, "AXChildren");
  const out = [];
  if (kids === undefined) return out;
  for (let i = 0; i < kids.count; i++) out.push(kids.objectAtIndex(i));
  return out;
}

function setAttr(element, name, value) {
  return $.AXUIElementSetAttributeValue(element, $(name), value);
}

function press(element, timeout) {
  $.AXUIElementSetMessagingTimeout(element, timeout);
  let err = $.AXUIElementPerformAction(element, $("AXPress"));
  if (err === AX_ACTION_UNSUPPORTED) {
    err = $.AXUIElementPerformAction(element, $("AXShowMenu"));
  }
  return err;
}

function pidOf(app) {
  if (/^[0-9]+$/.test(app)) return Number(app);
  const running = $.NSWorkspace.sharedWorkspace.runningApplications;
  const found = [];
  for (let i = 0; i < running.count; i++) {
    const a = running.objectAtIndex(i);
    if (ObjC.unwrap(a.localizedName) === app || ObjC.unwrap(a.bundleIdentifier) === app) {
      found.push(a.processIdentifier);
    }
  }
  if (found.length === 0) fail("no running app named " + app);
  if (found.length > 1) fail(found.length + " running apps named " + app + ": " + found);
  return found[0];
}

function appElement(app) {
  const element = $.AXUIElementCreateApplication(pidOf(app));
  $.AXUIElementSetMessagingTimeout(element, 5);
  return element;
}

// Depth-first, at most MAX_NODES elements.
function walk(root, visit) {
  const stack = [{ element: root, depth: 0 }];
  let seen = 0;
  while (stack.length > 0 && seen < MAX_NODES) {
    const { element, depth } = stack.pop();
    seen++;
    if (visit(element, depth) === true) return element;
    const kids = children(element);
    for (let i = kids.length - 1; i >= 0; i--) stack.push({ element: kids[i], depth: depth + 1 });
  }
  return undefined;
}

function describe(element) {
  const parts = [text(element, "AXRole")];
  const fields = [
    "AXSubrole",
    "AXTitle",
    "AXDescription",
    "AXDOMIdentifier",
    "AXIdentifier",
    "AXPlaceholderValue",
  ];
  for (const name of fields) {
    const value = text(element, name);
    if (value !== "") parts.push(name.slice(2) + "=" + JSON.stringify(value));
  }
  const value = text(element, "AXValue");
  if (value !== "") parts.push("Value=" + JSON.stringify(value.slice(0, 80)));
  return parts.join(" ");
}

// --- Menus --------------------------------------------------------------------

// The items of `menuBarItem`'s menu once it is open (attached to the item,
// or else an open menu anywhere in the app).
function openMenuItems(appEl, menuBarItem, polls = 30) {
  for (let i = 0; i < polls; i++) {
    for (const menu of children(menuBarItem)) {
      const items = children(menu);
      if (items.length > 0) return items;
    }
    for (const kid of children(appEl)) {
      if (text(kid, "AXRole") === "AXMenu" && children(kid).length > 0) return children(kid);
    }
    delay(0.1);
  }
  return [];
}

// Presses a menu bar item, then the item of its menu whose title starts with
// `itemTitle`. Pressing a menu bar item opens its menu and doesn't return
// until the menu closes, so it is given a short messaging timeout: the call
// gives up (AX_CANNOT_COMPLETE) with the menu still open.
function clickMenu(appEl, menuBarItem, itemTitle) {
  const err = press(menuBarItem, 1.0);
  if (err !== 0 && err !== AX_CANNOT_COMPLETE)
    fail("could not open the menu (AX error " + err + ")");
  return pressMenuItem(appEl, menuBarItem, itemTitle);
}

// Presses the item of `menuBarItem`'s open menu whose title starts with
// `itemTitle`; closes the menu and fails if there is none.
function pressMenuItem(appEl, menuBarItem, itemTitle) {
  const items = openMenuItems(appEl, menuBarItem);
  const titles = items.map((item) => text(item, "AXTitle"));
  const index = titles.findIndex((title) => title !== "" && title.startsWith(itemTitle));
  if (index < 0) {
    postKey(KEY_ESCAPE, 0);
    fail(
      "no menu item starting with " + JSON.stringify(itemTitle) + " in " + JSON.stringify(titles),
    );
  }
  // Quit ends the app, so its answer may never come; that's fine.
  press(items[index], 2.0);
  return "clicked " + titles[index];
}

// The status item's window: owned by the app, at the status-bar level (25),
// in the menu bar at the top of the screen. The overlay shares the level but
// isn't up there.
function statusItemCenter(pid) {
  const list = ObjC.castRefToObject($.CGWindowListCopyWindowInfo(1, 0));
  for (let i = 0; i < list.count; i++) {
    const info = list.objectAtIndex(i);
    if (ObjC.unwrap(info.objectForKey($("kCGWindowOwnerPID"))) !== pid) continue;
    if (ObjC.unwrap(info.objectForKey($("kCGWindowLayer"))) !== 25) continue;
    const b = ObjC.deepUnwrap(info.objectForKey($("kCGWindowBounds")));
    if (b.Y < 5 && b.Height <= 40 && b.Width > 0) {
      return { x: b.X + b.Width / 2, y: b.Y + b.Height / 2 };
    }
  }
  return fail("the app's status item isn't on screen");
}

// A real left click at `point`, as a person would: the tray opens its menu
// from its own mouse-down handler, which an AXPress never reaches.
function click(point) {
  for (const type of [1, 2]) {
    // kCGEventLeftMouseDown, kCGEventLeftMouseUp
    $.CGEventPost($.kCGHIDEventTap, $.CGEventCreateMouseEvent(null, type, point, 0));
    delay(0.05);
  }
}

// Opens the tray's menu with a real click and presses the item whose title
// starts with `itemTitle`. The Terminal window this script runs in may still
// be coming to the front, and an app switch closes the menu again, so a click
// whose menu doesn't show within a second is retried.
function clickTray(appEl, pid, itemTitle) {
  const item = trayItem(appEl).item;
  for (let attempt = 0; attempt < 4; attempt++) {
    click(statusItemCenter(pid));
    if (openMenuItems(appEl, item, 10).length > 0) break;
    postKey(KEY_ESCAPE, 0);
    delay(0.5);
  }
  return pressMenuItem(appEl, item, itemTitle);
}

// Opens the tray's menu like clickTray, lists it and closes it again.
function listTray(appEl, pid) {
  const item = trayItem(appEl).item;
  let items = [];
  for (let attempt = 0; attempt < 4; attempt++) {
    click(statusItemCenter(pid));
    items = openMenuItems(appEl, item, 10);
    if (items.length > 0) break;
    postKey(KEY_ESCAPE, 0);
    delay(0.5);
  }
  const lines = items.map((entry) => {
    const title = text(entry, "AXTitle");
    if (title === "") return "-";
    const enabled = get(entry, "AXEnabled");
    return (enabled !== undefined && ObjC.unwrap(enabled) === false ? "(off) " : "") + title;
  });
  postKey(KEY_ESCAPE, 0);
  if (lines.length === 0) fail("the tray's menu did not open");
  return lines.join("\n");
}

function trayItem(appEl) {
  const bar = get(appEl, "AXExtrasMenuBar");
  const items = bar === undefined ? [] : children(bar);
  if (items.length === 0) fail("the app has no status item");
  return { item: items[0], count: items.length };
}

function mainMenuItem(appEl, title) {
  const bar = get(appEl, "AXMenuBar");
  if (bar === undefined) fail("the app has no menu bar");
  for (const item of children(bar)) {
    if (text(item, "AXTitle") === title) return item;
  }
  fail("no menu " + JSON.stringify(title));
}

// --- Windows ------------------------------------------------------------------

// On-screen, layer-0 windows of `pid` (the window server's list, which needs
// no Screen Recording for owner and layer), as [{number, title}].
function onScreenWindows(pid, appEl) {
  const list = ObjC.castRefToObject($.CGWindowListCopyWindowInfo(1, 0));
  let count = 0;
  for (let i = 0; i < list.count; i++) {
    const info = list.objectAtIndex(i);
    const owner = ObjC.unwrap(info.objectForKey($("kCGWindowOwnerPID")));
    const layer = ObjC.unwrap(info.objectForKey($("kCGWindowLayer")));
    if (owner === pid && layer === 0) count++;
  }
  const titles = [];
  const windows = get(appEl, "AXWindows");
  if (windows !== undefined) {
    for (let i = 0; i < windows.count; i++) titles.push(text(windows.objectAtIndex(i), "AXTitle"));
  }
  return { count: count, titles: titles };
}

function findWindow(appEl, title) {
  const windows = get(appEl, "AXWindows");
  const all = [];
  if (windows !== undefined) {
    for (let i = 0; i < windows.count; i++) all.push(windows.objectAtIndex(i));
  }
  const named = all.filter((w) => text(w, "AXTitle").includes(title));
  if (named.length > 0) return named[0];
  if (all.length === 1) return all[0];
  fail(
    "no window titled " +
      JSON.stringify(title) +
      " among " +
      JSON.stringify(all.map((w) => text(w, "AXTitle"))),
  );
}

function matchesField(element, field) {
  const role = text(element, "AXRole");
  if (role !== "AXTextField" && role !== "AXTextArea" && role !== "AXComboBox") return false;
  const want = field.toLowerCase();
  const names = [
    text(element, "AXDOMIdentifier"),
    text(element, "AXIdentifier"),
    text(element, "AXTitle"),
    text(element, "AXDescription"),
    text(element, "AXPlaceholderValue"),
    text(element, "AXLabel"),
  ];
  const label = get(element, "AXTitleUIElement");
  if (label !== undefined) names.push(text(label, "AXValue"), text(label, "AXTitle"));
  return names.some((name) => name.trim().toLowerCase() === want);
}

function matchesCheckbox(element, name) {
  if (text(element, "AXRole") !== "AXCheckBox") return false;
  const want = name.toLowerCase();
  const names = [
    text(element, "AXDOMIdentifier"),
    text(element, "AXIdentifier"),
    text(element, "AXTitle"),
    text(element, "AXDescription"),
  ];
  return names.some((n) => n.trim().toLowerCase() === want);
}

function findCheckbox(window, name) {
  const found = walk(window, (element) => matchesCheckbox(element, name));
  if (found === undefined) fail("no checkbox " + JSON.stringify(name));
  return found;
}

// A checkbox's AXValue is a number: 1 checked, 0 not (2 mixed).
function isChecked(box) {
  const value = get(box, "AXValue");
  return value !== undefined && Number(ObjC.unwrap(value)) === 1;
}

function findRadio(window, title) {
  const want = title.toLowerCase();
  const found = walk(window, (element) => {
    if (text(element, "AXRole") !== "AXRadioButton") return false;
    const names = [text(element, "AXTitle"), text(element, "AXDescription")];
    for (const kid of children(element)) names.push(text(kid, "AXValue"));
    return names.some((name) => name.trim().toLowerCase() === want);
  });
  if (found === undefined) fail("no radio button " + JSON.stringify(title));
  return found;
}

function findField(window, field) {
  const found = walk(window, (element) => matchesField(element, field));
  if (found === undefined) fail("no text field " + JSON.stringify(field));
  return found;
}

function findButton(window, title) {
  const want = title.toLowerCase();
  const found = walk(window, (element) => {
    if (text(element, "AXRole") !== "AXButton") return false;
    const names = [text(element, "AXTitle"), text(element, "AXDescription")];
    for (const kid of children(element)) names.push(text(kid, "AXValue"));
    return names.some((name) => name.trim().toLowerCase() === want);
  });
  if (found === undefined) fail("no button " + JSON.stringify(title));
  return found;
}

// The app's windows (AXWindows) in which some element shows <wanted>.
function windowsShowing(appEl, wanted) {
  const windows = get(appEl, "AXWindows");
  const found = [];
  if (windows !== undefined) {
    for (let i = 0; i < windows.count; i++) {
      const window = windows.objectAtIndex(i);
      if (showsText(window, wanted)) found.push(window);
    }
  }
  return found;
}

function showsText(window, wanted) {
  return (
    walk(window, (element) =>
      ["AXValue", "AXTitle", "AXDescription"].some((name) => text(element, name).includes(wanted)),
    ) !== undefined
  );
}

// --- The overlay ------------------------------------------------------------------

const OVERLAY_TITLE = "overlay";

// What the overlay shows: "hidden", or "shown: " and its rendered text. Read
// from the WebView's accessibility tree, so it is what WebKit laid out, not
// what the app meant to send.
function overlayState(appEl) {
  const windows = get(appEl, "AXWindows");
  let overlay;
  if (windows !== undefined) {
    for (let i = 0; i < windows.count; i++) {
      const w = windows.objectAtIndex(i);
      if (text(w, "AXTitle") === OVERLAY_TITLE) overlay = w;
    }
  }
  if (overlay === undefined) return "hidden";
  const parts = [];
  walk(overlay, (element) => {
    if (text(element, "AXRole") === "AXStaticText") {
      const value = text(element, "AXValue").trim();
      if (value !== "") parts.push(value);
    }
    return false;
  });
  return "shown: " + parts.join(" ");
}

// --- Keys -----------------------------------------------------------------------

function postKey(code, flags) {
  for (const down of [true, false]) {
    const event = $.CGEventCreateKeyboardEvent(null, code, down);
    $.CGEventSetFlags(event, flags);
    $.CGEventPost($.kCGHIDEventTap, event);
    delay(0.03);
  }
}

// How many characters a field holds. A password field (AXSecureTextField)
// shows only bullets, if anything, so its count is all a test can check.
function charCount(field) {
  const count = get(field, "AXNumberOfCharacters");
  if (count !== undefined && typeof ObjC.unwrap(count) === "number") return ObjC.unwrap(count);
  return text(field, "AXValue").length;
}

function typeInto(appEl, window, field, value, secret = false, pick = null) {
  const keys = [];
  for (const ch of value) {
    if (!/^[a-z0-9]$/.test(ch)) {
      fail("field-set can't type " + JSON.stringify(ch) + "; only lowercase letters and digits");
    }
    keys.push(KEY_LAYOUT.indexOf(ch));
  }
  // In front, so the keys go to it: the script runs in Terminal, which is.
  setAttr(appEl, "AXFrontmost", $(true));
  setAttr(window, "AXMain", $(true));
  setAttr(field, "AXFocused", $(true));
  delay(0.5);
  if (ObjC.unwrap(get(field, "AXFocused")) !== true) {
    fail("could not focus the field (frontmost=" + ObjC.unwrap(get(appEl, "AXFrontmost")) + ")");
  }
  // Empty it first. Through AX if it takes that (the keys typed next fire
  // the page's input events with the whole value, so a framework holding
  // the old value catches up); else, or when nothing is typed after it,
  // select all and delete, as a person would.
  if (
    value === "" ||
    setAttr(field, "AXValue", $("")) !== 0 ||
    (secret ? charCount(field) : text(field, "AXValue").length) !== 0
  ) {
    postKey(KEY_A, COMMAND);
    postKey(KEY_DELETE, 0);
  }
  for (const code of keys) postKey(code, 0);
  delay(0.5);
  // A popup: Return takes the first match, which then shows in the field.
  if (pick !== null) {
    postKey(KEY_RETURN, 0);
    delay(0.5);
    const shown = text(field, "AXValue");
    if (shown !== pick)
      fail("searched " + JSON.stringify(value) + " but the field shows " + JSON.stringify(shown));
    return "picked";
  }
  if (secret) {
    const count = charCount(field);
    if (count !== value.length)
      fail("typed " + value.length + " characters but the field holds " + count);
    return "typed";
  }
  const now = text(field, "AXValue");
  if (now !== value)
    fail("typed " + JSON.stringify(value) + " but the field shows " + JSON.stringify(now));
  return "typed";
}

function waitFor(seconds, what, check) {
  const deadline = Date.now() + seconds * 1000;
  for (;;) {
    if (check()) return;
    if (Date.now() > deadline) fail(what + " not within " + seconds + "s");
    delay(0.25);
  }
}

// --- Commands ---------------------------------------------------------------------

function run(argv) {
  const [command, app, ...rest] = argv;
  if (command === "trusted") return $.AXIsProcessTrusted() ? "trusted" : "untrusted";
  if (app === undefined) fail("usage: ax.js <command> <app> [args...] (see the file)");
  if (!$.AXIsProcessTrusted()) fail("untrusted: this process has no Accessibility permission");
  const appEl = appElement(app);
  switch (command) {
    case "tray-info": {
      const { item, count } = trayItem(appEl);
      return [
        "count=" + count,
        "title=" + text(item, "AXTitle"),
        "help=" + text(item, "AXHelp"),
        "description=" + text(item, "AXDescription"),
      ].join("\n");
    }
    case "tray-menu":
      return listTray(appEl, pidOf(app));
    case "tray-open": {
      const item = trayItem(appEl).item;
      for (let attempt = 0; attempt < 4; attempt++) {
        click(statusItemCenter(pidOf(app)));
        if (openMenuItems(appEl, item, 10).length > 0) return "open";
        postKey(KEY_ESCAPE, 0);
        delay(0.5);
      }
      return fail("the tray's menu did not open");
    }
    case "tray-close":
      postKey(KEY_ESCAPE, 0);
      return "closed";
    case "tray-click":
      return clickTray(appEl, pidOf(app), rest[0]);
    case "menu-click":
      return clickMenu(appEl, mainMenuItem(appEl, rest[0]), rest[1]);
    case "menu-mark": {
      const menuBarItem = mainMenuItem(appEl, rest[0]);
      for (const menu of children(menuBarItem)) {
        for (const item of children(menu)) {
          if (text(item, "AXTitle").startsWith(rest[1])) {
            return text(item, "AXMenuItemMarkChar") === "" ? "unchecked" : "checked";
          }
        }
      }
      return fail("no menu item " + JSON.stringify(rest[1]));
    }
    case "windows": {
      const { count, titles } = onScreenWindows(pidOf(app), appEl);
      return count + " " + JSON.stringify(titles);
    }
    case "wait-window":
    case "wait-no-window": {
      const pid = pidOf(app);
      const want = command === "wait-window";
      // On screen, by the window server's list; the title as findWindow
      // matches it (an app with one window needn't match).
      waitFor(Number(rest[1] || 10), (want ? "a window " : "no window ") + rest[0], () => {
        const { count, titles } = onScreenWindows(pid, appEl);
        const titled = titles.length <= 1 || titles.some((title) => title.includes(rest[0]));
        return want ? count > 0 && titled : count === 0;
      });
      return want ? "shown" : "gone";
    }
    case "field-get":
      return text(findField(findWindow(appEl, rest[0]), rest[1]), "AXValue");
    case "field-set": {
      const window = findWindow(appEl, rest[0]);
      return typeInto(appEl, window, findField(window, rest[1]), rest[2] || "");
    }
    case "combo-pick": {
      const window = findWindow(appEl, rest[0]);
      return typeInto(appEl, window, findField(window, rest[1]), rest[2] || "", false, rest[3]);
    }
    case "secret-set": {
      const window = findWindow(appEl, rest[0]);
      return typeInto(appEl, window, findField(window, rest[1]), rest[2] || "", true);
    }
    case "press": {
      const err = press(findButton(findWindow(appEl, rest[0]), rest[1]), 5.0);
      if (err !== 0) fail("pressing " + rest[1] + " failed (AX error " + err + ")");
      return "pressed " + rest[1];
    }
    case "radio-press": {
      const err = press(findRadio(findWindow(appEl, rest[0]), rest[1]), 5.0);
      if (err !== 0) fail("pressing radio " + rest[1] + " failed (AX error " + err + ")");
      return "pressed radio " + rest[1];
    }
    case "check-get":
      return isChecked(findCheckbox(findWindow(appEl, rest[0]), rest[1])) ? "checked" : "unchecked";
    case "check-set": {
      if (rest[2] !== "on" && rest[2] !== "off") fail("check-set wants on or off, not " + rest[2]);
      const want = rest[2] === "on";
      const box = findCheckbox(findWindow(appEl, rest[0]), rest[1]);
      if (isChecked(box) !== want) {
        const err = press(box, 5.0);
        if (err !== 0) fail("pressing " + rest[1] + " failed (AX error " + err + ")");
        waitFor(5, rest[1] + " " + rest[2], () => isChecked(box) === want);
      }
      return want ? "checked" : "unchecked";
    }
    case "wait-text": {
      const window = findWindow(appEl, rest[0]);
      waitFor(Number(rest[2] || 10), JSON.stringify(rest[1]), () => showsText(window, rest[1]));
      return "shown " + rest[1];
    }
    case "wait-no-text": {
      const window = findWindow(appEl, rest[0]);
      waitFor(
        Number(rest[2] || 10),
        "no " + JSON.stringify(rest[1]),
        () => !showsText(window, rest[1]),
      );
      return "gone " + rest[1];
    }
    case "alert-press": {
      let target;
      waitFor(Number(rest[2] || 10), "a window showing " + JSON.stringify(rest[0]), () => {
        target = windowsShowing(appEl, rest[0])[0];
        return target !== undefined;
      });
      const err = press(findButton(target, rest[1]), 5.0);
      if (err !== 0) fail("pressing " + rest[1] + " failed (AX error " + err + ")");
      return "pressed " + rest[1];
    }
    case "alert-count":
      return String(windowsShowing(appEl, rest[0]).length);
    case "alert-dismiss": {
      let dismissed = 0;
      for (const window of windowsShowing(appEl, rest[0])) {
        for (const title of rest.slice(1)) {
          let button;
          try {
            button = findButton(window, title);
          } catch (e) {
            continue;
          }
          if (press(button, 5.0) === 0) dismissed++;
          break;
        }
      }
      return "dismissed " + dismissed;
    }
    case "close": {
      const button = get(findWindow(appEl, rest[0]), "AXCloseButton");
      if (button === undefined) fail("the window has no close button");
      const err = press(button, 5.0);
      if (err !== 0) fail("pressing the close button failed (AX error " + err + ")");
      return "closed";
    }
    case "overlay":
      return overlayState(appEl);
    case "overlay-wait": {
      const deadline = Date.now() + Number(rest[1] || 10) * 1000;
      for (;;) {
        const now = overlayState(appEl);
        if (now === rest[0]) return now;
        if (Date.now() > deadline) {
          fail(
            "overlay " +
              JSON.stringify(rest[0]) +
              " not within " +
              rest[1] +
              "s; it is " +
              JSON.stringify(now),
          );
        }
        delay(0.1);
      }
    }
    case "overlay-watch": {
      const start = Date.now();
      const end = start + Number(rest[0] || 5) * 1000;
      const lines = [];
      let last;
      while (Date.now() < end) {
        const now = overlayState(appEl);
        if (now !== last) {
          lines.push(Date.now() - start + " " + now);
          last = now;
        }
        delay(0.1);
      }
      return lines.join("\n");
    }
    case "dump": {
      const lines = [];
      const root = rest[0] === undefined ? appEl : findWindow(appEl, rest[0]);
      walk(root, (element, depth) => {
        lines.push("  ".repeat(depth) + describe(element));
        return false;
      });
      return lines.join("\n");
    }
    default:
      return fail("unknown command " + command + " (see the file)");
  }
}
