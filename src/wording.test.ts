import { describe, expect, it } from "vitest";

// U12 criterion 6: "dictation key" in all user-facing text. Identifiers and
// analytics names stay. Setup's Try it screen is U11's, and its copy is out of
// scope, but it holds neither word either, so it stays in the scan.
const sources = import.meta.glob<string>(
  ["/src/**/*.{ts,tsx}", "!/src/**/*.test.{ts,tsx}", "!/src/bindings.ts", "!/src/testKeys.ts"],
  { query: "?raw", import: "default", eager: true },
);

// Not class names such as `hotkey-recorder`.
const WORD = /\b(hotkey|shortcut)s?\b(?!-)/i;

/** Text a person could read: JSX text, and string literals with a space in them. */
function readable(source: string): string[] {
  const withoutComments = source.replace(/\/\*[\s\S]*?\*\//g, "").replace(/^\s*\/\/.*$/gm, "");
  const literals = [...withoutComments.matchAll(/"([^"\n]* [^"\n]*)"|`([^`\n]* [^`\n]*)`/g)].map(
    (found) => found[1] ?? found[2] ?? "",
  );
  const jsx = [...withoutComments.matchAll(/>\s*([^<>{}]*[A-Za-z][^<>{}]*)\s*</g)].map(
    (found) => found[1] ?? "",
  );
  return [...literals, ...jsx];
}

describe("u12_criterion_6 one word for the key", () => {
  it("finds the word when it is there", () => {
    expect(readable('label="Your hotkey is"').some((text) => WORD.test(text))).toBe(true);
    expect(readable("<p>The shortcut works</p>").some((text) => WORD.test(text))).toBe(true);
    expect(readable('saveHotkey("RightAlt"); // the hotkey').some((text) => WORD.test(text))).toBe(
      false,
    );
  });

  it("no user-facing string in src/ says hotkey or shortcut", () => {
    const offenders = Object.entries(sources).flatMap(([path, source]) =>
      readable(source)
        .filter((text) => WORD.test(text))
        .map((text) => `${path}: ${text}`),
    );
    expect(offenders).toEqual([]);
  });
});
