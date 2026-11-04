// M5.1 criterion 1, U4: the overlay renders the recording pill's states
// (Listening with the level meter, No sound, Transcribing, Pasted, Error) and
// nothing while hidden; it follows the events the Rust side sends.

import { act, cleanup, render, screen } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import overlayCss from "./overlay.css?raw";
import { LISTENING_HINT, Overlay, OverlayContent } from "./Overlay";
import {
  BAR_MAX_PX,
  BAR_MIN_PX,
  FLOOR_DBFS,
  LISTENING_TITLE,
  METER_BARS,
  OVERLAY_EVENT,
  OVERLAY_LEVEL_EVENT,
  SILENT_LEVELS,
  barHeight,
  parseLevel,
  parseView,
  pushLevel,
} from "./view";
import type { OverlayView } from "./view";

const MESSAGE = "No speech was recognised.";

afterEach(async () => {
  cleanup();
  // Unmounting unlistens asynchronously, through the mocked IPC; let that
  // finish before the mocks go.
  await new Promise((resolve) => setTimeout(resolve, 0));
  clearMocks();
});

/** The meter's bar heights, in px, left to right. */
function barHeights(): number[] {
  return screen.queryAllByTestId("overlay-bar").map((bar) => Number.parseInt(bar.style.height, 10));
}

const ALL_VIEWS: OverlayView[] = [
  { state: "recording" },
  { state: "no_sound" },
  { state: "no_sound", device: "MacBook Pro Microphone" },
  { state: "transcribing" },
  { state: "pasted" },
  { state: "error", message: MESSAGE },
];

describe("criterion_m5_1_1 criterion_u4_3 OverlayContent", () => {
  it("draws nothing while hidden", () => {
    const { container } = render(<OverlayContent view={{ state: "hidden" }} />);
    expect(container.innerHTML).toBe("");
  });

  it("shows Listening with the hint and a seven-bar meter", () => {
    render(<OverlayContent view={{ state: "recording" }} />);
    const status = screen.getByRole("status");
    expect(status.dataset.state).toBe("recording");
    expect(status.querySelector(".overlay-label")?.textContent).toBe("Listening");
    expect(status.querySelector(".overlay-hint")?.textContent).toBe(
      "Release to paste · Esc to cancel",
    );
    expect(LISTENING_HINT).toBe("Release to paste · Esc to cancel");
    // A flat meter before any level.
    expect(barHeights()).toEqual(Array.from({ length: METER_BARS }, () => BAR_MIN_PX));
  });

  it("draws each bar from its level, newest on the right", () => {
    const levels = [-100, -60, -35, -10, 0, -47.5, -22.5];
    render(<OverlayContent view={{ state: "recording" }} levels={levels} />);
    expect(barHeights()).toEqual([4, 4, 11, 18, 18, 8, 15]);
  });

  it("hides the meter from assistive tech", () => {
    const { container } = render(<OverlayContent view={{ state: "recording" }} />);
    const meter = container.querySelector(".overlay-meter");
    expect(meter?.getAttribute("aria-hidden")).toBe("true");
    expect(screen.getByRole("status").textContent).toBe(`Listening${LISTENING_HINT}`);
  });

  it("shows No sound with an idle meter", () => {
    const { container } = render(<OverlayContent view={{ state: "no_sound" }} />);
    const status = screen.getByRole("status");
    expect(status.dataset.state).toBe("no_sound");
    expect(status.querySelector(".overlay-label")?.textContent).toBe("No sound");
    expect(status.querySelector(".overlay-hint")?.textContent).toBe("Check your microphone");
    expect(container.querySelector(".overlay-meter-idle")).not.toBeNull();
  });

  it("names the microphone in No sound when it is known", () => {
    render(<OverlayContent view={{ state: "no_sound", device: "MacBook Pro Microphone" }} />);
    expect(screen.getByRole("status").querySelector(".overlay-hint")?.textContent).toBe(
      "Check your microphone (MacBook Pro Microphone)",
    );
  });

  it("shows Transcribing", () => {
    render(<OverlayContent view={{ state: "transcribing" }} />);
    const status = screen.getByRole("status");
    expect(status.textContent).toBe("Transcribing…");
    expect(status.dataset.state).toBe("transcribing");
  });

  it("shows Pasted with a check", () => {
    render(<OverlayContent view={{ state: "pasted" }} />);
    const status = screen.getByRole("status");
    expect(status.dataset.state).toBe("pasted");
    expect(status.querySelector(".overlay-label")?.textContent).toBe("Pasted");
    expect(status.querySelector(".overlay-check")?.textContent).toBe("✓");
  });

  // U4 criterion 6: the message and the amber badge, nothing else.
  it("shows an error as its one message and the badge, with no code", () => {
    render(<OverlayContent view={{ state: "error", message: MESSAGE }} />);
    const alert = screen.getByRole("alert");
    expect(alert.dataset.state).toBe("error");
    expect(alert.querySelector(".overlay-icon")?.textContent).toBe("!");
    expect(alert.querySelector(".overlay-message")?.textContent).toBe(MESSAGE);
    expect(alert.textContent).toBe(`!${MESSAGE}`);
    expect(screen.queryByRole("status")).toBeNull();
  });

  // U4 criterion 6: the overlay never takes focus, so nothing in it can.
  it.each(ALL_VIEWS)("holds no button, link or focusable element: %j", (view) => {
    const { container } = render(<OverlayContent view={view} />);
    expect(container.firstElementChild).not.toBeNull();
    expect(
      container.querySelectorAll("button, a, input, select, textarea, [tabindex]"),
    ).toHaveLength(0);
  });
});

describe("criterion_u4_3 the meter", () => {
  it("maps -60 dBFS to 4 px and -10 dBFS to 18 px, clamped", () => {
    expect(barHeight(-60)).toBe(BAR_MIN_PX);
    expect(barHeight(-10)).toBe(BAR_MAX_PX);
    expect(barHeight(FLOOR_DBFS)).toBe(4);
    expect(barHeight(0)).toBe(18);
    expect(barHeight(-35)).toBe(11);
  });

  it("scrolls levels right to left, seven at a time", () => {
    let levels: readonly number[] = SILENT_LEVELS;
    for (const dbfs of [-10, -20, -30]) {
      levels = pushLevel(levels, dbfs);
    }
    expect(levels).toHaveLength(METER_BARS);
    expect(levels.slice(-3)).toEqual([-10, -20, -30]);
    expect(levels[0]).toBe(FLOOR_DBFS);
  });

  // jsdom doesn't apply media queries, so this reads the stylesheet.
  it("has no transitions on the bars with Reduce Motion", () => {
    const reduced = /@media \(prefers-reduced-motion: reduce\) \{([\s\S]*?)\n\}/.exec(overlayCss);
    expect(reduced).not.toBeNull();
    const body = reduced?.[1] ?? "";
    expect(body).toMatch(/\.overlay-bar \{\s*transition: none;\s*\}/);
    // The only transition in the file is the bars' own.
    expect(overlayCss.match(/transition:/g)).toHaveLength(2);
  });
});

describe("criterion_m5_1_1 criterion_u4_2 Overlay follows the overlay events", () => {
  beforeEach(() => {
    // No commands: the overlay only listens (criterion 2).
    mockIPC(
      (command) => {
        throw new Error(`the overlay must not call ${command}`);
      },
      { shouldMockEvents: true },
    );
  });

  async function send(view: unknown) {
    await act(async () => {
      await emit(OVERLAY_EVENT, view);
    });
  }

  async function level(dbfs: unknown) {
    await act(async () => {
      await emit(OVERLAY_LEVEL_EVENT, { dbfs });
    });
  }

  it("starts hidden, then shows each state the app sends", async () => {
    const { container } = render(<Overlay />);
    expect(container.innerHTML).toBe("");

    await send({ state: "recording" });
    expect(screen.getByRole("status").dataset.state).toBe("recording");

    await send({ state: "no_sound", device: "USB Mic" });
    expect(screen.getByRole("status").textContent).toContain("Check your microphone (USB Mic)");

    await send({ state: "transcribing" });
    expect(screen.getByRole("status").textContent).toBe("Transcribing…");

    await send({ state: "pasted" });
    expect(screen.getByRole("status").textContent).toContain("Pasted");

    await send({ state: "error", message: MESSAGE });
    expect(screen.getByRole("alert").textContent).toContain(MESSAGE);

    await send({ state: "hidden" });
    expect(container.innerHTML).toBe("");
  });

  it("draws the levels it hears while recording", async () => {
    render(<Overlay />);
    await send({ state: "recording" });
    for (const dbfs of [-60, -35, -10]) {
      await level(dbfs);
    }
    expect(barHeights()).toEqual([4, 4, 4, 4, 4, 11, 18]);
    // Junk is ignored.
    await level("loud");
    await level(Number.NaN);
    expect(barHeights()).toEqual([4, 4, 4, 4, 4, 11, 18]);

    // No sound keeps the meter, now idle, and keeps following the level.
    await send({ state: "no_sound" });
    await level(-90);
    expect(barHeights()).toEqual([4, 4, 4, 4, 11, 18, 4]);
  });

  it("ignores levels outside a recording and starts each recording flat", async () => {
    render(<Overlay />);
    await level(-10);
    await send({ state: "recording" });
    expect(barHeights()).toEqual(Array.from({ length: METER_BARS }, () => 4));
    await level(-10);
    await send({ state: "transcribing" });
    await level(-10);
    await send({ state: "recording" });
    expect(barHeights()).toEqual(Array.from({ length: METER_BARS }, () => 4));
  });

  // #119: an event sent before the listener is registered is lost, so the
  // page says it listens through its title and the app sends the view again.
  it("says through its title once it listens, with a new title each time", async () => {
    // The same text as LISTENING_TITLE in src-tauri/src/overlay.rs.
    expect(LISTENING_TITLE).toBe("dictation-app overlay listening");
    document.title = "dictation-app overlay";
    const first = render(<Overlay />);
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    const title = document.title;
    expect(title.startsWith(`${LISTENING_TITLE} `)).toBe(true);
    // And it does listen by then, to both events.
    await send({ state: "recording" });
    await level(-10);
    expect(screen.getByRole("status").dataset.state).toBe("recording");
    expect(barHeights().at(-1)).toBe(18);
    first.unmount();

    render(<Overlay />);
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    expect(document.title.startsWith(`${LISTENING_TITLE} `)).toBe(true);
    expect(document.title).not.toBe(title);
  });

  it("doesn't announce a listener it has already dropped", async () => {
    document.title = "dictation-app overlay";
    const { unmount } = render(<Overlay />);
    unmount();
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    expect(document.title).toBe("dictation-app overlay");
  });

  it("stops listening once unmounted", async () => {
    const { container, unmount } = render(<Overlay />);
    await send({ state: "recording" });
    unmount();
    await send({ state: "transcribing" });
    await level(-10);
    expect(container.innerHTML).toBe("");
  });
});

describe("parseView", () => {
  const cases: [unknown, OverlayView][] = [
    [{ state: "recording" }, { state: "recording" }],
    [{ state: "no_sound" }, { state: "no_sound" }],
    [
      { state: "no_sound", device: "MacBook Pro Microphone" },
      { state: "no_sound", device: "MacBook Pro Microphone" },
    ],
    [{ state: "no_sound", device: "" }, { state: "no_sound" }],
    [{ state: "no_sound", device: 7 }, { state: "no_sound" }],
    [{ state: "transcribing" }, { state: "transcribing" }],
    [{ state: "pasted" }, { state: "pasted" }],
    [
      { state: "error", message: MESSAGE },
      { state: "error", message: MESSAGE },
    ],
    // A code, if one ever came, isn't kept (U4 criterion 6).
    [
      { state: "error", message: MESSAGE, code: "mic_denied" },
      { state: "error", message: MESSAGE },
    ],
    [{ state: "hidden" }, { state: "hidden" }],
    // Anything unexpected hides the overlay rather than showing junk.
    [{ state: "error" }, { state: "hidden" }],
    [{ state: "error", message: 42 }, { state: "hidden" }],
    [{ state: "dancing" }, { state: "hidden" }],
    [{}, { state: "hidden" }],
    ["recording", { state: "hidden" }],
    [null, { state: "hidden" }],
  ];
  it.each(cases)("%j -> %j", (payload, expected) => {
    expect(parseView(payload)).toEqual(expected);
  });
});

describe("parseLevel", () => {
  const cases: [unknown, number | null][] = [
    [{ dbfs: -32.5 }, -32.5],
    [{ dbfs: -100 }, -100],
    [{ dbfs: "loud" }, null],
    [{ dbfs: Number.POSITIVE_INFINITY }, null],
    [{}, null],
    [-20, null],
    [null, null],
  ];
  it.each(cases)("%j -> %j", (payload, expected) => {
    expect(parseLevel(payload)).toBe(expected);
  });
});
