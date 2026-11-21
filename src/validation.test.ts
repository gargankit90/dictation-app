import { describe, expect, it } from "vitest";
import { MAX_KEY_LEN, validateKey } from "./validation";

describe("validateKey", () => {
  it("accepts a key, with the whitespace a paste leaves around it", () => {
    expect(validateKey("sk-or-dummy-0123")).toBeNull();
    expect(validateKey("  sk-or-dummy-0123\n")).toBeNull();
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
