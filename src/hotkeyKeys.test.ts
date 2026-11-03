import { describe, expect, it } from "vitest";
import { INITIAL_RECORDER, keycapLabel, recorderStep } from "./hotkeyKeys";
import type { RecorderOutcome } from "./hotkeyKeys";

/** Plays `[type, code]` events and returns the outcome of the last one. */
function play(...events: ["down" | "up", string][]): RecorderOutcome | undefined {
  let state = INITIAL_RECORDER;
  let outcome: RecorderOutcome | undefined;
  for (const [type, code] of events) {
    const step = recorderStep(state, { type, code });
    state = step.state;
    outcome = step.outcome;
  }
  return outcome;
}

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

  it("starts afresh after a refusal", () => {
    const first = recorderStep(INITIAL_RECORDER, { type: "down", code: "KeyA" });
    expect(first.outcome?.kind).toBe("refused");
    expect(first.state).toEqual(INITIAL_RECORDER);
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
