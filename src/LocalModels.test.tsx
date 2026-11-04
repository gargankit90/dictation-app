// L1 criterion 4: Settings' engine choice and the on-device model picker,
// with the app's commands replaced by a fake backend through `mockIPC`.

import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { LocalModelInfo_Serialize, Settings_Serialize } from "./bindings";
import { megabytes } from "./LocalModels";
import { Settings } from "./Settings";

const PARAKEET = "parakeet-unified-en-0.6b-q4_k_m";
const QWEN = "qwen3-asr-0.6b-q4_k_m";

function model(id: string, extra: Partial<LocalModelInfo_Serialize>): LocalModelInfo_Serialize {
  const parakeet = id === PARAKEET;
  return {
    id,
    name: parakeet ? "Parakeet Unified 0.6B" : "Qwen3-ASR 0.6B",
    language_codes: parakeet ? ["en"] : ["en", "de", "hi", "fr"],
    languages: parakeet ? "English only" : "30 languages, including English and Hindi",
    size_bytes: parakeet ? 477_274_496 : 589_560_480,
    downloaded_bytes: 0,
    state: "missing",
    error: null,
    default: parakeet,
    ...extra,
  };
}

class Backend {
  settings: Settings_Serialize = {
    version: 2,
    provider: "groq",
    model: "whisper-large-v3-turbo",
    language: null,
    hotkey: "RightAlt",
    launch_at_login: false,
    transcription: "cloud",
    local_model: PARAKEET,
    share_usage_events: true,
  };
  models: LocalModelInfo_Serialize[] = [model(PARAKEET, {}), model(QWEN, {})];
  failing = new Map<string, string>();
  calls: { command: string; args: Record<string, unknown> }[] = [];

  handle = (command: string, payload: unknown): unknown => {
    const args = (payload ?? {}) as Record<string, unknown>;
    this.calls.push({ command, args });
    const failure = this.failing.get(command);
    // eslint-disable-next-line @typescript-eslint/only-throw-error
    if (failure !== undefined) throw failure;
    switch (command) {
      case "get_settings":
        return this.settings;
      case "save_settings":
        this.settings = args.settings as Settings_Serialize;
        return null;
      case "has_api_key":
        return false;
      case "list_local_models":
        return this.models;
      case "download_local_model":
        this.set(args.id as string, { state: "downloading", downloaded_bytes: 0 });
        return null;
      case "cancel_local_model_download":
        this.set(args.id as string, { state: "partial" });
        return null;
      case "delete_local_model":
        this.set(args.id as string, { state: "missing", downloaded_bytes: 0 });
        return null;
      default:
        throw new Error(`unexpected command ${command}`);
    }
  };

  set(id: string, extra: Partial<LocalModelInfo_Serialize>) {
    this.models = this.models.map((m) => (m.id === id ? { ...m, ...extra } : m));
  }

  named(command: string) {
    return this.calls.filter((call) => call.command === command);
  }
}

let backend: Backend;

beforeEach(() => {
  backend = new Backend();
  mockIPC((command, payload) => backend.handle(command, payload));
});

afterEach(() => {
  cleanup();
  clearMocks();
});

async function open() {
  render(<Settings />);
  await screen.findByLabelText("Or type it");
}

const item = (name: RegExp) => screen.getByRole("radio", { name }).closest("li") as HTMLElement;

describe("the engine choice", () => {
  it("offers On this Mac and Your own key, and the key fields only for the latter", async () => {
    await open();
    expect(screen.getByRole<HTMLInputElement>("radio", { name: "Your own key" }).checked).toBe(
      true,
    );
    expect(screen.getByLabelText(/API key/)).toBeTruthy();
    expect(backend.named("list_local_models")).toHaveLength(0);

    fireEvent.click(screen.getByRole("radio", { name: "On this Mac" }));
    expect(screen.queryByLabelText(/API key/)).toBeNull();
    expect(screen.queryByLabelText("Provider")).toBeNull();
    expect(screen.getByText(/No key needed/)).toBeTruthy();
    await screen.findByRole("radio", { name: /Parakeet Unified 0.6B \(recommended\)/ });
    expect(screen.getByText(/\(recommended\) — English only/)).toBeTruthy();
    expect(screen.getByText(/including English and Hindi/)).toBeTruthy();
  });

  it("saves the choice and the picked model", async () => {
    await open();
    fireEvent.click(screen.getByRole("radio", { name: "On this Mac" }));
    fireEvent.click(await screen.findByRole("radio", { name: /Qwen3-ASR/ }));
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await screen.findByText("Settings saved.");
    expect(backend.settings.transcription).toBe("local");
    expect(backend.settings.local_model).toBe(QWEN);
    // The cloud settings are kept for when the user switches back.
    expect(backend.settings.provider).toBe("groq");
    expect(backend.settings.model).toBe("whisper-large-v3-turbo");
  });

  it("opens on the saved local model", async () => {
    backend.settings = { ...backend.settings, transcription: "local", local_model: QWEN };
    await open();
    expect(screen.getByRole<HTMLInputElement>("radio", { name: "On this Mac" }).checked).toBe(true);
    const qwen = await screen.findByRole<HTMLInputElement>("radio", { name: /Qwen3-ASR/ });
    expect(qwen.checked).toBe(true);
  });
});

describe("the model picker", () => {
  beforeEach(() => {
    backend.settings = { ...backend.settings, transcription: "local" };
  });

  it("shows each model's status", async () => {
    backend.set(PARAKEET, { state: "ready", downloaded_bytes: 477_274_496 });
    backend.set(QWEN, { state: "partial", downloaded_bytes: 100_000_000 });
    await open();
    await screen.findByText("Downloaded");
    expect(screen.getByText("Paused at 100 MB of 590 MB")).toBeTruthy();
    expect(within(item(/Parakeet/)).getByRole("button", { name: "Delete" })).toBeTruthy();
    expect(within(item(/Qwen/)).getByRole("button", { name: "Resume download" })).toBeTruthy();
  });

  it("downloads with progress until ready", async () => {
    await open();
    await screen.findByText("Not downloaded (478 MB)");
    fireEvent.click(within(item(/Parakeet/)).getByRole("button", { name: "Download" }));
    await waitFor(() => {
      expect(backend.named("download_local_model")).toEqual([
        { command: "download_local_model", args: { id: PARAKEET } },
      ]);
    });
    await screen.findByText("Downloading… 0% of 478 MB");
    backend.set(PARAKEET, { downloaded_bytes: 238_637_248 });
    await screen.findByText("Downloading… 50% of 478 MB", undefined, { timeout: 3000 });
    expect(screen.getByRole("progressbar", { name: /Parakeet/ })).toBeTruthy();
    backend.set(PARAKEET, { state: "ready", downloaded_bytes: 477_274_496 });
    await screen.findByText("Downloaded", undefined, { timeout: 3000 });
    // Polling stops once nothing downloads.
    const polls = backend.named("list_local_models").length;
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 1200));
    });
    expect(backend.named("list_local_models").length).toBe(polls);
  });

  it("downloads one model at a time", async () => {
    backend.set(PARAKEET, { state: "downloading", downloaded_bytes: 1_000_000 });
    await open();
    await screen.findByText("One model downloads at a time.");
    const download = within(item(/Qwen/)).getByRole<HTMLButtonElement>("button", {
      name: "Download",
    });
    expect(download.disabled).toBe(true);
    backend.set(PARAKEET, { state: "ready", downloaded_bytes: 477_274_496 });
    await waitFor(
      () => {
        expect(download.disabled).toBe(false);
      },
      { timeout: 3000 },
    );
    expect(screen.queryByText("One model downloads at a time.")).toBeNull();
  });

  it("cancels and deletes", async () => {
    backend.set(PARAKEET, { state: "downloading", downloaded_bytes: 1_000_000 });
    await open();
    fireEvent.click(await screen.findByRole("button", { name: "Cancel" }));
    await screen.findByText(/Paused at/);
    expect(backend.named("cancel_local_model_download")[0]?.args).toEqual({ id: PARAKEET });
    fireEvent.click(within(item(/Parakeet/)).getByRole("button", { name: "Delete" }));
    await screen.findByText("Not downloaded (478 MB)");
    expect(backend.named("delete_local_model")[0]?.args).toEqual({ id: PARAKEET });
  });

  it("shows a failed download's message and a failed command's error", async () => {
    backend.set(QWEN, {
      state: "partial",
      downloaded_bytes: 10,
      error: "The download failed. Check your internet connection and try again.",
    });
    backend.failing.set("delete_local_model", "couldn't remove the file");
    await open();
    await screen.findByText(/The download failed/);
    fireEvent.click(within(item(/Qwen/)).getByRole("button", { name: "Delete" }));
    expect((await screen.findByText("couldn't remove the file")).getAttribute("role")).toBe(
      "alert",
    );
  });

  it("says when the list can't be read", async () => {
    backend.failing.set("list_local_models", "no models folder");
    await open();
    await screen.findByText("Couldn't list the models: no models folder");
  });
});

it("rounds megabytes up", () => {
  expect(megabytes(477_274_496)).toBe("478 MB");
  expect(megabytes(0)).toBe("0 MB");
});

describe("U2: the language picker follows the engine", () => {
  const language = () => screen.getByLabelText<HTMLInputElement>("Language");
  const optionNames = () => screen.getAllByRole("option").map((o) => o.textContent);

  it("offers only the selected model's languages, and re-filters when the model changes", async () => {
    backend.settings = { ...backend.settings, transcription: "local" };
    await open();
    await screen.findByRole("radio", { name: /Parakeet/ });
    fireEvent.focus(language());
    await waitFor(() => {
      expect(optionNames()).toEqual(["Detect automatically", "English"]);
    });
    fireEvent.blur(language());

    fireEvent.click(screen.getByRole("radio", { name: /Qwen3-ASR/ }));
    fireEvent.focus(language());
    expect(optionNames()).toEqual([
      "Detect automatically",
      "English",
      "French (Français)",
      "German (Deutsch)",
      "Hindi (हिन्दी)",
    ]);
  });

  it("shows the helper line and link with the English-only model, and the link switches model", async () => {
    backend.settings = { ...backend.settings, transcription: "local" };
    await open();
    await screen.findByText(/understands English only\. For Hindi and other languages,/);
    fireEvent.click(screen.getByRole("button", { name: "choose the multilingual model" }));
    expect(screen.getByRole<HTMLInputElement>("radio", { name: /Qwen3-ASR/ }).checked).toBe(true);
    expect(screen.queryByText(/understands English only/)).toBeNull();
  });

  it("keeps and flags a saved language the model can't do; saving doesn't change it", async () => {
    backend.settings = { ...backend.settings, transcription: "local", language: "de" };
    await open();
    await waitFor(() => {
      expect(language().value).toBe("German, not supported by Parakeet Unified 0.6B");
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await screen.findByText("Settings saved.");
    expect(backend.settings.language).toBe("de");

    // Switching to the multilingual model clears the flag.
    fireEvent.click(screen.getByRole("radio", { name: /Qwen3-ASR/ }));
    expect(language().value).toBe("German (Deutsch)");
  });

  it("flags a code the cloud engine doesn't know, without changing it", async () => {
    backend.settings = { ...backend.settings, language: "yue" };
    await open();
    expect(language().value).toBe("Cantonese, not supported by Groq");
  });
});
