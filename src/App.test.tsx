import { cleanup, render, screen } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "./App";

afterEach(() => {
  cleanup();
  clearMocks();
});

describe("App", () => {
  it("is the settings window", async () => {
    mockIPC((command) => {
      if (command === "get_settings") {
        return {
          version: 2,
          provider: "groq",
          model: "whisper-large-v3-turbo",
          language: null,
          hotkey: "RightAlt",
        };
      }
      return false; // has_api_key
    });
    render(<App />);

    expect(screen.getByRole("heading", { level: 1 }).textContent).toBe("Settings");
    expect(await screen.findByLabelText("Hotkey")).toBeTruthy();
  });
});
