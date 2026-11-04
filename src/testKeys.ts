// Test helper: key events as a real keyboard makes them, with the modifier
// flags of the keys held at that moment, sent to the window.

import { act, fireEvent } from "@testing-library/react";

const FLAG_OF: Record<string, "ctrlKey" | "altKey" | "shiftKey" | "metaKey"> = {
  ControlLeft: "ctrlKey",
  ControlRight: "ctrlKey",
  AltLeft: "altKey",
  AltRight: "altKey",
  ShiftLeft: "shiftKey",
  ShiftRight: "shiftKey",
  MetaLeft: "metaKey",
  MetaRight: "metaKey",
};

const held = new Set<string>();

function flags() {
  const out = { ctrlKey: false, altKey: false, shiftKey: false, metaKey: false };
  for (const code of held) {
    const flag = FLAG_OF[code];
    if (flag !== undefined) out[flag] = true;
  }
  return out;
}

/** Presses `code`; a modifier stays held until `keyUp`. */
export function keyDown(code: string) {
  if (code in FLAG_OF) held.add(code);
  act(() => void fireEvent.keyDown(window, { code, ...flags() }));
}

export function keyUp(code: string) {
  held.delete(code);
  act(() => void fireEvent.keyUp(window, { code, ...flags() }));
}

/** Holds `code` without telling the page, as when it was pressed before. */
export function holdSilently(code: string) {
  held.add(code);
}

/** Forgets every key held; call after each test. */
export function releaseAllKeys() {
  held.clear();
}
