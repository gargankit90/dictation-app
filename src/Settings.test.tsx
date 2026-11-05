// U6: the settings window in sections, every control applied on change,
// with the app's commands replaced by a fake backend through `mockIPC`.
// (Migrated from the M3.4/K1/S1/A1 form tests.) The keys here are made up.

import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type {
  EngineStatus,
  KeyTestError,
  LocalModelInfo_Serialize,
  Provider,
  Settings_Serialize,
} from "./bindings";
import { SECTION_KEY, Settings } from "./Settings";
import type { SectionId } from "./Settings";
import { CONNECTED, KEY_NOT_SAVED, NOTICE_DISMISSED_KEY } from "./settings/Speech";
import { keyDown, keyUp, releaseAllKeys } from "./testKeys";

const KEY = "gsk_dummy_NOT_A_REAL_KEY_0123456789";
const ENGLISH = "parakeet-unified-en-0.6b-q4_k_m";
const MULTI = "qwen3-asr-0.6b-q4_k_m";

interface Call {
  command: string;
  args: Record<string, unknown>;
}

function model(id: string, overrides: Partial<LocalModelInfo_Serialize> = {}) {
  return {
    id,
    name: id === ENGLISH ? "Parakeet English" : "Qwen3-ASR multilingual",
    languages: id === ENGLISH ? "English only" : "30 languages",
    language_codes: id === ENGLISH ? ["en"] : ["en", "hi", "de"],
    size_bytes: id === ENGLISH ? 477_274_496 : 600_000_000,
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
    provider: "groq",
    model: "whisper-large-v3-turbo",
    language: null,
    hotkey: "RightAlt",
    launch_at_login: false,
    transcription: "cloud",
    local_model: ENGLISH,
    share_usage_events: true,
    history_limit: 25,
  };
  keys: Record<Provider, boolean> = { groq: false, openai: false };
  models: LocalModelInfo_Serialize[] = [model(ENGLISH), model(MULTI)];
  engine: EngineStatus = { state: "ready", message: null };
  /** What `test_api_key` answers: `null` for success, or an error to throw. */
  testResult: KeyTestError | null = null;
  /** What `launch_notice` says: the first launch after the rename (U1). */
  launchNotice: string | null = null;
  /** The levels the microphone test reads, in order (then the last again). */
  micLevels: number[] = [-90];
  micRunning = false;
  /** Fails the command named, with the message (a string, as the app sends). */
  failing = new Map<string, string>();
  /** Holds the command named until the promise settles. */
  holds = new Map<string, Promise<void>>();
  calls: Call[] = [];

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
      case "launch_notice":
        return this.launchNotice;
      case "list_history":
        return { rows: [], unreadable: false };
      case "set_hotkey":
        this.settings = { ...this.settings, hotkey: args.spec as string };
        return null;
      case "set_hotkey_recording":
        return null;
      case "set_language":
        this.settings = { ...this.settings, language: args.language as string | null };
        return null;
      case "set_launch_at_login":
        this.settings = { ...this.settings, launch_at_login: args.enabled as boolean };
        return args.enabled;
      case "set_share_usage_events":
        this.settings = { ...this.settings, share_usage_events: args.share as boolean };
        return null;
      case "use_cloud_engine":
        this.settings = {
          ...this.settings,
          transcription: "cloud",
          provider: args.provider as Provider,
        };
        return null;
      case "use_local_model":
        this.settings = {
          ...this.settings,
          transcription: "local",
          local_model: args.id as string,
        };
        return null;
      case "get_engine_status":
        return this.engine;
      case "list_local_models":
        return this.models;
      case "download_local_model":
        this.models = this.models.map((m) =>
          m.id === args.id ? { ...m, state: "downloading", downloaded_bytes: 212_000_000 } : m,
        );
        return null;
      case "cancel_local_model_download":
      case "delete_local_model":
        return null;
      case "set_api_key":
        this.keys[args.provider as Provider] = true;
        return null;
      case "delete_api_key":
        this.keys[args.provider as Provider] = false;
        return null;
      case "has_api_key":
        return this.keys[args.provider as Provider];
      case "test_api_key":
        // eslint-disable-next-line @typescript-eslint/only-throw-error
        if (this.testResult !== null) throw this.testResult;
        return null;
      case "start_microphone_test":
        this.micRunning = true;
        return null;
      case "read_microphone_test": {
        const level = this.micLevels.length > 1 ? this.micLevels.shift() : this.micLevels[0];
        return { level: this.micRunning ? level : null, device: "MacBook Pro Microphone" };
      }
      case "stop_microphone_test":
        this.micRunning = false;
        return null;
      case "get_app_info":
        return { version: "0.1.0", last_update_check_ms: null };
      case "check_for_updates":
      case "open_link":
        return null;
      case "save_settings":
        throw new Error("U6 has no global Save: save_settings must not be called");
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

/** Chooses a language the way a person does: search by name, press Enter. */
const pick = (name: string) => {
  const box = screen.getByLabelText("Language");
  fireEvent.focus(box);
  fireEvent.change(box, { target: { value: name } });
  fireEvent.keyDown(box, { key: "Enter" });
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
    expect(screen.getByRole("button", { name: "Change key…" })).toBeTruthy();
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

  it("language: choosing one saves only the language", async () => {
    await open("dictation");
    pick("German");
    expect((await screen.findByRole("status")).textContent).toBe("Saved");
    expect(backend.named("set_language")[0]?.args).toEqual({ language: "de" });
    pick("Detect");
    await waitFor(() => {
      expect(backend.named("set_language").at(-1)?.args).toEqual({ language: null });
    });
  });

  it("language: a refused language shows the app's error next to it", async () => {
    await open("dictation");
    backend.failing.set("set_language", "That isn't a language code.");
    pick("German");
    expect((await screen.findByRole("alert")).textContent).toContain("isn't a language code");
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

  it("closing and reopening shows the saved values, and nothing ever calls save_settings", async () => {
    await open("dictation");
    pick("German");
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
    expect(screen.getByLabelText<HTMLInputElement>("Language").value).toContain("German");
    fireEvent.click(screen.getByRole("tab", { name: "Privacy" }));
    expect(
      screen.getByRole<HTMLInputElement>("switch", { name: "Share usage and error events" })
        .checked,
    ).toBe(false);
    expect(backend.named("save_settings")).toHaveLength(0);
  });

  it("a change made elsewhere is never put back by another control", async () => {
    await open("dictation");
    // The hotkey changes behind the window's back (another window, the tray).
    backend.settings = { ...backend.settings, hotkey: "F7" };
    pick("German");
    await waitFor(() => {
      expect(backend.settings.language).toBe("de");
    });
    expect(backend.settings.hotkey).toBe("F7");
    expect(screen.getByLabelText("Dictation key").textContent).toBe("F7");
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
    pick("German");
    await waitFor(() => {
      expect(backend.settings.language).toBe("de");
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

  it("ends on the last choice when toggled again while the OS is busy", async () => {
    const slow = gate();
    backend.holds.set("set_launch_at_login", slow.closed);
    await open("general");
    fireEvent.click(box());
    fireEvent.click(box());
    fireEvent.click(box());
    slow.open();
    await waitFor(() => {
      expect(screen.queryByText(/Applying…/u)).toBeNull();
    });
    expect(box().checked).toBe(true);
    expect(backend.named("set_launch_at_login").map((call) => call.args.enabled)).toEqual([
      true,
      false,
      true,
    ]);
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
    backend.keys.groq = true;
  });

  it("names lead, with no Downloaded, Use this, Active or Hide", async () => {
    backend.models = [ready(ENGLISH), model(MULTI)];
    await open("speech");
    expect(radio(/^English/)).toBeTruthy();
    expect(radio(/^All languages/)).toBeTruthy();
    expect(radio(/^Your own key/)).toBeTruthy();
    expect(
      within(screen.getByRole("radiogroup", { name: "Speech engine" })).getAllByRole("radio"),
    ).toHaveLength(3);
    expect(screen.getAllByText("On this Mac")).toHaveLength(2);
    expect(screen.getByText("Groq · uses internet")).toBeTruthy();
    for (const gone of ["Downloaded", "✓ Active", "Use this", "Hide", "Set up…", "Multilingual"]) {
      expect(screen.queryByText(gone), gone).toBeNull();
    }
  });

  it("exactly one row is selected", async () => {
    backend.models = [ready(ENGLISH), ready(MULTI)];
    await open("speech");
    expect(checked().map((row) => row.id)).toEqual(["engine-own-key"]);
  });

  it("a row not downloaded shows Download and its size, and can't be selected", async () => {
    await open("speech");
    expect(radio(/^English/).disabled).toBe(true);

    expect(screen.getByRole("button", { name: "Download · 477 MB" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Download · 600 MB" })).toBeTruthy();
    fireEvent.click(radio(/^English/));
    expect(backend.named("use_local_model")).toHaveLength(0);
  });

  it("Download starts at once and shows progress with Pause; it can't be selected yet", async () => {
    await open("speech");
    click("Download · 477 MB");
    expect(await screen.findByText("↓ 212 of 477 MB")).toBeTruthy();
    expect(screen.getByRole("progressbar", { name: "English download" })).toBeTruthy();
    expect(radio(/^English/).disabled).toBe(true);
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
    expect(await screen.findByText("Download paused, 212 of 477 MB")).toBeTruthy();
    expect(radio(/^English/).disabled).toBe(true);
    expect(screen.getByTestId("blocked-reason").textContent).toBe(
      "Finish the download to choose it.",
    );
    click("Resume");
    expect(await screen.findByText("↓ 212 of 477 MB")).toBeTruthy();
    expect(backend.named("download_local_model")[0]?.args).toEqual({ id: ENGLISH });
  });

  it("a model already chosen while it downloads is the selected row", async () => {
    backend.settings = { ...backend.settings, transcription: "local", local_model: ENGLISH };
    backend.models = [model(ENGLISH, { state: "downloading", downloaded_bytes: 212_000_000 })];
    await open("speech");
    expect(checked().map((row) => row.id)).toEqual([`engine-${ENGLISH}`]);
    expect(radio(/^English/).disabled).toBe(true);
  });

  it("selecting a model loads it, blocks the others with a reason, then selects it", async () => {
    backend.models = [ready(ENGLISH), ready(MULTI)];
    const loading = gate();
    backend.holds.set("use_local_model", loading.closed);
    await open("speech");
    fireEvent.click(radio(/^English/));
    expect(await screen.findByText("Loading…")).toBeTruthy();
    expect(checked().map((row) => row.id)).toEqual([`engine-${ENGLISH}`]);
    expect(radio(/^All languages/).disabled).toBe(true);
    expect(radio(/^Your own key/).disabled).toBe(true);
    expect(screen.getByTestId("blocked-reason").textContent).toBe("Wait until English has loaded.");
    loading.open();
    await waitFor(() => {
      expect(screen.queryByText("Loading…")).toBeNull();
    });
    expect(backend.settings.transcription).toBe("local");
    expect(checked().map((row) => row.id)).toEqual([`engine-${ENGLISH}`]);
  });

  it("a model that fails to load says so, with Retry and Delete in its menu", async () => {
    backend.models = [ready(ENGLISH)];
    backend.failing.set("use_local_model", "Couldn't load Parakeet English: the file is damaged.");
    await open("speech");
    fireEvent.click(radio(/^English/));
    expect(await screen.findByText("! Couldn't load")).toBeTruthy();
    expect(screen.getByRole("alert").textContent).toContain("file is damaged");
    expect(checked().map((row) => row.id)).toEqual(["engine-own-key"]);
    expect(screen.getByRole("button", { name: "Retry" })).toBeTruthy();
    click("More actions for English");
    expect(screen.getByRole("menuitem", { name: "Delete English" })).toBeTruthy();
  });

  it("Delete is in the … menu; the row in use has the menu but can't be deleted", async () => {
    backend.settings = { ...backend.settings, transcription: "local", local_model: ENGLISH };
    backend.models = [ready(ENGLISH), ready(MULTI)];
    await open("speech");
    click("More actions for English");
    expect(screen.queryByRole("menuitem")).toBeNull();
    expect(screen.getByText(/Choose another engine to delete it/)).toBeTruthy();
    expect(screen.getByText(/Parakeet English · English only · works offline/)).toBeTruthy();
    click("More actions for English");
    click("More actions for All languages");
    fireEvent.click(screen.getByRole("menuitem", { name: "Delete All languages" }));
    await waitFor(() => {
      expect(backend.named("delete_local_model")[0]?.args).toEqual({ id: MULTI });
    });
  });

  it("the engine in use couldn't load: it says so and stays selected", async () => {
    backend.settings = { ...backend.settings, transcription: "local", local_model: ENGLISH };
    backend.models = [ready(ENGLISH)];
    backend.engine = { state: "failed", message: "The on-device model couldn't be loaded." };
    await open("speech");
    expect(await screen.findByText("! Couldn't load")).toBeTruthy();
    expect(checked().map((row) => row.id)).toEqual([`engine-${ENGLISH}`]);
  });

  it("selecting the own key with a key saved switches at once, without the sheet", async () => {
    backend.settings = { ...backend.settings, transcription: "local", local_model: ENGLISH };
    backend.models = [ready(ENGLISH)];
    await open("speech");
    fireEvent.click(radio(/^Your own key/));
    await waitFor(() => {
      expect(backend.named("use_cloud_engine")[0]?.args).toEqual({ provider: "groq" });
    });
    expect(screen.queryByRole("dialog")).toBeNull();
    await waitFor(() => {
      expect(checked().map((row) => row.id)).toEqual(["engine-own-key"]);
    });
  });
});

describe("u12_criterion_4 the key sheet", () => {
  const radio = (name: string | RegExp) => screen.getByRole<HTMLInputElement>("radio", { name });
  const onDevice = () => {
    backend.settings = { ...backend.settings, transcription: "local", local_model: ENGLISH };
    backend.models = [model(ENGLISH, { state: "ready", downloaded_bytes: 477_274_496 })];
  };

  it("selecting the own key with no key opens the sheet and doesn't switch until one works", async () => {
    onDevice();
    await open("speech");
    expect(screen.queryByRole("dialog")).toBeNull();
    fireEvent.click(radio(/^Your own key/));
    expect(await screen.findByRole("dialog", { name: "Your own key" })).toBeTruthy();
    expect(backend.named("use_cloud_engine")).toHaveLength(0);
    expect(radio(/^Your own key/).checked).toBe(false);
    // A key the provider rejects switches nothing.
    backend.testResult = { kind: "invalid_key", message: "Groq rejected this key." };
    fireEvent.change(screen.getByLabelText(/API key/), { target: { value: KEY } });
    click("Test & Save");
    await screen.findByText("Groq rejected this key.");
    expect(backend.named("use_cloud_engine")).toHaveLength(0);
    // One that works does.
    backend.testResult = null;
    click("Test & Save");
    expect(await screen.findByText(CONNECTED)).toBeTruthy();
    await waitFor(() => {
      expect(backend.named("use_cloud_engine")[0]?.args).toEqual({ provider: "groq" });
    });
    await waitFor(() => {
      expect(radio(/^Your own key/).checked).toBe(true);
    });
  });

  it("Done without a working key leaves the engine, and a later key doesn't switch it", async () => {
    onDevice();
    await open("speech");
    fireEvent.click(radio(/^Your own key/));
    await screen.findByRole("dialog");
    click("Done");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(radio(/^English/).checked).toBe(true);
    click("Change key…");
    fireEvent.change(screen.getByLabelText(/API key/), { target: { value: KEY } });
    click("Test & Save");
    expect(await screen.findByText(CONNECTED)).toBeTruthy();
    expect(backend.named("use_cloud_engine")).toHaveLength(0);
  });

  it("closed unless asked for, while another engine is in use", async () => {
    onDevice();
    await open("speech");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.queryByLabelText(/API key/)).toBeNull();
  });

  it("opens by itself when the own key is the engine and needs a key", async () => {
    await open("speech");
    expect(await screen.findByRole("dialog", { name: "Your own key" })).toBeTruthy();
    expect(screen.getByText("! Needs a key")).toBeTruthy();
    click("Done");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByText("! Needs a key")).toBeTruthy();
  });

  it("shows one status line: saved, or connected after a test", async () => {
    backend.keys.groq = true;
    await open("speech");
    click("Change key…");
    expect(await screen.findByText("A key is saved in your Keychain.")).toBeTruthy();
    expect(screen.queryByText(/never shown again/)).toBeNull();
    fireEvent.change(screen.getByLabelText(/API key/), { target: { value: KEY } });
    click("Test & Save");
    expect(await screen.findByText(CONNECTED)).toBeTruthy();
    expect(screen.queryByText("A key is saved in your Keychain.")).toBeNull();
    expect(screen.queryByText(/never shown again/)).toBeNull();
  });

  it("has Provider, the how-to, the field, Test & Save, Remove key and Done", async () => {
    backend.keys.groq = true;
    await open("speech");
    click("Change key…");
    const sheet = within(await screen.findByRole("dialog"));
    expect(sheet.getByLabelText("Provider")).toBeTruthy();
    expect(sheet.getByRole("button", { name: "console.groq.com" })).toBeTruthy();
    expect(sheet.getByLabelText("Groq API key")).toBeTruthy();
    for (const name of ["Test & Save", "Remove key", "Done"]) {
      expect(sheet.getByRole("button", { name })).toBeTruthy();
    }
  });

  it("the keyboard stays inside; Esc closes it and drops what was typed", async () => {
    backend.keys.groq = true;
    await open("speech");
    const opener = screen.getByRole("button", { name: "Change key…" });
    opener.focus();
    fireEvent.click(opener);
    const dialog = await screen.findByRole("dialog");
    const field = screen.getByLabelText<HTMLInputElement>(/API key/);
    expect(document.activeElement).toBe(field);
    const done = screen.getByRole("button", { name: "Done" });
    done.focus();
    fireEvent.keyDown(done, { key: "Tab" });
    expect(document.activeElement).toBe(screen.getByLabelText("Provider"));
    fireEvent.change(field, { target: { value: KEY } });
    fireEvent.keyDown(dialog, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(opener);
    click("Change key…");
    expect(screen.getByLabelText<HTMLInputElement>(/API key/).value).toBe("");
  });

  it("closing it during a test drops the test, so its key isn't saved", async () => {
    backend.keys.groq = true;
    await open("speech");
    click("Change key…");
    const slow = gate();
    backend.holds.set("test_api_key", slow.closed);
    fireEvent.change(screen.getByLabelText(/API key/), { target: { value: KEY } });
    click("Test & Save");
    await waitFor(() => {
      expect(backend.named("test_api_key")).toHaveLength(1);
    });
    click("Done");
    slow.open();
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });
    expect(backend.named("set_api_key")).toHaveLength(0);
  });

  it("while the own key is the engine, changing the provider applies at once", async () => {
    backend.keys.groq = true;
    await open("speech");
    click("Change key…");
    fireEvent.change(screen.getByLabelText("Provider"), { target: { value: "openai" } });
    await waitFor(() => {
      expect(backend.named("use_cloud_engine")[0]?.args).toEqual({ provider: "openai" });
    });
    expect(await screen.findByText("Saved")).toBeTruthy();
  });
});

describe("criterion_u6_5 own key: Test & Save", () => {
  const keyBox = () => screen.getByLabelText<HTMLInputElement>(/API key/);
  const OLD = "gsk_dummy_OLDER_TEST_NOT_REAL_000000";
  const savedKeys = () => backend.named("set_api_key").map((call) => call.args.key);

  // Review round 1, finding 1: an obsolete test must not write its key.
  it("a test still running when the key is removed doesn't put its key back", async () => {
    await openWithKey();
    const slow = gate();
    backend.holds.set("test_api_key", slow.closed);
    fireEvent.change(keyBox(), { target: { value: OLD } });
    click("Test & Save");
    // The user changes their mind: edits the field, then removes the key.
    fireEvent.change(keyBox(), { target: { value: "" } });
    click("Remove key");
    expect(await screen.findByText("Key removed from your Keychain.")).toBeTruthy();
    slow.open();
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });
    expect(savedKeys()).toEqual([]);
    expect(backend.keys.groq).toBe(false);
    expect(screen.queryByText(CONNECTED)).toBeNull();
  });

  // Review round 2: the check must survive leaving Speech and coming back.
  const leaveAndReturn = async () => {
    fireEvent.click(screen.getByRole("tab", { name: "General" }));
    await screen.findByRole("heading", { level: 1, name: "General" });
    fireEvent.click(screen.getByRole("tab", { name: "Speech" }));
    await screen.findByRole("button", { name: "Change key…" });
    click("Change key…");
    await screen.findByText("A key is saved in your Keychain.");
  };
  const openWithKey = async () => {
    backend.keys.groq = true;
    await open("speech");
    click("Change key…");
    await screen.findByText("A key is saved in your Keychain.");
  };

  it("after leaving Speech and coming back, an old test doesn't undo Remove key", async () => {
    await openWithKey();
    const slow = gate();
    backend.holds.set("test_api_key", slow.closed);
    fireEvent.change(keyBox(), { target: { value: OLD } });
    click("Test & Save");
    await leaveAndReturn();
    click("Remove key");
    expect(await screen.findByText("Key removed from your Keychain.")).toBeTruthy();
    slow.open();
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });
    expect(savedKeys()).toEqual([]);
    expect(backend.keys.groq).toBe(false);
  });

  it("after leaving Speech and coming back, an old test can't replace a newer key", async () => {
    await openWithKey();
    const slow = gate();
    backend.holds.set("test_api_key", slow.closed);
    fireEvent.change(keyBox(), { target: { value: OLD } });
    click("Test & Save");
    await waitFor(() => {
      expect(backend.named("test_api_key")).toHaveLength(1);
    });
    await leaveAndReturn();
    backend.holds.delete("test_api_key");
    fireEvent.change(keyBox(), { target: { value: KEY } });
    click("Test & Save");
    expect((await screen.findByRole("status")).textContent).toBe(CONNECTED);
    slow.open();
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });
    expect(savedKeys()).toEqual([KEY]);
  });

  it("a slow test can't overwrite a newer key that was saved", async () => {
    await open("speech");
    const slow = gate();
    backend.holds.set("test_api_key", slow.closed);
    fireEvent.change(keyBox(), { target: { value: OLD } });
    click("Test & Save");
    // Only the first test is slow.
    await waitFor(() => {
      expect(backend.named("test_api_key")).toHaveLength(1);
    });
    backend.holds.delete("test_api_key");
    fireEvent.change(keyBox(), { target: { value: KEY } });
    click("Test & Save");
    expect((await screen.findByRole("status")).textContent).toBe(CONNECTED);
    slow.open();
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });
    expect(savedKeys()).toEqual([KEY]);
  });

  it("tests, then saves; success only after both, and the key is never shown back", async () => {
    await open("speech");
    fireEvent.change(keyBox(), { target: { value: KEY } });
    click("Test & Save");
    expect((await screen.findByRole("status")).textContent).toBe(CONNECTED);
    const order = backend.calls
      .map((call) => call.command)
      .filter((c) => c === "test_api_key" || c === "set_api_key");
    expect(order).toEqual(["test_api_key", "set_api_key"]);
    expect(backend.named("test_api_key")[0]?.args).toEqual({ provider: "groq", key: KEY });
    expect(keyBox().value).toBe("");
    expect(document.body.innerHTML).not.toContain(KEY);
  });

  it("a key the provider rejects is not saved", async () => {
    backend.testResult = { kind: "invalid_key", message: "Groq rejected this key." };
    await open("speech");
    fireEvent.change(keyBox(), { target: { value: KEY } });
    click("Test & Save");
    expect((await screen.findByRole("alert")).textContent).toBe("Groq rejected this key.");
    expect(backend.named("set_api_key")).toHaveLength(0);
    expect(screen.queryByText(CONNECTED)).toBeNull();
  });

  it("a Keychain failure after a good test is its own error", async () => {
    backend.failing.set("set_api_key", "The keychain refused the write.");
    await open("speech");
    fireEvent.change(keyBox(), { target: { value: KEY } });
    click("Test & Save");
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain(KEY_NOT_SAVED);
    expect(alert.textContent).not.toContain(KEY);
    expect(screen.queryByText(CONNECTED)).toBeNull();
  });

  it("an empty or malformed key is refused before anything is sent", async () => {
    await open("speech");
    click("Test & Save");
    expect((await screen.findByRole("alert")).textContent).toBe("Paste your API key.");
    fireEvent.change(keyBox(), { target: { value: "has spaces in it" } });
    click("Test & Save");
    await waitFor(() => {
      expect(screen.getByRole("alert").textContent).not.toBe("Paste your API key.");
    });
    expect(backend.named("test_api_key")).toHaveLength(0);
  });

  it("Remove key deletes the saved key", async () => {
    await openWithKey();
    click("Remove key");
    expect(await screen.findByText("Key removed from your Keychain.")).toBeTruthy();
    expect(backend.named("delete_api_key")[0]?.args).toEqual({ provider: "groq" });
    await waitFor(() => {
      expect(screen.queryByRole("button", { name: "Remove key" })).toBeNull();
    });
  });

  it("the how-to links to the provider's key page through the app", async () => {
    await open("speech");
    click("console.groq.com");
    await waitFor(() => {
      expect(backend.named("open_link")[0]?.args).toEqual({ link: "groq_keys" });
    });
  });

  it("asks the app only whether a key exists, never for the key", async () => {
    await openWithKey();
    expect(keyBox().value).toBe("");
    expect(backend.calls.every((call) => !("key" in call.args) || call.command === "test_api_key"));
  });
});

describe("criterion_u6_6 Test microphone", () => {
  it("shows the live meter and says it works when it hears sound", async () => {
    backend.micLevels = [-80, -20, -25];
    await open("dictation");
    click("Test microphone");
    await waitFor(() => {
      expect(screen.getAllByTestId("mic-bar").length).toBeGreaterThan(0);
    });
    expect(await screen.findByText("Hearing you.")).toBeTruthy();
    expect(screen.getByText("MacBook Pro Microphone")).toBeTruthy();
  });

  it("says No sound after 5 s of silence, and stops the microphone", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    backend.micLevels = [-90];
    await open("dictation");
    click("Test microphone");
    await screen.findByText("Listening… say something.");
    await act(async () => {
      await vi.advanceTimersByTimeAsync(5_200);
    });
    expect((await screen.findByRole("alert")).textContent).toContain("No sound");
    expect(backend.named("stop_microphone_test").length).toBeGreaterThan(0);
  });

  it("says why when the microphone can't start", async () => {
    backend.failing.set("start_microphone_test", "Vocet isn't allowed to use the microphone.");
    await open("dictation");
    click("Test microphone");
    expect((await screen.findByRole("alert")).textContent).toContain("isn't allowed");
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
  const KEYCHAIN_NOTICE =
    "After this update, macOS may ask once whether Vocet can use the key saved in your Keychain. Choose Always Allow. Your key stays in your Keychain and is sent only to your speech provider (Groq or OpenAI).";

  it("U1: says what to expect from the Keychain on the first launch after the rename", async () => {
    backend.launchNotice = KEYCHAIN_NOTICE;
    backend.keys.groq = true;
    await open("speech");
    const notice = await screen.findByTestId("launch-notice");
    expect(notice.textContent).toContain("Choose Always Allow");
    expect(notice.textContent).toContain("sent only to your speech provider (Groq or OpenAI)");
  });

  // U12 criterion 11.
  it("the Keychain notice shows only in Speech, and isn't styled as a success", async () => {
    backend.launchNotice = KEYCHAIN_NOTICE;
    backend.keys.groq = true;
    await open("speech");
    const notice = await screen.findByTestId("launch-notice");
    expect(notice.className).not.toMatch(/\bok\b/);
    for (const name of ["General", "Dictation", "Privacy", "History", "Help", "About"]) {
      fireEvent.click(screen.getByRole("tab", { name }));
      await screen.findByRole("heading", { level: 1, name });
      expect(screen.queryByTestId("launch-notice"), name).toBeNull();
    }
  });

  it("the Keychain notice doesn't show with no key saved", async () => {
    backend.launchNotice = KEYCHAIN_NOTICE;
    await open("speech");
    await screen.findByText("! Needs a key");
    expect(screen.queryByTestId("launch-notice")).toBeNull();
  });

  it("Got it hides the Keychain notice for good", async () => {
    backend.launchNotice = KEYCHAIN_NOTICE;
    backend.keys.groq = true;
    await open("speech");
    await screen.findByTestId("launch-notice");
    click("Got it");
    expect(screen.queryByTestId("launch-notice")).toBeNull();
    expect(window.localStorage.getItem(NOTICE_DISMISSED_KEY)).toBe("1");
    cleanup();
    await open("speech");
    await screen.findByRole("button", { name: "Change key…" });
    expect(screen.queryByTestId("launch-notice")).toBeNull();
    window.localStorage.removeItem(NOTICE_DISMISSED_KEY);
  });

  it("U1: says nothing about the Keychain on any other launch", async () => {
    await open();
    expect(screen.queryByTestId("launch-notice")).toBeNull();
  });

  it("shows why the settings couldn't load", async () => {
    backend.failing.set("get_settings", "The settings file is damaged.");
    render(<Settings />);
    expect((await screen.findByRole("alert")).textContent).toContain("damaged");
  });
});
