// Q4: the cleanup model's line under "On this Mac" in Settings, with the app's commands replaced
// by a fake backend.

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { LocalModelInfo_Serialize } from "../bindings";
import { CleanupStatus, cleanupLine } from "./CleanupModel";

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
      case "cancel_local_model_download":
        model = { ...model, state: "partial" };
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

describe("cleanupLine", () => {
  it("offers one action per state, with the size, and says nothing leaves the Mac", () => {
    expect(cleanupLine(GEMMA)).toEqual({
      text: "Cleanup (punctuation, fillers) · 3107 MB · not downloaded",
      action: "download",
      progress: null,
    });
    expect(cleanupLine({ ...GEMMA, state: "downloading", downloaded_bytes: 500_000_000 })).toEqual({
      text: "Cleanup (punctuation, fillers) · 3107 MB · ↓ 500 of 3107 MB",
      action: "cancel",
      progress: { value: 500_000_000, max: 3_106_738_272 },
    });
    expect(cleanupLine({ ...GEMMA, state: "partial", downloaded_bytes: 2_000_000 }).action).toBe(
      "resume",
    );
    expect(cleanupLine({ ...GEMMA, state: "ready" })).toEqual({
      text: "Cleanup (punctuation, fillers) · 3107 MB · ✓ Ready · Private: nothing leaves this Mac",
      action: null,
      progress: null,
    });
  });
});

describe("CleanupStatus", () => {
  it("starts the cleanup model's download", async () => {
    render(<CleanupStatus model={model} />);
    fireEvent.click(screen.getByRole("button", { name: "Download cleanup" }));
    await waitFor(() => {
      expect(calls.find((call) => call.command === "download_local_model")?.args).toEqual({
        id: GEMMA.id,
      });
    });
  });

  it("shows progress and cancels a running download", async () => {
    render(
      <CleanupStatus model={{ ...GEMMA, state: "downloading", downloaded_bytes: 1_000_000_000 }} />,
    );
    expect(screen.getByRole("progressbar", { name: "Cleanup model download" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Cancel cleanup" }));
    await waitFor(() => {
      expect(calls.map((call) => call.command)).toContain("cancel_local_model_download");
    });
  });

  it("offers nothing once it is ready", () => {
    render(<CleanupStatus model={{ ...GEMMA, state: "ready" }} />);
    expect(screen.queryByRole("button")).toBeNull();
  });
});
