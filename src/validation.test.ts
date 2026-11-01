import { describe, expect, it } from "vitest";
import {
  MAX_KEY_LEN,
  normalizeLanguage,
  validateHotkey,
  validateKey,
  validateLanguage,
} from "./validation";

describe("validateKey", () => {
  it("accepts a key, with the whitespace a paste leaves around it", () => {
    expect(validateKey("gsk_dummy_0123")).toBeNull();
    expect(validateKey("  gsk_dummy_0123\n")).toBeNull();
  });

  it("refuses an empty, too long or spaced key", () => {
    expect(validateKey("")).toMatch(/Paste your API key/);
    expect(validateKey("   ")).toMatch(/Paste your API key/);
    expect(validateKey("k".repeat(MAX_KEY_LEN + 1))).toMatch(/too long/);
    expect(validateKey("k".repeat(MAX_KEY_LEN))).toBeNull();
    expect(validateKey("two words")).toMatch(/no spaces/);
  });

  it("never repeats the key in its message", () => {
    expect(validateKey("secret key")).not.toContain("secret");
  });
});

describe("validateLanguage", () => {
  it("accepts empty (detect) and two or three letter codes", () => {
    for (const ok of ["", "  ", "en", "EN", " de ", "yue"]) {
      expect(validateLanguage(ok), ok).toBeNull();
    }
  });

  it("refuses anything else", () => {
    for (const bad of ["e", "english", "e1", "en-US", "e n"]) {
      expect(validateLanguage(bad), bad).toMatch(/language code/);
    }
  });

  it("normalizes to lowercase, and empty to null", () => {
    expect(normalizeLanguage(" EN ")).toBe("en");
    expect(normalizeLanguage("  ")).toBeNull();
  });
});

describe("validateHotkey", () => {
  it("accepts single keys and + combinations", () => {
    for (const ok of ["RightAlt", "F5", "Ctrl+Shift+Space", " ctrl+d "]) {
      expect(validateHotkey(ok), ok).toBeNull();
    }
  });

  it("refuses empty and malformed hotkeys", () => {
    expect(validateHotkey("")).toMatch(/Enter a hotkey/);
    for (const bad of ["Ctrl+", "+A", "Ctrl++A", "Ctrl Shift", "Ctrl-A"]) {
      expect(validateHotkey(bad), bad).toMatch(/joined by/);
    }
  });
});
