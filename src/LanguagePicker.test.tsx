// U2 criterion 4: search, keyboard selection and the stored value.

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { LanguagePicker } from "./LanguagePicker";
import { cloudLanguages, languageLabel } from "./languages";

afterEach(() => {
  cleanup();
  chosen.length = 0;
});

/** Every value the picker reported, in order. */
const chosen: string[] = [];
const stored = () => chosen.at(-1);

function Harness({
  initial = "",
  supported = ["en", "de", "hi", "fr"],
}: {
  initial?: string;
  supported?: readonly string[];
}) {
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
    for (const code of cloudLanguages("groq"))
      expect(languageLabel(code)).not.toMatch(/Unrecognised/);
  });

  it("covers the 99 Whisper languages for Groq, including Bengali and Telugu", () => {
    const groq = cloudLanguages("groq");
    expect(groq.length).toBeGreaterThanOrEqual(99);
    expect(new Set(groq).size).toBe(groq.length);
    for (const code of ["bn", "te", "yue", "haw", "su"]) expect(groq).toContain(code);
    expect(cloudLanguages("openai")).toEqual(groq);
  });

  it("finds Hindi by its full native name, a prefix with vowel signs, and either Unicode form", () => {
    render(<Harness />);
    fireEvent.focus(box());
    for (const query of ["हिन्दी", "हिन", "हि", "हिन्दी".normalize("NFD"), "HINDI"]) {
      fireEvent.change(box(), { target: { value: query } });
      expect(screen.getAllByRole("option").map((o) => o.textContent)).toEqual(["Hindi (हिन्दी)"]);
    }
    fireEvent.change(box(), { target: { value: "francais" } });
    expect(screen.getAllByRole("option").map((o) => o.textContent)).toEqual(["French (Français)"]);
  });

  it("scrolls the keyboard-active option into view on open, arrows, End and typing", () => {
    const scroll = vi.fn();
    Element.prototype.scrollIntoView = scroll;
    render(<Harness initial="hi" supported={cloudLanguages("groq")} />);
    fireEvent.keyDown(box(), { key: "ArrowDown" });
    expect(scroll).toHaveBeenCalled();
    const opened = document.getElementById(box().getAttribute("aria-activedescendant") ?? "");
    expect(opened?.textContent).toBe("Hindi (हिन्दी)");
    expect(scroll.mock.instances.at(-1)).toBe(opened);

    scroll.mockClear();
    fireEvent.keyDown(box(), { key: "ArrowDown" });
    const next = document.getElementById(box().getAttribute("aria-activedescendant") ?? "");
    expect(next).not.toBe(opened);
    expect(scroll.mock.instances.at(-1)).toBe(next);

    scroll.mockClear();
    fireEvent.keyDown(box(), { key: "End" });
    const last = document.getElementById(box().getAttribute("aria-activedescendant") ?? "");
    expect(scroll.mock.instances.at(-1)).toBe(last);

    scroll.mockClear();
    fireEvent.change(box(), { target: { value: "ta" } });
    expect(scroll).toHaveBeenCalled();
  });
});
