// Puts a PNG on the general pasteboard or reads it back (JXA, AppKit):
//   osascript -l JavaScript clipboard-image.js set <file.png>
//   osascript -l JavaScript clipboard-image.js save <out.png>   // the public.png data
//   osascript -l JavaScript clipboard-image.js types            // each item's types
// `save` fails if the pasteboard holds no PNG.
ObjC.import("AppKit");

function run(argv) {
  const board = $.NSPasteboard.generalPasteboard;
  const command = argv[0];
  if (command === "set") {
    const data = $.NSData.dataWithContentsOfFile(argv[1]);
    if (data.isNil()) {
      throw new Error("cannot read " + argv[1]);
    }
    board.clearContents;
    if (!board.setDataForType(data, "public.png")) {
      throw new Error("could not put the PNG on the pasteboard");
    }
    return "set " + data.length + " bytes";
  }
  if (command === "save") {
    const data = board.dataForType("public.png");
    if (data.isNil()) {
      throw new Error("no PNG on the pasteboard");
    }
    if (!data.writeToFileAtomically(argv[1], true)) {
      throw new Error("cannot write " + argv[1]);
    }
    return "saved " + data.length + " bytes";
  }
  if (command === "types") {
    const items = board.pasteboardItems;
    const lines = [];
    for (let i = 0; i < items.count; i++) {
      lines.push(ObjC.deepUnwrap(items.objectAtIndex(i).types).join(" "));
    }
    return lines.join("\n");
  }
  throw new Error("usage: clipboard-image.js set <png> | save <png> | types");
}
