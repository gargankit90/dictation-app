// Q4: the cleanup model's row in Settings, with the app's commands replaced
// by a fake backend.

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { LocalModelInfo_Serialize } from "../bindings";
import { CleanupModel, cleanupCaption, cleanupRow } from "./CleanupModel";

const GEMMA: LocalModelInfo_Serialize = {
  id: "gemma-4-e2b-it-q4_k_m",
  name: "Gemma 4 E2B",
  languages: "English",
  language_codes: ["en"],
  size_bytes: 3_106_738_272,
  downloaded_bytes: 0,
  state: "missing",
  resumed_from_bytes: null,
  restarted: false,
  error: null,
  default: true,
};

let model: LocalModelInfo_Serialize;
let calls: { command: string; args: unknown }[];

beforeEach(() => {
  model = { ...GEMMA };
  calls = [];
  mockIPC((command, args) => {
    calls.push({ command, args });
    switch (command) {
      case "list_cleanup_models":
        return [model];
      case "download_local_model":
        model = { ...model, state: "downloading", downloaded_bytes: 1_000_000_000 };
        return null;
      case "delete_local_model":
        model = { ...model, state: "missing", downloaded_bytes: 0 };
        return null;
      default:
        throw new Error(`unexpected command ${command}`);
    }
  });
});

afterEach(() => {
  cleanup();
  clearMocks();
});

describe("cleanupRow", () => {
  it("says the size and that nothing leaves the Mac", () => {
    expect(cleanupCaption(GEMMA)).toBe(
      "Adds punctuation, removes fillers · 3107 MB · Private: nothing leaves this Mac",
    );
  });

  it("offers one action per state", () => {
    expect(cleanupRow(GEMMA)).toEqual({ chip: null, action: "download", progress: null });
    expect(cleanupRow({ ...GEMMA, state: "downloading", downloaded_bytes: 500_000_000 })).toEqual({
      chip: "↓ 500 of 3107 MB",
      action: "cancel",
      progress: { value: 500_000_000, max: 3_106_738_272 },
    });
    expect(cleanupRow({ ...GEMMA, state: "partial", downloaded_bytes: 2_000_000 }).action).toBe(
      "resume",
    );
    expect(cleanupRow({ ...GEMMA, state: "ready" })).toEqual({
      chip: "✓ Ready",
      action: "delete",
      progress: null,
    });
  });
});

describe("CleanupModel", () => {
  it("downloads the cleanup model and shows its progress", async () => {
    render(<CleanupModel />);
    expect(await screen.findByText("Gemma 4 E2B")).toBeTruthy();
    expect(screen.getByText(/Private: nothing leaves this Mac/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Download" }));
    await waitFor(() => {
      expect(screen.getByText("↓ 1000 of 3107 MB")).toBeTruthy();
    });
    expect(calls.find((call) => call.command === "download_local_model")?.args).toEqual({
      id: GEMMA.id,
    });
    expect(screen.getByRole("progressbar", { name: "Gemma 4 E2B download" })).toBeTruthy();
  });

  it("deletes a downloaded model", async () => {
    model = { ...GEMMA, state: "ready", downloaded_bytes: GEMMA.size_bytes };
    render(<CleanupModel />);
    expect(await screen.findByText("✓ Ready")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Download" })).toBeTruthy();
    });
  });
});
