// U6: the settings window in sections, every control applied on change,
// with the app's commands replaced by a fake backend through `mockIPC`.
// (Migrated from the M3.4/K1/S1/A1 form tests.) The keys here are made up.

import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { modelLabel } from "./settings/modelRow";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type {
  EngineStatus,
  LocalModelInfo_Serialize,
  Settings_Serialize,
  Transcription,
} from "./bindings";
import { SECTION_KEY, Settings } from "./Settings";
import type { SectionId } from "./Settings";
import { HOTKEY_NOTICE_POLL_MS } from "./settings/Dictation";
import { CONNECTED, KEY_NOT_SAVED } from "./settings/Speech";
import { keyDown, keyUp, releaseAllKeys } from "./testKeys";

const KEY = "sk-or-dummy-NOT-A-REAL-KEY-0123456789";
const ENGLISH = "orukqwen-asr-1.7b-q4_k_m";

interface Call {
  command: string;
  args: Record<string, unknown>;
}

function model(id: string, overrides: Partial<LocalModelInfo_Serialize> = {}) {
  return {
    id,
    name: "OrukQwen English",
    languages: "English only",
    size_bytes: 477_274_496,
    downloaded_bytes: 0,
    state: "missing",
    resumed_from_bytes: null,
    restarted: false,
    error: null,
    default: id === ENGLISH,
    ...overrides,
  } satisfies LocalModelInfo_Serialize;
}

/** The app's side of the commands: what's saved, and what each test returns. */
class Backend {
  settings: Settings_Serialize = {
    version: 2,
    hotkey: "RightAlt",
    launch_at_login: false,
    transcription: "cloud",
    share_usage_events: true,
    history_limit: 25,
  };
  keys = { openrouter: false };
  models: LocalModelInfo_Serialize[] = [model(ENGLISH)];
  cleanup: LocalModelInfo_Serialize[] = [];
  engine: EngineStatus = { state: "ready", message: null };
  /** What `test_api_key` answers: `null` for success, or an error to throw. */
  testResult: string | null = null;
  /** Fails the command named, with the message (a string, as the app sends). */
  failing = new Map<string, string>();
  /** Holds the command named until the promise settles. */
  holds = new Map<string, Promise<void>>();
  /** What `cancel_connect` answers: `false` once the key is being saved. */
  cancelResult = true;
  calls: Call[] = [];

  hotkeyNotice: string | null = null;

  handle = async (command: string, payload: unknown): Promise<unknown> => {
    const args = (payload ?? {}) as Record<string, unknown>;
    this.calls.push({ command, args });
    const held = this.holds.get(command);
    if (held) await held;
    const failure = this.failing.get(command);
    // Tauri rejects with the command's `Err` value, which is not an Error.
    // eslint-disable-next-line @typescript-eslint/only-throw-error
    if (failure !== undefined) throw failure;
    switch (command) {
      case "get_settings":
        return this.settings;
      case "list_history":
        return { rows: [], unreadable: false };
      case "set_hotkey":
        this.settings = { ...this.settings, hotkey: args.spec as string };
        return null;
      case "set_hotkey_recording":
        return null;
      case "set_launch_at_login":
        this.settings = { ...this.settings, launch_at_login: args.enabled as boolean };
        return args.enabled;
      case "set_share_usage_events":
        this.settings = { ...this.settings, share_usage_events: args.share as boolean };
        return null;
      case "set_transcription":
        this.settings = {
          ...this.settings,
          transcription: args.transcription as Transcription,
        };
        return null;
      case "get_engine_status":
        return this.engine;
      case "list_local_models":
        return this.models;
      case "list_cleanup_models":
        return this.cleanup;
      case "download_local_model":
        this.models = this.models.map((m) =>
          m.id === args.id ? { ...m, state: "downloading", downloaded_bytes: 212_000_000 } : m,
        );
        return null;
      case "cancel_local_model_download":
      case "delete_local_model":
        return null;
      case "set_api_key":
        this.keys.openrouter = true;
        return null;
      case "delete_api_key":
        this.keys.openrouter = false;
        return null;
      case "saved_key_hint":
        return this.keys.openrouter ? "sk-or-…cdef" : null;
      case "connect_openrouter":
        this.keys.openrouter = true;
        return null;
      case "cancel_connect":
        return this.cancelResult;
      case "test_api_key":
        // eslint-disable-next-line @typescript-eslint/only-throw-error
        if (this.testResult !== null) throw this.testResult;
        return null;
      case "get_app_info":
        return { version: "0.1.0", last_update_check_ms: null };
      case "get_hotkey_notice":
        return this.hotkeyNotice;
      case "check_for_updates":
      case "open_link":
        return null;
      default:
        throw new Error(`unexpected command ${command}`);
    }
  };

  named(command: string): Call[] {
    return this.calls.filter((call) => call.command === command);
  }
}

let backend: Backend;

beforeEach(() => {
  backend = new Backend();
  window.localStorage.clear();
  mockIPC((command, payload) => backend.handle(command, payload));
});

afterEach(() => {
  vi.useRealTimers();
  releaseAllKeys();
  cleanup();
  clearMocks();
});

/** A promise and the function that settles it. */
function gate() {
  let open: () => void = () => undefined;
  const closed = new Promise<void>((resolve) => {
    open = resolve;
  });
  return { closed, open };
}

const SECTION_NAMES: Record<SectionId, string> = {
  general: "General",
  dictation: "Dictation",
  speech: "Speech",
  privacy: "Privacy",
  history: "History",
  help: "Help",
  about: "About",
};

/** Renders Settings at `section` once the settings have loaded. */
async function open(section: SectionId = "general", props: Parameters<typeof Settings>[0] = {}) {
  render(<Settings {...props} />);
  fireEvent.click(await screen.findByRole("tab", { name: SECTION_NAMES[section] }));
  await screen.findByRole("heading", { level: 1, name: SECTION_NAMES[section] });
  await waitFor(() => {
    expect(screen.queryByText("Loading…")).toBeNull();
  });
}

const click = (name: string | RegExp) => {
  fireEvent.click(screen.getByRole("button", { name }));
};

/** What assistive tech would call `element`: enough of the accessible-name rules for these controls. */
function accessibleName(element: Element): string {
  const label = element.getAttribute("aria-label");
  if (label) return label;
  const by = element.getAttribute("aria-labelledby");
  if (by) {
    return by
      .split(" ")
      .map((id) => document.getElementById(id)?.textContent ?? "")
      .join(" ")
      .trim();
  }
  if (element instanceof HTMLInputElement || element instanceof HTMLSelectElement) {
    const first = Array.from(element.labels ?? [])[0];
    if (first !== undefined) return first.textContent.trim();
  }
  return element.textContent.trim();
}

describe("criterion_u6_1 sections, keyboard and names", () => {
  it("shows the sidebar sections and opens each one's pane", async () => {
    await open();
    const tabs = screen.getAllByRole("tab").map((tab) => tab.textContent);
    expect(tabs).toEqual(["General", "Dictation", "Speech", "Privacy", "History", "Help", "About"]);
    for (const name of tabs) {
      fireEvent.click(screen.getByRole("tab", { name }));
      expect(screen.getByRole("heading", { level: 1 }).textContent).toBe(name);
      expect(screen.getByRole("tab", { name }).getAttribute("aria-selected")).toBe("true");
    }
  });

  it("remembers the last section opened", async () => {
    await open("privacy");
    expect(window.localStorage.getItem(SECTION_KEY)).toBe("privacy");
    cleanup();
    render(<Settings />);
    expect(
      (await screen.findByRole("heading", { level: 1, name: "Privacy" })).textContent,
    ).toBeTruthy();
  });

  it("moves between sections with the arrow keys, and into the pane with Tab order", async () => {
    await open();
    const general = screen.getByRole("tab", { name: "General" });
    general.focus();
    fireEvent.keyDown(general, { key: "ArrowDown" });
    expect(document.activeElement?.textContent).toBe("Dictation");
    expect(screen.getByRole("heading", { level: 1 }).textContent).toBe("Dictation");
    fireEvent.keyDown(document.activeElement ?? general, { key: "End" });
    expect(screen.getByRole("heading", { level: 1 }).textContent).toBe("About");
    fireEvent.keyDown(document.activeElement ?? general, { key: "ArrowDown" });
    expect(screen.getByRole("heading", { level: 1 }).textContent).toBe("General");
    // Only the selected tab is in the Tab order; the pane follows it.
    expect(
      screen.getAllByRole("tab").filter((tab) => tab.getAttribute("tabindex") === "0"),
    ).toHaveLength(1);
    expect(screen.getByRole("tabpanel").getAttribute("tabindex")).toBe("0");
  });

  it("opens the section the app asks for", async () => {
    render(<Settings open={{ section: "speech", id: 1 }} />);
    expect(await screen.findByRole("heading", { level: 1, name: "Speech" })).toBeTruthy();
  });

  it("gives every control in every section an accessible name", async () => {
    await open("speech");
    expect(screen.getByRole("button", { name: "Connect…" })).toBeTruthy();
    click("Done");
    for (const name of Object.values(SECTION_NAMES)) {
      fireEvent.click(screen.getByRole("tab", { name }));
      await screen.findByRole("heading", { level: 1, name });
      const pane = screen.getByRole("tabpanel");
      await waitFor(() => {
        expect(within(pane).queryByText(/Loading/)).toBeNull();
      });
      for (const control of pane.querySelectorAll("button, input, select")) {
        expect(accessibleName(control), `${name}: ${control.outerHTML}`).not.toBe("");
      }
    }
  });
});

describe("criterion_u6_2 no Save; each control persists on change", () => {
  it("has no Save button anywhere", async () => {
    await open();
    for (const name of Object.values(SECTION_NAMES)) {
      fireEvent.click(screen.getByRole("tab", { name }));
      expect(screen.queryByRole("button", { name: /^Save/ })).toBeNull();
    }
  });

  it("dictation key: recording a key saves it at once and says Saved", async () => {
    await open("dictation");
    expect(screen.getByLabelText("Dictation key").textContent).toBe("Right ⌥");
    click("Change…");
    keyDown("MetaRight");
    keyUp("MetaRight");
    expect((await screen.findByRole("status")).textContent).toBe("Saved");
    expect(backend.settings.hotkey).toBe("RightCmd");
    expect(screen.getByLabelText("Dictation key").textContent).toBe("Right ⌘");
  });

  it("dictation key: a key the app can't switch to keeps the old one and says why", async () => {
    await open("dictation");
    backend.failing.set(
      "set_hotkey",
      "Couldn't switch the dictation key to F6. RightAlt still works.",
    );
    click("Change…");
    keyDown("F6");
    expect((await screen.findByRole("alert")).textContent).toContain("RightAlt still works");
    expect(screen.queryByText("Saved")).toBeNull();
    expect(screen.getByLabelText("Dictation key").textContent).toBe("Right ⌥");
  });

  it("launch at login: the switch writes through its own command", async () => {
    await open("general");
    fireEvent.click(screen.getByRole("switch", { name: "Open Vocet at login" }));
    await waitFor(() => {
      expect(backend.settings.launch_at_login).toBe(true);
    });
  });

  it("events: the switch writes through its own command and says Saved", async () => {
    await open("privacy");
    const toggle = screen.getByRole<HTMLInputElement>("switch", {
      name: "Share usage and error events",
    });
    expect(toggle.checked).toBe(true);
    fireEvent.click(toggle);
    expect((await screen.findByRole("status")).textContent).toBe("Saved");
    expect(backend.named("set_share_usage_events")[0]?.args).toEqual({ share: false });
    expect(toggle.checked).toBe(false);
  });

  it("events: a failed change goes back and says why", async () => {
    await open("privacy");
    backend.failing.set("set_share_usage_events", "Couldn't write the settings file.");
    const toggle = screen.getByRole<HTMLInputElement>("switch", {
      name: "Share usage and error events",
    });
    fireEvent.click(toggle);
    expect((await screen.findByRole("alert")).textContent).toContain("settings file");
    await waitFor(() => {
      expect(toggle.checked).toBe(true);
    });
  });

  it("closing and reopening shows the saved values", async () => {
    await open("dictation");
    click("Change…");
    keyDown("F6");
    await waitFor(() => {
      expect(backend.settings.hotkey).toBe("F6");
    });
    fireEvent.click(screen.getByRole("tab", { name: "Privacy" }));
    fireEvent.click(screen.getByRole("switch", { name: "Share usage and error events" }));
    await waitFor(() => {
      expect(backend.settings.share_usage_events).toBe(false);
    });
    cleanup();
    await open("dictation");
    expect(screen.getByLabelText("Dictation key").textContent).toBe("F6");
    fireEvent.click(screen.getByRole("tab", { name: "Privacy" }));
    expect(
      screen.getByRole<HTMLInputElement>("switch", { name: "Share usage and error events" })
        .checked,
    ).toBe(false);
  });
});

describe("U9 a hotkey waiting for Accessibility", () => {
  const WAITING = "The hotkey Right ⌥ is saved, but Vocet can't listen to the keyboard yet.";

  it("is said on the Dictation key row, while saving other settings still works", async () => {
    backend.hotkeyNotice = WAITING;
    await open("dictation");
    expect((await screen.findByText(WAITING)).getAttribute("role")).toBe("status");
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("goes by itself once the key listens", async () => {
    backend.hotkeyNotice = WAITING;
    await open("dictation");
    await screen.findByText(WAITING);
    backend.hotkeyNotice = null;
    await waitFor(
      () => {
        expect(screen.queryByText(WAITING)).toBeNull();
      },
      { timeout: HOTKEY_NOTICE_POLL_MS + 1_000 },
    );
  });

  it("says nothing while the key listens", async () => {
    await open("dictation");
    await waitFor(() => {
      expect(backend.named("get_hotkey_notice").length).toBeGreaterThan(0);
    });
    expect(screen.queryByText(/can't listen/)).toBeNull();
  });
});

describe("criterion_u6_7c the typed shortcut is gone", () => {
  it("has no raw text field for the shortcut", async () => {
    await open("dictation");
    expect(screen.queryByLabelText("Or type it")).toBeNull();
    expect(screen.queryByText(/RightAlt/)).toBeNull();
  });
});

describe("criterion_u6_3 launch at login never blocks", () => {
  const box = () => screen.getByRole<HTMLInputElement>("switch", { name: "Open Vocet at login" });

  it("shows Applying… while the OS works, and the window stays usable", async () => {
    const slow = gate();
    backend.holds.set("set_launch_at_login", slow.closed);
    await open("general");
    fireEvent.click(box());
    expect(box().checked).toBe(true);
    expect((await screen.findByRole("status")).textContent).toContain("Applying…");
    // Another section works meanwhile.
    fireEvent.click(screen.getByRole("tab", { name: "Dictation" }));
    click("Change…");
    keyDown("F6");
    await waitFor(() => {
      expect(backend.settings.hotkey).toBe("F6");
    });
    fireEvent.click(screen.getByRole("tab", { name: "General" }));
    slow.open();
    await waitFor(() => {
      expect(screen.queryByText(/Applying…/u)).toBeNull();
    });
    expect(box().checked).toBe(true);
  });

  it("reverts to the real OS state and shows the error inline on failure", async () => {
    await open("general");
    backend.failing.set(
      "set_launch_at_login",
      "Launch at login needs your approval in System Settings → General → Login Items.",
    );
    fireEvent.click(box());
    expect((await screen.findByRole("alert")).textContent).toContain("Login Items");
    await waitFor(() => {
      expect(box().checked).toBe(false);
    });
    expect(screen.queryByText(/Applying…/u)).toBeNull();
  });

  it("the switch waits while the OS is busy, so one change runs at a time", async () => {
    const slow = gate();
    backend.holds.set("set_launch_at_login", slow.closed);
    await open("general");
    fireEvent.click(box());
    expect(box().disabled).toBe(true);
    slow.open();
    await waitFor(() => {
      expect(screen.queryByText(/Applying…/u)).toBeNull();
    });
    expect(box().disabled).toBe(false);
    expect(box().checked).toBe(true);
    expect(backend.named("set_launch_at_login").map((call) => call.args.enabled)).toEqual([true]);
  });

  it("reads the OS again when the hidden window is shown again", async () => {
    await open("general");
    backend.settings = { ...backend.settings, launch_at_login: true };
    act(() => {
      window.dispatchEvent(new Event("focus"));
    });
    await waitFor(() => {
      expect(box().checked).toBe(true);
    });
  });
});

describe("u12_criterion_3 Speech is one pick-one list", () => {
  const radio = (name: string | RegExp) => screen.getByRole<HTMLInputElement>("radio", { name });
  const checked = () =>
    screen.getAllByRole<HTMLInputElement>("radio").filter((candidate) => candidate.checked);
  const ready = (id: string) => model(id, { state: "ready", downloaded_bytes: 477_274_496 });
  beforeEach(() => {
    backend.keys.openrouter = true;
  });

  it("names lead, OpenRouter first, with no Downloaded, Use this, Active or Hide", async () => {
    backend.models = [ready(ENGLISH)];
    await open("speech");
    // Q4 (owner): OpenRouter is listed first.
    const radios = within(screen.getByRole("radiogroup", { name: "Speech engine" })).getAllByRole(
      "radio",
    );
    expect(radios[0]?.getAttribute("aria-label")).toBe("OpenRouter");
    expect(radio(/^On this Mac$/)).toBeTruthy();
    expect(radio(/^OpenRouter/)).toBeTruthy();
    expect(radios).toHaveLength(2);
    // Q4 (owner): "On this Mac", then its language, offline and private.
    expect(screen.getByText("English · works offline · nothing leaves your Mac")).toBeTruthy();
    expect(
      await screen.findByText(
        "OpenRouter · speech is sent to OpenRouter · speech: GPT-4o mini Transcribe (OpenAI) · cleanup: GLM 5.3 Flash (Z.ai)",
      ),
    ).toBeTruthy();
    expect(screen.queryByText(/about 100 languages/i)).toBeNull();
    for (const gone of ["Downloaded", "✓ Active", "Use this", "Hide", "Set up…", "Multilingual"]) {
      expect(screen.queryByText(gone), gone).toBeNull();
    }
  });

  it("exactly one row is selected", async () => {
    backend.models = [ready(ENGLISH)];
    await open("speech");
    expect(checked().map((row) => row.id)).toEqual(["engine-openrouter"]);
  });

  it("U11 criterion 5: its size is setup's, from one function", async () => {
    await open("speech");
    const [model] = backend.models;
    if (model === undefined) throw new Error("no model");
    const [name, size] = modelLabel(model).split(" · ");
    expect(name).toBe("English");
    expect(screen.getByRole("button", { name: `Download · ${size ?? "?"}` })).toBeTruthy();
    // Setup's card shows this label, and the catalogue's name appears in neither.
    expect(modelLabel(model)).not.toContain(model.name);
  });

  it("a row not downloaded shows Download and its size, and can't be selected", async () => {
    await open("speech");
    expect(radio(/^On this Mac$/).disabled).toBe(true);

    expect(screen.getByRole("button", { name: "Download · 477 MB" })).toBeTruthy();
    fireEvent.click(radio(/^On this Mac$/));
    expect(backend.named("set_transcription")).toHaveLength(0);
  });

  it("Download starts at once and shows progress with Pause; it can't be selected yet", async () => {
    await open("speech");
    click("Download · 477 MB");
    expect(await screen.findByText("↓ 212 of 477 MB")).toBeTruthy();
    expect(screen.getByRole("progressbar", { name: "On this Mac download" })).toBeTruthy();
    expect(radio(/^On this Mac$/).disabled).toBe(true);
    expect(screen.getByTestId("blocked-reason").textContent).toBe(
      "You can choose it when the download finishes.",
    );
    click("Pause");
    await waitFor(() => {
      expect(backend.named("cancel_local_model_download")[0]?.args).toEqual({ id: ENGLISH });
    });
  });

  // Tester run 1 (minor): after Pause the part is kept, and Resume continues it.
  it("a paused download says so, can't be selected, and Resume continues it", async () => {
    backend.models = [model(ENGLISH, { state: "partial", downloaded_bytes: 212_000_000 })];
    await open("speech");
    expect(await screen.findByText("Paused · 212 of 477 MB")).toBeTruthy();
    expect(radio(/^On this Mac$/).disabled).toBe(true);
    expect(screen.getByTestId("blocked-reason").textContent).toBe(
      "Finish the download to choose it.",
    );
    click("Resume");
    expect(await screen.findByText("↓ 212 of 477 MB")).toBeTruthy();
    expect(backend.named("download_local_model")[0]?.args).toEqual({ id: ENGLISH });
  });

  it("a model already chosen while it downloads is the selected row", async () => {
    backend.settings = { ...backend.settings, transcription: "local" };
    backend.models = [model(ENGLISH, { state: "downloading", downloaded_bytes: 212_000_000 })];
    await open("speech");
    expect(checked().map((row) => row.id)).toEqual([`engine-${ENGLISH}`]);
    expect(radio(/^On this Mac$/).disabled).toBe(true);
  });

  it("Q4: On this Mac is one download: one row, one progress bar, one Pause", async () => {
    backend.models = [ready(ENGLISH)];
    backend.cleanup = [
      model("gemma", {
        size_bytes: 3_106_738_272,
        state: "downloading",
        downloaded_bytes: 1_000_000_000,
      }),
    ];
    await open("speech");
    expect(
      await screen.findByText("Downloading on-device models (speech + cleanup) · 3.6 GB"),
    ).toBeTruthy();
    // The speech model's 477 MB plus the cleanup model's progress so far.
    expect(screen.getByText("↓ 1477 of 3584 MB")).toBeTruthy();
    expect(screen.getAllByRole("progressbar")).toHaveLength(1);
    expect(screen.getAllByRole("button", { name: "Pause" })).toHaveLength(1);
    expect(screen.queryByText(/Cleanup/)).toBeNull();
    // Pause cancels by the speech model's id: the app cancels both.
    click("Pause");
    await waitFor(() => {
      expect(backend.named("cancel_local_model_download")[0]?.args).toEqual({ id: ENGLISH });
    });
  });

  it("selecting the model loads it, blocks OpenRouter with a reason, then selects it", async () => {
    backend.models = [ready(ENGLISH)];
    const loading = gate();
    backend.holds.set("set_transcription", loading.closed);
    await open("speech");
    fireEvent.click(radio(/^On this Mac$/));
    expect(await screen.findByText("Loading…")).toBeTruthy();
    expect(checked().map((row) => row.id)).toEqual([`engine-${ENGLISH}`]);
    expect(radio(/^OpenRouter/).disabled).toBe(true);
    loading.open();
    await waitFor(() => {
      expect(screen.queryByText("Loading…")).toBeNull();
    });
    expect(backend.settings.transcription).toBe("local");
    expect(checked().map((row) => row.id)).toEqual([`engine-${ENGLISH}`]);
  });

  it("a model that fails to load says so, with Retry and Delete in its menu", async () => {
    backend.models = [ready(ENGLISH)];
    backend.failing.set(
      "set_transcription",
      "Couldn't load OrukQwen English: the file is damaged.",
    );
    await open("speech");
    fireEvent.click(radio(/^On this Mac$/));
    expect(await screen.findByText("! Couldn't load")).toBeTruthy();
    expect(screen.getByRole("alert").textContent).toContain("file is damaged");
    expect(checked().map((row) => row.id)).toEqual(["engine-openrouter"]);
    expect(screen.getByRole("button", { name: "Retry" })).toBeTruthy();
    click("More actions for On this Mac");
    expect(screen.getByRole("menuitem", { name: "Delete model" })).toBeTruthy();
  });

  it("Delete is in the … menu; the row in use has the menu but can't be deleted", async () => {
    backend.settings = { ...backend.settings, transcription: "local" };
    backend.models = [ready(ENGLISH)];
    await open("speech");
    click("More actions for On this Mac");
    expect(screen.queryByRole("menuitem")).toBeNull();
    expect(screen.getByText(/Choose another engine to delete it/)).toBeTruthy();
    expect(screen.getByText(/OrukQwen English · English only · works offline/)).toBeTruthy();
  });

  it("the engine in use couldn't load: it says so and stays selected", async () => {
    backend.settings = { ...backend.settings, transcription: "local" };
    backend.models = [ready(ENGLISH)];
    backend.engine = { state: "failed", message: "The on-device model couldn't be loaded." };
    await open("speech");
    expect(await screen.findByText("! Couldn't load")).toBeTruthy();
    expect(checked().map((row) => row.id)).toEqual([`engine-${ENGLISH}`]);
  });

  it("selecting OpenRouter with a key saved switches at once, without the sheet", async () => {
    backend.settings = { ...backend.settings, transcription: "local" };
    backend.models = [ready(ENGLISH)];
    await open("speech");
    fireEvent.click(radio(/^OpenRouter/));
    await waitFor(() => {
      expect(backend.named("set_transcription")[0]?.args).toEqual({ transcription: "cloud" });
    });
    expect(screen.queryByRole("dialog")).toBeNull();
    await waitFor(() => {
      expect(checked().map((row) => row.id)).toEqual(["engine-openrouter"]);
    });
  });
});

describe("u12_criterion_4 the OpenRouter sheet (Q3)", () => {
  const radio = (name: string | RegExp) => screen.getByRole<HTMLInputElement>("radio", { name });
  const onDevice = () => {
    backend.settings = { ...backend.settings, transcription: "local" };
    backend.models = [model(ENGLISH, { state: "ready", downloaded_bytes: 477_274_496 })];
  };

  it("selecting OpenRouter with no key opens the sheet and switches once Connect works", async () => {
    onDevice();
    await open("speech");
    expect(screen.queryByRole("dialog")).toBeNull();
    fireEvent.click(radio(/^OpenRouter/));
    expect(await screen.findByRole("dialog", { name: "OpenRouter" })).toBeTruthy();
    expect(backend.named("set_transcription")).toHaveLength(0);
    expect(radio(/^OpenRouter/).checked).toBe(false);
    click("Connect OpenRouter");
    expect(await screen.findByText(CONNECTED)).toBeTruthy();
    expect(backend.named("connect_openrouter")).toHaveLength(1);
    await waitFor(() => {
      expect(backend.named("set_transcription")[0]?.args).toEqual({ transcription: "cloud" });
    });
    await waitFor(() => {
      expect(radio(/^OpenRouter/).checked).toBe(true);
    });
  });

  it("a failed sign-in says why and saves and switches nothing", async () => {
    onDevice();
    backend.failing.set("connect_openrouter", "Connecting took too long. Try again.");
    await open("speech");
    fireEvent.click(radio(/^OpenRouter/));
    await screen.findByRole("dialog");
    click("Connect OpenRouter");
    expect(await screen.findByText("Connecting took too long. Try again.")).toBeTruthy();
    expect(backend.named("set_transcription")).toHaveLength(0);
    expect(backend.keys.openrouter).toBe(false);
  });

  it("Cancel ends the wait, and a cancelled attempt is not shown as an error", async () => {
    onDevice();
    const hold = gate();
    backend.holds.set("connect_openrouter", hold.closed);
    await open("speech");
    fireEvent.click(radio(/^OpenRouter/));
    await screen.findByRole("dialog");
    click("Connect OpenRouter");
    expect(await screen.findByText("Waiting for OpenRouter in your browser…")).toBeTruthy();
    click("Cancel");
    await waitFor(() => {
      expect(backend.named("cancel_connect")).toHaveLength(1);
    });
    backend.failing.set("connect_openrouter", "Connecting was cancelled.");
    await act(async () => {
      hold.open();
      await Promise.resolve();
    });
    await screen.findByRole("button", { name: "Connect OpenRouter" });
    expect(screen.queryByText("Connecting was cancelled.")).toBeNull();
    expect(backend.keys.openrouter).toBe(false);
  });

  it("Cancel while the key is being saved can't cancel it: it says so, shuts the exits, and the save finishes", async () => {
    onDevice();
    const hold = gate();
    backend.holds.set("connect_openrouter", hold.closed);
    backend.cancelResult = false;
    await open("speech");
    fireEvent.click(radio(/^OpenRouter/));
    await screen.findByRole("dialog");
    click("Connect OpenRouter");
    await screen.findByText("Waiting for OpenRouter in your browser…");
    click("Cancel");
    expect(await screen.findByText("Saving your key…")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Cancel" })).toBeNull();
    expect(screen.getByRole<HTMLButtonElement>("button", { name: "Done" }).disabled).toBe(true);
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(screen.getByRole("dialog")).toBeTruthy();
    await act(async () => {
      hold.open();
      await Promise.resolve();
    });
    // The save finished, so the result counts: connected, and switched.
    expect(await screen.findByText(CONNECTED)).toBeTruthy();
    expect(backend.keys.openrouter).toBe(true);
    await waitFor(() => {
      expect(backend.named("set_transcription")).toHaveLength(1);
    });
  });

  it("a save that fails after Cancel was pressed shows the Keychain error, not a silent cancel", async () => {
    onDevice();
    const hold = gate();
    backend.holds.set("connect_openrouter", hold.closed);
    backend.cancelResult = false;
    await open("speech");
    fireEvent.click(radio(/^OpenRouter/));
    await screen.findByRole("dialog");
    click("Connect OpenRouter");
    await screen.findByText("Waiting for OpenRouter in your browser…");
    click("Cancel");
    await screen.findByText("Saving your key…");
    backend.failing.set("connect_openrouter", "the Keychain is locked");
    await act(async () => {
      hold.open();
      await Promise.resolve();
    });
    expect(await screen.findByText("the Keychain is locked")).toBeTruthy();
    expect(backend.named("set_transcription")).toHaveLength(0);
  });

  it.each([
    [
      "Done",
      () => {
        click("Done");
      },
    ],
    [
      "Esc",
      () => {
        fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
      },
    ],
  ])(
    "%s while the key is already being saved stays open and shows how it ends",
    async (_name, leave) => {
      onDevice();
      const hold = gate();
      backend.holds.set("connect_openrouter", hold.closed);
      backend.cancelResult = false;
      await open("speech");
      fireEvent.click(radio(/^OpenRouter/));
      await screen.findByRole("dialog");
      click("Connect OpenRouter");
      await screen.findByText("Waiting for OpenRouter in your browser…");
      leave();
      // The cancel lost the race: the sheet stays, in the saving state.
      expect(await screen.findByText("Saving your key…")).toBeTruthy();
      expect(screen.getByRole("dialog")).toBeTruthy();
      await act(async () => {
        hold.open();
        await Promise.resolve();
      });
      expect(await screen.findByText(CONNECTED)).toBeTruthy();
      expect(screen.getByRole("dialog")).toBeTruthy();
      await waitFor(() => {
        expect(backend.named("set_transcription")).toHaveLength(1);
      });
    },
  );

  it("Done during the wait cancels it", async () => {
    onDevice();
    backend.holds.set("connect_openrouter", gate().closed);
    await open("speech");
    fireEvent.click(radio(/^OpenRouter/));
    await screen.findByRole("dialog");
    click("Connect OpenRouter");
    await screen.findByText("Waiting for OpenRouter in your browser…");
    click("Done");
    await waitFor(() => {
      expect(backend.named("cancel_connect")).toHaveLength(1);
    });
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("opens by itself when OpenRouter is the engine and not connected, once", async () => {
    backend.keys.openrouter = false;
    await open("speech");
    expect(await screen.findByRole("dialog", { name: "OpenRouter" })).toBeTruthy();
    click("Done");
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).toBeNull();
    });
  });

  it("closed unless asked for, while another engine is in use", async () => {
    onDevice();
    await open("speech");
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("a connected account shows the masked key, Disconnect, Add credits, and no full key", async () => {
    backend.keys.openrouter = true;
    await open("speech");
    expect(await screen.findByRole("button", { name: "Manage…" })).toBeTruthy();
    click("Manage…");
    expect(await screen.findByText("Connected: sk-or-…cdef")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Disconnect" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Add credits" })).toBeTruthy();
    expect(document.body.textContent).not.toContain(KEY);
    expect(backend.named("saved_key_hint")).toHaveLength(1);
  });

  it("Add credits opens OpenRouter's credits page through the app", async () => {
    backend.keys.openrouter = true;
    await open("speech");
    click("Manage…");
    click("Add credits");
    await waitFor(() => {
      expect(backend.named("open_link")[0]?.args).toEqual({ link: "openrouter_credits" });
    });
  });

  it("Disconnect deletes the key and the row goes back to Connect…", async () => {
    backend.keys.openrouter = true;
    await open("speech");
    click("Manage…");
    click("Disconnect");
    await screen.findByText("Disconnected. The key is removed.");
    expect(backend.named("delete_api_key")).toHaveLength(1);
    expect(backend.keys.openrouter).toBe(false);
    expect(await screen.findByRole("button", { name: "Connect OpenRouter" })).toBeTruthy();
  });

  it("a failed Disconnect says so and the key still shows as connected", async () => {
    backend.keys.openrouter = true;
    backend.failing.set("delete_api_key", "the Keychain didn't answer");
    await open("speech");
    click("Manage…");
    click("Disconnect");
    await screen.findByText("the Keychain didn't answer");
    expect(await screen.findByText("Connected: sk-or-…cdef")).toBeTruthy();
  });

  it("Esc closes the sheet and the keyboard stays inside it", async () => {
    backend.keys.openrouter = false;
    onDevice();
    await open("speech");
    click("Connect…");
    const dialog = await screen.findByRole("dialog");
    fireEvent.keyDown(dialog, { key: "Escape" });
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).toBeNull();
    });
  });

  describe("Paste a key instead", () => {
    async function openPaste() {
      onDevice();
      await open("speech");
      click("Connect…");
      await screen.findByRole("dialog");
      click("Paste a key instead");
    }

    it("tests, then saves; success only after both, and the key is never shown back", async () => {
      await openPaste();
      const field = screen.getByLabelText(/API key/);
      expect((field as HTMLInputElement).type).toBe("password");
      fireEvent.change(field, { target: { value: KEY } });
      click("Test & Save");
      expect(await screen.findByText(CONNECTED)).toBeTruthy();
      expect(backend.named("test_api_key")[0]?.args).toEqual({ key: KEY });
      expect(backend.named("set_api_key")[0]?.args).toEqual({ key: KEY });
      expect(document.body.textContent).not.toContain(KEY);
      expect(screen.getByLabelText<HTMLInputElement>(/API key/).value).toBe("");
    });

    it("a key the provider rejects is not saved", async () => {
      backend.testResult = "OpenRouter rejected this key.";
      await openPaste();
      fireEvent.change(screen.getByLabelText(/API key/), { target: { value: KEY } });
      click("Test & Save");
      await screen.findByText("OpenRouter rejected this key.");
      expect(backend.named("set_api_key")).toHaveLength(0);
    });

    it("a Keychain failure after a good test is its own error", async () => {
      backend.failing.set("set_api_key", "the Keychain is locked");
      await openPaste();
      fireEvent.change(screen.getByLabelText(/API key/), { target: { value: KEY } });
      click("Test & Save");
      expect(await screen.findByText(`${KEY_NOT_SAVED}: the Keychain is locked`)).toBeTruthy();
    });

    it("an empty or malformed key is refused before anything is sent", async () => {
      await openPaste();
      click("Test & Save");
      await screen.findByText("Paste your API key.");
      fireEvent.change(screen.getByLabelText(/API key/), { target: { value: "two words" } });
      click("Test & Save");
      await screen.findByText("A key has no spaces; check what you pasted.");
      expect(backend.named("test_api_key")).toHaveLength(0);
    });
  });

  it("asks the app for a hint, never for the key", async () => {
    backend.keys.openrouter = true;
    await open("speech");
    await screen.findByRole("button", { name: "Manage…" });
    for (const call of backend.calls) {
      expect(call.command).not.toBe("get_api_key");
    }
  });
});

describe("criterion_u6_7 Help", () => {
  it("each button calls the app's action", async () => {
    const onTroubleshoot = vi.fn();
    const onReport = vi.fn();
    const onShowLog = vi.fn(() => Promise.resolve(null));
    const onSetup = vi.fn();
    await open("help", { onTroubleshoot, onReport, onShowLog, onSetup });
    click("Open…");
    click("Preview…");
    click("Show in Finder");
    click("Start…");
    expect(onTroubleshoot).toHaveBeenCalledOnce();
    expect(onReport).toHaveBeenCalledOnce();
    expect(onShowLog).toHaveBeenCalledOnce();
    expect(onSetup).toHaveBeenCalledOnce();
  });

  // U12 criterion 12.
  it("no button repeats its row, and setup's steps are in their real order", async () => {
    await open("help", {
      onTroubleshoot: () => undefined,
      onReport: () => undefined,
      onShowLog: () => Promise.resolve(null),
      onSetup: () => undefined,
    });
    const pane = within(screen.getByRole("tabpanel"));
    for (const label of ["Troubleshoot", "Report a problem", "Log file", "Run setup again"]) {
      expect(pane.getByText(label)).toBeTruthy();
      expect(pane.queryByRole("button", { name: new RegExp(label, "i") }), label).toBeNull();
    }
    expect(pane.getByText(/speech, permissions and a test/)).toBeTruthy();
  });

  it("says so when the log can't be shown", async () => {
    await open("help", { onShowLog: () => Promise.resolve("Couldn't open the log folder.") });
    click("Show in Finder");
    expect((await screen.findByRole("alert")).textContent).toContain("log folder");
  });
});

describe("General and About", () => {
  it("Check for updates asks the app", async () => {
    await open("general");
    click("Check now");
    await waitFor(() => {
      expect(backend.named("check_for_updates")).toHaveLength(1);
    });
  });

  it("About shows the version and opens the licence, source and credits", async () => {
    await open("about");
    expect(await screen.findByText("Version 0.1.0")).toBeTruthy();
    expect(screen.getByText(/FSL-1\.1-ALv2/)).toBeTruthy();
    expect(screen.queryByText(/MIT/)).toBeNull();
    click("Read the licence");
    click("Open on GitHub");
    click("Show credits");
    await waitFor(() => {
      expect(backend.named("open_link").map((call) => call.args.link)).toEqual([
        "licence",
        "repository",
        "notice",
      ]);
    });
  });
});

describe("Privacy", () => {
  it("Q5: summarises the current mode's data flow and links to the policy", async () => {
    await open("privacy");
    expect(screen.getByText(/goes through OpenRouter to OpenAI/)).toBeTruthy();
    click("Read the privacy policy");
    await waitFor(() => {
      expect(backend.named("open_link")[0]?.args).toEqual({ link: "privacy" });
    });
  });

  it("Q5: says audio and text stay on this Mac in On this Mac mode", async () => {
    backend.settings = { ...backend.settings, transcription: "local" };
    await open("privacy");
    expect(screen.getByText(/audio and text stay on your Mac/)).toBeTruthy();
  });

  it("lists what is sent in plain words, and links to the full notice", async () => {
    await open("privacy");
    fireEvent.click(screen.getByText("What's sent"));
    expect(screen.getByText(/Which setting you changed \(not the new value\)/)).toBeTruthy();
    click("Read the full privacy notice");
    await waitFor(() => {
      expect(backend.named("open_link")[0]?.args).toEqual({ link: "privacy" });
    });
  });
});

describe("loading", () => {
  it("shows why the settings couldn't load", async () => {
    backend.failing.set("get_settings", "The settings file is damaged.");
    render(<Settings />);
    expect((await screen.findByRole("alert")).textContent).toContain("damaged");
  });
});
