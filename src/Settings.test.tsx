// M3.4 criteria 1-3: the settings form, with the app's commands replaced by
// a fake backend through `mockIPC`. The keys here are made up.

import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { KeyTestError, Provider, Settings_Serialize } from "./bindings";
import { Settings } from "./Settings";
import { keyDown, keyUp, releaseAllKeys } from "./testKeys";

const KEY = "gsk_dummy_NOT_A_REAL_KEY_0123456789";
const OTHER_KEY = "gsk_dummy_ANOTHER_FAKE_KEY_9876543210";

interface Call {
  command: string;
  args: Record<string, unknown>;
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
    local_model: "parakeet-unified-en-0.6b-q4_k_m",
    share_usage_events: true,
  };
  keys: Record<Provider, boolean> = { groq: false, openai: false };
  /** What `test_api_key` answers: `null` for success, or an error to throw. */
  testResult: KeyTestError | Error | null = null;
  /** Fails the command named, with the message (a string, as the app sends). */
  failing = new Map<string, string>();
  calls: Call[] = [];
  /** Holds `test_api_key` until called, to test a reply that arrives late. */
  holdTest: Promise<void> | null = null;
  /** Holds `set_share_usage_events` until released. */
  holdShare: Promise<void> | null = null;
  /** Holds `set_launch_at_login` until released: the OS being slow (#238). */
  holdLaunch: Promise<void> | null = null;
  /** Holds `set_hotkey` until released, to test the form while a key is saved. */
  holdSetHotkey: Promise<void> | null = null;
  /** Holds `set_api_key` until released, to test the form while a save runs. */
  holdSetKey: Promise<void> | null = null;
  /**
   * Holds the next `has_api_key` reply for a provider. The answer is decided
   * when the call arrives, but delivered only when the promise settles.
   */
  holdKeyCheck = new Map<Provider, Promise<void>>();

  handle = async (command: string, payload: unknown): Promise<unknown> => {
    const args = (payload ?? {}) as Record<string, unknown>;
    this.calls.push({ command, args });
    const failure = this.failing.get(command);
    // Tauri rejects with the command's `Err` value, which is not an Error.
    // eslint-disable-next-line @typescript-eslint/only-throw-error
    if (failure !== undefined) throw failure;
    switch (command) {
      case "get_settings":
        return this.settings;
      case "save_settings":
        // Like the app: Save never changes the sharing choice.
        // And never the hotkey, which only `set_hotkey` changes.
        this.settings = {
          ...(args.settings as Settings_Serialize),
          share_usage_events: this.settings.share_usage_events,
          hotkey: this.settings.hotkey,
          // And never the login item, which only `set_launch_at_login` changes.
          launch_at_login: this.settings.launch_at_login,
        };
        return null;
      case "set_hotkey":
        if (this.holdSetHotkey) await this.holdSetHotkey;
        this.settings = { ...this.settings, hotkey: args.spec as string };
        return null;
      case "set_hotkey_recording":
        return null;
      case "set_launch_at_login":
        if (this.holdLaunch) await this.holdLaunch;
        this.settings = { ...this.settings, launch_at_login: args.enabled as boolean };
        return args.enabled;
      case "set_share_usage_events":
        this.settings = { ...this.settings, share_usage_events: args.share as boolean };
        // The write is done; the reply is what waits.
        if (this.holdShare) await this.holdShare;
        return null;
      case "set_api_key":
        if (this.holdSetKey) await this.holdSetKey;
        this.keys[args.provider as Provider] = true;
        return null;
      case "has_api_key": {
        const provider = args.provider as Provider;
        const answer = this.keys[provider];
        const held = this.holdKeyCheck.get(provider);
        this.holdKeyCheck.delete(provider);
        if (held) await held;
        return answer;
      }
      case "test_api_key":
        if (this.holdTest) await this.holdTest;
        // eslint-disable-next-line @typescript-eslint/only-throw-error
        if (this.testResult !== null) throw this.testResult;
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
  mockIPC((command, payload) => backend.handle(command, payload));
});

afterEach(() => {
  releaseAllKeys();
  cleanup();
  clearMocks();
});

/** Renders the form once the saved settings have loaded. */
async function open() {
  render(<Settings />);
  await screen.findByLabelText("Or type it");
  await waitFor(() => {
    expect(screen.queryByText("Checking for a saved key…")).toBeNull();
  });
}

const keyBox = () => screen.getByLabelText<HTMLInputElement>(/API key/);
const type = (label: string | RegExp, value: string) => {
  fireEvent.change(screen.getByLabelText(label), { target: { value } });
};
const click = (name: string) => {
  fireEvent.click(screen.getByRole("button", { name }));
};

/** A promise and the function that settles it. */
function gate() {
  let open: () => void = () => undefined;
  const closed = new Promise<void>((resolve) => {
    open = resolve;
  });
  return { closed, open };
}

/** Lets pending promise callbacks (the held replies) run. */
async function flush() {
  for (let round = 0; round < 5; round += 1) {
    await act(async () => {
      await Promise.resolve();
    });
  }
}

/** Disabled itself or through a disabled fieldset (jsdom's `.disabled` ignores the latter). */
const locked = (element: HTMLElement) => element.matches(":disabled");

const checks = (provider: Provider) =>
  backend.named("has_api_key").filter((call) => call.args.provider === provider).length;

describe("loading", () => {
  it("shows the saved settings", async () => {
    backend.settings = { ...backend.settings, provider: "openai", language: "de", hotkey: "F5" };
    await open();

    expect(screen.getByLabelText<HTMLSelectElement>("Provider").value).toBe("openai");
    expect(screen.getByLabelText<HTMLInputElement>("Or type it").value).toBe("F5");
    expect(screen.getByLabelText<HTMLInputElement>("Language").value).toBe("de");
    expect(screen.getByLabelText(/OpenAI API key/)).toBeTruthy();
  });

  it("shows the defaults without a language as 'detect'", async () => {
    await open();

    expect(screen.getByLabelText<HTMLInputElement>("Language").value).toBe("");
    expect(screen.getByLabelText<HTMLInputElement>("Language").placeholder).toBe(
      "Detect automatically",
    );
  });

  it("says Key saved only when the app says a key is stored", async () => {
    await open();
    expect(screen.queryByText("Key saved")).toBeNull();
    expect(screen.getByText(/No key saved yet/)).toBeTruthy();
    cleanup();

    backend.keys.groq = true;
    await open();
    expect(screen.getByText("Key saved")).toBeTruthy();
    expect(backend.named("has_api_key").at(-1)?.args).toEqual({ provider: "groq" });
  });

  it("shows why the settings couldn't load", async () => {
    backend.failing.set("get_settings", "the settings file is malformed: bad");
    render(<Settings />);

    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("the settings file is malformed: bad");
    expect(screen.queryByLabelText("Or type it")).toBeNull();
  });

  it("copes with the IPC itself failing", async () => {
    mockIPC(() => {
      throw new Error("ipc is down");
    });
    render(<Settings />);

    expect((await screen.findByRole("alert")).textContent).toContain("Something went wrong");
  });
});

describe("criterion 1: the form validates input", () => {
  it("refuses a bad language, hotkey and key without calling the app", async () => {
    await open();
    type("Language", "english");
    type("Or type it", "Ctrl+");
    type(/API key/, "two words");
    click("Save");

    expect((await screen.findAllByRole("alert")).map((alert) => alert.textContent)).toEqual([
      "A key has no spaces; check what you pasted.",
      "Write the hotkey as keys joined by +, such as Ctrl+Shift+Space.",
      "Use a two or three letter language code such as en, or leave it empty to detect.",
    ]);
    expect(backend.named("set_api_key")).toEqual([]);
    expect(backend.named("save_settings")).toEqual([]);
    expect(screen.getByLabelText("Or type it").getAttribute("aria-invalid")).toBe("true");
  });

  it("refuses an empty hotkey, and clears an error once the field is edited", async () => {
    await open();
    type("Or type it", "  ");
    click("Save");
    expect((await screen.findByRole("alert")).textContent).toMatch(/Enter a hotkey/);

    type("Or type it", "RightAlt");
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("Test key needs a key: typed, or saved", async () => {
    await open();
    click("Test key");

    expect((await screen.findByRole("alert")).textContent).toBe("Paste your API key.");
    expect(backend.named("test_api_key")).toEqual([]);
  });

  it("Test key refuses a typed key that can't be a key, without sending it", async () => {
    await open();
    type(/API key/, "has a space");
    click("Test key");

    expect((await screen.findByRole("alert")).textContent).toMatch(/no spaces/);
    expect(backend.named("test_api_key")).toEqual([]);
  });

  it("the key box is a password field that browsers don't autofill", async () => {
    await open();

    expect(keyBox().type).toBe("password");
    expect(keyBox().autocomplete).toBe("off");
    expect(keyBox().value).toBe("");
  });

  it("an empty key is fine when saving the other settings", async () => {
    await open();
    type("Language", "fr");
    click("Save");

    expect((await screen.findByRole("status")).textContent).toBe("Settings saved.");
    expect(backend.named("set_api_key")).toEqual([]);
  });
});

describe("criterion 1: Test key shows success and each failure", () => {
  it("success: tests the typed key without saving it", async () => {
    await open();
    type(/API key/, `  ${KEY} `);
    click("Test key");

    expect((await screen.findByRole("status")).textContent).toBe(
      "The key works. Groq accepted it.",
    );
    const [test] = backend.named("test_api_key");
    expect(test?.args).toEqual({ provider: "groq", key: `  ${KEY} ` });
    expect(backend.named("set_api_key")).toEqual([]);
    expect(backend.keys.groq).toBe(false);
  });

  it("with the box empty, tests the saved key (no key sent)", async () => {
    backend.keys.groq = true;
    await open();
    click("Test key");

    await screen.findByRole("status");
    expect(backend.named("test_api_key")[0]?.args).toEqual({ provider: "groq", key: null });
  });

  it("names the provider that's selected", async () => {
    await open();
    type("Provider", "openai");
    await waitFor(() => {
      expect(screen.getByLabelText(/OpenAI API key/)).toBeTruthy();
    });
    type(/API key/, KEY);
    click("Test key");

    expect((await screen.findByRole("status")).textContent).toBe(
      "The key works. OpenAI accepted it.",
    );
    expect(backend.named("test_api_key")[0]?.args.provider).toBe("openai");
  });

  const failures: KeyTestError[] = [
    {
      kind: "invalid_key",
      message: "Groq rejected this API key. Check that you copied the whole key from Groq.",
    },
    {
      kind: "rate_limited",
      message: "Groq is limiting requests right now. The key may be fine; try again in a minute.",
    },
    {
      kind: "network",
      message: "Couldn't reach Groq. Check your internet connection and try again.",
    },
    {
      kind: "server",
      message:
        "Groq had a problem, so the key couldn't be checked (the provider failed (HTTP 503)). Try again later.",
    },
    { kind: "no_key", message: "No Groq API key is saved yet. Paste one first." },
    { kind: "keychain", message: "Couldn't read the saved key: keychain access was denied." },
  ];
  for (const failure of failures) {
    it(`failure ${failure.kind}: shows the app's message as an alert`, async () => {
      backend.testResult = failure;
      await open();
      type(/API key/, KEY);
      click("Test key");

      const alert = await screen.findByRole("alert");
      expect(alert.textContent).toBe(failure.message);
      expect(screen.queryByRole("status")).toBeNull();
      // The button is usable again, and the typed key is still there to fix.
      expect(screen.getByRole<HTMLButtonElement>("button", { name: "Test key" }).disabled).toBe(
        false,
      );
      expect(keyBox().value).toBe(KEY);
    });
  }

  it("a broken IPC is a generic message, not a crash", async () => {
    backend.testResult = new Error("ipc is down");
    await open();
    type(/API key/, KEY);
    click("Test key");

    expect((await screen.findByRole("alert")).textContent).toContain("Something went wrong");
  });

  it("shows Testing… and disables the buttons while waiting", async () => {
    let release: () => void = () => undefined;
    backend.holdTest = new Promise((resolve) => {
      release = resolve;
    });
    await open();
    type(/API key/, KEY);
    click("Test key");

    const testing = await screen.findByRole("button", { name: "Testing…" });
    expect((testing as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByRole<HTMLButtonElement>("button", { name: "Save" }).disabled).toBe(true);
    release();
    await screen.findByRole("status");
  });

  it("drops a reply that arrives after the key was edited", async () => {
    let release: () => void = () => undefined;
    backend.holdTest = new Promise((resolve) => {
      release = resolve;
    });
    await open();
    type(/API key/, KEY);
    click("Test key");
    await screen.findByRole("button", { name: "Testing…" });
    type(/API key/, OTHER_KEY);
    release();

    await waitFor(() => {
      expect(screen.getByRole<HTMLButtonElement>("button", { name: "Test key" }).disabled).toBe(
        false,
      );
    });
    await Promise.resolve();
    expect(screen.queryByRole("status")).toBeNull();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("clears an old result when the key or provider changes", async () => {
    await open();
    type(/API key/, KEY);
    click("Test key");
    await screen.findByRole("status");

    type(/API key/, OTHER_KEY);
    expect(screen.queryByRole("status")).toBeNull();
  });
});

describe("criterion 2: settings save and reload", () => {
  it("saves the form: the key first, then the settings, then reloads", async () => {
    await open();
    type("Provider", "openai");
    await waitFor(() => {
      expect(screen.getByLabelText(/OpenAI API key/)).toBeTruthy();
    });
    type(/API key/, ` ${KEY} `);
    type("Or type it", " Ctrl+Shift+Space ");
    type("Language", " DE ");
    click("Save");

    expect((await screen.findByRole("status")).textContent).toBe("Settings saved.");
    const order = backend.calls
      .map((call) => call.command)
      .filter((command) => ["set_api_key", "save_settings", "set_hotkey"].includes(command));
    // The typed dictation key goes through its own writer, after the rest.
    expect(order).toEqual(["set_api_key", "save_settings", "set_hotkey"]);
    expect(backend.named("set_hotkey")[0]?.args).toEqual({ spec: "Ctrl+Shift+Space" });
    expect(backend.named("set_api_key")[0]?.args).toEqual({ provider: "openai", key: ` ${KEY} ` });
    expect(backend.named("save_settings")[0]?.args).toEqual({
      settings: {
        version: 2,
        provider: "openai",
        model: "whisper-1",
        language: "de",
        hotkey: "RightAlt",
        launch_at_login: false,
        transcription: "cloud",
        local_model: "parakeet-unified-en-0.6b-q4_k_m",
        share_usage_events: true,
      },
    });
    expect(screen.getByText("Key saved")).toBeTruthy();
  });

  it("a fresh window shows what was saved", async () => {
    await open();
    type("Provider", "openai");
    type("Or type it", "F6");
    type("Language", "es");
    click("Save");
    await screen.findByText("Settings saved.");
    cleanup();

    await open();

    expect(screen.getByLabelText<HTMLSelectElement>("Provider").value).toBe("openai");
    expect(screen.getByLabelText<HTMLInputElement>("Or type it").value).toBe("F6");
    expect(screen.getByLabelText<HTMLInputElement>("Language").value).toBe("es");
  });

  it("clearing the language saves 'detect' (null)", async () => {
    backend.settings = { ...backend.settings, language: "en" };
    await open();
    type("Language", "");
    click("Save");
    await screen.findByText("Settings saved.");

    expect(backend.settings.language).toBeNull();
  });

  it("switching provider resets the model to that provider's default", async () => {
    backend.settings = { ...backend.settings, model: "some-custom-model" };
    await open();
    type("Provider", "openai");
    click("Save");
    await screen.findByText("Settings saved.");
    expect(backend.settings.model).toBe("whisper-1");

    type("Provider", "groq");
    click("Save");
    await waitFor(() => {
      expect(backend.settings.model).toBe("whisper-large-v3-turbo");
    });
  });

  it("shows the form after the app writes back the canonical hotkey", async () => {
    // The app may normalise what was typed (`ctrl+shift+space` → `Ctrl+Shift+Space`).
    const handle = backend.handle;
    mockIPC(async (command, payload) => {
      const result = await handle(command, payload);
      if (command === "set_hotkey") {
        backend.settings = { ...backend.settings, hotkey: "Ctrl+Shift+Space" };
      }
      return result;
    });
    await open();
    type("Or type it", "ctrl+shift+space");
    click("Save");
    await screen.findByText("Settings saved.");

    expect(screen.getByLabelText<HTMLInputElement>("Or type it").value).toBe("Ctrl+Shift+Space");
  });

  it("records the dictation key by pressing it, and saves it at once", async () => {
    await open();
    expect(screen.getByLabelText("Dictation key").textContent).toBe("Right ⌥");
    click("Record a different key");
    keyDown("MetaRight");
    keyUp("MetaRight");
    await screen.findByText("Dictation key saved.");
    expect(backend.settings.hotkey).toBe("RightCmd");
    expect(screen.getByLabelText("Dictation key").textContent).toBe("Right ⌘");
    expect(screen.getByLabelText<HTMLInputElement>("Or type it").value).toBe("RightCmd");
  });

  it("keeps the old key and says why when the app can't switch to the recorded one", async () => {
    await open();
    click("Record a different key");
    backend.failing.set("set_hotkey", "Couldn't switch the hotkey to F6. RightAlt still works.");
    keyDown("F6");
    expect((await screen.findByRole("alert")).textContent).toContain("RightAlt still works");
    expect(backend.settings.hotkey).toBe("RightAlt");
    expect(screen.getByLabelText("Dictation key").textContent).toBe("Right ⌥");
  });

  it("pauses the real hotkey while recording, and not a moment longer", async () => {
    await open();
    const flags = () =>
      backend.named("set_hotkey_recording").map((call) => call.args.recording as boolean);
    click("Record a different key");
    await waitFor(() => {
      expect(flags()).toEqual([true]);
    });
    keyDown("Escape");
    await waitFor(() => {
      expect(flags()).toEqual([true, false]);
    });
    // Choosing a key ends it too (the app also resumes when it sets the key).
    click("Record a different key");
    keyDown("F6");
    await screen.findByText("Dictation key saved.");
    expect(flags().at(-1)).toBe(false);
  });

  it("locks the form while a recorded key is being saved, and keeps both changes", async () => {
    await open();
    const hold = gate();
    backend.holdSetHotkey = hold.closed;
    type("Language", "de");
    click("Record a different key");
    keyDown("F6");
    await waitFor(() => {
      expect(backend.named("set_hotkey")).toHaveLength(1);
    });
    // The key isn't stored yet: Save can't start and take the old key back.
    expect(locked(screen.getByRole("button", { name: "Saving…" }))).toBe(true);
    expect(locked(screen.getByRole("button", { name: "Record a different key" }))).toBe(true);
    fireEvent.submit(
      screen.getByRole("button", { name: "Saving…" }).closest("form") as HTMLElement,
    );
    await flush();
    expect(backend.named("save_settings")).toHaveLength(0);

    hold.open();
    await screen.findByText("Dictation key saved.");
    expect(backend.settings.hotkey).toBe("F6");
    // Now Save works, and saves the language without touching the key.
    click("Save");
    await screen.findByText("Settings saved.");
    expect(backend.settings.language).toBe("de");
    expect(backend.settings.hotkey).toBe("F6");
    expect(backend.named("set_hotkey")).toHaveLength(1);
  });

  it("can't save the form while the recorder is listening, and the pause stays", async () => {
    await open();
    type("Or type it", "F6");
    click("Record a different key");
    await waitFor(() => {
      expect(backend.named("set_hotkey_recording")).toHaveLength(1);
    });
    const save = screen.getByRole<HTMLButtonElement>("button", { name: "Save" });
    expect(locked(save)).toBe(true);
    fireEvent.click(save);
    fireEvent.submit(save.closest("form") as HTMLElement);
    await flush();
    // Nothing was saved, so nothing could have made the real F6 live.
    expect(backend.named("save_settings")).toHaveLength(0);
    expect(backend.named("set_hotkey")).toHaveLength(0);
    expect(backend.named("set_hotkey_recording").map((c) => c.args.recording)).toEqual([true]);

    // Once the recorder is cancelled the pause ends, and Save works.
    keyDown("Escape");
    await waitFor(() => {
      expect(backend.named("set_hotkey_recording").map((c) => c.args.recording)).toEqual([
        true,
        false,
      ]);
    });
    expect(locked(screen.getByRole("button", { name: "Save" }))).toBe(false);
    click("Save");
    await screen.findByText("Settings saved.");
    expect(backend.settings.hotkey).toBe("F6");
  });

  it("can't start recording a key while a Save runs", async () => {
    await open();
    const hold = gate();
    backend.holdSetKey = hold.closed;
    type(/API key/, KEY);
    click("Save");
    await waitFor(() => {
      expect(backend.named("set_api_key")).toHaveLength(1);
    });
    expect(locked(screen.getByRole("button", { name: "Record a different key" }))).toBe(true);
    expect(locked(screen.getByRole("button", { name: "Reset to default" }))).toBe(true);
    hold.open();
    await screen.findByText("Settings saved.");
    expect(backend.named("set_hotkey")).toHaveLength(0);
  });

  it("a Save made after the key was recorded doesn't take the old key back", async () => {
    // The form was loaded with RightAlt; the key is recorded; Save sends the
    // loaded RightAlt again, and the app (one writer of the key) ignores it.
    await open();
    click("Record a different key");
    keyDown("MetaRight");
    keyUp("MetaRight");
    await screen.findByText("Dictation key saved.");
    type("Language", "fr");
    click("Save");
    await screen.findByText("Settings saved.");
    expect(backend.settings.hotkey).toBe("RightCmd");
    expect(backend.settings.language).toBe("fr");
    expect(backend.named("set_hotkey")).toHaveLength(1);
    expect(screen.getByLabelText("Dictation key").textContent).toBe("Right ⌘");
  });

  it("resets the dictation key to Right Option", async () => {
    backend.settings = { ...backend.settings, hotkey: "Ctrl+Shift+Space" };
    await open();
    expect(screen.getByLabelText("Dictation key").textContent).toBe("⌃⇧Space");
    click("Reset to default");
    await screen.findByText("Dictation key saved.");
    expect(backend.settings.hotkey).toBe("RightAlt");
  });

  it("shows the app's message when saving the settings fails", async () => {
    backend.failing.set("save_settings", "invalid hotkey: Escape can't be a hotkey");
    await open();
    type("Or type it", "Escape");
    click("Save");

    expect((await screen.findByRole("alert")).textContent).toBe(
      "invalid hotkey: Escape can't be a hotkey",
    );
    expect(screen.queryByText("Settings saved.")).toBeNull();
  });

  it("when the key can't be stored, nothing else is saved", async () => {
    backend.failing.set("set_api_key", "keychain access was denied");
    await open();
    type(/API key/, KEY);
    type("Language", "fr");
    click("Save");

    expect((await screen.findByRole("alert")).textContent).toBe("keychain access was denied");
    expect(backend.named("save_settings")).toEqual([]);
    expect(keyBox().value).toBe(KEY); // still there to retry
  });
});

describe("criterion 3: the key is never rendered back", () => {
  it("clears the box after saving and shows only 'Key saved'", async () => {
    await open();
    type(/API key/, KEY);
    click("Save");
    await screen.findByText("Settings saved.");

    expect(keyBox().value).toBe("");
    expect(screen.getByText("Key saved")).toBeTruthy();
    expect(document.body.innerHTML).not.toContain(KEY);
    expect(document.body.innerHTML).not.toContain(KEY.slice(0, 12));
  });

  it("a fresh window never pre-fills the key, even with one saved", async () => {
    await open();
    type(/API key/, KEY);
    click("Save");
    await screen.findByText("Settings saved.");
    cleanup();

    await open();

    expect(keyBox().value).toBe("");
    expect(screen.getByText("Key saved")).toBeTruthy();
    expect(document.body.innerHTML).not.toContain(KEY);
  });

  it("asks the app only whether a key exists, never for the key", async () => {
    await open();
    type(/API key/, KEY);
    click("Save");
    await screen.findByText("Settings saved.");

    const used = new Set(backend.calls.map((call) => call.command));
    expect([...used].sort()).toEqual([
      "get_settings",
      "has_api_key",
      "save_settings",
      "set_api_key",
    ]);
    // The key travels once, in set_api_key; saving the settings doesn't carry it.
    const withKey = backend.calls.filter((call) => JSON.stringify(call.args).includes(KEY));
    expect(withKey.map((call) => call.command)).toEqual(["set_api_key"]);
  });

  it("an error from the app can't put the key on screen", async () => {
    backend.testResult = { kind: "invalid_key", message: "Groq rejected this API key." };
    await open();
    type(/API key/, KEY);
    click("Test key");
    await screen.findByRole("alert");

    // While typing, the key is only in the (password) box's value, not the page text.
    expect(document.body.textContent).not.toContain(KEY);
    expect(keyBox().type).toBe("password");
  });

  it("switching provider clears the typed key, so it isn't sent to the wrong service", async () => {
    await open();
    type(/API key/, KEY);
    type("Provider", "openai");

    await waitFor(() => {
      expect(screen.getByLabelText(/OpenAI API key/)).toBeTruthy();
    });
    expect(keyBox().value).toBe("");
    expect(within(document.body).queryByDisplayValue(KEY)).toBeNull();
  });
});

describe("review round 1", () => {
  describe("a settings-only save keeps the model", () => {
    it("saving another field leaves a non-default model alone", async () => {
      backend.settings = { ...backend.settings, model: "whisper-large-v3", language: null };
      await open();
      type("Language", "de");
      click("Save");
      await screen.findByText("Settings saved.");

      expect(backend.settings.model).toBe("whisper-large-v3");
      expect(backend.settings.language).toBe("de");
    });

    it("says that a provider change switches to the new default model", async () => {
      await open();
      expect(screen.queryByText(/default model/)).toBeNull();

      type("Provider", "openai");
      expect(
        screen.getByText(/Saving switches to OpenAI's default model, whisper-1\./),
      ).toBeTruthy();

      type("Provider", "groq");
      expect(screen.queryByText(/default model/)).toBeNull();
    });

    it("keeps the model on the next save after a provider change", async () => {
      await open();
      type("Provider", "openai");
      click("Save");
      await screen.findByText("Settings saved.");
      expect(backend.settings.model).toBe("whisper-1");
      expect(screen.queryByText(/default model/)).toBeNull();

      backend.settings = { ...backend.settings, model: "whisper-custom" };
      cleanup();
      await open();
      type("Language", "fr");
      click("Save");
      await screen.findByText("Settings saved.");
      expect(backend.settings.model).toBe("whisper-custom");
    });
  });

  describe("a key-status reply for a provider that was left is ignored", () => {
    it("a late 'no key' for OpenAI doesn't hide the saved Groq key", async () => {
      backend.keys = { groq: true, openai: false };
      await open();
      const openai = gate();
      backend.holdKeyCheck.set("openai", openai.closed);

      type("Provider", "openai");
      await waitFor(() => {
        expect(checks("openai")).toBe(1);
      });
      type("Provider", "groq");
      expect(await screen.findByText("Key saved")).toBeTruthy();
      openai.open();
      await flush();

      expect(screen.getByText("Key saved")).toBeTruthy();
      expect(screen.queryByText(/No key saved yet/)).toBeNull();
      click("Test key");
      await screen.findByRole("status");
      expect(backend.named("test_api_key")[0]?.args).toEqual({ provider: "groq", key: null });
    });

    it("a late 'saved' for OpenAI doesn't show a key for Groq", async () => {
      backend.keys = { groq: false, openai: true };
      await open();
      const openai = gate();
      backend.holdKeyCheck.set("openai", openai.closed);

      type("Provider", "openai");
      await waitFor(() => {
        expect(checks("openai")).toBe(1);
      });
      type("Provider", "groq");
      expect(await screen.findByText(/No key saved yet/)).toBeTruthy();
      openai.open();
      await flush();

      expect(screen.getByText(/No key saved yet/)).toBeTruthy();
      expect(screen.queryByText("Key saved")).toBeNull();
      click("Test key");
      expect((await screen.findByRole("alert")).textContent).toBe("Paste your API key.");
      expect(backend.named("test_api_key")).toEqual([]);
    });

    it("an old reply doesn't undo a key that was just saved", async () => {
      await open();
      const stale = gate();
      backend.holdKeyCheck.set("groq", stale.closed);
      type("Provider", "openai");
      type("Provider", "groq"); // asks again; this reply is held (and says 'no key')
      await waitFor(() => {
        expect(checks("groq")).toBe(2);
      });
      type(/API key/, KEY);
      click("Save");
      await screen.findByText("Settings saved.");
      stale.open();
      await flush();

      expect(screen.getByText("Key saved")).toBeTruthy();
    });
  });

  describe("the form is locked while saving", () => {
    it("edits during a save are refused, and what shows is what was saved", async () => {
      await open();
      type(/API key/, KEY);
      const hold = gate();
      backend.holdSetKey = hold.closed;
      click("Save");
      await screen.findByRole("button", { name: "Saving…" });

      expect(locked(screen.getByLabelText<HTMLSelectElement>("Provider"))).toBe(true);
      expect(locked(keyBox())).toBe(true);
      expect(locked(screen.getByLabelText<HTMLInputElement>("Or type it"))).toBe(true);
      expect(locked(screen.getByLabelText<HTMLInputElement>("Language"))).toBe(true);
      // Even an edit that gets through (a change event on a disabled control)
      // changes nothing.
      type("Provider", "openai");
      type(/API key/, OTHER_KEY);
      type("Language", "de");
      hold.open();
      await screen.findByText("Settings saved.");

      expect(screen.getByLabelText<HTMLSelectElement>("Provider").value).toBe("groq");
      expect(backend.settings.provider).toBe("groq");
      expect(backend.settings.language).toBeNull();
      expect(screen.getByLabelText<HTMLInputElement>("Language").value).toBe("");
      expect(screen.getByLabelText(/Groq API key/)).toBeTruthy();
      expect(screen.getByText("Key saved")).toBeTruthy();
      expect(backend.keys).toEqual({ groq: true, openai: false });
      expect(backend.named("set_api_key").map((call) => call.args.provider)).toEqual(["groq"]);
      expect(keyBox().value).toBe("");
      // Editable again afterwards.
      expect(locked(screen.getByLabelText("Provider"))).toBe(false);
    });

    // T3 (#113): the app gives up on a keychain that doesn't answer and
    // says so; the form unlocks, keeps the key for a retry and claims nothing.
    it("blocked_key_save_reenables_form_without_claiming_key_saved", async () => {
      const TIMED_OUT =
        "keychain error: no answer from the keychain after 15 s, so the key may not be saved";
      await open();
      expect(screen.getByText(/No key saved yet/)).toBeTruthy();
      const hold = gate();
      const handle = backend.handle;
      let stuck = true;
      mockIPC(async (command, payload) => {
        if (command === "set_api_key" && stuck) {
          backend.calls.push({ command, args: (payload ?? {}) as Record<string, unknown> });
          await hold.closed;
          // eslint-disable-next-line @typescript-eslint/only-throw-error
          throw TIMED_OUT;
        }
        return handle(command, payload);
      });
      type(/API key/, KEY);
      type("Language", "fr");
      click("Save");
      await screen.findByRole("button", { name: "Saving…" });
      expect(locked(keyBox())).toBe(true);

      hold.open();
      expect((await screen.findByRole("alert")).textContent).toBe(TIMED_OUT);
      expect(locked(keyBox())).toBe(false);
      expect(locked(screen.getByRole("button", { name: "Save" }))).toBe(false);
      expect(screen.queryByText("Key saved")).toBeNull();
      expect(screen.getByText(/No key saved yet/)).toBeTruthy();
      expect(backend.named("save_settings")).toEqual([]);
      expect(keyBox().value).toBe(KEY);

      // The keychain answers again: the same Save works.
      stuck = false;
      click("Save");
      await screen.findByText("Settings saved.");
      expect(screen.getByText("Key saved")).toBeTruthy();
      expect(backend.named("set_api_key")).toHaveLength(2);
      expect(backend.settings.language).toBe("fr");
    });

    it("shows the stored provider if the settings come back different", async () => {
      await open();
      type("Provider", "openai");
      const handle = backend.handle;
      mockIPC(async (command, payload) => {
        const result = await handle(command, payload);
        if (command === "save_settings") {
          backend.settings = { ...backend.settings, provider: "groq", model: "whisper-large-v3" };
        }
        return result;
      });
      click("Save");
      await screen.findByText("Settings saved.");

      expect(screen.getByLabelText<HTMLSelectElement>("Provider").value).toBe("groq");
    });
  });

  describe("a keychain lookup error is shown, not swallowed", () => {
    it("shows the app's message with a way to retry, not 'Checking…'", async () => {
      backend.failing.set("has_api_key", "keychain access was denied");
      await open();

      const alert = await screen.findByRole("alert");
      expect(alert.textContent).toBe("Couldn't check for a saved key: keychain access was denied");
      expect(screen.queryByText("Checking for a saved key…")).toBeNull();
      expect(screen.queryByText("Key saved")).toBeNull();
      expect(screen.queryByText(/No key saved yet/)).toBeNull();

      backend.failing.delete("has_api_key");
      backend.keys.groq = true;
      click("Retry");
      expect(await screen.findByText("Key saved")).toBeTruthy();
      expect(screen.queryByRole("button", { name: "Retry" })).toBeNull();
    });

    it("an IPC failure is a generic message", async () => {
      const handle = backend.handle;
      mockIPC((command, payload) => {
        if (command === "has_api_key") throw new Error("ipc is down");
        return handle(command, payload);
      });
      await open();

      expect((await screen.findByRole("alert")).textContent).toContain("Something went wrong");
      expect(screen.getByRole("button", { name: "Retry" })).toBeTruthy();
    });

    it("Test key still asks the app about the stored key, which explains itself", async () => {
      backend.failing.set("has_api_key", "keychain access was denied");
      backend.testResult = {
        kind: "keychain",
        message: "Couldn't read the saved key: keychain access was denied.",
      };
      await open();
      click("Test key");

      await waitFor(() => {
        expect(screen.getByRole<HTMLButtonElement>("button", { name: "Test key" }).disabled).toBe(
          false,
        );
        expect(backend.named("test_api_key")).toHaveLength(1);
      });
      expect(backend.named("test_api_key")[0]?.args).toEqual({ provider: "groq", key: null });
      expect(
        screen.getAllByRole("alert").some((alert) => alert.textContent.includes("Couldn't read")),
      ).toBe(true);
    });

    it("the error carries no key", async () => {
      backend.failing.set("has_api_key", "keychain access was denied");
      await open();
      await screen.findByRole("alert");

      expect(document.body.textContent).not.toContain("gsk_");
    });
  });
});

describe("launch at login", () => {
  const box = () => screen.getByLabelText<HTMLInputElement>(/^Launch at login/u);

  it("shows what the OS says, off by default", async () => {
    await open();
    expect(box().checked).toBe(false);
    cleanup();
    backend.settings = { ...backend.settings, launch_at_login: true };
    await open();
    expect(box().checked).toBe(true);
  });

  it("applies the choice at once, with Applying… until the OS answers, and Save is not involved", async () => {
    await open();
    const slow = gate();
    backend.holdLaunch = slow.closed;
    fireEvent.click(box());
    expect(box().checked).toBe(true);
    expect((await screen.findByRole("status")).textContent).toContain("Applying…");
    expect(backend.named("set_launch_at_login")[0]?.args.enabled).toBe(true);
    expect(backend.named("save_settings")).toHaveLength(0);
    // Saving the rest does not wait for the login item.
    type("Language", "de");
    click("Save");
    await screen.findByText("Settings saved.");
    expect(screen.getByText(/Applying…/u)).toBeTruthy();
    expect(backend.settings.language).toBe("de");
    slow.open();
    await waitFor(() => {
      expect(screen.queryByText(/Applying…/u)).toBeNull();
    });
    expect(box().checked).toBe(true);
    expect(backend.settings.launch_at_login).toBe(true);
  });

  it("shows the real state and the next step when macOS refuses", async () => {
    await open();
    backend.failing.set(
      "set_launch_at_login",
      "Launch at login needs your approval: open System Settings, General, Login Items, and allow dictation-app.",
    );
    fireEvent.click(box());
    expect((await screen.findByRole("alert")).textContent).toContain("General, Login Items");
    // The app still has it off, so the box goes back to off.
    await waitFor(() => {
      expect(box().checked).toBe(false);
    });
    expect(screen.queryByText(/Applying…/u)).toBeNull();
  });

  it("ends on the last choice when toggled again while the OS is busy", async () => {
    await open();
    const slow = gate();
    backend.holdLaunch = slow.closed;
    fireEvent.click(box());
    fireEvent.click(box());
    fireEvent.click(box());
    expect(box().checked).toBe(true);
    slow.open();
    await waitFor(() => {
      expect(screen.queryByText(/Applying…/u)).toBeNull();
    });
    expect(box().checked).toBe(true);
    expect(backend.settings.launch_at_login).toBe(true);
    expect(backend.named("set_launch_at_login").map((call) => call.args.enabled)).toEqual([
      true,
      false,
      true,
    ]);
  });

  /** The window comes back to the front, as when the tray shows it again. */
  const refocus = () => {
    act(() => {
      window.dispatchEvent(new Event("focus"));
    });
  };

  it("reads the OS again when the hidden window is shown again", async () => {
    await open();
    expect(box().checked).toBe(false);
    // Switched on in System Settings while this window was hidden.
    backend.settings = { ...backend.settings, launch_at_login: true };
    refocus();
    await waitFor(() => {
      expect(box().checked).toBe(true);
    });
    backend.settings = { ...backend.settings, launch_at_login: false };
    refocus();
    await waitFor(() => {
      expect(box().checked).toBe(false);
    });
  });

  it("saving only the language leaves a login item changed in System Settings alone", async () => {
    await open();
    // On in System Settings; this window has not seen a focus event yet.
    backend.settings = { ...backend.settings, launch_at_login: true };
    type("Language", "de");
    click("Save");
    await screen.findByText("Settings saved.");
    expect(backend.named("set_launch_at_login")).toHaveLength(0);
    expect(backend.settings.launch_at_login).toBe(true);
  });

  it("keeps the user's own change while the OS applies it, when the window regains focus", async () => {
    await open();
    const slow = gate();
    backend.holdLaunch = slow.closed;
    fireEvent.click(box());
    backend.settings = { ...backend.settings, launch_at_login: false };
    refocus();
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(box().checked).toBe(true);
    slow.open();
    await waitFor(() => {
      expect(screen.queryByText(/Applying…/u)).toBeNull();
    });
  });

  it("keeps a typed hotkey when the login item could not change", async () => {
    await open();
    backend.failing.set("set_launch_at_login", "Couldn't change launch at login: no bundle");
    fireEvent.click(box());
    type("Or type it", "F5");
    await screen.findByText(/no bundle/);
    expect(screen.getByLabelText<HTMLInputElement>("Or type it").value).toBe("F5");
  });
});

// T2 (P2-01): what a half-finished save, a failed re-read, a late key-test
// reply and an extra Enter do to the form.
describe("T2: settings edges", () => {
  const form = () => {
    const element = screen.getByLabelText("Or type it").closest("form");
    if (element === null) throw new Error("no form");
    return element;
  };

  it("partial_save_keeps_saved_key_status_and_allows_retry", async () => {
    await open();
    type(/API key/, KEY);
    type("Language", "de");
    // The key is stored; the settings file then can't be written.
    backend.failing.set("save_settings", "could not write the settings file: disk full");
    click("Save");
    expect((await screen.findByRole("alert")).textContent).toContain("disk full");

    // The key really is saved: the form says so and won't send it again.
    expect(backend.keys.groq).toBe(true);
    expect(screen.getByText("Key saved")).toBeTruthy();
    expect(keyBox().value).toBe("");
    expect(screen.queryByText("Settings saved.")).toBeNull();
    // The rest is still the user's, ready to retry.
    expect(screen.getByLabelText<HTMLInputElement>("Language").value).toBe("de");
    expect(screen.getByRole<HTMLButtonElement>("button", { name: "Save" }).disabled).toBe(false);

    backend.failing.delete("save_settings");
    click("Save");
    await screen.findByText("Settings saved.");
    expect(backend.named("set_api_key")).toHaveLength(1);
    expect(backend.named("save_settings")).toHaveLength(2);
    expect(backend.settings.language).toBe("de");
    expect(screen.getByText("Key saved")).toBeTruthy();
  });

  it("failed_reread_does_not_claim_unsaved_values", async () => {
    // A save that worked, then a re-read that failed: the form shows exactly
    // what was saved, and the next save builds on it.
    backend.settings = { ...backend.settings, model: "whisper-large-v3" };
    await open();
    type("Provider", "openai");
    type("Or type it", "  F6 ");
    const handle = backend.handle;
    let rereads = 0;
    mockIPC(async (command, payload) => {
      const saved = backend.named("save_settings").length > 0;
      if (command === "get_settings" && saved && rereads++ === 0) {
        // eslint-disable-next-line @typescript-eslint/only-throw-error
        throw "the settings file is busy";
      }
      return handle(command, payload);
    });
    click("Save");
    await screen.findByText("Settings saved.");
    expect(screen.getByLabelText<HTMLInputElement>("Or type it").value).toBe("F6");
    expect(screen.getByLabelText<HTMLSelectElement>("Provider").value).toBe("openai");
    expect(screen.queryByText(/default model/)).toBeNull();
    type("Language", "fr");
    click("Save");
    await waitFor(() => {
      expect(backend.named("save_settings")).toHaveLength(2);
    });
    expect(backend.settings.model).toBe("whisper-1");
  });

  it("late_provider_test_result_is_ignored", async () => {
    backend.keys = { groq: true, openai: false };
    await open();
    const late = gate();
    backend.holdTest = late.closed;
    backend.testResult = { kind: "invalid_key", message: "Groq rejected this API key." };
    click("Test key");
    await screen.findByRole("button", { name: "Testing…" });

    // The user moves to OpenAI before Groq answers.
    type("Provider", "openai");
    late.open();
    await flush();

    expect(screen.queryByText(/Groq rejected/)).toBeNull();
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.getByRole<HTMLButtonElement>("button", { name: "Test key" }).disabled).toBe(
      false,
    );
    expect(screen.getByLabelText(/OpenAI API key/)).toBeTruthy();
  });

  it("a hotkey the app couldn't switch to is not reported as saved", async () => {
    await open();
    type("Or type it", "F6");
    type("Language", "de");
    backend.failing.set(
      "set_hotkey",
      "Couldn't switch the hotkey to F6 (hotkey unavailable: busy). RightAlt still works.",
    );
    click("Save");
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("RightAlt still works");
    expect(screen.queryByText("Settings saved.")).toBeNull();
    expect(backend.settings.language).toBe("de"); // the rest was saved
    // What the user typed stays, so Save can be tried again.
    expect(screen.getByLabelText<HTMLInputElement>("Or type it").value).toBe("F6");
    backend.failing.delete("set_hotkey");
    click("Save");
    await screen.findByText("Settings saved.");
    expect(backend.settings.hotkey).toBe("F6");
  });

  it("enter_while_busy_starts_no_save", async () => {
    await open();
    type(/API key/, KEY);
    const hold = gate();
    backend.holdSetKey = hold.closed;
    // Enter in a field submits the form: once, then again while saving.
    fireEvent.submit(form());
    await screen.findByRole("button", { name: "Saving…" });
    fireEvent.submit(form());
    fireEvent.submit(form());
    hold.open();
    await screen.findByText("Settings saved.");
    expect(backend.named("set_api_key")).toHaveLength(1);
    expect(backend.named("save_settings")).toHaveLength(1);

    // While a key test runs, Save is disabled, and Enter does nothing either.
    const test = gate();
    backend.holdTest = test.closed;
    click("Test key");
    await screen.findByRole("button", { name: "Testing…" });
    type("Language", "de");
    fireEvent.submit(form());
    await flush();
    expect(backend.named("save_settings")).toHaveLength(1);
    test.open();
    await screen.findByRole("status");
    expect(backend.settings.language).toBeNull();
  });
});

describe("share usage and error events", () => {
  const box = () => screen.getByLabelText<HTMLInputElement>("Share usage and error events");

  it("is on by default", async () => {
    await open();
    expect(box().checked).toBe(true);
  });

  it("turning it off uses its own command at once, without the Save button", async () => {
    await open();
    fireEvent.click(box());
    await waitFor(() => {
      expect(backend.named("set_share_usage_events")).toHaveLength(1);
    });
    expect(backend.named("set_share_usage_events")[0]?.args.share).toBe(false);
    expect(backend.named("save_settings")).toHaveLength(0);
    expect(backend.settings.share_usage_events).toBe(false);
    await waitFor(() => {
      expect(box().checked).toBe(false);
    });
  });

  it("turning it on again sends true", async () => {
    backend.settings = { ...backend.settings, share_usage_events: false };
    await open();
    expect(box().checked).toBe(false);
    fireEvent.click(box());
    await waitFor(() => {
      expect(backend.settings.share_usage_events).toBe(true);
    });
    await waitFor(() => {
      expect(box().checked).toBe(true);
    });
  });

  it("stays as it was and says why when the command fails", async () => {
    await open();
    backend.failing.set("set_share_usage_events", "The settings file is read-only.");
    fireEvent.click(box());
    expect((await screen.findByRole("alert")).textContent).toContain("read-only");
    expect(box().checked).toBe(true);
  });

  it("a Save pressed while the opt-out is pending does not undo it", async () => {
    await open();
    let release: (() => void) | undefined;
    backend.holdShare = new Promise<void>((resolve) => {
      release = resolve;
    });
    fireEvent.click(box());
    await waitFor(() => {
      expect(backend.named("set_share_usage_events")).toHaveLength(1);
    });
    // The form still holds the sharing value it was loaded with.
    click("Save");
    await screen.findByText("Settings saved.");
    release?.();
    await waitFor(() => {
      expect(box().checked).toBe(false);
    });
    expect(backend.settings.share_usage_events).toBe(false);
    expect(backend.named("save_settings")).toHaveLength(1);
  });

  it("a later Save keeps the choice", async () => {
    await open();
    fireEvent.click(box());
    await waitFor(() => {
      expect(box().checked).toBe(false);
    });
    click("Save");
    await screen.findByText("Settings saved.");
    expect(backend.settings.share_usage_events).toBe(false);
    expect(box().checked).toBe(false);
  });

  it("lists what is sent in plain words, and what never is", async () => {
    await open();
    const list = screen.getByText("What's sent").closest("details");
    expect(list).not.toBeNull();
    expect(within(list as HTMLElement).getByText(/Which setting you changed/)).toBeTruthy();
    expect(
      within(list as HTMLElement).getByText(/Error reports, sent to Sentry \(US\)/),
    ).toBeTruthy();
    expect(screen.getByText(/Never your voice, your text, your API keys/)).toBeTruthy();
  });
});
