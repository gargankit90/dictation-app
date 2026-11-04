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
import { CONNECTED, KEY_NOT_SAVED } from "./settings/Speech";
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
  };
  keys: Record<Provider, boolean> = { groq: false, openai: false };
  models: LocalModelInfo_Serialize[] = [model(ENGLISH), model(MULTI)];
  engine: EngineStatus = { state: "ready", message: null };
  /** What `test_api_key` answers: `null` for success, or an error to throw. */
  testResult: KeyTestError | null = null;
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
    expect(tabs).toEqual(["General", "Dictation", "Speech", "Privacy", "Help", "About"]);
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
    // The own-key panel is open while the own key is the engine.
    expect(screen.getByRole("button", { name: "Hide" })).toBeTruthy();
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

  it("shortcut: recording a key saves it at once and says Saved", async () => {
    await open("dictation");
    expect(screen.getByLabelText("Dictation key").textContent).toBe("Right ⌥");
    click("Record a different key");
    keyDown("MetaRight");
    keyUp("MetaRight");
    expect((await screen.findByRole("status")).textContent).toBe("Saved");
    expect(backend.settings.hotkey).toBe("RightCmd");
    expect(screen.getByLabelText("Dictation key").textContent).toBe("Right ⌘");
  });

  it("shortcut: a key the app can't switch to keeps the old one and says why", async () => {
    await open("dictation");
    backend.failing.set("set_hotkey", "Couldn't switch the hotkey to F6. RightAlt still works.");
    click("Record a different key");
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
    click("Record a different key");
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

describe("criterion_u6_4 Speech: exactly one Active engine", () => {
  const actives = () => screen.queryAllByText("✓ Active");
  // The own key is Active only with a key saved (Design Lead, round 1).
  beforeEach(() => {
    backend.keys.groq = true;
  });

  it("the own key without a saved key says Needs a key until Test & Save works", async () => {
    backend.keys.groq = false;
    await open("speech");
    expect(await screen.findByText("! Needs a key")).toBeTruthy();
    expect(actives()).toHaveLength(0);
    fireEvent.change(screen.getByLabelText(/API key/), { target: { value: KEY } });
    click("Test & Save");
    expect(await screen.findByText("✓ Active")).toBeTruthy();
    expect(screen.queryByText("! Needs a key")).toBeNull();
  });

  // Tester run 1 (minor): after Cancel the part is kept, and Resume continues it.
  it("a cancelled download says it is paused and Resume continues it", async () => {
    backend.models = [model(ENGLISH, { state: "partial", downloaded_bytes: 212_000_000 })];
    await open("speech");
    expect(await screen.findByText("Download paused, 212 of 477 MB")).toBeTruthy();
    click("Resume");
    expect(await screen.findByText("↓ 212 of 477 MB")).toBeTruthy();
    expect(backend.named("download_local_model")[0]?.args).toEqual({ id: ENGLISH });
  });

  it("titles are English and Multilingual, with honest captions", async () => {
    await open("speech");
    expect(await screen.findByText("Multilingual")).toBeTruthy();
    expect(
      screen.getByText(/3 languages, including English and Hindi · works offline/),
    ).toBeTruthy();
    expect(screen.queryByText(/nothing is sent/)).toBeNull();
  });

  it("the own key is Active with the cloud; models show their size and Download", async () => {
    await open("speech");
    await screen.findByText("English");
    expect(actives()).toHaveLength(1);
    expect(screen.getByText("477 MB")).toBeTruthy();
    expect(screen.getAllByRole("button", { name: "Download" })).toHaveLength(2);
  });

  it("Download starts at once and shows progress with Cancel", async () => {
    await open("speech");
    await screen.findByText("English");
    const [download] = screen.getAllByRole("button", { name: "Download" });
    if (download === undefined) throw new Error("no Download button");
    fireEvent.click(download);
    expect(await screen.findByText("↓ 212 of 477 MB")).toBeTruthy();
    expect(screen.getByRole("progressbar", { name: "English download" })).toBeTruthy();
    click("Cancel");
    await waitFor(() => {
      expect(backend.named("cancel_local_model_download")[0]?.args).toEqual({ id: ENGLISH });
    });
  });

  it("Use this switches only once the model is ready; until then the own key stays Active", async () => {
    backend.models = [model(ENGLISH, { state: "ready", downloaded_bytes: 477_274_496 })];
    const loading = gate();
    backend.holds.set("use_local_model", loading.closed);
    await open("speech");
    click("Use this");
    expect(await screen.findByText("Loading…")).toBeTruthy();
    // The own-key row is still the one in use.
    expect(actives()).toHaveLength(1);
    expect(screen.queryByRole("button", { name: "Use this" })).toBeNull();
    loading.open();
    await waitFor(() => {
      expect(screen.queryByText("Loading…")).toBeNull();
    });
    expect(actives()).toHaveLength(1);
    // Now the model is Active and the own key offers Use this.
    expect(screen.getByRole("button", { name: "Use this" }).getAttribute("aria-describedby")).toBe(
      "own-key-label",
    );
    expect(backend.settings.transcription).toBe("local");
  });

  it("a model that fails to load says Couldn't load, with Retry and Delete", async () => {
    backend.models = [model(ENGLISH, { state: "ready" })];
    backend.failing.set("use_local_model", "Couldn't load Parakeet English: the file is damaged.");
    await open("speech");
    click("Use this");
    expect(await screen.findByText("! Couldn't load")).toBeTruthy();
    expect(screen.getByRole("alert").textContent).toContain("file is damaged");
    expect(screen.getByRole("button", { name: "Retry" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Delete" })).toBeTruthy();
    expect(actives()).toHaveLength(1);
  });

  it("the active model has no Delete; a downloaded one has Delete in its … menu", async () => {
    backend.settings = { ...backend.settings, transcription: "local", local_model: ENGLISH };
    backend.models = [model(ENGLISH, { state: "ready" }), model(MULTI, { state: "ready" })];
    await open("speech");
    await screen.findByText("English");
    expect(actives()).toHaveLength(1);
    expect(screen.queryByRole("button", { name: "More actions for English" })).toBeNull();
    click("More actions for Multilingual");
    fireEvent.click(screen.getByRole("menuitem", { name: "Delete Multilingual" }));
    await waitFor(() => {
      expect(backend.named("delete_local_model")[0]?.args).toEqual({ id: MULTI });
    });
  });

  it("an active model the engine couldn't load says so", async () => {
    backend.settings = { ...backend.settings, transcription: "local", local_model: ENGLISH };
    backend.models = [model(ENGLISH, { state: "ready" })];
    backend.engine = { state: "failed", message: "The on-device model couldn't be loaded." };
    await open("speech");
    expect(await screen.findByText("! Couldn't load")).toBeTruthy();
    expect(actives()).toHaveLength(0);
    expect(screen.getByRole("button", { name: "Use this" })).toBeTruthy();
  });

  it("choosing the own key again switches at once", async () => {
    backend.settings = { ...backend.settings, transcription: "local", local_model: ENGLISH };
    backend.models = [model(ENGLISH, { state: "ready" })];
    await open("speech");
    await screen.findByText("English");
    click("Use this");
    await waitFor(() => {
      expect(backend.named("use_cloud_engine")[0]?.args).toEqual({ provider: "groq" });
    });
    await waitFor(() => {
      expect(actives()).toHaveLength(1);
    });
  });

  it("while the own key is Active, changing the provider applies at once", async () => {
    await open("speech");
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
    backend.keys.groq = true;
    await open("speech");
    await screen.findByText("A key is saved in your Keychain.");
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
    expect(screen.getByText("A key is saved in your Keychain.")).toBeTruthy();
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
    backend.keys.groq = true;
    await open("speech");
    click("Remove key");
    expect(await screen.findByText("Key removed from your Keychain.")).toBeTruthy();
    expect(backend.named("delete_api_key")[0]?.args).toEqual({ provider: "groq" });
    expect(await screen.findByText("No key saved yet.", { exact: false })).toBeTruthy();
  });

  it("the how-to links to the provider's key page through the app", async () => {
    await open("speech");
    click("console.groq.com");
    await waitFor(() => {
      expect(backend.named("open_link")[0]?.args).toEqual({ link: "groq_keys" });
    });
  });

  it("asks the app only whether a key exists, never for the key", async () => {
    backend.keys.groq = true;
    await open("speech");
    await screen.findByText("A key is saved in your Keychain.");
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
    click("Troubleshoot…");
    click("Report a problem…");
    click("Show log file");
    click("Run setup again");
    expect(onTroubleshoot).toHaveBeenCalledOnce();
    expect(onReport).toHaveBeenCalledOnce();
    expect(onShowLog).toHaveBeenCalledOnce();
    expect(onSetup).toHaveBeenCalledOnce();
  });

  it("says so when the log can't be shown", async () => {
    await open("help", { onShowLog: () => Promise.resolve("Couldn't open the log folder.") });
    click("Show log file");
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
  it("shows why the settings couldn't load", async () => {
    backend.failing.set("get_settings", "The settings file is damaged.");
    render(<Settings />);
    expect((await screen.findByRole("alert")).textContent).toContain("damaged");
  });
});
