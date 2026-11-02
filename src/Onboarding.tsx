import { useCallback, useEffect, useRef, useState } from "react";
import { commands } from "./bindings";
import type {
  AppPermission,
  OnboardingStatus,
  PermissionState,
  Provider,
  Settings_Serialize,
  Step,
} from "./bindings";
import { BROKEN, messageOf, testKey } from "./keyTest";
import type { Notice } from "./keyTest";
import { PROVIDERS, providerInfo } from "./providers";
import "./settings.css";
import { validateKey } from "./validation";

const STEPS: readonly Step[] = ["microphone", "accessibility", "key", "try"];

/** How often a permission step asks the app whether the user has answered. */
export const POLL_MS = 1000;

/** Where each provider hands out keys, for the key step's hint. */
const KEY_PAGES: Record<Provider, string> = {
  groq: "console.groq.com/keys",
  openai: "platform.openai.com/api-keys",
};

/** The hotkey as a person would name it. */
export function hotkeyName(hotkey: string): string {
  switch (hotkey) {
    case "RightAlt":
      return "the right Option key";
    case "LeftAlt":
      return "the left Option key";
    default:
      return hotkey;
  }
}

/** The permission a step waits for, if it is a permission step. */
function permissionOf(step: Step): AppPermission | null {
  return step === "microphone" || step === "accessibility" ? step : null;
}

function stateOf(status: OnboardingStatus, permission: AppPermission): PermissionState {
  return permission === "microphone" ? status.microphone : status.accessibility;
}

interface Props {
  /** Called once the user is done and the app has hidden the window. */
  onFinished: () => void;
}

/**
 * The first-run steps (M5.2): microphone, Accessibility, provider and key
 * (tested before it is saved), then a test dictation into a field here. The
 * app keeps the step and refuses to skip one that isn't done; this view
 * shows it, moves on by itself when a permission it waits for is granted,
 * and never shows a key back. The test dictation's text stays in the field.
 */
export function Onboarding({ onFinished }: Props) {
  const [status, setStatus] = useState<OnboardingStatus | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  // Why the last step change or permission request didn't work.
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [askedAccessibility, setAskedAccessibility] = useState(false);
  // The key step.
  const [settings, setSettings] = useState<Settings_Serialize | null>(null);
  const [provider, setProvider] = useState<Provider>("groq");
  const [keyInput, setKeyInput] = useState("");
  const [keySaved, setKeySaved] = useState(false);
  const [notice, setNotice] = useState<Notice | null>(null);
  const [testing, setTesting] = useState(false);
  // The test dictation lands here.
  const [tryText, setTryText] = useState("");
  const tryField = useRef<HTMLTextAreaElement>(null);
  // What the permission of the step on screen was when last seen, so the
  // step moves on only when it turns granted here, not when the user comes
  // back to a step that was already done.
  const seen = useRef<{ step: Step; state: PermissionState } | null>(null);

  const goTo = useCallback(async (step: Step) => {
    setBusy(true);
    try {
      const result = await commands.setOnboardingStep(step);
      if (result.status === "ok") {
        setStatus(result.data);
        setError(null);
        setNotice(null);
      } else {
        setError(messageOf(result.error));
      }
    } catch {
      setError(BROKEN);
    } finally {
      setBusy(false);
    }
  }, []);

  /**
   * Asks the app where onboarding is. If the permission the step on screen
   * waits for has just turned granted, moves on by itself; a step that was
   * already done when the user came back to it stays.
   */
  const refresh = useCallback(async () => {
    let next: OnboardingStatus;
    try {
      const result = await commands.getOnboarding();
      if (result.status === "error") {
        setLoadError(messageOf(result.error));
        return;
      }
      next = result.data;
    } catch {
      setLoadError(BROKEN);
      return;
    }
    setStatus(next);
    setLoadError(null);
    const permission = permissionOf(next.step);
    const before = seen.current;
    if (permission === null) {
      seen.current = null;
      return;
    }
    const state = stateOf(next, permission);
    seen.current = { step: next.step, state };
    if (state === "granted" && before?.step === next.step && before.state !== "granted") {
      const after = STEPS[STEPS.indexOf(next.step) + 1];
      if (after !== undefined) await goTo(after);
    }
  }, [goTo]);

  // Load, and ask again whenever the window comes back.
  useEffect(() => {
    const first = setTimeout(() => void refresh(), 0);
    const onShow = () => {
      if (document.visibilityState !== "hidden") void refresh();
    };
    window.addEventListener("focus", onShow);
    document.addEventListener("visibilitychange", onShow);
    return () => {
      clearTimeout(first);
      window.removeEventListener("focus", onShow);
      document.removeEventListener("visibilitychange", onShow);
    };
  }, [refresh]);

  const step = status?.step ?? null;
  const permission = step === null ? null : permissionOf(step);
  const waiting =
    status !== null && permission !== null && stateOf(status, permission) !== "granted";

  // While a permission step waits, ask the app every second: the answer
  // comes from a system prompt or System Settings, outside this window.
  useEffect(() => {
    if (!waiting) return;
    const timer = window.setInterval(() => {
      if (document.visibilityState !== "hidden") void refresh();
    }, POLL_MS);
    return () => {
      window.clearInterval(timer);
    };
  }, [waiting, refresh]);

  // The key step needs the settings (provider, hotkey) and whether a key is
  // saved; the last step needs the hotkey's name.
  useEffect(() => {
    if (step !== "key" && step !== "try") return;
    // An object, not a `let`, so the checks after each `await` see changes.
    const state = { cancelled: false };
    const current = () => !state.cancelled;
    void (async () => {
      try {
        const result = await commands.getSettings();
        if (!current() || result.status === "error") return;
        setSettings(result.data);
        setProvider(result.data.provider);
        const saved = await commands.hasApiKey(result.data.provider);
        if (current()) setKeySaved(saved.status === "ok" && saved.data);
      } catch {
        // The key step still works: Test asks for a key.
      }
    })();
    return () => {
      state.cancelled = true;
    };
  }, [step]);

  // The paste goes wherever the focus is: put it in the field.
  useEffect(() => {
    if (step === "try") tryField.current?.focus();
  }, [step]);

  async function ask(which: AppPermission) {
    setBusy(true);
    setError(null);
    try {
      const result = await commands.requestPermission(which);
      if (result.status === "ok") {
        setStatus(result.data);
        if (which === "accessibility") setAskedAccessibility(true);
      } else {
        setError(messageOf(result.error));
      }
    } catch {
      setError(BROKEN);
    } finally {
      setBusy(false);
    }
  }

  async function relaunch() {
    setBusy(true);
    setError(null);
    try {
      const result = await commands.relaunchApp();
      if (result.status === "error") setError(messageOf(result.error));
    } catch {
      setError(BROKEN);
    } finally {
      setBusy(false);
    }
  }

  function changeProvider(next: Provider) {
    setProvider(next);
    setKeyInput("");
    setNotice(null);
    setKeySaved(false);
    void (async () => {
      try {
        const saved = await commands.hasApiKey(next);
        setKeySaved(saved.status === "ok" && saved.data);
      } catch {
        setKeySaved(false);
      }
    })();
  }

  /**
   * Tests the typed key (or the saved one), and only if it works saves it
   * and the provider, then moves on. A failed test keeps the user here with
   * the reason, and nothing is saved.
   */
  async function testAndContinue() {
    const typed = keyInput.trim() === "" ? null : keyInput;
    if (typed === null && !keySaved) {
      setNotice({ kind: "error", text: "Paste your API key." });
      return;
    }
    if (typed !== null) {
      const problem = validateKey(typed);
      if (problem !== null) {
        setNotice({ kind: "error", text: problem });
        return;
      }
    }
    setBusy(true);
    setTesting(true);
    setNotice(null);
    try {
      const outcome = await testKey(provider, typed);
      if (outcome.kind === "error") {
        setNotice(outcome);
        return;
      }
      if (typed !== null) {
        const saved = await commands.setApiKey(provider, typed);
        if (saved.status === "error") {
          setNotice({ kind: "error", text: messageOf(saved.error) });
          return;
        }
        setKeyInput("");
        setKeySaved(true);
      }
      if (settings !== null && provider !== settings.provider) {
        const next: Settings_Serialize = {
          ...settings,
          provider,
          model: providerInfo(provider).defaultModel,
        };
        const result = await commands.saveSettings(next);
        if (result.status === "error") {
          setNotice({ kind: "error", text: messageOf(result.error) });
          return;
        }
        setSettings(next);
      }
    } catch {
      setNotice({ kind: "error", text: BROKEN });
      return;
    } finally {
      setBusy(false);
      setTesting(false);
    }
    await goTo("try");
  }

  async function finish() {
    setBusy(true);
    try {
      const result = await commands.finishOnboarding();
      if (result.status === "error") {
        setError(messageOf(result.error));
        return;
      }
      onFinished();
    } catch {
      setError(BROKEN);
    } finally {
      setBusy(false);
    }
  }

  if (status === null) {
    return (
      <main className="settings onboarding">
        <h1>Set up dictation</h1>
        {loadError === null ? (
          <p>Loading…</p>
        ) : (
          <p role="alert" className="notice error">
            Couldn&apos;t load the setup: {loadError}
          </p>
        )}
      </main>
    );
  }

  const index = STEPS.indexOf(status.step);
  const previous = index > 0 ? STEPS[index - 1] : undefined;
  const next = STEPS[index + 1];
  const info = providerInfo(provider);

  return (
    <main className="settings onboarding">
      <h1>Set up dictation</h1>
      <p className="hint" aria-label="progress">
        Step {index + 1} of {STEPS.length}
      </p>

      {status.step === "microphone" && (
        <section className="field">
          <h2>Allow the microphone</h2>
          <p>dictation-app records only while you hold the hotkey.</p>
          {status.microphone === "granted" ? (
            <p role="status" className="notice ok">
              The microphone is allowed.
            </p>
          ) : status.microphone === "denied" ? (
            <>
              <p className="hint">
                The microphone is turned off for dictation-app. Turn it on in System Settings,
                Privacy &amp; Security, Microphone. This step moves on by itself once it&apos;s on.
              </p>
              <div className="row">
                <button type="button" disabled={busy} onClick={() => void ask("microphone")}>
                  Open System Settings
                </button>
              </div>
            </>
          ) : (
            <>
              <p className="hint">macOS asks you once. Choose Allow.</p>
              <div className="row">
                <button
                  type="button"
                  className="primary"
                  disabled={busy}
                  onClick={() => void ask("microphone")}
                >
                  Allow microphone
                </button>
              </div>
            </>
          )}
        </section>
      )}

      {status.step === "accessibility" && (
        <section className="field">
          <h2>Allow Accessibility</h2>
          <p>The hotkey and pasting the text into other apps need Accessibility.</p>
          {status.accessibility === "granted" ? (
            <p role="status" className="notice ok">
              Accessibility is allowed.
            </p>
          ) : (
            <>
              <p className="hint">
                In System Settings, turn on dictation-app under Privacy &amp; Security,
                Accessibility. This step moves on by itself once it&apos;s on.
              </p>
              <div className="row">
                <button
                  type="button"
                  className={askedAccessibility ? undefined : "primary"}
                  disabled={busy}
                  onClick={() => void ask("accessibility")}
                >
                  Open System Settings
                </button>
              </div>
              {askedAccessibility && (
                <>
                  <p className="hint">
                    Relaunch to finish if you turned it on and this step doesn&apos;t move on.
                    dictation-app opens again here.
                  </p>
                  <div className="row">
                    <button type="button" disabled={busy} onClick={() => void relaunch()}>
                      Relaunch
                    </button>
                  </div>
                </>
              )}
            </>
          )}
        </section>
      )}

      {status.step === "key" && (
        <section>
          <h2>Add your API key</h2>
          <div className="field">
            <label htmlFor="onboarding-provider">Provider</label>
            <select
              id="onboarding-provider"
              value={provider}
              disabled={busy}
              onChange={(event) => {
                changeProvider(event.target.value as Provider);
              }}
            >
              {PROVIDERS.map((option) => (
                <option key={option.id} value={option.id}>
                  {option.name}
                </option>
              ))}
            </select>
            <p className="hint">Speech is sent to this service to be turned into text.</p>
          </div>
          <div className="field">
            <label htmlFor="onboarding-key">{info.name} API key</label>
            <input
              id="onboarding-key"
              type="password"
              value={keyInput}
              autoComplete="off"
              autoCapitalize="off"
              autoCorrect="off"
              spellCheck={false}
              disabled={busy}
              placeholder={keySaved ? "Paste a new key to replace it" : "Paste your key"}
              aria-describedby="onboarding-key-hint"
              onChange={(event) => {
                setKeyInput(event.target.value);
                setNotice(null);
              }}
            />
            <p id="onboarding-key-hint" className="hint">
              {keySaved
                ? "A key is saved. Continue to test it, or paste a new one. "
                : `Get one at ${KEY_PAGES[provider]}. `}
              It is tested first and then kept in the system keychain; it is never shown again.
            </p>
            {notice !== null && (
              <p
                role={notice.kind === "error" ? "alert" : "status"}
                className={`notice ${notice.kind}`}
              >
                {notice.text}
              </p>
            )}
          </div>
        </section>
      )}

      {status.step === "try" && (
        <section className="field">
          <h2>Try it</h2>
          <label htmlFor="try-field">
            Hold {hotkeyName(settings?.hotkey ?? "RightAlt")} and say a few words. Let go, and the
            text appears here.
          </label>
          <textarea
            id="try-field"
            ref={tryField}
            rows={4}
            value={tryText}
            onChange={(event) => {
              setTryText(event.target.value);
            }}
          />
          {tryText.trim() === "" ? (
            <p className="hint">
              Nothing yet? If something went wrong, the menu bar icon says what.
            </p>
          ) : (
            <p role="status" className="notice ok">
              It works. Dictation pastes wherever your cursor is.
            </p>
          )}
        </section>
      )}

      {error !== null && (
        <p role="alert" className="notice error">
          {error}
        </p>
      )}

      <div className="row actions">
        {previous !== undefined && (
          <button type="button" disabled={busy} onClick={() => void goTo(previous)}>
            Back
          </button>
        )}
        {status.step === "key" ? (
          <button
            type="button"
            className="primary"
            disabled={busy}
            onClick={() => void testAndContinue()}
          >
            {testing ? "Testing…" : "Test and continue"}
          </button>
        ) : next !== undefined ? (
          <button
            type="button"
            className="primary"
            disabled={busy || waiting}
            onClick={() => void goTo(next)}
          >
            Continue
          </button>
        ) : (
          <button type="button" className="primary" disabled={busy} onClick={() => void finish()}>
            Done
          </button>
        )}
      </div>
    </main>
  );
}
