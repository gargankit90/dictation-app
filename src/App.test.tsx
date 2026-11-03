import { cleanup, render, screen } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "./App";

afterEach(() => {
  cleanup();
  clearMocks();
});

describe("App", () => {
  it("is the settings window unless the app asks for onboarding", async () => {
    mockIPC((command) => {
      if (command === "get_onboarding") {
        return {
          view: "settings",
          step: "microphone",
          microphone: "granted",
          accessibility: "granted",
        };
      }
      if (command === "get_settings") {
        return {
          version: 2,
          provider: "groq",
          model: "whisper-large-v3-turbo",
          language: null,
          hotkey: "RightAlt",
          launch_at_login: false,
          share_usage_events: true,
        };
      }
      return false; // has_api_key
    });
    render(<App />);

    expect((await screen.findByRole("heading", { level: 1 })).textContent).toBe("Settings");
    expect(await screen.findByLabelText("Hotkey")).toBeTruthy();
  });
});
