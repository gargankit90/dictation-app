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
          setup_finished_before: false,
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
          setup_finished_before: false,
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
    fireEvent.click(await screen.findByRole("button", { name: "Open…" }));
    expect((await screen.findByRole("heading", { level: 1 })).textContent).toBe("Troubleshoot");
    // ⌘, goes back to Settings (U6).
    fireEvent.keyDown(window, { key: ",", metaKey: true });
    expect(await screen.findByRole("tablist", { name: "Settings sections" })).toBeTruthy();
  });

  /** A backend whose first window request is `request`; returns how to ask again. */
  function backendAsking(request: string, seen: string[] = [], pages: unknown[] = []) {
    let asked = request;
    mockIPC((command, payload) => {
      seen.push(command);
      switch (command) {
        case "set_window_page":
          pages.push((payload as { page: unknown }).page);
          return null;
        case "get_onboarding":
          return {
            view: "settings",
            step: "welcome",
            needs_relaunch: false,
            setup_finished_before: false,
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
            history_limit: 25,
          };
        case "list_history":
          return { rows: [], unreadable: false };
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
    return (next: string) => {
      asked = next;
      fireEvent.focus(window);
    };
  }

  // U5 criterion 3, U6: the fix rows open the section that fixes the problem.
  it("opens Speech when the tray's Open Speech settings asked", async () => {
    backendAsking("speech");
    render(<App />);
    expect(await screen.findByRole("heading", { level: 1, name: "Speech" })).toBeTruthy();
  });

  it("opens Dictation, on the dictation key, when the tray's Choose another key asked", async () => {
    backendAsking("shortcut");
    render(<App />);
    expect(await screen.findByRole("heading", { level: 1, name: "Dictation" })).toBeTruthy();
    expect(await screen.findByRole("button", { name: "Change…" })).toBeTruthy();
  });

  // U7 criterion 6: the menu's "History…".
  it("opens History when the tray's History… asked", async () => {
    backendAsking("history");
    render(<App />);
    expect(await screen.findByRole("heading", { level: 1, name: "History" })).toBeTruthy();
    expect(await screen.findByLabelText("Keep recent dictations")).toBeTruthy();
  });

  // U6 criterion 7: Help → Show log file, through the app's Opener (U5's command).
  it("shows the log file from Settings, Help", async () => {
    const seen: string[] = [];
    backendAsking("", seen);
    render(<App />);
    fireEvent.click(await screen.findByRole("tab", { name: "Help" }));
    fireEvent.click(await screen.findByRole("button", { name: "Show in Finder" }));
    await waitFor(() => {
      expect(seen).toContain("reveal_log_file");
    });
    expect(screen.queryByRole("alert")).toBeNull();
  });

  const heading = async () => (await screen.findByRole("heading", { level: 1 })).textContent;

  // U12 criterion 1, #277: an old ask from the tray isn't followed again when
  // Settings comes back.
  for (const [screenName, button, back] of [
    ["Troubleshoot", "Open…", "Back to Settings"],
    ["Report a problem", "Preview…", "Back to Settings"],
  ] as const) {
    it(`u12_criterion_1 Back from ${screenName} returns to Help, after the tray opened History`, async () => {
      window.localStorage.clear();
      backendAsking("history");
      render(<App />);
      expect(await screen.findByRole("heading", { level: 1, name: "History" })).toBeTruthy();
      fireEvent.click(screen.getByRole("tab", { name: "Help" }));
      expect(await screen.findByRole("heading", { level: 1, name: "Help" })).toBeTruthy();
      fireEvent.click(await screen.findByRole("button", { name: button }));
      expect(await heading()).toBe(screenName);
      fireEvent.click(screen.getByRole("button", { name: back }));
      expect(await screen.findByRole("heading", { level: 1, name: "Help" })).toBeTruthy();
    });
  }

  it("u12_criterion_1 a repeated ask from the tray still counts", async () => {
    window.localStorage.clear();
    const ask = backendAsking("history");
    render(<App />);
    expect(await screen.findByRole("heading", { level: 1, name: "History" })).toBeTruthy();
    fireEvent.click(screen.getByRole("tab", { name: "Help" }));
    expect(await screen.findByRole("heading", { level: 1, name: "Help" })).toBeTruthy();
    ask("history");
    expect(await screen.findByRole("heading", { level: 1, name: "History" })).toBeTruthy();
  });

  // U12 criterion 2.
  it("u12_criterion_2 Report opened from Troubleshoot goes back to Troubleshoot", async () => {
    let asked = "troubleshoot";
    backendAsking("troubleshoot");
    mockIPC((command) => {
      switch (command) {
        case "take_window_request": {
          const answer = asked;
          asked = "";
          return answer;
        }
        case "get_onboarding":
          return { view: "settings" };
        case "run_checks":
          return [
            {
              id: "microphone",
              label: "Microphone",
              ok: false,
              detail: "Not allowed.",
              code: "mic_denied",
              details: "mic_denied",
              fix: null,
              fix_label: null,
              local_model: null,
            },
          ];
        case "build_report":
          return "Vocet diagnostics report\n";
        default:
          return null;
      }
    });
    render(<App />);
    expect(await heading()).toBe("Troubleshoot");
    fireEvent.click(await screen.findByRole("button", { name: "Report a problem…" }));
    expect(await heading()).toBe("Report a problem");
    expect(await screen.findByLabelText("Report")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Back to Troubleshoot" }));
    expect(await heading()).toBe("Troubleshoot");
  });

  it("u12_criterion_2 each screen gives the window its title", async () => {
    const pages: unknown[] = [];
    window.localStorage.clear();
    backendAsking("help", [], pages);
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "Open…" }));
    expect(await heading()).toBe("Troubleshoot");
    fireEvent.click(screen.getByRole("button", { name: "Back to Settings" }));
    fireEvent.click(await screen.findByRole("button", { name: "Preview…" }));
    expect(await heading()).toBe("Report a problem");
    await waitFor(() => {
      expect(pages).toEqual(["settings", "troubleshoot", "settings", "report"]);
    });
  });

  // U12 criterion 7: the menu's Help… and Settings….
  it("u12_criterion_7 opens Help when the tray's Help… asked", async () => {
    backendAsking("help");
    render(<App />);
    expect(await screen.findByRole("heading", { level: 1, name: "Help" })).toBeTruthy();
  });

  it("u12_criterion_7 the tray's Settings… leaves Troubleshoot for Settings", async () => {
    const ask = backendAsking("troubleshoot");
    render(<App />);
    expect(await heading()).toBe("Troubleshoot");
    ask("settings");
    expect(await screen.findByRole("tablist", { name: "Settings sections" })).toBeTruthy();
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
            setup_finished_before: false,
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
    fireEvent.click(await screen.findByRole("button", { name: "Start…" }));
    await waitFor(() => {
      expect(seen).toContain("open_setup");
      expect(screen.queryByRole("tablist", { name: "Settings sections" })).toBeNull();
    });
  });
});
