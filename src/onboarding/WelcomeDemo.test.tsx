// U11 criterion 2: Welcome's demo is the real pill on a fixed script.

import { act, cleanup, render } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { barHeight, BAR_MAX_PX, BAR_MIN_PX, METER_BARS } from "../overlay/view";
import { LISTENING_HINT } from "../overlay/Overlay";
import { setReducedMotion } from "../testSetup";
import {
  DEMO_FRAMES,
  DEMO_TICK_MS,
  DEMO_WORDS,
  DONE_END_MS,
  FINAL_FRAME,
  HOLD_END_MS,
  LOOP_MS,
  SPEAK_END_MS,
  SPEECH_LEVELS,
  TRANSCRIBE_END_MS,
  frameAt,
} from "./demoScript";
import demoScript from "./demoScript.ts?raw";
import { WelcomeDemo } from "./WelcomeDemo";
import demoView from "./WelcomeDemo.tsx?raw";

function pill(): HTMLElement | null {
  return document.querySelector<HTMLElement>(".ob-demo-pill .overlay");
}

function words(): string | null {
  return document.querySelector(".ob-demo-words")?.textContent ?? null;
}

function lit(): string[] {
  return [...document.querySelectorAll(".ob-beat.lit")].map(
    (beat) => beat.querySelector("span")?.textContent ?? "",
  );
}

function bars(): number[] {
  return [...document.querySelectorAll<HTMLElement>(".overlay-bar")].map((bar) =>
    Number.parseInt(bar.style.height, 10),
  );
}

/** Moves the loop to `ms` from where it is. */
function advance(ms: number) {
  act(() => {
    vi.advanceTimersByTime(ms);
  });
}

describe("the script", () => {
  it("has a level for each tick of the speaking part, and no more", () => {
    expect(SPEECH_LEVELS).toHaveLength((SPEAK_END_MS - HOLD_END_MS) / DEMO_TICK_MS);
    expect(DEMO_FRAMES).toHaveLength(LOOP_MS / DEMO_TICK_MS);
  });

  it("is about 8 s, with about 1 s of Transcribing… and 0.8 s of Done", () => {
    expect(LOOP_MS).toBe(8000);
    expect(TRANSCRIBE_END_MS - SPEAK_END_MS).toBe(1000);
    expect(DONE_END_MS - TRANSCRIBE_END_MS).toBe(800);
  });

  it("moves like speech: syllables that use the meter's height, with short gaps", () => {
    const heights = SPEECH_LEVELS.map(barHeight);
    expect(Math.max(...heights)).toBe(BAR_MAX_PX);
    expect(Math.min(...heights)).toBeLessThanOrEqual(BAR_MIN_PX + 1);
    const quiet = heights.map((height) => height <= BAR_MIN_PX + 3);
    // Gaps: separate quiet runs, each short.
    let run = 0;
    let runs = 0;
    for (const isQuiet of quiet) {
      if (isQuiet) run += 1;
      else if (run > 0) {
        expect(run).toBeLessThanOrEqual(4);
        runs += 1;
        run = 0;
      }
    }
    expect(runs).toBeGreaterThanOrEqual(5);
    // Never the same height for long.
    expect(new Set(heights).size).toBeGreaterThan(8);
  });

  it("is fixed: no random numbers anywhere in the demo", () => {
    expect(demoScript).not.toContain("Math.random");
    expect(demoView).not.toContain("Math.random");
    expect(frameAt(1500)).toBe(frameAt(1500));
    expect(frameAt(1500 + LOOP_MS)).toBe(frameAt(1500));
  });

  it("uses only the pill's drawing, nothing that listens or reads a level", () => {
    expect(demoView).toContain('import { OverlayContent } from "../overlay/Overlay";');
    expect(demoView).not.toContain("<Overlay ");
    expect(demoView).not.toMatch(/invoke|listen\(|commands\.|LEVEL_COMMAND/);
  });

  it("keeps one object for frames that look the same, so nothing renders for them", () => {
    expect(frameAt(0)).toBe(frameAt(HOLD_END_MS - 1));
    expect(frameAt(SPEAK_END_MS)).toBe(frameAt(TRANSCRIBE_END_MS - 1));
    expect(frameAt(DONE_END_MS)).toBe(frameAt(LOOP_MS - 1));
  });
});

describe("the demo", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    // Any call to the app fails the test that made it.
    mockIPC((command) => {
      throw new Error(`the demo called ${command}`);
    });
  });

  afterEach(() => {
    cleanup();
    clearMocks();
    vi.useRealTimers();
  });

  it("plays the real pill in each state, with its hint, in order", () => {
    render(<WelcomeDemo keyLabel="Right Option (⌥)" />);

    // Hold: Listening, the real hint, bars flat.
    expect(pill()?.dataset.state).toBe("recording");
    expect(pill()?.querySelector(".overlay-label")?.textContent).toBe("Listening");
    expect(pill()?.querySelector(".overlay-hint")?.textContent).toBe(LISTENING_HINT);
    expect(pill()?.querySelector(".overlay-dot")).not.toBeNull();
    expect(bars()).toEqual(Array.from({ length: METER_BARS }, () => BAR_MIN_PX));
    expect(lit()).toEqual(["Hold the key"]);
    expect(words()).toBeNull();

    // Speak: the bars follow the list.
    advance(HOLD_END_MS + 1000);
    expect(pill()?.dataset.state).toBe("recording");
    expect(lit()).toEqual(["Say what you mean"]);
    expect(Math.max(...bars())).toBeGreaterThan(BAR_MIN_PX + 4);
    expect(words()).toBeNull();

    // Let go: Transcribing…, still no words.
    advance(SPEAK_END_MS - HOLD_END_MS - 1000 + 100);
    expect(pill()?.dataset.state).toBe("transcribing");
    expect(pill()?.querySelector(".overlay-label")?.textContent).toBe("Transcribing…");
    expect(pill()?.querySelector(".overlay-spinner")).not.toBeNull();
    expect(lit()).toEqual(["Let go. The words land at your cursor"]);
    expect(words()).toBeNull();

    // The words land as it shows the check and its real label.
    advance(TRANSCRIBE_END_MS - SPEAK_END_MS);
    expect(pill()?.dataset.state).toBe("paste_sent");
    expect(pill()?.querySelector(".overlay-label")?.textContent).toBe("Done");
    expect(pill()?.querySelector(".overlay-check")).not.toBeNull();
    expect(words()).toBe(DEMO_WORDS);

    // Then the pill goes and the words stay.
    advance(DONE_END_MS - TRANSCRIBE_END_MS);
    expect(pill()).toBeNull();
    expect(words()).toBe(DEMO_WORDS);

    // And it goes round.
    advance(LOOP_MS - DONE_END_MS);
    expect(pill()?.dataset.state).toBe("recording");
    expect(words()).toBeNull();
    expect(lit()).toEqual(["Hold the key"]);
  });

  it("stops when the step is left: no timers leak", () => {
    const { unmount } = render(<WelcomeDemo keyLabel="Right Option (⌥)" />);
    advance(500);
    expect(vi.getTimerCount()).toBeGreaterThan(0);
    unmount();
    expect(vi.getTimerCount()).toBe(0);
  });

  it("with Reduce Motion: the final frame, all three captions, no timers", () => {
    setReducedMotion(true);
    render(<WelcomeDemo keyLabel="Right Option (⌥)" />);
    expect(vi.getTimerCount()).toBe(0);
    expect(pill()).toBeNull();
    expect(words()).toBe(DEMO_WORDS);
    expect(lit()).toEqual([
      "Hold the key",
      "Say what you mean",
      "Let go. The words land at your cursor",
    ]);
    advance(LOOP_MS * 2);
    expect(vi.getTimerCount()).toBe(0);
    expect(words()).toBe(DEMO_WORDS);
    expect(FINAL_FRAME.beat).toBeNull();
  });

  it("follows Reduce Motion when it changes while the screen is open", () => {
    render(<WelcomeDemo keyLabel="Right Option (⌥)" />);
    expect(vi.getTimerCount()).toBeGreaterThan(0);
    act(() => {
      setReducedMotion(true);
    });
    expect(vi.getTimerCount()).toBe(0);
    expect(pill()).toBeNull();
    act(() => {
      setReducedMotion(false);
    });
    expect(vi.getTimerCount()).toBeGreaterThan(0);
    expect(pill()?.dataset.state).toBeTruthy();
  });

  it("is hidden from assistive tech as a whole", () => {
    const { container } = render(<WelcomeDemo keyLabel="Right Option (⌥)" />);
    expect(container.firstElementChild?.getAttribute("aria-hidden")).toBe("true");
  });

  it("shows the key it's given", () => {
    const { container } = render(<WelcomeDemo keyLabel="Right Command (⌘)" />);
    expect(container.querySelector("kbd")?.textContent).toBe("Right Command (⌘)");
  });
});
