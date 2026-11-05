import { describe, expect, it } from "vitest";
import {
  INITIAL_RECORDER,
  keyLongLabel,
  keySpokenName,
  keycapLabel,
  recorderStep,
} from "./hotkeyKeys";
import type { RecorderOutcome } from "./hotkeyKeys";

const FLAG_OF: Record<string, "ctrl" | "alt" | "shift" | "meta"> = {
  ControlLeft: "ctrl",
  ControlRight: "ctrl",
  AltLeft: "alt",
  AltRight: "alt",
  ShiftLeft: "shift",
  ShiftRight: "shift",
  MetaLeft: "meta",
  MetaRight: "meta",
};

/**
 * Plays `[type, code]` events, with the modifier flags a real keyboard puts
 * on each (`held` were already down before recording began, unseen), and
 * returns the outcome of the last one.
 */
function playHeld(held: string[], events: ["down" | "up", string][]): RecorderOutcome | undefined {
  const down = new Set(held);
  let state = INITIAL_RECORDER;
  let outcome: RecorderOutcome | undefined;
  for (const [type, code] of events) {
    if (type === "down") down.add(code);
    else down.delete(code);
    const has = (flag: string) => [...down].some((c) => FLAG_OF[c] === flag);
    const step = recorderStep(state, {
      type,
      code,
      flags: { ctrl: has("ctrl"), alt: has("alt"), shift: has("shift"), meta: has("meta") },
    });
    state = step.state;
    outcome = step.outcome;
  }
  return outcome;
}

const play = (...events: ["down" | "up", string][]) => playHeld([], events);

describe("recorderStep", () => {
  it("records a modifier pressed and released alone, with its side", () => {
    expect(play(["down", "MetaRight"], ["up", "MetaRight"])).toEqual({
      kind: "recorded",
      spec: "RightCmd",
    });
    expect(play(["down", "MetaLeft"], ["up", "MetaLeft"])).toEqual({
      kind: "recorded",
      spec: "LeftCmd",
    });
    expect(play(["down", "AltRight"], ["up", "AltRight"])).toEqual({
      kind: "recorded",
      spec: "RightAlt",
    });
  });

  it("does not finish while a modifier is still held", () => {
    expect(play(["down", "ControlLeft"], ["down", "ShiftLeft"], ["up", "ShiftLeft"])).toBe(
      undefined,
    );
  });

  it("records several modifiers alone in canonical order", () => {
    expect(
      play(["down", "MetaRight"], ["down", "AltRight"], ["up", "MetaRight"], ["up", "AltRight"]),
    ).toEqual({ kind: "recorded", spec: "RightAlt+RightCmd" });
  });

  it("refuses both sides of one modifier", () => {
    const outcome = play(
      ["down", "AltLeft"],
      ["down", "AltRight"],
      ["up", "AltLeft"],
      ["up", "AltRight"],
    );
    expect(outcome).toEqual({ kind: "refused", reason: "Use one side of Option, not both." });
  });

  it("records a combination with either-side modifiers, canonical", () => {
    expect(play(["down", "ShiftRight"], ["down", "ControlLeft"], ["down", "Space"])).toEqual({
      kind: "recorded",
      spec: "Ctrl+Shift+Space",
    });
    expect(play(["down", "MetaLeft"], ["down", "KeyK"])).toEqual({
      kind: "recorded",
      spec: "Cmd+K",
    });
  });

  it("records keys that work alone: function keys and arrows", () => {
    expect(play(["down", "F5"])).toEqual({ kind: "recorded", spec: "F5" });
    expect(play(["down", "F20"])).toEqual({ kind: "recorded", spec: "F20" });
    expect(play(["down", "ArrowUp"])).toEqual({ kind: "recorded", spec: "Up" });
  });

  it("refuses plain text keys, also with only Shift", () => {
    for (const code of ["KeyA", "Digit4", "Space", "Enter", "Tab", "Backspace"]) {
      const outcome = play(["down", code]);
      expect(outcome?.kind, code).toBe("refused");
    }
    expect(play(["down", "ShiftLeft"], ["down", "KeyA"])?.kind).toBe("refused");
  });

  it("refuses keys no hotkey can be", () => {
    for (const code of ["CapsLock", "F21", "NumpadEnter", "Fn", "Lang1"]) {
      expect(play(["down", code]), code).toEqual({
        kind: "refused",
        reason: "That key can't be a dictation key. Try another, such as a modifier key.",
      });
    }
  });

  it("cancels on Escape, also with modifiers held", () => {
    expect(play(["down", "Escape"])).toEqual({ kind: "cancelled" });
    expect(play(["down", "MetaRight"], ["down", "Escape"])).toEqual({ kind: "cancelled" });
  });

  it("counts a modifier held before recording began", () => {
    expect(playHeld(["ShiftLeft"], [["down", "F5"]])).toEqual({
      kind: "recorded",
      spec: "Shift+F5",
    });
    expect(playHeld(["MetaRight", "ShiftLeft"], [["down", "KeyK"]])).toEqual({
      kind: "recorded",
      spec: "Shift+Cmd+K",
    });
  });

  it("keeps the held modifiers across a refused key", () => {
    // Ctrl is still down when K comes after the refused Caps Lock.
    expect(
      play(["down", "ControlLeft"], ["down", "CapsLock"], ["up", "CapsLock"], ["down", "KeyK"]),
    ).toEqual({ kind: "recorded", spec: "Ctrl+K" });
    expect(
      play(["down", "ShiftRight"], ["down", "CapsLock"], ["up", "CapsLock"], ["down", "F5"]),
    ).toEqual({ kind: "recorded", spec: "Shift+F5" });
    // A plain K is still refused: Shift alone doesn't make it a hotkey.
    expect(
      play(["down", "ShiftRight"], ["down", "CapsLock"], ["up", "CapsLock"], ["down", "KeyK"])
        ?.kind,
    ).toBe("refused");
  });

  it("records nothing when the modifiers are let go after a refused key", () => {
    expect(
      play(["down", "MetaRight"], ["down", "CapsLock"], ["up", "CapsLock"], ["up", "MetaRight"]),
    ).toBeUndefined();
    // ...and the next modifier alone is a fresh choice.
    let state = INITIAL_RECORDER;
    for (const [type, code] of [
      ["down", "MetaRight"],
      ["down", "CapsLock"],
      ["up", "MetaRight"],
      ["down", "AltRight"],
      ["up", "AltRight"],
    ] as const) {
      const step = recorderStep(state, { type, code });
      state = step.state;
      if (code === "AltRight" && type === "up") {
        expect(step.outcome).toEqual({ kind: "recorded", spec: "RightAlt" });
      }
    }
  });

  it("drops a modifier the flags say was let go unseen", () => {
    // Command was released in another window: its keyup never came.
    let state = recorderStep(INITIAL_RECORDER, {
      type: "down",
      code: "MetaLeft",
      flags: { ctrl: false, alt: false, shift: false, meta: true },
    }).state;
    const step = recorderStep(state, {
      type: "down",
      code: "F5",
      flags: { ctrl: false, alt: false, shift: false, meta: false },
    });
    expect(step.outcome).toEqual({ kind: "recorded", spec: "F5" });
    state = step.state;
    expect(state).toEqual(INITIAL_RECORDER);
  });

  it("ignores the release of a key it never saw go down", () => {
    expect(play(["up", "MetaRight"])).toBe(undefined);
  });
});

describe("keycapLabel", () => {
  it("shows sided modifiers with their side", () => {
    expect(keycapLabel("RightAlt")).toBe("Right ⌥");
    expect(keycapLabel("RightCmd")).toBe("Right ⌘");
    expect(keycapLabel("LeftCtrl")).toBe("Left ⌃");
    expect(keycapLabel("LeftShift+LeftCmd")).toBe("Left ⇧ Left ⌘");
  });

  it("joins either-side modifiers and the key", () => {
    expect(keycapLabel("Ctrl+Shift+Space")).toBe("⌃⇧Space");
    expect(keycapLabel("alt+cmd+k")).toBe("⌥⌘k");
    expect(keycapLabel("Ctrl+Right")).toBe("⌃Right");
    expect(keycapLabel("F5")).toBe("F5");
  });
});

describe("keyLongLabel (U11)", () => {
  it.each([
    ["RightAlt", "Right Option (⌥)"],
    ["LeftAlt", "Left Option (⌥)"],
    ["RightCmd", "Right Command (⌘)"],
    ["LeftCmd", "Left Command (⌘)"],
    ["RightCtrl", "Right Control (⌃)"],
    ["LeftShift", "Left Shift (⇧)"],
    ["RightAlt+RightCmd", "Right Option (⌥) + Right Command (⌘)"],
    ["Ctrl+Shift+Space", "Control (⌃) + Shift (⇧) + Space"],
    ["Cmd+F5", "Command (⌘) + F5"],
    ["F13", "F13"],
    // The arrow keys start like a side, and aren't one.
    ["Ctrl+Left", "Control (⌃) + Left"],
  ])("%s reads %s", (spec, label) => {
    expect(keyLongLabel(spec)).toBe(label);
  });

  it("never shows a modifier's raw name", () => {
    for (const spec of ["RightAlt", "LeftCmd", "RightCtrl", "LeftShift"]) {
      expect(keyLongLabel(spec)).not.toBe(spec);
      expect(keyLongLabel(spec)).not.toMatch(/Alt|Cmd|Ctrl/);
    }
  });

  it("has a form in words only, for a sentence read aloud", () => {
    expect(keySpokenName("RightAlt")).toBe("Right Option");
    expect(keySpokenName("Ctrl+Shift+Space")).toBe("Control + Shift + Space");
  });

  it("leaves the keycap's short form as it was", () => {
    expect(keycapLabel("RightAlt")).toBe("Right ⌥");
  });
});
