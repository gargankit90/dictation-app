// K1: the dictation-key recorder, with a fake `onChange` standing in for the
// app. Key events go to the window, as the real ones do.

import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { HotkeyRecorder } from "./HotkeyRecorder";

afterEach(cleanup);

const down = (code: string) => {
  act(() => void fireEvent.keyDown(window, { code }));
};
const up = (code: string) => {
  act(() => void fireEvent.keyUp(window, { code }));
};
const record = () => {
  fireEvent.click(screen.getByRole("button", { name: "Record a different key" }));
};

function setup(value = "RightAlt", result: string | null = null) {
  const onChange = vi.fn<(spec: string) => Promise<string | null>>(() => Promise.resolve(result));
  render(<HotkeyRecorder value={value} onChange={onChange} />);
  return onChange;
}

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
    const onChange = setup("RightAlt", "Couldn't switch the hotkey to F6. RightAlt still works.");
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
});
