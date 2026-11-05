import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Report } from "./Report";

const REPORT = "Vocet diagnostics report\napp_version: 1.0.0\n";

let calls: string[] = [];
let saveAnswer: () => unknown = () => "vocet-report.txt";

function backend(command: string) {
  calls.push(command);
  switch (command) {
    case "build_report":
      return REPORT;
    case "save_report":
      return saveAnswer();
    default:
      return null;
  }
}

function view(onBack: () => void = () => undefined) {
  return render(<Report backLabel="Back to Settings" onBack={onBack} />);
}

beforeEach(() => {
  calls = [];
  saveAnswer = () => "vocet-report.txt";
  mockIPC(backend);
});

afterEach(() => {
  cleanup();
  clearMocks();
});

// E1 criterion 3, on its own screen since U12.
describe("u12_criterion_2 Report a problem", () => {
  it("shows the exact report once, and sends nothing by itself", async () => {
    view();
    expect((await screen.findByLabelText("Report")).textContent).toBe(REPORT);
    expect(calls.filter((call) => call === "build_report")).toHaveLength(1);
    expect(calls).not.toContain("open_issue");
    expect(calls).not.toContain("save_report");
  });

  it("saves, copies or opens an issue on request", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    view();
    await screen.findByLabelText("Report");
    fireEvent.click(screen.getByRole("button", { name: "Copy" }));
    await waitFor(() => {
      expect(writeText).toHaveBeenCalledWith(REPORT);
    });
    expect(calls).not.toContain("save_report");
    fireEvent.click(screen.getByRole("button", { name: "Save report…" }));
    expect(await screen.findByText("Saved as vocet-report.txt.")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Open a GitHub issue" }));
    await waitFor(() => {
      expect(calls).toContain("open_issue");
    });
    expect(await screen.findByText(/submit it yourself/)).toBeTruthy();
  });

  it("says nothing was saved when the user cancels the dialog", async () => {
    saveAnswer = () => null;
    view();
    await screen.findByLabelText("Report");
    fireEvent.click(screen.getByRole("button", { name: "Save report…" }));
    await waitFor(() => {
      expect(calls).toContain("save_report");
    });
    expect(screen.queryByText(/Saved as/)).toBeNull();
  });

  it("reports a failed save in words", async () => {
    saveAnswer = () => {
      // The app sends its errors as strings.
      // eslint-disable-next-line @typescript-eslint/only-throw-error
      throw "Couldn't save the report there. Try another folder.";
    };
    view();
    await screen.findByLabelText("Report");
    fireEvent.click(screen.getByRole("button", { name: "Save report…" }));
    expect((await screen.findByRole("alert")).textContent).toContain(
      "Couldn't save the report there",
    );
  });

  it("offers Try again when the report can't be built", async () => {
    let fail = true;
    mockIPC((command) => {
      if (command === "build_report") {
        if (fail) {
          // The app sends its errors as strings.
          // eslint-disable-next-line @typescript-eslint/only-throw-error
          throw "Couldn't build the report.";
        }
        return REPORT;
      }
      return null;
    });
    view();
    expect((await screen.findByRole("alert")).textContent).toContain("Couldn't build the report");
    fail = false;
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect((await screen.findByLabelText("Report")).textContent).toBe(REPORT);
  });

  it("says Dictation key, not hotkey, and goes back", async () => {
    const onBack = vi.fn();
    view(onBack);
    expect(screen.getByText(/your dictation key and speech engine/)).toBeTruthy();
    fireEvent.click(await screen.findByRole("button", { name: "Back to Settings" }));
    expect(onBack).toHaveBeenCalledOnce();
  });
});
