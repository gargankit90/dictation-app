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
    // OpenAI's whisper-1 is large-v2: no Cantonese, otherwise the same.
    const openai = cloudLanguages("openai");
    expect(openai).not.toContain("yue");
    expect(groq.filter((code) => code !== "yue")).toEqual(openai);
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

  it("keeps the keyboard-active option in view by moving only the list", () => {
    const scroll = vi.fn();
    Element.prototype.scrollIntoView = scroll;
    // jsdom has no layout: rows of 20 px, in a list that shows 5 of them.
    const rowAt = (element: HTMLElement) =>
      Array.from(element.parentElement?.children ?? []).indexOf(element) * 20;
    const spies = [
      vi.spyOn(HTMLElement.prototype, "offsetTop", "get").mockImplementation(function (
        this: HTMLElement,
      ) {
        return rowAt(this);
      }),
      vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(20),
      vi.spyOn(Element.prototype, "clientHeight", "get").mockReturnValue(100),
    ];
    render(<Harness initial="hi" supported={cloudLanguages("groq")} />);
    fireEvent.keyDown(box(), { key: "ArrowDown" });
    const list = screen.getByRole("listbox");
    const inView = () => {
      const active = document.getElementById(box().getAttribute("aria-activedescendant") ?? "");
      if (active === null) throw new Error("no active row");
      const top = rowAt(active);
      expect(top).toBeGreaterThanOrEqual(list.scrollTop);
      expect(top + 20).toBeLessThanOrEqual(list.scrollTop + 100);
    };
    inView();
    // Only the field and the list are revealed: a row's scrollIntoView also
    // scrolls the page, which hid the field at the smallest window.
    expect(scroll.mock.instances).toEqual([box(), list]);

    fireEvent.keyDown(box(), { key: "End" });
    inView();
    expect(list.scrollTop).toBeGreaterThan(0);
    fireEvent.keyDown(box(), { key: "Home" });
    inView();
    expect(list.scrollTop).toBe(0);
    fireEvent.change(box(), { target: { value: "ta" } });
    inView();
    expect(scroll.mock.instances).toEqual([box(), list]);
    for (const spy of spies) spy.mockRestore();
  });

  it("fits the list to the area it opens in, between three rows and its full height", () => {
    const fit = (areaHeight: number) => {
      const spy = vi.spyOn(Element.prototype, "clientHeight", "get").mockReturnValue(areaHeight);
      const { unmount } = render(
        <div className="ob-content">
          <Harness initial="hi" supported={cloudLanguages("groq")} />
        </div>,
      );
      fireEvent.keyDown(box(), { key: "ArrowDown" });
      const height = screen.getByRole("listbox").style.maxHeight;
      unmount();
      spy.mockRestore();
      return height;
    };
    expect(fit(1000)).toBe("220px");
    expect(fit(200)).toBe("168px");
    expect(fit(50)).toBe("96px");
  });

  it("reads as a picker: a chevron, hidden from screen readers, that turns when open", () => {
    render(<Harness initial="hi" supported={cloudLanguages("groq")} />);
    const chevron = document.querySelector(".language-chevron");
    expect(chevron?.getAttribute("aria-hidden")).toBe("true");
    expect(chevron?.classList.contains("open")).toBe(false);
    fireEvent.keyDown(box(), { key: "ArrowDown" });
    expect(document.querySelector(".language-chevron")?.classList.contains("open")).toBe(true);
  });

  it("typing into the closed field searches for what was typed, not the name plus it", () => {
    render(<Harness initial="hi" supported={cloudLanguages("groq")} />);
    expect(box().value).toBe("Hindi (हिन्दी)");
    fireEvent.change(box(), { target: { value: "Hindi (हिन्दी)fr" } });
    expect(box().value).toBe("fr");
    expect(screen.getAllByRole("option").map((o) => o.textContent)).toContain("French (Français)");
  });
});
