// K1: the dictation-key recorder, with a fake `onChange` standing in for the
// app. Key events go to the window, as the real ones do.

import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { HotkeyRecorder, RENEW_MS } from "./HotkeyRecorder";
import { holdSilently, keyDown, keyUp, releaseAllKeys } from "./testKeys";

afterEach(() => {
  releaseAllKeys();
  cleanup();
  vi.useRealTimers();
});

const down = keyDown;
const up = keyUp;
const record = () => {
  fireEvent.click(screen.getByRole("button", { name: "Record a different key" }));
};

function setup(value = "RightAlt", result: string | null = null) {
  const onChange = vi.fn<(spec: string) => Promise<string | null>>(() => Promise.resolve(result));
  const onRecording = vi.fn<(recording: boolean) => void>();
  render(<HotkeyRecorder value={value} onChange={onChange} onRecording={onRecording} />);
  recording = onRecording;
  return onChange;
}

/** The last `setup`'s recording notices. */
let recording = vi.fn<(recording: boolean) => void>();

describe("HotkeyRecorder", () => {
  it("shows the current key as a keycap", () => {
    setup("RightCmd");
    expect(screen.getByLabelText("Dictation key").textContent).toBe("Right ⌘");
  });

  it("shows a combination as one keycap", () => {
    setup("Ctrl+Shift+Space");
    expect(screen.getByLabelText("Dictation key").textContent).toBe("⌃⇧Space");
  });

  it("records a modifier pressed and released alone", async () => {
    const onChange = setup();
    record();
    expect(screen.getByLabelText("Dictation key").textContent).toBe("Press a key…");
    down("MetaRight");
    up("MetaRight");
    await waitFor(() => {
      expect(onChange).toHaveBeenCalledWith("RightCmd");
    });
    expect(screen.queryByRole("status")).toBeNull();
  });

  it("records a combination", async () => {
    const onChange = setup();
    record();
    down("ControlLeft");
    down("ShiftRight");
    down("Space");
    await waitFor(() => {
      expect(onChange).toHaveBeenCalledWith("Ctrl+Shift+Space");
    });
  });

  it("cancels on Esc and keeps the key", () => {
    const onChange = setup("RightCmd");
    record();
    down("Escape");
    expect(onChange).not.toHaveBeenCalled();
    expect(screen.getByLabelText("Dictation key").textContent).toBe("Right ⌘");
    // Nothing is listening now.
    down("F5");
    expect(onChange).not.toHaveBeenCalled();
  });

  it("cancels with the Cancel button", () => {
    const onChange = setup();
    record();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    down("F5");
    expect(onChange).not.toHaveBeenCalled();
  });

  it("refuses a plain letter with a reason, and goes on listening", async () => {
    const onChange = setup();
    record();
    down("KeyA");
    expect((await screen.findByRole("alert")).textContent).toMatch(/Hold Ctrl, Option or Command/u);
    expect(onChange).not.toHaveBeenCalled();
    down("F6");
    await waitFor(() => {
      expect(onChange).toHaveBeenCalledWith("F6");
    });
  });

  it("shows the app's message when it can't use the key", async () => {
    const onChange = setup(
      "RightAlt",
      "Couldn't switch the dictation key to F6. RightAlt still works.",
    );
    record();
    down("F6");
    expect((await screen.findByRole("alert")).textContent).toContain("RightAlt still works");
    expect(onChange).toHaveBeenCalledTimes(1);
  });

  it("resets to Right Option, and only when it isn't the key already", async () => {
    const onChange = setup("RightCmd");
    fireEvent.click(screen.getByRole("button", { name: "Reset to default" }));
    await waitFor(() => {
      expect(onChange).toHaveBeenCalledWith("RightAlt");
    });
    cleanup();
    setup("RightAlt");
    expect(
      screen.getByRole<HTMLButtonElement>("button", { name: "Reset to default" }).disabled,
    ).toBe(true);
  });

  it("stops recording when the window loses focus", () => {
    const onChange = setup();
    record();
    act(() => void fireEvent.blur(window));
    down("F5");
    expect(onChange).not.toHaveBeenCalled();
  });

  it("counts a modifier already held when recording starts", async () => {
    const onChange = setup();
    holdSilently("ShiftLeft");
    record();
    down("F5");
    await waitFor(() => {
      expect(onChange).toHaveBeenCalledWith("Shift+F5");
    });
  });

  it("keeps the held modifier across a refused key", async () => {
    const onChange = setup();
    record();
    down("ControlLeft");
    down("CapsLock");
    expect(await screen.findByRole("alert")).toBeTruthy();
    up("CapsLock");
    down("KeyK");
    await waitFor(() => {
      expect(onChange).toHaveBeenCalledWith("Ctrl+K");
    });
  });
});

describe("HotkeyRecorder pausing the real hotkey", () => {
  it("says so while recording, and when it ends by each way out", async () => {
    // Esc, Cancel, a key chosen, a refusal that goes on, blur, unmount.
    setup();
    expect(recording).not.toHaveBeenCalled();
    record();
    expect(recording.mock.calls).toEqual([[true]]);
    down("Escape");
    expect(recording.mock.calls).toEqual([[true], [false]]);

    record();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(recording.mock.calls.at(-1)).toEqual([false]);

    record();
    down("F6");
    await waitFor(() => {
      expect(recording.mock.calls.at(-1)).toEqual([false]);
    });

    record();
    down("KeyA"); // refused: goes on recording
    await screen.findByRole("alert");
    expect(recording.mock.calls.at(-1)).toEqual([true]);
    act(() => void fireEvent.blur(window));
    expect(recording.mock.calls.at(-1)).toEqual([false]);

    record();
    cleanup();
    expect(recording.mock.calls.at(-1)).toEqual([false]);
  });

  it("stops recording when the window is hidden", () => {
    setup();
    record();
    const state = vi.spyOn(document, "visibilityState", "get").mockReturnValue("hidden");
    act(() => void fireEvent(document, new Event("visibilitychange")));
    expect(recording.mock.calls.at(-1)).toEqual([false]);
    state.mockRestore();
  });

  it("renews while recording goes on, so its pause doesn't lapse", () => {
    vi.useFakeTimers();
    setup();
    record();
    expect(recording).toHaveBeenCalledTimes(1);
    act(() => void vi.advanceTimersByTime(RENEW_MS * 2 + 1));
    expect(recording.mock.calls).toEqual([[true], [true], [true]]);
  });
});

describe("u12_criterion_5 Settings' compact row", () => {
  function compact(value = "RightCmd") {
    const onChange = vi.fn<(spec: string) => Promise<string | null>>(() => Promise.resolve(null));
    render(<HotkeyRecorder value={value} onChange={onChange} compact />);
    return onChange;
  }

  it("shows the keycap and one Change… beside it, and no permanent reset", () => {
    compact();
    const line = screen.getByLabelText("Dictation key").parentElement;
    expect(line?.className).toBe("recorder-line");
    expect(line?.querySelectorAll("button")).toHaveLength(1);
    expect(screen.getByRole("button", { name: "Change…" }).parentElement).toBe(line);
    expect(screen.queryByRole("button", { name: "Reset to default" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Use default" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Record a different key" })).toBeNull();
  });

  it("recording says Press a key… with Use default and Cancel", async () => {
    const onChange = compact();
    fireEvent.click(screen.getByRole("button", { name: "Change…" }));
    expect(screen.getByLabelText("Dictation key").textContent).toBe("Press a key…");
    fireEvent.click(screen.getByRole("button", { name: "Use default" }));
    await waitFor(() => {
      expect(onChange).toHaveBeenCalledWith("RightAlt");
    });
    expect(screen.getByRole("button", { name: "Change…" })).toBeTruthy();
  });

  it("Cancel keeps the key and gives the keyboard back to Change…", () => {
    const onChange = compact();
    fireEvent.click(screen.getByRole("button", { name: "Change…" }));
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onChange).not.toHaveBeenCalled();
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Change…" }));
  });

  it("Use default is off when the key already is the default", () => {
    compact("RightAlt");
    fireEvent.click(screen.getByRole("button", { name: "Change…" }));
    expect(screen.getByRole<HTMLButtonElement>("button", { name: "Use default" }).disabled).toBe(
      true,
    );
  });

  it("a key pressed while recording still saves", async () => {
    const onChange = compact();
    fireEvent.click(screen.getByRole("button", { name: "Change…" }));
    keyDown("F6");
    keyUp("F6");
    await waitFor(() => {
      expect(onChange).toHaveBeenCalledWith("F6");
    });
  });
});

describe("setup keeps its recorder (U11 changes it)", () => {
  it("has Record a different key and Reset to default without the option", () => {
    render(<HotkeyRecorder value="RightCmd" onChange={() => Promise.resolve(null)} />);
    expect(screen.getByRole("button", { name: "Record a different key" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Reset to default" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Change…" })).toBeNull();
  });
});

describe("setup's option (U11)", () => {
  it("has a plain Change key, without Use default, also while recording", () => {
    render(<HotkeyRecorder setup value="RightCmd" onChange={() => Promise.resolve(null)} />);
    const change = screen.getByRole("button", { name: "Change key" });
    expect(change.className).toBe("link");
    expect(screen.queryByRole("button", { name: /default/i })).toBeNull();
    fireEvent.click(change);
    screen.getByText("Press a key…");
    expect(screen.queryByRole("button", { name: /default/i })).toBeNull();
    screen.getByRole("button", { name: "Cancel" });
  });

  it("leaves the focus where the step puts it", () => {
    const box = document.createElement("textarea");
    document.body.append(box);
    render(
      <HotkeyRecorder
        setup
        value="RightAlt"
        onChange={() => Promise.resolve(null)}
        onRecording={(now) => {
          if (!now) box.focus();
        }}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Change key" }));
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(document.activeElement).toBe(box);
    box.remove();
  });
});
