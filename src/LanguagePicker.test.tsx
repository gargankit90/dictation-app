// U2 criterion 4: search, keyboard selection and the stored value.

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it } from "vitest";
import { LanguagePicker } from "./LanguagePicker";
import { CLOUD_LANGUAGES, languageLabel } from "./languages";

afterEach(() => {
  cleanup();
  chosen.length = 0;
});

/** Every value the picker reported, in order. */
const chosen: string[] = [];
const stored = () => chosen.at(-1);

function Harness({ initial = "", supported = ["en", "de", "hi", "fr"] }) {
  const [value, setValue] = useState(initial);
  return (
    <LanguagePicker
      id="language"
      value={value}
      onChange={(next) => {
        chosen.push(next);
        setValue(next);
      }}
      supported={supported}
      engineName="Test engine"
    />
  );
}

const box = () => screen.getByRole<HTMLInputElement>("combobox");

describe("LanguagePicker", () => {
  it("lists Detect automatically first, then names, never codes", () => {
    render(<Harness />);
    fireEvent.focus(box());
    const names = screen.getAllByRole("option").map((o) => o.textContent);
    expect(names).toEqual([
      "Detect automatically",
      "English",
      "French (Français)",
      "German (Deutsch)",
      "Hindi (हिन्दी)",
    ]);
    for (const name of names) expect(name).not.toMatch(/\b(en|de|hi|fr)\b/);
  });

  it("searches by typing: 'hin' finds Hindi, and Enter stores its code", () => {
    render(<Harness />);
    fireEvent.focus(box());
    fireEvent.change(box(), { target: { value: "hin" } });
    expect(screen.getAllByRole("option").map((o) => o.textContent)).toEqual(["Hindi (हिन्दी)"]);
    fireEvent.keyDown(box(), { key: "Enter" });
    expect(stored()).toBe("hi");
    expect(box().value).toBe("Hindi (हिन्दी)");
    expect(screen.queryByRole("listbox")).toBeNull();
  });

  it("matches the start of a word, so 'hin' does not find Chinese", () => {
    render(<Harness supported={["zh", "hi"]} />);
    fireEvent.focus(box());
    fireEvent.change(box(), { target: { value: "hin" } });
    expect(screen.getAllByRole("option").map((o) => o.textContent)).toEqual(["Hindi (हिन्दी)"]);
  });

  it("also finds a language by its own name", () => {
    render(<Harness />);
    fireEvent.focus(box());
    fireEvent.change(box(), { target: { value: "deutsch" } });
    expect(screen.getAllByRole("option").map((o) => o.textContent)).toEqual(["German (Deutsch)"]);
  });

  it("works from the keyboard: arrows move, Enter picks, Escape closes without changing", () => {
    render(<Harness initial="en" />);
    fireEvent.keyDown(box(), { key: "ArrowDown" });
    expect(box().getAttribute("aria-expanded")).toBe("true");
    // Opens on the current choice (English, index 1); two steps down is German.
    fireEvent.keyDown(box(), { key: "ArrowDown" });
    fireEvent.keyDown(box(), { key: "ArrowDown" });
    const active = box().getAttribute("aria-activedescendant");
    expect(document.getElementById(active ?? "")?.textContent).toBe("German (Deutsch)");
    fireEvent.keyDown(box(), { key: "Enter" });
    expect(stored()).toBe("de");

    fireEvent.keyDown(box(), { key: "ArrowDown" });
    fireEvent.keyDown(box(), { key: "Escape" });
    expect(screen.queryByRole("listbox")).toBeNull();
    expect(stored()).toBe("de");
  });

  it("Detect automatically stores no language", () => {
    render(<Harness initial="de" />);
    fireEvent.focus(box());
    fireEvent.mouseDown(screen.getByRole("option", { name: "Detect automatically" }));
    expect(stored()).toBe("");
    expect(box().value).toBe("Detect automatically");
  });

  it("gives every option an accessible name and marks the selected one", () => {
    render(<Harness initial="fr" />);
    fireEvent.focus(box());
    for (const option of screen.getAllByRole("option")) {
      expect(option.textContent).toBeTruthy();
    }
    expect(
      screen.getByRole("option", { name: "French (Français)" }).getAttribute("aria-selected"),
    ).toBe("true");
  });

  it("flags an unsupported saved code and keeps it", () => {
    render(<Harness initial="ja" />);
    expect(box().value).toBe("Japanese, not supported by Test engine");
    expect(chosen).toEqual([]); // nothing was reported, so the value is untouched
  });

  it("names every cloud language", () => {
    for (const code of CLOUD_LANGUAGES) expect(languageLabel(code)).not.toMatch(/Unrecognised/);
  });
});
