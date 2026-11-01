// M3.4 criteria 1-3: the settings form, with the app's commands replaced by
// a fake backend through `mockIPC`. The keys here are made up.

import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { KeyTestError, Provider, Settings_Serialize } from "./bindings";
import { Settings } from "./Settings";

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
  };
  keys: Record<Provider, boolean> = { groq: false, openai: false };
  /** What `test_api_key` answers: `null` for success, or an error to throw. */
  testResult: KeyTestError | Error | null = null;
  /** Fails the command named, with the message (a string, as the app sends). */
  failing = new Map<string, string>();
  calls: Call[] = [];
  /** Holds `test_api_key` until called, to test a reply that arrives late. */
  holdTest: Promise<void> | null = null;
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
        this.settings = args.settings as Settings_Serialize;
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
  cleanup();
  clearMocks();
});

/** Renders the form once the saved settings have loaded. */
async function open() {
  render(<Settings />);
  await screen.findByLabelText("Hotkey");
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
    expect(screen.getByLabelText<HTMLInputElement>("Hotkey").value).toBe("F5");
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
    expect(screen.queryByLabelText("Hotkey")).toBeNull();
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
    type("Hotkey", "Ctrl+");
    type(/API key/, "two words");
    click("Save");

    expect((await screen.findAllByRole("alert")).map((alert) => alert.textContent)).toEqual([
      "A key has no spaces; check what you pasted.",
      "Write the hotkey as keys joined by +, such as Ctrl+Shift+Space.",
      "Use a two or three letter language code such as en, or leave it empty to detect.",
    ]);
    expect(backend.named("set_api_key")).toEqual([]);
    expect(backend.named("save_settings")).toEqual([]);
    expect(screen.getByLabelText("Hotkey").getAttribute("aria-invalid")).toBe("true");
  });

  it("refuses an empty hotkey, and clears an error once the field is edited", async () => {
    await open();
    type("Hotkey", "  ");
    click("Save");
    expect((await screen.findByRole("alert")).textContent).toMatch(/Enter a hotkey/);

    type("Hotkey", "RightAlt");
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
    type("Hotkey", " Ctrl+Shift+Space ");
    type("Language", " DE ");
    click("Save");

    expect((await screen.findByRole("status")).textContent).toBe("Settings saved.");
    const order = backend.calls
      .map((call) => call.command)
      .filter((command) => ["set_api_key", "save_settings"].includes(command));
    expect(order).toEqual(["set_api_key", "save_settings"]);
    expect(backend.named("set_api_key")[0]?.args).toEqual({ provider: "openai", key: ` ${KEY} ` });
    expect(backend.named("save_settings")[0]?.args).toEqual({
      settings: {
        version: 2,
        provider: "openai",
        model: "whisper-1",
        language: "de",
        hotkey: "Ctrl+Shift+Space",
      },
    });
    expect(screen.getByText("Key saved")).toBeTruthy();
  });

  it("a fresh window shows what was saved", async () => {
    await open();
    type("Provider", "openai");
    type("Hotkey", "F6");
    type("Language", "es");
    click("Save");
    await screen.findByText("Settings saved.");
    cleanup();

    await open();

    expect(screen.getByLabelText<HTMLSelectElement>("Provider").value).toBe("openai");
    expect(screen.getByLabelText<HTMLInputElement>("Hotkey").value).toBe("F6");
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
      if (command === "save_settings") {
        backend.settings = { ...backend.settings, hotkey: "Ctrl+Shift+Space" };
      }
      return result;
    });
    await open();
    type("Hotkey", "ctrl+shift+space");
    click("Save");
    await screen.findByText("Settings saved.");

    expect(screen.getByLabelText<HTMLInputElement>("Hotkey").value).toBe("Ctrl+Shift+Space");
  });

  it("shows the app's message when saving the settings fails", async () => {
    backend.failing.set("save_settings", "invalid hotkey: Escape can't be a hotkey");
    await open();
    type("Hotkey", "Escape");
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
      expect(locked(screen.getByLabelText<HTMLInputElement>("Hotkey"))).toBe(true);
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
