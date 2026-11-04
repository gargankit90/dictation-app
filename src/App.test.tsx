import { cleanup, fireEvent, render, screen } from "@testing-library/react";
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
      if (command === "list_local_models") return [];
      if (command === "get_engine_status") return { state: "ready", message: null };
      return false; // has_api_key
    });
    render(<App />);

    expect(await screen.findByRole("tablist", { name: "Settings sections" })).toBeTruthy();
    expect((await screen.findByRole("heading", { level: 1 })).textContent).toBe("General");
  });

  it("opens Troubleshoot when the tray asked for it, and from the Settings button", async () => {
    let asked = true;
    mockIPC((command) => {
      if (command === "get_onboarding") {
        return {
          view: "settings",
          step: "microphone",
          microphone: "granted",
          accessibility: "granted",
        };
      }
      if (command === "take_troubleshoot_request") {
        const answer = asked;
        asked = false;
        return answer;
      }
      if (command === "run_checks") return [];
      if (command === "get_settings") {
        return {
          version: 2,
          provider: "groq",
          model: "whisper-large-v3-turbo",
          language: null,
          hotkey: "RightAlt",
          launch_at_login: false,
          transcription: "cloud",
          local_model: "parakeet-unified-en-0.6b-q4_k_m",
          share_usage_events: true,
        };
      }
      if (command === "list_local_models") return [];
      if (command === "get_engine_status") return { state: "ready", message: null };
      return false;
    });
    render(<App />);

    expect((await screen.findByRole("heading", { level: 1 })).textContent).toBe("Troubleshoot");
    fireEvent.click(screen.getByRole("button", { name: "Back to Settings" }));
    expect(await screen.findByRole("tablist", { name: "Settings sections" })).toBeTruthy();
    fireEvent.click(screen.getByRole("tab", { name: "Help" }));
    fireEvent.click(await screen.findByRole("button", { name: "Troubleshoot…" }));
    expect((await screen.findByRole("heading", { level: 1 })).textContent).toBe("Troubleshoot");
    // ⌘, goes back to Settings (U6).
    fireEvent.keyDown(window, { key: ",", metaKey: true });
    expect(await screen.findByRole("tablist", { name: "Settings sections" })).toBeTruthy();
  });
});
