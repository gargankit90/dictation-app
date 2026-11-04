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
          step: "welcome",
          needs_relaunch: false,
          try_outcome: "not_tried",
          download_paused: false,
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
    let asked = "troubleshoot";
    mockIPC((command) => {
      if (command === "get_onboarding") {
        return {
          view: "settings",
          step: "welcome",
          needs_relaunch: false,
          try_outcome: "not_tried",
          download_paused: false,
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

  /** A backend whose first window request is `request`. */
  function backendAsking(request: string, seen: string[] = []) {
    let asked = request;
    mockIPC((command) => {
      seen.push(command);
      switch (command) {
        case "get_onboarding":
          return {
            view: "settings",
            step: "welcome",
            needs_relaunch: false,
            try_outcome: "not_tried",
            download_paused: false,
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
        case "list_local_models":
          return [];
        case "get_engine_status":
          return { state: "ready", message: null };
        case "reveal_log_file":
          return null;
        default:
          return false;
      }
    });
  }

  // U5 criterion 3, U6: the fix rows open the section that fixes the problem.
  it("opens Speech when the tray's Open Speech settings asked", async () => {
    backendAsking("speech");
    render(<App />);
    expect(await screen.findByRole("heading", { level: 1, name: "Speech" })).toBeTruthy();
  });

  it("opens Dictation, with the shortcut, when the tray's Choose another key asked", async () => {
    backendAsking("shortcut");
    render(<App />);
    expect(await screen.findByRole("heading", { level: 1, name: "Dictation" })).toBeTruthy();
    expect(await screen.findByRole("button", { name: "Record a different key" })).toBeTruthy();
  });

  // U6 criterion 7: Help → Show log file, through the app's Opener (U5's command).
  it("shows the log file from Settings, Help", async () => {
    const seen: string[] = [];
    backendAsking("", seen);
    render(<App />);
    fireEvent.click(await screen.findByRole("tab", { name: "Help" }));
    fireEvent.click(await screen.findByRole("button", { name: "Show log file" }));
    await waitFor(() => {
      expect(seen).toContain("reveal_log_file");
    });
    expect(screen.queryByRole("alert")).toBeNull();
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

  // U5 criterion 6, U6 criterion 7: "Resume setup…" leaves the tray once setup
  // is done; Settings, Help → Run setup again opens the first-run steps.
  it("opens the first-run steps from Settings, Help", async () => {
    let view = "settings";
    const seen: string[] = [];
    mockIPC((command) => {
      seen.push(command);
      switch (command) {
        case "get_onboarding":
          return {
            view,
            step: "welcome",
            needs_relaunch: false,
            try_outcome: "not_tried",
            download_paused: false,
            microphone: "granted",
            accessibility: "granted",
          };
        case "open_setup":
          view = "onboarding";
          return null;
        case "take_window_request":
          return "";
        case "list_local_models":
          return [];
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
    render(<App />);
    fireEvent.click(await screen.findByRole("tab", { name: "Help" }));
    fireEvent.click(await screen.findByRole("button", { name: "Run setup again" }));
    await waitFor(() => {
      expect(seen).toContain("open_setup");
      expect(screen.queryByRole("tablist", { name: "Settings sections" })).toBeNull();
    });
  });
});
