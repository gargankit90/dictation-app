// Plays a user who copies something while a dictation's paste is pending
// (JXA, for e2e_user_copy_wins). Needs no Accessibility: the pasteboard is
// reachable from an ssh session, as pbcopy is.
//
//   osascript -l JavaScript clipboard-race.js copy-during-paste <text> [timeout] [wait] [board]
//   osascript -l JavaScript clipboard-race.js count [board]
//
// copy-during-paste waits (up to `timeout` seconds, default 30) for the app's
// dictation write: the item carrying its private com.dictationapp.session
// type, with a session token that wasn't there when it started. Then it
// waits `wait` seconds (default 0.15; e2e_user_copy_wins passes 1, inside the
// 3 s an unconfirmed paste keeps the transcript on the clipboard), says
// whether the dictation's item is still there (so no restore has run yet),
// and copies <text> as plain text, like a user's Cmd-C. Progress goes to
// stderr as it happens ("armed" first, so the caller knows it is watching); the result goes to stdout:
//
//   pending=yes|no     the dictation's item was still there when <text> was copied
//   copied_count=<n>   the pasteboard's change count right after the copy
//
// count prints the change count, so a caller can tell that nothing wrote to
// the clipboard after the copy. [board] is a named pasteboard instead of the
// general one, for trying this out without touching your own clipboard.
ObjC.import("AppKit");

const SESSION_TYPE = "com.dictationapp.session";

function board(name) {
  return name ? $.NSPasteboard.pasteboardWithName($(name)) : $.NSPasteboard.generalPasteboard;
}

// The session token of the dictation write on the clipboard, if any.
function dictationToken(pasteboard) {
  return ObjC.unwrap(pasteboard.stringForType($(SESSION_TYPE)));
}

function run(argv) {
  const command = argv[0];
  if (command === "count") {
    return String(board(argv[1]).changeCount);
  }
  if (command !== "copy-during-paste" || argv[1] === undefined) {
    throw new Error(
      "usage: clipboard-race.js copy-during-paste <text> [timeout] [wait] [board] | count [board]",
    );
  }
  const text = argv[1];
  const timeout = Number(argv[2] || 30);
  const wait = Number(argv[3] || 0.15);
  const pasteboard = board(argv[4]);
  const started = Date.now();
  // A dictation item already there is a stale one (a restore that didn't
  // run); only a new write counts.
  const stale = dictationToken(pasteboard);
  let token = stale;
  console.log("armed");
  while (token === undefined || token === stale) {
    if (Date.now() - started > timeout * 1000) {
      throw new Error("no dictation write on the clipboard within " + timeout + "s");
    }
    delay(0.005);
    token = dictationToken(pasteboard);
  }
  console.log("dictation write seen after " + (Date.now() - started) + " ms");
  delay(wait);
  const pending = dictationToken(pasteboard) === token;
  pasteboard.clearContents;
  if (!pasteboard.setStringForType($(text), $.NSPasteboardTypeString)) {
    throw new Error("the pasteboard refused the copy");
  }
  const count = pasteboard.changeCount;
  console.log("copied; the dictation's item was " + (pending ? "still there" : "gone already"));
  return "pending=" + (pending ? "yes" : "no") + "\ncopied_count=" + count;
}
