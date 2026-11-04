// M5.1 criterion 1: the overlay renders Recording, Transcribing and Error,
// and nothing while hidden; it follows the events the Rust side sends.

import { act, cleanup, render, screen } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { Overlay, OverlayContent } from "./Overlay";
import { LISTENING_TITLE, OVERLAY_EVENT, parseView } from "./view";
import type { OverlayView } from "./view";

const MESSAGE = "No speech was recognised.";

afterEach(async () => {
  cleanup();
  // Unmounting unlistens asynchronously, through the mocked IPC; let that
  // finish before the mocks go.
  await new Promise((resolve) => setTimeout(resolve, 0));
  clearMocks();
});

describe("criterion_m5_1_1 OverlayContent", () => {
  it("draws nothing while hidden", () => {
    const { container } = render(<OverlayContent view={{ state: "hidden" }} />);
    expect(container.innerHTML).toBe("");
  });

  it("shows Recording", () => {
    render(<OverlayContent view={{ state: "recording" }} />);
    const status = screen.getByRole("status");
    expect(status.textContent).toBe("Recording");
    expect(status.dataset.state).toBe("recording");
  });

  it("shows Transcribing", () => {
    render(<OverlayContent view={{ state: "transcribing" }} />);
    const status = screen.getByRole("status");
    expect(status.textContent).toBe("Transcribing…");
    expect(status.dataset.state).toBe("transcribing");
  });

  it("shows one sentence and no error code (U1)", () => {
    // Even if a payload carried a code, parseView drops it and nothing renders it.
    const view = parseView({ state: "error", message: MESSAGE, code: "mic_denied" });
    render(<OverlayContent view={view} />);
    expect(screen.getByRole("alert").textContent).toBe(MESSAGE);
  });

  it("shows an error with its message", () => {
    render(<OverlayContent view={{ state: "error", message: MESSAGE }} />);
    const alert = screen.getByRole("alert");
    expect(alert.textContent).toContain(MESSAGE);
    expect(alert.dataset.state).toBe("error");
    expect(screen.queryByRole("status")).toBeNull();
  });
});

describe("criterion_m5_1_1 Overlay follows the overlay event", () => {
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

  it("starts hidden, then shows each state the app sends", async () => {
    const { container } = render(<Overlay />);
    expect(container.innerHTML).toBe("");

    await send({ state: "recording" });
    expect(screen.getByRole("status").textContent).toBe("Recording");

    await send({ state: "transcribing" });
    expect(screen.getByRole("status").textContent).toBe("Transcribing…");

    await send({ state: "error", message: MESSAGE });
    expect(screen.getByRole("alert").textContent).toContain(MESSAGE);

    await send({ state: "hidden" });
    expect(container.innerHTML).toBe("");
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
    // And it does listen by then.
    await send({ state: "recording" });
    expect(screen.getByRole("status").textContent).toBe("Recording");
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
    expect(container.innerHTML).toBe("");
  });
});

describe("parseView", () => {
  const cases: [unknown, OverlayView][] = [
    [{ state: "recording" }, { state: "recording" }],
    [{ state: "transcribing" }, { state: "transcribing" }],
    [
      { state: "error", message: MESSAGE },
      { state: "error", message: MESSAGE },
    ],
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
