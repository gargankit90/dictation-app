import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
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
    expect(await screen.findByLabelText("Or type it")).toBeTruthy();
  });

  it("opens Troubleshoot when the tray asked for it, and from the Settings button", async () => {
    let asked = "troubleshoot";
    mockIPC((command) => {
      if (command === "get_onboarding") {
        return {
          view: "settings",
          step: "microphone",
          microphone: "granted",
          accessibility: "granted",
        };
      }
      if (command === "take_window_request") {
        const answer = asked;
        asked = "";
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
      return false;
    });
    render(<App />);

    expect((await screen.findByRole("heading", { level: 1 })).textContent).toBe("Troubleshoot");
    fireEvent.click(screen.getByRole("button", { name: "Back to Settings" }));
    expect((await screen.findByRole("heading", { level: 1 })).textContent).toBe("Settings");
    fireEvent.click(await screen.findByRole("button", { name: "Troubleshoot…" }));
    expect((await screen.findByRole("heading", { level: 1 })).textContent).toBe("Troubleshoot");
  });

  /** A backend whose first window request is `request`. */
  function backendAsking(request: string, seen: string[] = []) {
    let asked = request;
    mockIPC((command) => {
      seen.push(command);
      switch (command) {
        case "get_onboarding":
          return {
            view: "settings",
            step: "microphone",
            microphone: "granted",
            accessibility: "granted",
          };
        case "take_window_request": {
          const answer = asked;
          asked = "";
          return answer;
        }
        case "run_checks":
          return [];
        case "build_report":
          return "Vocet diagnostics report\n";
        case "get_settings":
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
        default:
          return false;
      }
    });
  }

  // U5 criterion 3: the fix rows land on the field that fixes the problem.
  it("focuses the speech engine when the tray's Open Speech settings asked", async () => {
    backendAsking("speech");
    render(<App />);
    await screen.findByLabelText("Or type it");
    await waitFor(() => {
      const checked = document.querySelector<HTMLElement>("input[name='transcription']:checked");
      expect(checked).not.toBeNull();
      expect(document.activeElement).toBe(checked);
    });
  });

  it("focuses the dictation key when the tray's Choose another key asked", async () => {
    backendAsking("shortcut");
    render(<App />);
    const typed = await screen.findByLabelText("Or type it");
    await waitFor(() => {
      expect(document.activeElement).toBe(typed);
    });
  });

  // U5: "Report a problem…" opens Troubleshoot with the report preview built.
  it("opens Troubleshoot with the report built when the tray asked for it", async () => {
    const seen: string[] = [];
    backendAsking("report", seen);
    render(<App />);
    expect((await screen.findByRole("heading", { level: 1 })).textContent).toBe("Troubleshoot");
    expect(await screen.findByLabelText("Report")).toBeTruthy();
    expect(seen.filter((command) => command === "build_report")).toHaveLength(1);
  });
});
