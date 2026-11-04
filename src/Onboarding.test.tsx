// M5.2 criterion 1: the onboarding steps, with the app's commands replaced
// by a fake backend through `mockIPC` that keeps the step and refuses to
// skip one, as the app does. The keys here are made up.

import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "./App";
import type {
  DictationProgressView,
  KeyTestError,
  OnboardingStatus,
  PermissionState,
  Provider,
  Settings_Serialize,
  Step,
  View,
} from "./bindings";
import { hotkeyName, Onboarding, POLL_MS } from "./Onboarding";
import { keyDown, keyUp, releaseAllKeys } from "./testKeys";

const KEY = "gsk_dummy_NOT_A_REAL_KEY_0123456789";
const STEPS: Step[] = ["microphone", "accessibility", "key", "try"];

interface Call {
  command: string;
  args: Record<string, unknown>;
}

/** The app's side: permissions, the step it keeps, keys and settings. */
class Backend {
  view: View = "onboarding";
  step: Step = "microphone";
  microphone: PermissionState = "not_determined";
  accessibility: PermissionState = "not_determined";
  /** What the next microphone prompt answers. */
  micAnswer: PermissionState = "granted";
  keys: Record<Provider, boolean> = { groq: false, openai: false };
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
  /** What the app says the last dictation got through (U1). */
  progress: DictationProgressView = {
    started: 0,
    completed: 0,
    errors: 0,
    stages: [],
    error: null,
  };
  /** What `test_api_key` answers: `null` for success. */
  testResult: KeyTestError | null = null;
  calls: Call[] = [];

  status(): OnboardingStatus {
    return {
      view: this.view,
      step: this.step,
      microphone: this.microphone,
      accessibility: this.accessibility,
    };
  }

  /** Whether every step before `step` is done, as `Readiness::allows`. */
  allows(step: Step): boolean {
    const done: Record<Step, boolean> = {
      microphone: this.microphone === "granted",
      accessibility: this.accessibility === "granted",
      // L1: the on-device engine needs no key (the app counts it as saved).
      key: this.settings.transcription === "local" || this.keys[this.settings.provider],
      try: true,
    };
    return STEPS.slice(0, STEPS.indexOf(step)).every((before) => done[before]);
  }

  handle = (command: string, payload: unknown): unknown => {
    const args = (payload ?? {}) as Record<string, unknown>;
    this.calls.push({ command, args });
    switch (command) {
      case "get_onboarding":
        return this.status();
      case "request_permission":
        if (args.permission === "microphone" && this.microphone === "not_determined") {
          this.microphone = this.micAnswer;
        }
        return this.status();
      case "set_onboarding_step": {
        const step = args.step as Step;
        // The app's `NotYet` message, sent as a string.
        // eslint-disable-next-line @typescript-eslint/only-throw-error
        if (!this.allows(step)) throw "Allow the microphone first.";
        this.step = step;
        return this.status();
      }
      case "finish_onboarding":
        this.view = "settings";
        return null;
      case "relaunch_app":
        return null;
      case "dictation_progress":
        return this.progress;
      case "get_settings":
        return this.settings;
      case "save_settings":
        this.settings = { ...(args.settings as Settings_Serialize), hotkey: this.settings.hotkey };
        return null;
      case "set_hotkey":
        this.settings = { ...this.settings, hotkey: args.spec as string };
        return null;
      case "set_hotkey_recording":
        return null;
      case "has_api_key":
        return this.keys[args.provider as Provider];
      case "set_api_key":
        this.keys[args.provider as Provider] = true;
        return null;
      case "test_api_key":
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
  mockIPC(backend.handle);
});

afterEach(() => {
  releaseAllKeys();
  cleanup();
  clearMocks();
  vi.useRealTimers();
});

/** The window coming back to the front (the app asks again then). */
function windowShown() {
  fireEvent.focus(window);
}

async function heading(name: string) {
  return screen.findByRole("heading", { level: 2, name });
}

function button(name: string): HTMLButtonElement {
  return screen.getByRole("button", { name });
}

/** The key step once its settings have loaded and Test can be pressed. */
async function keyStepReady() {
  await heading("Add your API key");
  await waitFor(() => {
    expect(button("Test and continue").disabled).toBe(false);
  });
}

describe("the microphone step", () => {
  it("moves on only once the microphone is allowed, and then by itself", async () => {
    render(<Onboarding onFinished={() => undefined} />);
    await heading("Allow the microphone");
    expect(screen.getByLabelText("progress").textContent).toBe("Step 1 of 4");
    expect(button("Continue").disabled).toBe(true);
    expect(screen.queryByRole("button", { name: "Back" })).toBeNull();

    // The prompt is up; the user hasn't answered yet.
    backend.micAnswer = "not_determined";
    fireEvent.click(button("Allow microphone"));
    await waitFor(() => {
      expect(backend.named("request_permission")).toEqual([
        { command: "request_permission", args: { permission: "microphone" } },
      ]);
    });
    expect(button("Continue").disabled).toBe(true);
    expect(backend.named("set_onboarding_step")).toEqual([]);

    // Allowed in the prompt: the next poll moves on by itself.
    backend.microphone = "granted";
    windowShown();
    await heading("Allow Accessibility");
    expect(backend.named("set_onboarding_step").map((call) => call.args.step)).toEqual([
      "accessibility",
    ]);
  });

  it("polls while it waits for the answer", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    render(<Onboarding onFinished={() => undefined} />);
    await heading("Allow the microphone");
    const asked = backend.named("get_onboarding").length;
    await act(() => vi.advanceTimersByTimeAsync(POLL_MS * 2));
    expect(backend.named("get_onboarding").length).toBeGreaterThanOrEqual(asked + 2);

    backend.microphone = "granted";
    await act(() => vi.advanceTimersByTimeAsync(POLL_MS));
    await heading("Allow Accessibility");
  });

  it("stays on the step when the microphone goes from not asked to refused (#159)", async () => {
    // A permission that changes is not a permission that was granted: only
    // "granted" moves the step on, so a refusal never asks for the next step.
    render(<Onboarding onFinished={() => undefined} />);
    await heading("Allow the microphone");

    backend.microphone = "denied";
    windowShown();
    await screen.findByText(/The microphone is turned off for Vocet/u);
    expect(await heading("Allow the microphone")).toBeTruthy();
    expect(backend.named("set_onboarding_step")).toEqual([]);
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("sends a refused microphone to System Settings", async () => {
    backend.microphone = "denied";
    render(<Onboarding onFinished={() => undefined} />);
    await heading("Allow the microphone");
    expect(screen.getByText(/The microphone is turned off for Vocet/u)).toBeTruthy();
    fireEvent.click(button("Open System Settings"));
    await waitFor(() => {
      expect(backend.named("request_permission")).toHaveLength(1);
    });
    expect(button("Continue").disabled).toBe(true);
  });
});

describe("the Accessibility step", () => {
  beforeEach(() => {
    backend.microphone = "granted";
    backend.step = "accessibility";
  });

  it("opens System Settings, offers Relaunch, and moves on by itself once granted", async () => {
    render(<Onboarding onFinished={() => undefined} />);
    await heading("Allow Accessibility");
    expect(button("Continue").disabled).toBe(true);
    expect(screen.queryByRole("button", { name: "Relaunch" })).toBeNull();

    fireEvent.click(button("Open System Settings"));
    await screen.findByText(/Relaunch to finish/u);
    expect(backend.named("request_permission")).toEqual([
      { command: "request_permission", args: { permission: "accessibility" } },
    ]);
    fireEvent.click(button("Relaunch"));
    await waitFor(() => {
      expect(backend.named("relaunch_app")).toHaveLength(1);
    });

    backend.accessibility = "granted";
    windowShown();
    await heading("Add your API key");
  });
});

describe("the Accessibility step refused", () => {
  it("stays on the step when Accessibility goes from not asked to refused (#159)", async () => {
    backend.microphone = "granted";
    backend.step = "accessibility";
    render(<Onboarding onFinished={() => undefined} />);
    await heading("Allow Accessibility");

    backend.accessibility = "denied";
    windowShown();
    await waitFor(() => {
      expect(backend.named("get_onboarding").length).toBeGreaterThanOrEqual(2);
    });
    expect(await heading("Allow Accessibility")).toBeTruthy();
    expect(backend.named("set_onboarding_step")).toEqual([]);
  });
});

describe("going back", () => {
  it("goes back to a done step and stays there until the user moves on", async () => {
    backend.microphone = "granted";
    backend.accessibility = "granted";
    backend.step = "key";
    render(<Onboarding onFinished={() => undefined} />);
    await heading("Add your API key");

    fireEvent.click(button("Back"));
    await heading("Allow Accessibility");
    expect(screen.getByText("Accessibility is allowed.")).toBeTruthy();
    // Done already: no moving on by itself, however often the app is asked.
    windowShown();
    windowShown();
    await waitFor(() => {
      expect(backend.named("get_onboarding").length).toBeGreaterThanOrEqual(3);
    });
    expect(backend.step).toBe("accessibility");
    expect(button("Continue").disabled).toBe(false);

    fireEvent.click(button("Continue"));
    await heading("Add your API key");
  });

  it("shows the app's refusal when a step isn't done", async () => {
    backend.step = "accessibility";
    backend.accessibility = "granted";
    // The microphone was turned off since; the app refuses the key step.
    render(<Onboarding onFinished={() => undefined} />);
    await heading("Allow Accessibility");
    fireEvent.click(button("Continue"));
    expect((await screen.findByRole("alert")).textContent).toBe("Allow the microphone first.");
    expect(backend.step).toBe("accessibility");
  });
});

describe("the key step", () => {
  beforeEach(() => {
    backend.microphone = "granted";
    backend.accessibility = "granted";
    backend.step = "key";
  });

  it("stays on the step with the error when the key test fails, saving nothing", async () => {
    backend.testResult = {
      kind: "invalid_key",
      message: "Groq rejected this API key. Check that you copied the whole key from Groq.",
    };
    render(<Onboarding onFinished={() => undefined} />);
    await keyStepReady();
    fireEvent.change(screen.getByLabelText("Groq API key"), { target: { value: KEY } });
    fireEvent.click(button("Test and continue"));

    expect((await screen.findByRole("alert")).textContent).toBe(
      "Groq rejected this API key. Check that you copied the whole key from Groq.",
    );
    expect(backend.named("test_api_key")).toEqual([
      { command: "test_api_key", args: { provider: "groq", key: KEY } },
    ]);
    expect(backend.named("set_api_key")).toEqual([]);
    expect(backend.named("set_onboarding_step")).toEqual([]);
    expect(screen.getByRole("heading", { level: 2 }).textContent).toBe("Add your API key");
  });

  it("confirms the saved key works when nothing is typed", async () => {
    backend.keys.groq = true;
    render(<Onboarding onFinished={() => undefined} />);
    await keyStepReady();
    fireEvent.click(button("Test and continue"));
    await heading("Try it");
    expect(await screen.findByText("Key works ✓")).toBeTruthy();
    expect(backend.named("test_api_key")).toEqual([
      { command: "test_api_key", args: { provider: "groq", key: null } },
    ]);
  });

  it("asks for a key before testing when none is saved or typed", async () => {
    render(<Onboarding onFinished={() => undefined} />);
    await keyStepReady();
    fireEvent.click(button("Test and continue"));
    expect((await screen.findByRole("alert")).textContent).toBe("Paste your API key.");
    expect(backend.named("test_api_key")).toEqual([]);
  });

  it("saves a key that works and moves on", async () => {
    render(<Onboarding onFinished={() => undefined} />);
    await keyStepReady();
    fireEvent.change(screen.getByLabelText("Groq API key"), { target: { value: KEY } });
    fireEvent.click(button("Test and continue"));

    await heading("Try it");
    expect((await screen.findByText("Key works ✓")).getAttribute("role")).toBe("status");
    expect(backend.named("set_api_key")).toEqual([
      { command: "set_api_key", args: { provider: "groq", key: KEY } },
    ]);
    expect(backend.named("save_settings")).toEqual([]);
    expect(backend.step).toBe("try");
  });

  it("saves a new provider with its default model", async () => {
    render(<Onboarding onFinished={() => undefined} />);
    await heading("Add your API key");
    await waitFor(() => {
      expect(backend.named("has_api_key")).toHaveLength(1);
    });
    fireEvent.change(screen.getByLabelText("Provider"), { target: { value: "openai" } });
    fireEvent.change(await screen.findByLabelText("OpenAI API key"), {
      target: { value: "sk-dummy-NOT-A-REAL-KEY" },
    });
    fireEvent.click(button("Test and continue"));

    await heading("Try it");
    expect(backend.settings.provider).toBe("openai");
    expect(backend.settings.model).toBe("whisper-1");
    expect(backend.keys.openai).toBe(true);
  });

  it("keeps the provider the user picked while the settings load, and tests with it", async () => {
    let answer: (settings: Settings_Serialize) => void = () => undefined;
    mockIPC((command, payload) =>
      command === "get_settings"
        ? new Promise((resolve) => {
            backend.calls.push({ command, args: {} });
            answer = resolve;
          })
        : backend.handle(command, payload),
    );
    render(<Onboarding onFinished={() => undefined} />);
    await heading("Add your API key");
    await waitFor(() => {
      expect(backend.named("get_settings")).toHaveLength(1);
    });
    expect(screen.getByText(/Loading…/u)).toBeTruthy();
    expect(button("Test and continue").disabled).toBe(true);
    fireEvent.change(screen.getByLabelText("Provider"), { target: { value: "openai" } });
    fireEvent.change(await screen.findByLabelText("OpenAI API key"), {
      target: { value: "sk-dummy-NOT-A-REAL-KEY" },
    });
    // The settings arrive late, still on Groq.
    act(() => {
      answer(backend.settings);
    });
    await keyStepReady();
    fireEvent.click(button("Test and continue"));

    await heading("Try it");
    expect(backend.named("test_api_key")).toEqual([
      { command: "test_api_key", args: { provider: "openai", key: "sk-dummy-NOT-A-REAL-KEY" } },
    ]);
    expect(backend.settings.provider).toBe("openai");
    expect(backend.keys).toEqual({ groq: false, openai: true });
  });

  it("says when the settings don't load, and sets up the key after Retry", async () => {
    let failures = 1;
    mockIPC((command, payload) => {
      if (command === "get_settings" && failures > 0) {
        failures -= 1;
        backend.calls.push({ command, args: {} });
        // eslint-disable-next-line @typescript-eslint/only-throw-error
        throw "The settings file can't be read.";
      }
      return backend.handle(command, payload);
    });
    render(<Onboarding onFinished={() => undefined} />);
    expect((await screen.findByRole("alert")).textContent).toMatch(
      /Couldn.t load your settings.*The settings file can't be read\./u,
    );
    expect(screen.queryByLabelText("Provider")).toBeNull();
    expect(button("Test and continue").disabled).toBe(true);

    fireEvent.click(button("Retry"));
    await keyStepReady();
    fireEvent.change(screen.getByLabelText("Provider"), { target: { value: "openai" } });
    fireEvent.change(await screen.findByLabelText("OpenAI API key"), {
      target: { value: "sk-dummy-NOT-A-REAL-KEY" },
    });
    fireEvent.click(button("Test and continue"));

    await heading("Try it");
    expect(backend.named("save_settings")).toHaveLength(1);
    expect(backend.settings.provider).toBe("openai");
    expect(backend.keys.openai).toBe(true);
    expect(backend.step).toBe("try");
  });

  it("says no key is needed with the on-device engine, and moves on (L1)", async () => {
    backend.settings = { ...backend.settings, transcription: "local" };
    render(<Onboarding onFinished={() => undefined} />);
    await heading("No key needed");
    expect(screen.queryByLabelText("Groq API key")).toBeNull();
    fireEvent.click(button("Continue"));
    await heading("Try it");
    expect(backend.named("test_api_key")).toEqual([]);
    expect(backend.named("set_api_key")).toEqual([]);
    expect(backend.step).toBe("try");
  });

  it("tests the saved key when the box is empty", async () => {
    backend.keys.groq = true;
    render(<Onboarding onFinished={() => undefined} />);
    await screen.findByText(/A key is saved/u);
    fireEvent.click(button("Test and continue"));
    await heading("Try it");
    expect(backend.named("test_api_key")).toEqual([
      { command: "test_api_key", args: { provider: "groq", key: null } },
    ]);
    expect(backend.named("set_api_key")).toEqual([]);
  });
});

describe("the try step", () => {
  beforeEach(() => {
    backend.microphone = "granted";
    backend.accessibility = "granted";
    backend.keys.groq = true;
    backend.step = "try";
  });

  /** The step has taken its first answer from the app (the baseline). */
  async function stepOpen() {
    await heading("Try it");
    await waitFor(() => {
      expect(backend.named("dictation_progress").length).toBeGreaterThan(0);
    });
    // One more poll, so the baseline is set before the test changes things.
    const polls = backend.named("dictation_progress").length;
    await waitFor(() => {
      expect(backend.named("dictation_progress").length).toBeGreaterThan(polls);
    });
  }

  it("puts the focus in the field", async () => {
    render(<Onboarding onFinished={() => undefined} />);
    await heading("Try it");
    const field = await screen.findByLabelText(/Hold the right Option key/u);
    expect(document.activeElement).toBe(field);
  });

  it("U1 criterion 6: typing in the box never shows success", async () => {
    render(<Onboarding onFinished={() => undefined} />);
    await stepOpen();
    const field = screen.getByLabelText(/Hold the right Option key/u);
    fireEvent.change(field, { target: { value: "hello world" } });
    // Give the poll a chance to (wrongly) change its mind.
    await new Promise((resolve) => setTimeout(resolve, POLL_MS));
    expect(screen.queryByText(/It works/u)).toBeNull();
    expect(button("Finish").disabled).toBe(true);
  });

  it("U1 criterion 6: shows success, listing the stages seen, after a completed dictation", async () => {
    const onFinished = vi.fn();
    render(<Onboarding onFinished={onFinished} />);
    await stepOpen();

    // Half way: recorded and transcribed, not pasted yet. No success.
    backend.progress = {
      started: 1,
      completed: 0,
      errors: 0,
      stages: ["recorded", "transcribed"],
      error: null,
    };
    expect(
      await screen.findByText(/So far Vocet recorded and transcribed/u, undefined, {
        timeout: 3000,
      }),
    ).toBeTruthy();
    expect(screen.queryByText(/It works/u)).toBeNull();
    expect(button("Finish").disabled).toBe(true);

    backend.progress = {
      started: 1,
      completed: 1,
      errors: 0,
      stages: ["recorded", "transcribed", "pasted"],
      error: null,
    };
    expect(
      await screen.findByText(/It works: Vocet recorded, transcribed and pasted/u, undefined, {
        timeout: 3000,
      }),
    ).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Skip for now" })).toBeNull();

    fireEvent.click(button("Finish"));
    await waitFor(() => {
      expect(onFinished).toHaveBeenCalledTimes(1);
    });
    expect(backend.named("finish_onboarding")).toHaveLength(1);
  });

  it("U1 criterion 6: a dictation from before the step opened doesn't count", async () => {
    backend.progress = {
      started: 1,
      completed: 1,
      errors: 0,
      stages: ["recorded", "transcribed", "pasted"],
      error: null,
    };
    render(<Onboarding onFinished={() => undefined} />);
    await stepOpen();
    expect(screen.queryByText(/It works/u)).toBeNull();
    expect(button("Finish").disabled).toBe(true);
  });

  it("U1 criterion 6: Skip for now is secondary, and Finish says the test was skipped", async () => {
    const onFinished = vi.fn();
    render(<Onboarding onFinished={onFinished} />);
    await stepOpen();
    expect(button("Finish").disabled).toBe(true);
    fireEvent.click(button("Skip for now"));
    expect(screen.getByText(/Test skipped/u)).toBeTruthy();
    expect(screen.queryByText(/It works/u)).toBeNull();
    const finish = button("Finish (test skipped)");
    expect(finish.disabled).toBe(false);
    expect(finish.className).toContain("primary");
    expect(button("Skip for now").className).not.toContain("primary");
    fireEvent.click(finish);
    await waitFor(() => {
      expect(onFinished).toHaveBeenCalledTimes(1);
    });
  });

  it("U1 criterion 7: a failed dictation shows its sentence inline, not the menu bar icon", async () => {
    render(<Onboarding onFinished={() => undefined} />);
    await stepOpen();
    backend.progress = {
      started: 1,
      completed: 0,
      errors: 1,
      stages: ["recorded"],
      error: "Your API key was rejected. Open Settings → Speech to fix it.",
    };
    const alert = await screen.findByRole("alert", undefined, { timeout: 3000 });
    expect(alert.textContent).toBe("Your API key was rejected. Open Settings → Speech to fix it.");
    expect(screen.queryByText(/menu bar icon/u)).toBeNull();
    expect(screen.queryByText(/key_rejected/u)).toBeNull();
    expect(button("Finish").disabled).toBe(true);
  });

  it("shows the dictation key and records a different one", async () => {
    render(<Onboarding onFinished={vi.fn()} />);
    await heading("Try it");
    await waitFor(() => {
      expect(screen.getByLabelText("Dictation key").textContent).toBe("Right ⌥");
    });
    fireEvent.click(button("Record a different key"));
    keyDown("MetaRight");
    keyUp("MetaRight");
    await waitFor(() => {
      expect(backend.settings.hotkey).toBe("RightCmd");
    });
    expect(await screen.findByLabelText(/Hold the right Command key/u)).toBeTruthy();
  });

  it("names the hotkey", () => {
    expect(hotkeyName("RightAlt")).toBe("the right Option key");
    expect(hotkeyName("LeftAlt")).toBe("the left Option key");
    expect(hotkeyName("RightCmd")).toBe("the right Command key");
    expect(hotkeyName("Ctrl+Shift+Space")).toBe("Ctrl+Shift+Space");
  });
});

describe("the window", () => {
  it("shows onboarding when the app says so, and settings once finished", async () => {
    backend.microphone = "granted";
    backend.accessibility = "granted";
    backend.keys.groq = true;
    backend.step = "try";
    render(<App />);
    await heading("Try it");
    fireEvent.click(button("Skip for now"));
    fireEvent.click(button("Finish (test skipped)"));
    expect(await screen.findByRole("tablist", { name: "Settings sections" })).toBeTruthy();
  });

  it("reopened from the tray, resumes at the step the user went back to", async () => {
    backend.microphone = "granted";
    backend.accessibility = "granted";
    backend.step = "key";
    render(<App />);
    await heading("Add your API key");
    fireEvent.click(button("Back"));
    await heading("Allow Accessibility");

    // Settings… in the tray, then Set up dictation… again.
    backend.view = "settings";
    windowShown();
    expect(await screen.findByRole("tablist", { name: "Settings sections" })).toBeTruthy();
    backend.view = "onboarding";
    windowShown();
    await heading("Allow Accessibility");
    expect(screen.queryByRole("tablist", { name: "Settings sections" })).toBeNull();
  });

  it("falls back to settings if the app can't say", async () => {
    clearMocks();
    mockIPC((command) => {
      if (command === "get_onboarding") throw new Error("broken");
      if (command === "get_settings") return backend.settings;
      return false;
    });
    render(<App />);
    expect(await screen.findByRole("tablist", { name: "Settings sections" })).toBeTruthy();
  });
});
