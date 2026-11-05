// U8: the five onboarding steps, with the app's commands replaced by a fake
// backend through `mockIPC` that keeps the step and refuses to skip one, as
// the app does. The keys here are made up.

import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "./App";
import type {
  DictationProgressView,
  KeyTestError,
  LocalModelInfo_Serialize,
  OnboardingStatus,
  PermissionState,
  Provider,
  Settings_Serialize,
  SpeechChoice,
  Step,
  TryOutcome,
  View,
} from "./bindings";
import { Onboarding, RELAUNCH_AFTER_POLLS, STEPS } from "./Onboarding";
import css from "./settings.css?raw";
import {
  EVENTS_CAPTION,
  EVENTS_OFF_CAPTION,
  FINISH_REASON,
  NOT_TESTED,
} from "./onboarding/ReadyStep";
import { KEY_REASON, MODEL_COPY } from "./onboarding/SpeechStep";
import { BOTH_REASON } from "./onboarding/PermissionsStep";
import { CONTINUE_REASON } from "./onboarding/TryStep";
import { Privacy } from "./settings/Privacy";
import { keyDown, keyUp, releaseAllKeys } from "./testKeys";

const KEY = "gsk_dummy_NOT_A_REAL_KEY_0123456789";
const ORDER: Step[] = ["welcome", "speech", "permissions", "try", "ready"];
const WORDS = "this is my first dictation";

const PARAKEET: LocalModelInfo_Serialize = {
  id: "parakeet-unified-en-0.6b-q4_k_m",
  name: "Parakeet Unified 0.6B",
  languages: "English only",
  language_codes: ["en"],
  size_bytes: 477_000_000,
  downloaded_bytes: 0,
  state: "missing",
  error: null,
  default: true,
  resumed_from_bytes: null,
  restarted: false,
};

const QWEN: LocalModelInfo_Serialize = {
  id: "qwen3-asr-0.6b-q4_k_m",
  name: "Qwen3-ASR 0.6B",
  languages: "30 languages",
  language_codes: ["en", "hi", "de"],
  size_bytes: 590_000_000,
  downloaded_bytes: 0,
  state: "missing",
  error: null,
  default: false,
  resumed_from_bytes: null,
  restarted: false,
};

interface Call {
  command: string;
  args: Record<string, unknown>;
}

const NO_PROGRESS: DictationProgressView = {
  started: 0,
  finished: 0,
  completed: 0,
  errors: 0,
  stages: [],
  error: null,
  failure: null,
  heard: false,
};

/** The app's side: permissions, the step it keeps, keys, settings and models. */
class Backend {
  view: View = "onboarding";
  step: Step = "welcome";
  welcomePassed = false;
  microphone: PermissionState = "not_determined";
  accessibility: PermissionState = "not_determined";
  needsRelaunch = false;
  tryOutcome: TryOutcome = "not_tried";
  downloadPaused = false;
  speechChoice: SpeechChoice | null = null;
  /** The server answers a resume with the whole file. */
  ignoresRange = false;
  micAnswer: PermissionState = "granted";
  keys: Record<Provider, boolean> = { groq: false, openai: false };
  models: LocalModelInfo_Serialize[] = [{ ...PARAKEET }, { ...QWEN }];
  settings: Settings_Serialize = {
    version: 2,
    provider: "groq",
    model: "whisper-large-v3-turbo",
    language: null,
    hotkey: "RightAlt",
    launch_at_login: false,
    transcription: "cloud",
    local_model: PARAKEET.id,
    share_usage_events: true,
    history_limit: 25,
  };
  engine: { state: "loading" | "ready" | "failed"; message: string | null } = {
    state: "ready",
    message: null,
  };
  progress: DictationProgressView = { ...NO_PROGRESS };
  words: string | null = null;
  testResult: KeyTestError | null = null;
  calls: Call[] = [];

  status(): OnboardingStatus {
    return {
      view: this.view,
      step: this.step,
      microphone: this.microphone,
      accessibility: this.accessibility,
      needs_relaunch: this.needsRelaunch,
      try_outcome: this.tryOutcome,
      download_paused: this.downloadPaused,
      speech_choice: this.speechChoice,
    };
  }

  allows(step: Step): boolean {
    const done: Record<Step, boolean> = {
      welcome: this.welcomePassed,
      speech: this.settings.transcription === "local" || this.keys[this.settings.provider],
      permissions: this.microphone === "granted" && this.accessibility === "granted",
      try: true,
      ready: true,
    };
    return ORDER.slice(0, ORDER.indexOf(step)).every((before) => done[before]);
  }

  model(id: string): LocalModelInfo_Serialize {
    const found = this.models.find((model) => model.id === id);
    if (found === undefined) throw new Error(`no model ${id}`);
    return found;
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
        if (step !== "welcome") this.welcomePassed = true;
        // The app's `NotYet` message, sent as a string.
        // eslint-disable-next-line @typescript-eslint/only-throw-error
        if (!this.allows(step)) throw "Choose how speech becomes text first.";
        this.step = step;
        return this.status();
      }
      case "finish_onboarding":
      case "leave_setup_for_settings":
        this.view = "settings";
        return null;
      case "list_history":
        return { entries: [], unreadable: false, write_error: null };
      case "relaunch_app":
        return null;
      case "dictation_progress":
        return this.progress;
      case "practice_paste_missed":
        return null;
      case "practice_words":
        return this.words;
      case "copy_practice_words":
      case "set_hotkey_recording":
      case "start_microphone_test":
      case "stop_microphone_test":
      case "open_link":
        return null;
      case "read_microphone_test":
        return { level: -20, device: "MacBook Microphone" };
      case "set_try_outcome":
        this.tryOutcome = args.outcome as TryOutcome;
        return null;
      case "set_download_paused":
        this.downloadPaused = args.paused as boolean;
        return null;
      case "get_settings":
        return this.settings;
      case "set_share_usage_events":
        this.settings = { ...this.settings, share_usage_events: args.share as boolean };
        return null;
      case "set_language":
        this.settings = { ...this.settings, language: args.language as string | null };
        return null;
      case "set_hotkey":
        this.settings = { ...this.settings, hotkey: args.spec as string };
        return null;
      case "set_launch_at_login":
        this.settings = { ...this.settings, launch_at_login: args.enabled as boolean };
        return args.enabled;
      case "has_api_key":
        return this.keys[args.provider as Provider];
      case "set_api_key":
        this.keys[args.provider as Provider] = true;
        return null;
      case "test_api_key":
        // eslint-disable-next-line @typescript-eslint/only-throw-error
        if (this.testResult !== null) throw this.testResult;
        return null;
      case "use_cloud_engine":
        this.settings = {
          ...this.settings,
          transcription: "cloud",
          provider: args.provider as Provider,
        };
        return null;
      case "choose_local_model":
        this.settings = {
          ...this.settings,
          transcription: "local",
          local_model: args.id as string,
        };
        return null;
      case "download_local_model": {
        const model = this.model(args.id as string);
        const part = model.downloaded_bytes ?? 0;
        model.state = "downloading";
        if (part > 0 && this.ignoresRange) model.restarted = true;
        else if (part > 0) model.resumed_from_bytes = part;
        return null;
      }
      case "set_speech_choice":
        this.speechChoice = args.choice as SpeechChoice;
        return null;
      case "cancel_local_model_download": {
        const model = this.model(args.id as string);
        model.state = model.downloaded_bytes ? "partial" : "missing";
        return null;
      }
      case "list_local_models":
        // A new list each time, as the app sends.
        return this.models.map((model) => ({ ...model }));
      case "get_engine_status":
        return this.engine;
      default:
        throw new Error(`unexpected command ${command}`);
    }
  };

  named(command: string): Call[] {
    return this.calls.filter((call) => call.command === command);
  }

  /** Permissions granted and a local model ready: every step can be reached. */
  allSet(step: Step) {
    this.step = step;
    this.welcomePassed = true;
    this.microphone = "granted";
    this.accessibility = "granted";
    this.settings = { ...this.settings, transcription: "local", language: "en" };
    this.model(PARAKEET.id).state = "ready";
    this.model(PARAKEET.id).downloaded_bytes = PARAKEET.size_bytes;
  }
}

let backend: Backend;

beforeEach(() => {
  backend = new Backend();
  mockIPC(backend.handle);
});

afterEach(() => {
  cleanup();
  clearMocks();
});

function show() {
  return render(<Onboarding onFinished={() => undefined} />);
}

/** What the window does when it comes back: asks the app again. */
function refocus() {
  act(() => {
    window.dispatchEvent(new Event("focus"));
  });
}

function primary(name: string | RegExp): HTMLButtonElement {
  return screen.getByRole("button", { name });
}

const LONG = "ह".repeat(300);

const SAVING_REASON = "Saving your choice…";

describe("the frame", () => {
  it("criterion 1: exactly these five steps, in order, on the rail", async () => {
    show();
    const rail = await screen.findByRole("navigation", { name: "Setup steps" });
    expect(
      within(rail)
        .getAllByRole("listitem")
        .map((item) => item.textContent),
    ).toEqual(["1Welcome", "2Speech", "3Permissions", "4Try it", "5Ready"]);
    expect(STEPS.map((step) => step.id)).toEqual(ORDER);
  });

  it("criterion 1: the rail marks done, current and upcoming", async () => {
    backend.allSet("permissions");
    show();
    const rail = await screen.findByRole("navigation", { name: "Setup steps" });
    const items = within(rail).getAllByRole("listitem");
    expect(items[0]?.textContent).toBe("✓Welcome");
    expect(items[1]?.textContent).toBe("✓Speech");
    expect(items[2]?.getAttribute("aria-current")).toBe("step");
    expect(items[3]?.className).toContain("upcoming");
  });

  it("criterion 1: every step has Back except Welcome, and one primary", async () => {
    for (const step of ORDER) {
      backend = new Backend();
      mockIPC(backend.handle);
      backend.allSet(step);
      const { unmount } = show();
      await screen.findByRole("navigation", { name: "Setup steps" });
      await waitFor(() => {
        expect(document.querySelectorAll(".ob-footer .primary")).toHaveLength(1);
      });
      expect(screen.queryByRole("button", { name: "Back" }) === null).toBe(step === "welcome");
      unmount();
    }
  });

  it("criterion 1: a disabled primary always says why", async () => {
    backend.allSet("permissions");
    backend.accessibility = "not_determined";
    show();
    await waitFor(() => {
      expect(primary("Continue").disabled).toBe(true);
    });
    expect(primary("Continue").getAttribute("aria-describedby")).toBe("ob-reason");
    expect(screen.getByText(BOTH_REASON)).toBeTruthy();
  });

  it("criterion 1: the layout holds: rail, scrolling content, fixed footer", () => {
    const rule = (selector: string) => {
      const found = new RegExp(`(^|\\n)${selector.replace(".", "\\.")} \\{([^}]*)\\}`).exec(css);
      expect(found, selector).not.toBeNull();
      return found?.[2] ?? "";
    };
    expect(rule(".ob-rail")).toContain("150px");
    expect(rule(".ob-content")).toContain("overflow: hidden auto");
    // Long words wrap instead of widening the window at 680×520.
    expect(rule(".ob-content")).toContain("overflow-wrap: anywhere");
    expect(rule(".ob-main")).toContain("min-width: 0");
    expect(rule(".ob-footer")).toContain("flex: 0 0 auto");
    expect(rule(".ob-reason")).toContain("overflow-wrap: anywhere");
  });

  it("criterion 1: pseudo-long labels stay inside their elements", async () => {
    backend.step = "speech";
    backend.welcomePassed = true;
    backend.models = [{ ...PARAKEET, name: `${LONG} x` }];
    show();
    // The long name is rendered as text in the wrapping column.
    const title = await screen.findByText(new RegExp(LONG));
    expect(title.closest(".ob-content")).not.toBeNull();
  });
});

describe("Welcome", () => {
  // Replaces U8's three tests here, which required the switch on this screen
  // and no event before leaving it (U10 reverses both).
  it("U10 criterion 1: has no sharing switch, caption or What's sent", async () => {
    show();
    await screen.findByText("Talk instead of type");
    await waitFor(() => {
      expect(primary("Continue").disabled).toBe(false);
    });
    expect(screen.queryByRole("switch")).toBeNull();
    expect(screen.queryByText(/usage|error events|anonymous/i)).toBeNull();
    expect(screen.queryByText("What's sent")).toBeNull();
  });

  it("Continue moves on", async () => {
    show();
    await screen.findByText("Talk instead of type");
    await waitFor(() => {
      expect(primary("Continue").disabled).toBe(false);
    });
    fireEvent.click(primary("Continue"));
    await screen.findByText("Which language will you mostly speak?");
    expect(backend.step).toBe("speech");
  });
});

describe("the sharing switch across setup (U10)", () => {
  it.each(["speech", "permissions", "try"] as const)(
    "criterion 1: %s has no sharing switch",
    async (step) => {
      backend.allSet(step);
      show();
      await screen.findByRole("navigation", { name: "Setup steps" });
      await act(async () => {
        await new Promise((resolve) => setTimeout(resolve, 20));
      });
      expect(screen.queryByRole("switch", { name: /usage and error events/i })).toBeNull();
      expect(screen.queryByText("What's sent")).toBeNull();
    },
  );

  it("criterion 1: Ready has the switch, on by default, with What's sent", async () => {
    backend.allSet("ready");
    show();
    const share = await screen.findByRole<HTMLInputElement>("switch", {
      name: "Share usage and error events",
    });
    expect(share.checked).toBe(true);
    expect(screen.getByText(EVENTS_CAPTION)).toBeTruthy();
    expect(screen.getByText("What's sent")).toBeTruthy();
    expect(screen.getByText("Change any time in Settings → Privacy.")).toBeTruthy();
  });

  it("criterion 3: a stored off shows off on Ready, and setup writes nothing", async () => {
    backend.settings = { ...backend.settings, share_usage_events: false };
    backend.allSet("ready");
    show();
    const share = await screen.findByRole<HTMLInputElement>("switch", {
      name: "Share usage and error events",
    });
    await waitFor(() => {
      expect(share.checked).toBe(false);
    });
    expect(screen.getByText(EVENTS_OFF_CAPTION)).toBeTruthy();
    expect(screen.queryByText(EVENTS_CAPTION)).toBeNull();
    fireEvent.click(primary("Start dictating"));
    await waitFor(() => {
      expect(backend.named("finish_onboarding")).toHaveLength(1);
    });
    expect(backend.named("set_share_usage_events")).toEqual([]);
    expect(backend.settings.share_usage_events).toBe(false);
  });

  it("criterion 7: What's sent says what really happens", async () => {
    backend.allSet("ready");
    show();
    await screen.findByText("What's sent");
    const words = screen.getByText("What's sent").closest("details")?.textContent ?? "";
    expect(words).toContain("from the first setup screen");
    expect(words).toContain("deletes what is waiting");
    expect(words).toContain("can't recall what was already sent");
    expect(words).toContain("where you stopped");
    expect(EVENTS_CAPTION).toContain("have been on since setup began");
    expect(EVENTS_CAPTION).toContain("Off stops them from now on");
    expect(EVENTS_CAPTION).not.toMatch(/nothing is sent|until you/i);
  });

  it("a failed write made elsewhere doesn't trap a step with no switch", async () => {
    const original = backend.handle;
    mockIPC((command, payload) => {
      // eslint-disable-next-line @typescript-eslint/only-throw-error
      if (command === "set_share_usage_events") throw "Couldn't write the settings file.";
      return original(command, payload);
    });
    const settings = render(<Privacy settings={backend.settings} onStored={() => undefined} />);
    fireEvent.click(screen.getByRole("switch", { name: "Share usage and error events" }));
    await screen.findByRole("alert");
    settings.unmount();

    show();
    await screen.findByText("Talk instead of type");
    await waitFor(() => {
      expect(primary("Continue").disabled).toBe(false);
    });
    fireEvent.click(primary("Continue"));
    await screen.findByText("Which language will you mostly speak?");
  });
});

describe("one queue for the whole window (round 5)", () => {
  /** Holds every sharing write until it is released, oldest first. */
  function holdWrites() {
    const pending: (() => void)[] = [];
    const original = backend.handle;
    mockIPC((command, payload) => {
      if (command !== "set_share_usage_events") return original(command, payload);
      return new Promise((resolve) => {
        pending.push(() => {
          resolve(original(command, payload));
        });
      });
    });
    return pending;
  }

  async function releaseOne(pending: (() => void)[]) {
    await act(async () => {
      pending.shift()?.();
      await new Promise((resolve) => setTimeout(resolve, 20));
    });
  }

  const noop = () => undefined;

  it("an old write from onboarding can't land after a newer off in Settings", async () => {
    backend.settings = { ...backend.settings, share_usage_events: false };
    backend.allSet("ready");
    const pending = holdWrites();
    const setup = show();
    const share = await screen.findByRole<HTMLInputElement>("switch", {
      name: "Share usage and error events",
    });
    await waitFor(() => {
      expect(share.checked).toBe(false);
    });
    fireEvent.click(share); // on, still on its way
    expect(pending).toHaveLength(1);

    // The tray's Settings… takes the view away; the write goes on.
    setup.unmount();
    render(
      <Privacy settings={{ ...backend.settings, share_usage_events: true }} onStored={noop} />,
    );
    const inSettings = screen.getByRole<HTMLInputElement>("switch", {
      name: "Share usage and error events",
    });
    expect(inSettings.checked).toBe(true);
    fireEvent.click(inSettings); // off, the user's last word

    // Not sent alongside the old one: it waits its turn, so nothing can overtake.
    expect(pending).toHaveLength(1);
    await releaseOne(pending);
    expect(pending).toHaveLength(1);
    expect(inSettings.checked).toBe(false);
    await releaseOne(pending);

    expect(backend.settings.share_usage_events).toBe(false);
    expect(backend.named("set_share_usage_events").map((call) => call.args.share)).toEqual([
      true,
      false,
    ]);
  });

  it("a write from Settings holds onboarding's Continue when it comes back", async () => {
    const pending = holdWrites();
    const settings = render(<Privacy settings={backend.settings} onStored={noop} />);
    fireEvent.click(screen.getByRole("switch", { name: "Share usage and error events" }));
    expect(pending).toHaveLength(1);
    settings.unmount();

    show();
    await screen.findByText("Talk instead of type");
    await waitFor(() => {
      expect(primary("Continue").disabled).toBe(true);
    });
    fireEvent.click(primary("Continue"));
    expect(backend.named("set_onboarding_step")).toEqual([]);

    await releaseOne(pending);
    await waitFor(() => {
      expect(primary("Continue").disabled).toBe(false);
    });
    expect(backend.settings.share_usage_events).toBe(false);
  });

  it("a view that stayed behind learns the final value", async () => {
    const stored: boolean[] = [];
    const pending = holdWrites();
    render(
      <Privacy
        settings={backend.settings}
        onStored={(settings) => stored.push(settings.share_usage_events)}
      />,
    );
    fireEvent.click(screen.getByRole("switch", { name: "Share usage and error events" }));
    await releaseOne(pending);
    await waitFor(() => {
      expect(stored).toEqual([false]);
    });
  });
});

// U8 finding 1 and round 2, moved from Welcome with the switch (U10 criterion 5):
// Off on Ready, right before Start dictating, can't be overtaken.
describe("Ready's switch before Start dictating", () => {
  beforeEach(() => {
    backend.allSet("ready");
    backend.tryOutcome = "passed";
  });

  /** Holds `set_share_usage_events` until `finish` is called. */
  function holdTheWrite(fail = false) {
    const held: { finish: () => void } = { finish: () => undefined };
    const original = backend.handle;
    mockIPC((command, payload) => {
      if (command !== "set_share_usage_events") return original(command, payload);
      return new Promise((resolve, reject) => {
        held.finish = () => {
          if (fail) {
            // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors
            reject("Couldn't write the settings file.");
            return;
          }
          resolve(original(command, payload));
        };
      });
    });
    return held;
  }

  it("Start dictating can't overtake an opt-out still being written", async () => {
    const held = holdTheWrite();
    show();
    fireEvent.click(await screen.findByRole("switch", { name: "Share usage and error events" }));
    await waitFor(() => {
      expect(primary("Start dictating").disabled).toBe(true);
    });
    expect(screen.getByText(SAVING_REASON)).toBeTruthy();
    fireEvent.click(primary("Start dictating"));
    expect(backend.named("finish_onboarding")).toEqual([]);

    act(() => {
      held.finish();
    });
    await waitFor(() => {
      expect(primary("Start dictating").disabled).toBe(false);
    });
    expect(backend.settings.share_usage_events).toBe(false);
    fireEvent.click(primary("Start dictating"));
    await waitFor(() => {
      expect(backend.named("finish_onboarding")).toHaveLength(1);
    });
  });

  /**
   * Holds every sharing write, applying each only when it is released, as the
   * app's blocking pool may: nothing orders commands sent one after another.
   */
  function holdEveryWrite() {
    const pending: (() => void)[] = [];
    const original = backend.handle;
    mockIPC((command, payload) => {
      if (command !== "set_share_usage_events") return original(command, payload);
      return new Promise((resolve) => {
        pending.push(() => {
          resolve(original(command, payload));
        });
      });
    });
    return pending;
  }

  /** Releases the newest write first, until none is left. */
  async function releaseNewestFirst(pending: (() => void)[]) {
    for (let round = 0; round < 8; round += 1) {
      await act(async () => {
        pending.pop()?.();
        await new Promise((resolve) => setTimeout(resolve, 20));
      });
    }
  }

  it("round 2: a slow on can't land after the final off", async () => {
    backend.settings = { ...backend.settings, share_usage_events: false };
    const pending = holdEveryWrite();
    show();
    const share = await screen.findByRole<HTMLInputElement>("switch", {
      name: "Share usage and error events",
    });
    await waitFor(() => {
      expect(share.checked).toBe(false);
    });
    fireEvent.click(share); // on
    fireEvent.click(share); // off, the user's last word
    expect(share.checked).toBe(false);
    await waitFor(() => {
      expect(primary("Start dictating").disabled).toBe(true);
    });
    // With overlapping writes, this applies off and then on.
    await releaseNewestFirst(pending);
    await waitFor(() => {
      expect(primary("Start dictating").disabled).toBe(false);
    });
    expect(backend.settings.share_usage_events).toBe(false);
    expect(share.checked).toBe(false);
  });

  it("round 2: on, off, on ends on, and Start dictating waits for all of it", async () => {
    const pending = holdEveryWrite();
    show();
    const share = await screen.findByRole<HTMLInputElement>("switch", {
      name: "Share usage and error events",
    });
    fireEvent.click(share); // off
    fireEvent.click(share); // on
    fireEvent.click(share); // off
    fireEvent.click(share); // on
    expect(pending).toHaveLength(1);
    expect(primary("Start dictating").disabled).toBe(true);
    await act(async () => {
      pending.pop()?.();
      await new Promise((resolve) => setTimeout(resolve, 20));
    });
    // The first write done: only the last choice is sent next, and Start dictating waits for it.
    expect(pending).toHaveLength(1);
    expect(primary("Start dictating").disabled).toBe(true);
    await releaseNewestFirst(pending);
    await waitFor(() => {
      expect(primary("Start dictating").disabled).toBe(false);
    });
    expect(backend.settings.share_usage_events).toBe(true);
    expect(backend.named("set_share_usage_events").map((call) => call.args.share)).toEqual([
      false,
      true,
    ]);
  });

  it("stays locked after a failed opt-out, and the switch shows what is saved", async () => {
    const held = holdTheWrite(true);
    show();
    const share = await screen.findByRole<HTMLInputElement>("switch", {
      name: "Share usage and error events",
    });
    fireEvent.click(share);
    await waitFor(() => {
      expect(primary("Start dictating").disabled).toBe(true);
    });
    act(() => {
      held.finish();
    });
    await screen.findByText(FINISH_REASON);
    expect(primary("Start dictating").disabled).toBe(true);
    expect(share.checked).toBe(true);
    fireEvent.click(primary("Start dictating"));
    expect(backend.named("finish_onboarding")).toEqual([]);
  });
});

describe("Speech", () => {
  beforeEach(() => {
    backend.step = "speech";
    backend.welcomePassed = true;
  });

  it("finding 3: the own key choice survives a quit, with no key saved and none kept", async () => {
    const { unmount } = show();
    await screen.findByText("Parakeet English · 477 MB");
    fireEvent.click(screen.getByRole("radio", { name: /Use my own Groq or OpenAI key/ }));
    await waitFor(() => {
      expect(backend.speechChoice).toBe("own_key");
    });
    fireEvent.change(await screen.findByLabelText("Groq API key"), { target: { value: KEY } });
    unmount();

    // The relaunch: the app's status has the choice, and the typed key is gone.
    show();
    const field = await screen.findByLabelText<HTMLInputElement>("Groq API key");
    expect(screen.getByRole<HTMLInputElement>("radio", { name: /own Groq/ }).checked).toBe(true);
    expect(field.value).toBe("");
    expect(JSON.stringify(backend.named("set_speech_choice"))).not.toContain(KEY);
  });

  it("finding 3: choosing local survives a quit, even with a cloud key saved", async () => {
    backend.keys.groq = true;
    const { unmount } = show();
    await waitFor(() => {
      expect(screen.getByRole<HTMLInputElement>("radio", { name: /own Groq/ }).checked).toBe(true);
    });
    fireEvent.click(screen.getByRole("radio", { name: "Recommended for your language" }));
    await waitFor(() => {
      expect(backend.speechChoice).toBe("local");
    });
    unmount();
    show();
    await screen.findByText("Parakeet English · 477 MB");
    await waitFor(() => {
      expect(backend.named("has_api_key").length).toBeGreaterThan(1);
    });
    expect(
      screen.getByRole<HTMLInputElement>("radio", { name: "Recommended for your language" })
        .checked,
    ).toBe(true);
  });

  it("criterion 3: English preselects Parakeet English, with its real size", async () => {
    show();
    const name = await screen.findByText("Parakeet English · 477 MB");
    expect(name).toBeTruthy();
    expect(screen.getByText("Recommended for your language")).toBeTruthy();
    expect(screen.getByText(MODEL_COPY)).toBeTruthy();
    expect(primary("Download and continue").disabled).toBe(false);
  });

  it("criterion 3: Hindi recommends the multilingual model, with its real size", async () => {
    show();
    await screen.findByText("Parakeet English · 477 MB");
    fireEvent.click(screen.getByRole("radio", { name: "Hindi" }));
    await screen.findByText("Qwen3-ASR Multilingual · 590 MB");
    await waitFor(() => {
      expect(backend.settings.language).toBe("hi");
    });
  });

  it("criterion 3: More… offers the searchable picker", async () => {
    show();
    await screen.findByText("Parakeet English · 477 MB");
    expect(screen.queryByRole("combobox")).toBeNull();
    fireEvent.click(screen.getByRole("radio", { name: "More…" }));
    expect(screen.getByRole("combobox")).toBeTruthy();
  });

  it("criterion 3: Download and continue chooses, starts the download and moves on", async () => {
    show();
    await screen.findByText("Parakeet English · 477 MB");
    fireEvent.click(primary("Download and continue"));
    await screen.findByText("Two permissions");
    expect(backend.named("choose_local_model")[0]?.args).toEqual({ id: PARAKEET.id });
    expect(backend.named("download_local_model")[0]?.args).toEqual({ id: PARAKEET.id });
    expect(backend.settings.transcription).toBe("local");
    expect(backend.step).toBe("permissions");
  });

  it("criterion 3: a model already here says Continue and downloads nothing", async () => {
    backend.model(PARAKEET.id).state = "ready";
    show();
    await screen.findByText("Parakeet English · 477 MB");
    fireEvent.click(primary("Continue"));
    await screen.findByText("Two permissions");
    expect(backend.named("download_local_model")).toEqual([]);
    expect(backend.settings.transcription).toBe("local");
  });

  it("stays on the step and says why when the download can't start", async () => {
    backend.handle = ((original) => (command: string, payload: unknown) => {
      if (command === "choose_local_model") {
        // eslint-disable-next-line @typescript-eslint/only-throw-error
        throw "The disk is full.";
      }
      return original(command, payload);
    })(backend.handle);
    mockIPC(backend.handle);
    show();
    await screen.findByText("Parakeet English · 477 MB");
    fireEvent.click(primary("Download and continue"));
    await screen.findByText("The disk is full.");
    expect(backend.step).toBe("speech");
  });

  async function chooseOwnKey() {
    show();
    await screen.findByText("Parakeet English · 477 MB");
    fireEvent.click(screen.getByRole("radio", { name: /Use my own Groq or OpenAI key/ }));
    return await screen.findByLabelText("Groq API key");
  }

  it("criterion 3: the own key path opens U6's panel, and Continue waits for Test & Save", async () => {
    const field = await chooseOwnKey();
    await screen.findByText("No key saved yet.", { exact: false });
    expect(primary("Continue").disabled).toBe(true);
    expect(screen.getByText(KEY_REASON)).toBeTruthy();
    expect(field.getAttribute("type")).toBe("password");
  });

  it("criterion 3: a bad key keeps Continue locked, and saves nothing", async () => {
    backend.testResult = { kind: "invalid_key", message: "Groq rejected this API key." };
    const field = await chooseOwnKey();
    fireEvent.change(field, { target: { value: KEY } });
    fireEvent.click(screen.getByRole("button", { name: "Test & Save" }));
    await screen.findByText("Groq rejected this API key.");
    expect(backend.named("set_api_key")).toEqual([]);
    expect(primary("Continue").disabled).toBe(true);
  });

  it("criterion 3: a good key unlocks Continue, and the provider is the engine", async () => {
    const field = await chooseOwnKey();
    fireEvent.change(field, { target: { value: KEY } });
    fireEvent.click(screen.getByRole("button", { name: "Test & Save" }));
    await screen.findByText("✓ Connected · saved in Keychain");
    expect(primary("Continue").disabled).toBe(false);
    fireEvent.click(primary("Continue"));
    await screen.findByText("Two permissions");
    expect(backend.named("use_cloud_engine")[0]?.args).toEqual({ provider: "groq" });
    expect(backend.named("download_local_model")).toEqual([]);
    expect(document.body.textContent).not.toContain(KEY);
  });

  it("criterion 3: a key saved earlier is tested before moving on", async () => {
    backend.keys.groq = true;
    backend.testResult = { kind: "invalid_key", message: "Groq rejected this API key." };
    await chooseOwnKey();
    await waitFor(() => {
      expect(primary("Continue").disabled).toBe(false);
    });
    fireEvent.click(primary("Continue"));
    await screen.findByText("Groq rejected this API key.");
    expect(backend.step).toBe("speech");
    expect(backend.named("test_api_key")[0]?.args).toEqual({ provider: "groq", key: null });
  });
});

describe("the download strip", () => {
  function downloading() {
    backend.step = "permissions";
    backend.welcomePassed = true;
    backend.settings = { ...backend.settings, transcription: "local" };
    const model = backend.model(PARAKEET.id);
    model.state = "downloading";
    model.downloaded_bytes = 212_000_000;
  }

  it("criterion 3: shows real progress and Cancel on every later step", async () => {
    for (const step of ["permissions", "try", "ready"] as const) {
      backend = new Backend();
      mockIPC(backend.handle);
      downloading();
      backend.step = step;
      backend.microphone = "granted";
      backend.accessibility = "granted";
      const { unmount } = show();
      await screen.findByText("Downloading Parakeet English · 212 of 477 MB");
      expect(screen.getByRole("button", { name: "Cancel" })).toBeTruthy();
      expect(screen.getByText("Download running")).toBeTruthy();
      unmount();
    }
  });

  it("Cancel stops it, keeps that it was the user's choice, and offers Resume", async () => {
    downloading();
    show();
    await screen.findByText("Downloading Parakeet English · 212 of 477 MB");
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await screen.findByText("Download paused · 212 of 477 MB");
    expect(backend.downloadPaused).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Resume" }));
    await screen.findByText("Downloading Parakeet English · 212 of 477 MB");
    expect(backend.downloadPaused).toBe(false);
  });

  it("a failed download offers Retry and the own key", async () => {
    downloading();
    const model = backend.model(PARAKEET.id);
    model.state = "partial";
    model.error = "The disk is full.";
    show();
    await screen.findByText(/stopped: The disk is full\./);
    expect(screen.getByRole("button", { name: "Retry" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Use my own key" }));
    await screen.findByText("Which language will you mostly speak?");
  });

  it("criterion 7: a part left by a quit picks up by itself, and says so", async () => {
    downloading();
    const model = backend.model(PARAKEET.id);
    model.state = "partial";
    show();
    await waitFor(() => {
      expect(backend.named("download_local_model")).toHaveLength(1);
    });
    await screen.findByText("Continued from 212 MB.");
  });

  it("criterion 7: a restart stays a restart, also once it passes the old part's size", async () => {
    downloading();
    backend.ignoresRange = true;
    const model = backend.model(PARAKEET.id);
    model.state = "partial";
    show();
    const said = "The earlier download couldn't continue, so it started again.";
    await screen.findByText(said);
    model.downloaded_bytes = 400_000_000;
    refocus();
    await screen.findByText("Downloading Parakeet English · 400 of 477 MB", undefined, {
      timeout: 2000,
    });
    expect(screen.getByText(said)).toBeTruthy();
    expect(screen.queryByText(/Continued from/)).toBeNull();
  });

  it("criterion 7: a download the user cancelled doesn't start again by itself", async () => {
    downloading();
    const model = backend.model(PARAKEET.id);
    model.state = "partial";
    backend.downloadPaused = true;
    show();
    await screen.findByText("Download paused · 212 of 477 MB");
    expect(backend.named("download_local_model")).toEqual([]);
  });
});

describe("Permissions", () => {
  beforeEach(() => {
    backend.step = "permissions";
    backend.welcomePassed = true;
    backend.keys.groq = true;
  });

  it("criterion 4: explains both before anything is asked", async () => {
    show();
    await screen.findByText("Records only while you hold your key.");
    expect(
      screen.getByText("Notices your key and pastes at your cursor. It doesn't read your screen."),
    ).toBeTruthy();
    expect(backend.named("request_permission")).toEqual([]);
    expect(primary("Continue").disabled).toBe(true);
    expect(screen.getByText(BOTH_REASON)).toBeTruthy();
  });

  it("criterion 4: Allow asks, and a grant is detected without pressing anything", async () => {
    backend.micAnswer = "not_determined";
    show();
    fireEvent.click(await screen.findByRole("button", { name: "Allow Microphone" }));
    await screen.findByText("Waiting for macOS…");
    backend.microphone = "granted";
    refocus();
    await screen.findByText("✓ Allowed");
    expect(backend.step).toBe("permissions");
  });

  it("criterion 4: Continue unlocks only when both are allowed", async () => {
    backend.microphone = "granted";
    show();
    await screen.findByText("✓ Allowed");
    expect(primary("Continue").disabled).toBe(true);
    backend.accessibility = "granted";
    refocus();
    await waitFor(() => {
      expect(primary("Continue").disabled).toBe(false);
    });
    fireEvent.click(primary("Continue"));
    await screen.findByText("Your dictation key");
  });

  it("criterion 4: the mic card shows the live meter and input name after the grant only", async () => {
    show();
    await screen.findByRole("button", { name: "Allow Microphone" });
    expect(document.querySelectorAll('[data-testid="mic-bar"]')).toHaveLength(0);
    backend.microphone = "granted";
    refocus();
    await screen.findByText("MacBook Microphone: say something, the bars should move.", undefined, {
      timeout: 2000,
    });
    expect(screen.getByText("No sound? Check System Settings → Sound.")).toBeTruthy();
    expect(document.querySelectorAll('[data-testid="mic-bar"]').length).toBeGreaterThan(0);
    await waitFor(() => {
      expect(backend.named("read_microphone_test").length).toBeGreaterThan(0);
    });
  });

  it("criterion 4: leaving the step stops the meter", async () => {
    backend.microphone = "granted";
    backend.accessibility = "granted";
    show();
    await waitFor(() => {
      expect(backend.named("start_microphone_test").length).toBeGreaterThan(0);
    });
    fireEvent.click(primary("Back"));
    await screen.findByText("Which language will you mostly speak?");
    expect(backend.named("stop_microphone_test").length).toBeGreaterThan(0);
  });

  it("criterion 4: denied gives Open System Settings, the exact path, and the relaunch", async () => {
    backend.microphone = "denied";
    show();
    await screen.findByText("System Settings → Privacy & Security → Microphone → turn on Vocet.");
    fireEvent.click(screen.getByRole("button", { name: "Open System Settings" }));
    await waitFor(() => {
      expect(backend.named("request_permission")[0]?.args).toEqual({ permission: "microphone" });
    });
    fireEvent.click(screen.getByRole("button", { name: "Relaunch and resume" }));
    await waitFor(() => {
      expect(backend.named("relaunch_app")).toHaveLength(1);
    });
  });

  it("criterion 4: granted but not applied offers Relaunch only once it has lasted", async () => {
    backend.microphone = "granted";
    backend.accessibility = "granted";
    backend.needsRelaunch = true;
    show();
    await screen.findByText("Two permissions");
    expect(screen.queryByRole("button", { name: "Relaunch and resume" })).toBeNull();
    for (let i = 0; i < RELAUNCH_AFTER_POLLS + 1; i += 1) {
      refocus();
      await act(async () => {
        await Promise.resolve();
      });
    }
    await screen.findByRole("button", { name: "Relaunch and resume" });
  });
});

describe("Try it", () => {
  beforeEach(() => {
    backend.allSet("try");
  });

  async function stepOpen() {
    show();
    await screen.findByText("Your dictation key");
    await waitFor(() => {
      expect(backend.named("dictation_progress").length).toBeGreaterThan(0);
    });
    return screen.getByLabelText<HTMLTextAreaElement>(
      "Click the box, hold the key, say a sentence, let go.",
    );
  }

  function stage(name: string) {
    return screen.getByText(name, { exact: false }).closest("li")?.className ?? "";
  }

  function report(update: Partial<DictationProgressView>) {
    backend.progress = { ...backend.progress, ...update };
  }

  it("criterion 5: typing in the box never advances anything", async () => {
    const box = await stepOpen();
    fireEvent.change(box, { target: { value: WORDS } });
    backend.words = WORDS;
    await new Promise((resolve) => setTimeout(resolve, 500));
    for (const name of ["Heard you", "Transcribed", "Pasted"]) {
      expect(stage(name)).not.toContain("done");
    }
    expect(primary("Continue").disabled).toBe(true);
    expect(screen.getByText(CONTINUE_REASON)).toBeTruthy();
  });

  it("criterion 5: each stage needs its own real event", async () => {
    const box = await stepOpen();
    report({ started: 1, stages: ["recorded"] });
    await waitFor(() => {
      expect(backend.named("dictation_progress").length).toBeGreaterThan(1);
    });
    await new Promise((resolve) => setTimeout(resolve, 450));
    expect(stage("Heard you")).not.toContain("done");

    report({ heard: true });
    await waitFor(() => {
      expect(stage("Heard you")).toContain("done");
    });
    expect(stage("Transcribed")).not.toContain("done");

    report({ stages: ["recorded", "transcribed"] });
    await waitFor(() => {
      expect(stage("Transcribed")).toContain("done");
    });
    expect(stage("Pasted")).not.toContain("done");

    // The paste landed in the box.
    backend.words = WORDS;
    fireEvent.change(box, { target: { value: WORDS } });
    report({ finished: 1, completed: 1, stages: ["recorded", "transcribed", "pasted"] });
    await waitFor(() => {
      expect(stage("Pasted")).toContain("done");
    });
    await waitFor(() => {
      expect(primary("Continue").disabled).toBe(false);
    });
    expect(backend.tryOutcome).toBe("passed");
    expect(screen.queryByRole("button", { name: "Skip for now" })).toBeNull();
  });

  it("criterion 5: a dictation from before the step opened doesn't count", async () => {
    backend.progress = {
      ...NO_PROGRESS,
      started: 3,
      finished: 3,
      completed: 3,
      heard: true,
      stages: ["recorded", "transcribed", "pasted"],
    };
    await stepOpen();
    await new Promise((resolve) => setTimeout(resolve, 450));
    expect(stage("Heard you")).not.toContain("done");
    expect(primary("Continue").disabled).toBe(true);
  });

  it("criterion 5: while the model downloads, practice is blocked with progress", async () => {
    const model = backend.model(PARAKEET.id);
    model.state = "downloading";
    model.downloaded_bytes = 477_000_000 * 0.82;
    const box = await stepOpen();
    expect(box.disabled).toBe(true);
    await screen.findByText("Getting the model ready: 82%");
    await waitFor(() => {
      expect(backend.named("set_hotkey_recording")[0]?.args).toEqual({ recording: true });
    });
  });

  it("criterion 5: the key comes back when the model is ready", async () => {
    const model = backend.model(PARAKEET.id);
    model.state = "downloading";
    const box = await stepOpen();
    await waitFor(() => {
      expect(box.disabled).toBe(true);
    });
    model.state = "ready";
    await waitFor(
      () => {
        expect(box.disabled).toBe(false);
      },
      { timeout: 2500 },
    );
    expect(backend.named("set_hotkey_recording").at(-1)?.args).toEqual({ recording: false });
  });

  it("criterion 5: a paste that missed shows the words with Copy, and never pastes again", async () => {
    await stepOpen();
    backend.words = WORDS;
    report({
      started: 1,
      finished: 1,
      completed: 1,
      heard: true,
      stages: ["recorded", "transcribed", "pasted"],
    });
    await screen.findByText(WORDS, undefined, { timeout: 2500 });
    expect(stage("Pasted")).not.toContain("done");
    expect(primary("Continue").disabled).toBe(true);
    expect(screen.getByText(/Click the box and press ⌘V/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Copy" }));
    await screen.findByText("Copied.");
    expect(backend.named("copy_practice_words")).toHaveLength(1);
  });

  it("U10 criterion 4: a missed paste tells the app once, with nothing from the box", async () => {
    await stepOpen();
    backend.words = WORDS;
    report({
      started: 1,
      finished: 1,
      completed: 1,
      heard: true,
      stages: ["recorded", "transcribed", "pasted"],
    });
    await screen.findByText(WORDS, undefined, { timeout: 2500 });
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 900));
    });
    expect(backend.named("practice_paste_missed")).toEqual([
      { command: "practice_paste_missed", args: {} },
    ]);
  });

  it("U10 criterion 4: a paste that landed isn't a missed one", async () => {
    await stepOpen();
    backend.words = WORDS;
    fireEvent.change(screen.getByRole("textbox"), { target: { value: WORDS } });
    report({
      started: 1,
      finished: 1,
      completed: 1,
      heard: true,
      stages: ["recorded", "transcribed", "pasted"],
    });
    await screen.findByText(/It works/, undefined, { timeout: 2500 });
    expect(backend.named("practice_paste_missed")).toEqual([]);
  });

  it("Design Lead: the title is Try it, with the key as a small label", async () => {
    await stepOpen();
    expect(screen.getByRole("heading", { level: 1 }).textContent).toBe("Try it");
    expect(screen.getByText("Your dictation key").tagName).toBe("SPAN");
  });

  it("Design Lead: the panel of a missed paste is scrolled into view, also once the words arrive", async () => {
    const scrolled = vi
      .spyOn(Element.prototype, "scrollIntoView")
      .mockImplementation(() => undefined);
    try {
      await stepOpen();
      expect(document.querySelector(".ob-fix-area")).toBeNull();
      backend.words = WORDS;
      report({
        started: 1,
        finished: 1,
        errors: 1,
        heard: true,
        stages: ["recorded", "transcribed"],
        failure: "paste",
        error: "Vocet couldn't paste.",
      });
      await screen.findByText(WORDS, undefined, { timeout: 2500 });
      const area = document.querySelector(".ob-fix-area");
      expect(area).not.toBeNull();
      await waitFor(() => {
        expect(scrolled.mock.contexts).toContain(area);
      });
      // Its last line, the instruction, is inside what was scrolled to.
      expect(area?.textContent).toContain("Click the box and press ⌘V");
    } finally {
      scrolled.mockRestore();
    }
  });

  it("criterion 5: a failed paste shows the words at once", async () => {
    await stepOpen();
    backend.words = WORDS;
    report({
      started: 1,
      finished: 1,
      errors: 1,
      heard: true,
      stages: ["recorded", "transcribed"],
      failure: "paste",
      error: "Vocet couldn't paste.",
    });
    await screen.findByText(WORDS, undefined, { timeout: 2500 });
    expect(primary("Continue").disabled).toBe(true);
  });

  it("no sound gives the microphone help", async () => {
    await stepOpen();
    report({ started: 1, finished: 1, heard: false, stages: ["recorded"] });
    await screen.findByText(
      "Vocet didn't hear anything. Check System Settings → Sound.",
      undefined,
      {
        timeout: 2000,
      },
    );
  });

  it("a rejected key goes back to Speech", async () => {
    await stepOpen();
    report({
      started: 1,
      finished: 1,
      errors: 1,
      heard: true,
      stages: ["recorded"],
      failure: "key",
      error: "Groq rejected the key.",
    });
    await screen.findByText("Groq rejected the key.", undefined, { timeout: 2000 });
    fireEvent.click(screen.getByRole("button", { name: "Back to Speech" }));
    await screen.findByText("Which language will you mostly speak?");
  });

  it("criterion 5: Skip for now leads to Ready with Not tested yet", async () => {
    await stepOpen();
    fireEvent.click(screen.getByRole("button", { name: "Skip for now" }));
    await screen.findByText("You're set");
    expect(backend.tryOutcome).toBe("skipped");
    expect(screen.getByText(NOT_TESTED)).toBeTruthy();
  });

  it("shows the dictation key and records a different one", async () => {
    await stepOpen();
    await waitFor(() => {
      expect(screen.getByLabelText("Dictation key").textContent).toBe("Right ⌥");
    });
    fireEvent.click(screen.getByRole("button", { name: "Record a different key" }));
    keyDown("MetaRight");
    keyUp("MetaRight");
    await waitFor(() => {
      expect(backend.settings.hotkey).toBe("RightCmd");
    });
    releaseAllKeys();
  });
});

describe("Ready", () => {
  beforeEach(() => {
    backend.allSet("ready");
    backend.tryOutcome = "passed";
  });

  it("criterion 6: shows the real states", async () => {
    show();
    await screen.findByText("You're set");
    await screen.findByText("Right ⌥ (hold)");
    await screen.findByText("On this Mac · Parakeet English · ✓ Ready");
    expect(screen.getByText("Find Vocet in the menu bar", { exact: false })).toBeTruthy();
    expect(screen.queryByText(NOT_TESTED)).toBeNull();
    expect(
      screen.getByRole<HTMLInputElement>("switch", { name: "Open Vocet when I log in" }).checked,
    ).toBe(false);
  });

  it("criterion 6: while the model isn't ready it doesn't say Ready", async () => {
    const model = backend.model(PARAKEET.id);
    model.state = "downloading";
    model.downloaded_bytes = 477_000_000 / 2;
    show();
    await screen.findByText("On this Mac · Parakeet English · Getting the model ready: 50%");
  });

  it("criterion 6: the own key says Connected only with a key saved", async () => {
    backend.settings = { ...backend.settings, transcription: "cloud", provider: "groq" };
    backend.keys.groq = true;
    show();
    await screen.findByText("Groq · ✓ Connected");
  });

  it("the events row is a switch that shows the real state and applies at once", async () => {
    backend.settings = { ...backend.settings, share_usage_events: false };
    show();
    const share = await screen.findByRole<HTMLInputElement>("switch", {
      name: "Share usage and error events",
    });
    await waitFor(() => {
      expect(share.checked).toBe(false);
    });
    fireEvent.click(share);
    await waitFor(() => {
      expect(backend.settings.share_usage_events).toBe(true);
    });
    expect(backend.named("set_share_usage_events")).toEqual([
      { command: "set_share_usage_events", args: { share: true } },
    ]);
    expect(backend.step).toBe("ready");
    expect(share.checked).toBe(true);
    expect(screen.queryByRole("button", { name: "Change" })?.closest("div")?.textContent).toContain(
      "History",
    );
  });

  it("nothing leaves Ready while the switch is saving, and the last choice wins", async () => {
    const pending: (() => void)[] = [];
    const original = backend.handle;
    mockIPC((command, payload) => {
      if (command !== "set_share_usage_events") return original(command, payload);
      return new Promise((resolve) => {
        pending.push(() => {
          resolve(original(command, payload));
        });
      });
    });
    show();
    const share = await screen.findByRole<HTMLInputElement>("switch", {
      name: "Share usage and error events",
    });
    fireEvent.click(share); // off
    fireEvent.click(share); // on
    fireEvent.click(share); // off
    expect(pending).toHaveLength(1);
    await waitFor(() => {
      expect(primary("Start dictating").disabled).toBe(true);
    });
    expect(primary("Back").disabled).toBe(true);
    for (let round = 0; round < 4; round += 1) {
      await act(async () => {
        pending.pop()?.();
        await new Promise((resolve) => setTimeout(resolve, 20));
      });
    }
    await waitFor(() => {
      expect(primary("Start dictating").disabled).toBe(false);
    });
    expect(backend.settings.share_usage_events).toBe(false);
    expect(share.checked).toBe(false);
  });

  // Round 4: every way out, tried while the writes are pending, between them,
  // and after them. Each of these commands is what can send an event.
  it.each([
    { exit: "Start dictating", command: "finish_onboarding" },
    { exit: "Back", command: "set_onboarding_step" },
    { exit: "History's Change", command: "leave_setup_for_settings" },
    { exit: "the strip's Use my own key", command: "set_onboarding_step" },
  ])("$exit waits for the sharing writes", async ({ exit, command }) => {
    // A paused model, so the strip offers its way out too.
    const model = backend.model(PARAKEET.id);
    model.state = "partial";
    model.downloaded_bytes = 212_000_000;
    backend.downloadPaused = true;
    const pending: (() => void)[] = [];
    const original = backend.handle;
    mockIPC((name, payload) => {
      if (name !== "set_share_usage_events") return original(name, payload);
      return new Promise((resolve) => {
        pending.push(() => {
          resolve(original(name, payload));
        });
      });
    });
    show();
    const share = await screen.findByRole<HTMLInputElement>("switch", {
      name: "Share usage and error events",
    });
    await screen.findByRole("button", { name: "Use my own key" });
    const pressExit = () => {
      const name =
        exit === "History's Change"
          ? "Change"
          : exit === "the strip's Use my own key"
            ? "Use my own key"
            : exit;
      fireEvent.click(screen.getByRole("button", { name }));
    };
    const exitCalls = () => backend.named(command);

    fireEvent.click(share); // off
    fireEvent.click(share); // on
    fireEvent.click(share); // off, the user's last word
    pressExit();
    expect(exitCalls()).toEqual([]);

    // Between the writes: the first is done, the last still pending.
    await act(async () => {
      pending.pop()?.();
      await new Promise((resolve) => setTimeout(resolve, 20));
    });
    expect(pending).toHaveLength(1);
    pressExit();
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 20));
    });
    expect(exitCalls()).toEqual([]);
    expect(backend.step).toBe("ready");

    await act(async () => {
      pending.pop()?.();
      await new Promise((resolve) => setTimeout(resolve, 20));
    });
    await waitFor(() => {
      expect(primary("Start dictating").disabled).toBe(false);
    });
    expect(backend.settings.share_usage_events).toBe(false);
    expect(exitCalls()).toEqual([]);

    // Now it works, and only after the final choice is stored.
    pressExit();
    await waitFor(() => {
      expect(exitCalls()).toHaveLength(1);
    });
    const last = backend.calls.map((call) => call.command);
    expect(last.lastIndexOf("set_share_usage_events")).toBeLessThan(last.lastIndexOf(command));
  });

  it("a failed save holds every way out", async () => {
    const model = backend.model(PARAKEET.id);
    model.state = "partial";
    backend.downloadPaused = true;
    const original = backend.handle;
    mockIPC((name, payload) => {
      // eslint-disable-next-line @typescript-eslint/only-throw-error
      if (name === "set_share_usage_events") throw "Couldn't write the settings file.";
      return original(name, payload);
    });
    show();
    fireEvent.click(await screen.findByRole("switch", { name: "Share usage and error events" }));
    await screen.findByText(FINISH_REASON);
    for (const name of ["Start dictating", "Back", "Change", "Use my own key"]) {
      fireEvent.click(screen.getByRole("button", { name }));
    }
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 20));
    });
    for (const command of [
      "finish_onboarding",
      "set_onboarding_step",
      "leave_setup_for_settings",
    ]) {
      expect(backend.named(command)).toEqual([]);
    }
  });

  it("stays on Ready after a failed opt-out, and says why", async () => {
    const original = backend.handle;
    mockIPC((command, payload) => {
      // eslint-disable-next-line @typescript-eslint/only-throw-error
      if (command === "set_share_usage_events") throw "Couldn't write the settings file.";
      return original(command, payload);
    });
    show();
    fireEvent.click(await screen.findByRole("switch", { name: "Share usage and error events" }));
    await screen.findByText(FINISH_REASON);
    expect(primary("Start dictating").disabled).toBe(true);
    fireEvent.click(primary("Start dictating"));
    expect(backend.named("finish_onboarding")).toEqual([]);
  });

  it("criterion 6: shows the real history limit, and Change opens Settings → History", async () => {
    backend.settings = { ...backend.settings, history_limit: 50 };
    render(<App />);
    await screen.findByText(/Keeps your last 50 dictations on this Mac/);
    const row = screen.getByText("History").closest("div");
    fireEvent.click(within(row as HTMLElement).getByRole("button", { name: "Change" }));
    await waitFor(() => {
      expect(backend.named("leave_setup_for_settings")).toHaveLength(1);
    });
    await waitFor(() => {
      expect(screen.queryByRole("navigation", { name: "Setup steps" })).toBeNull();
    });
  });

  it("criterion 6: says when history is off", async () => {
    backend.settings = { ...backend.settings, history_limit: 0 };
    show();
    await screen.findByText(/^Off/);
  });

  it("criterion 6: the login switch goes through S1's command", async () => {
    show();
    fireEvent.click(await screen.findByRole("switch", { name: "Open Vocet when I log in" }));
    await waitFor(() => {
      expect(backend.settings.launch_at_login).toBe(true);
    });
  });

  it("criterion 6: Start dictating finishes setup and leaves onboarding", async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "Start dictating" }));
    await waitFor(() => {
      expect(backend.named("finish_onboarding")).toHaveLength(1);
    });
    await waitFor(() => {
      expect(screen.queryByRole("navigation", { name: "Setup steps" })).toBeNull();
    });
  });
});

describe("the window", () => {
  it("criterion 7: resumes at the step the app kept", async () => {
    backend.allSet("permissions");
    show();
    await screen.findByText("Two permissions");
  });

  it("says when the setup can't load", async () => {
    mockIPC((command) => {
      if (command === "get_onboarding") {
        // eslint-disable-next-line @typescript-eslint/only-throw-error
        throw "No state.";
      }
      return null;
    });
    show();
    await screen.findByText(/Couldn't load the setup/);
  });

  it("shows the app's refusal when a step isn't done", async () => {
    // Both permissions, but no engine chosen: the app won't go to Try it.
    backend.step = "permissions";
    backend.welcomePassed = true;
    backend.microphone = "granted";
    backend.accessibility = "granted";
    show();
    await waitFor(() => {
      expect(primary("Continue").disabled).toBe(false);
    });
    fireEvent.click(primary("Continue"));
    await screen.findByText("Choose how speech becomes text first.");
    expect(backend.step).toBe("permissions");
  });
});
