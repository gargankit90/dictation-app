import { useCallback, useEffect, useRef, useState } from "react";
import type { SyntheticEvent } from "react";
import { commands } from "./bindings";
import type {
  LocalModelInfo_Serialize,
  Provider,
  Settings_Serialize,
  Transcription,
} from "./bindings";
import { BROKEN, messageOf, testKey as runKeyTest } from "./keyTest";
import { HotkeyRecorder } from "./HotkeyRecorder";
import { LanguagePicker } from "./LanguagePicker";
import { cloudLanguages } from "./languages";
import { LocalModels } from "./LocalModels";
import type { Notice } from "./keyTest";
import { PROVIDERS, providerInfo } from "./providers";
import { saveHotkey, setHotkeyRecording } from "./saveHotkey";
import "./settings.css";
import { normalizeLanguage, validateHotkey, validateKey, validateLanguage } from "./validation";

/**
 * What the app knows about the selected provider's stored key. "checking" and
 * "error" are different: a failed lookup (say, a denied keychain) must not
 * look like one still running.
 */
type KeyStatus =
  | { state: "checking" }
  | { state: "saved" }
  | { state: "none" }
  | { state: "error"; message: string };

const CHECKING: KeyStatus = { state: "checking" };

interface FieldErrors {
  key?: string;
  language?: string;
  hotkey?: string;
}

/**
 * The settings window: provider, API key (write-only), hotkey and language,
 * with a Test key button. The key lives in this component only while it is
 * being typed; it is cleared after a save and never loaded back. The only
 * thing the app tells us about a stored key is whether one exists.
 */
export function Settings({ onTroubleshoot }: { onTroubleshoot?: () => void } = {}) {
  // `null` until the saved settings load. Fields the form doesn't show
  // (version) are sent back as loaded.
  const [loaded, setLoaded] = useState<Settings_Serialize | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [provider, setProvider] = useState<Provider>("groq");
  const [transcription, setTranscription] = useState<Transcription>("cloud");
  const [localModel, setLocalModel] = useState("");
  // The on-device catalogue, once LocalModels has read it.
  const [catalogue, setCatalogue] = useState<LocalModelInfo_Serialize[] | null>(null);
  const [language, setLanguage] = useState("");
  const [hotkey, setHotkey] = useState("");
  // What the OS says about the login item, as the app last reported it.
  const [launchAtLogin, setLaunchAtLogin] = useState(false);
  // Whether the user has changed the checkbox since. Until then the box
  // follows the OS, and a save leaves the login item alone: the user may have
  // changed it in System Settings while this window was hidden.
  const [launchEdited, setLaunchEdited] = useState(false);
  const [keyInput, setKeyInput] = useState("");
  const [keyStatus, setKeyStatus] = useState<KeyStatus>(CHECKING);
  const [errors, setErrors] = useState<FieldErrors>({});
  const [notice, setNotice] = useState<Notice | null>(null);
  const [testing, setTesting] = useState(false);
  const [saving, setSaving] = useState(false);
  // The recorder is listening for the new key: the real hotkey is paused
  // until it ends, so nothing else may save (or change the key) meanwhile.
  const [listening, setListening] = useState(false);
  // Bumped whenever a test result would be out of date (a new test, or the
  // key or provider changed), so a slow reply can't show up under a newer key.
  const testRun = useRef(0);
  // Bumped by every key-status lookup (and when a save settles the status), so
  // only the newest reply counts: replies can arrive in any order, and one for
  // a provider the user has since left must not overwrite the current one.
  const keyCheck = useRef(0);

  const checkKeySaved = useCallback(async (which: Provider) => {
    keyCheck.current += 1;
    const run = keyCheck.current;
    setKeyStatus(CHECKING);
    let status: KeyStatus;
    try {
      const result = await commands.hasApiKey(which);
      status =
        result.status === "error"
          ? { state: "error", message: messageOf(result.error) }
          : { state: result.data ? "saved" : "none" };
    } catch {
      status = { state: "error", message: BROKEN };
    }
    if (run === keyCheck.current) setKeyStatus(status);
  }, []);

  useEffect(() => {
    // An object, not a `let`, so the checks after each `await` see changes.
    const state = { cancelled: false };
    void (async () => {
      try {
        const result = await commands.getSettings();
        if (state.cancelled) return;
        if (result.status === "error") {
          setLoadError(messageOf(result.error));
          return;
        }
        setLoaded(result.data);
        setProvider(result.data.provider);
        setTranscription(result.data.transcription);
        setLocalModel(result.data.local_model);
        setLanguage(result.data.language ?? "");
        setHotkey(result.data.hotkey);
        setLaunchAtLogin(result.data.launch_at_login);
        await checkKeySaved(result.data.provider);
      } catch {
        if (!state.cancelled) setLoadError(BROKEN);
      }
    })();
    return () => {
      state.cancelled = true;
    };
  }, [checkKeySaved]);

  // The settings window is hidden, not closed, so it outlives the OS state it
  // loaded. Whenever it comes back, read the login item again, unless the
  // user is in the middle of changing it. Nothing else in the form is touched.
  useEffect(() => {
    if (launchEdited) return;
    const state = { cancelled: false };
    const refresh = () => {
      if (document.visibilityState === "hidden") return;
      void (async () => {
        try {
          const result = await commands.getSettings();
          if (!state.cancelled && result.status === "ok") {
            setLaunchAtLogin(result.data.launch_at_login);
          }
        } catch {
          // Keep what is shown; the next focus tries again.
        }
      })();
    };
    window.addEventListener("focus", refresh);
    document.addEventListener("visibilitychange", refresh);
    return () => {
      state.cancelled = true;
      window.removeEventListener("focus", refresh);
      document.removeEventListener("visibilitychange", refresh);
    };
  }, [launchEdited]);

  const info = providerInfo(provider);
  // The languages the active engine offers (U2). Before the on-device list
  // has been read, nothing is filtered or flagged.
  const selectedModel = catalogue?.find((model) => model.id === localModel);
  const onDevice = transcription === "local";
  const languageChoices: readonly string[] =
    onDevice && selectedModel !== undefined
      ? selectedModel.language_codes
      : cloudLanguages(provider);
  // Plain words, not catalogue names (those belong in Speech details).
  const languageEngine = onDevice
    ? selectedModel === undefined
      ? null
      : selectedModel.language_codes.join() === "en"
        ? "the English model"
        : "the multilingual model"
    : info.name;
  const englishOnlyModel =
    onDevice && selectedModel?.language_codes.join() === "en" ? selectedModel : undefined;
  const englishOnly = englishOnlyModel !== undefined;
  const multilingual = catalogue?.find((model) => model.language_codes.length > 1);

  // The form is locked while a save runs, so what it shows is what gets saved.
  // (The fieldset disables the controls; these guards cover anything else.)
  const changesModel = loaded !== null && provider !== loaded.provider;

  function changeProvider(next: Provider) {
    if (saving) return;
    testRun.current += 1;
    setTesting(false);
    setProvider(next);
    setKeyInput("");
    setErrors({});
    setNotice(null);
    void checkKeySaved(next);
  }

  function changeKey(value: string) {
    if (saving) return;
    testRun.current += 1;
    setTesting(false);
    setKeyInput(value);
    setErrors((current) => ({ ...current, key: undefined }));
    setNotice(null);
  }

  async function testKey() {
    // A typed key is tested without saving it; with the field empty, the
    // saved key is tested.
    const typed = keyInput.trim() === "" ? null : keyInput;
    // While the lookup is running or failed, the app is asked anyway: it
    // answers for the stored key, with its own typed error if it can't read it.
    if (typed === null && keyStatus.state === "none") {
      setErrors((current) => ({ ...current, key: "Paste your API key." }));
      return;
    }
    if (typed !== null) {
      const problem = validateKey(typed);
      if (problem !== null) {
        setErrors((current) => ({ ...current, key: problem }));
        return;
      }
    }
    setErrors((current) => ({ ...current, key: undefined }));
    setNotice(null);
    testRun.current += 1;
    const run = testRun.current;
    setTesting(true);
    const outcome = await runKeyTest(provider, typed);
    if (run !== testRun.current) return;
    setTesting(false);
    setNotice(outcome);
  }

  /**
   * Applies a recorded (or reset) key at once; returns a message if it can't.
   * Busy like a Save: neither starts while the other runs, and the key has
   * one writer on the app's side, so the two can't undo each other.
   */
  async function recordHotkey(spec: string): Promise<string | null> {
    if (loaded === null || saving || testing) return "Wait for the other change to finish.";
    setSaving(true);
    try {
      const result = await saveHotkey(spec);
      if (result.settings !== null) {
        setLoaded(result.settings);
        // The form follows the key that listens, unless the change failed
        // and the user has a typed value of their own.
        if (result.error === null) setHotkey(result.settings.hotkey);
      }
      if (result.error === null) setNotice({ kind: "ok", text: "Dictation key saved." });
      return result.error;
    } finally {
      setSaving(false);
    }
  }

  async function save(event: SyntheticEvent) {
    event.preventDefault();
    // Busy like the Save button: a submit that gets past it (Enter in a field)
    // starts nothing while a save or a key test runs.
    if (loaded === null || saving || testing || listening) return;
    const found: FieldErrors = {
      key: keyInput.trim() === "" ? undefined : (validateKey(keyInput) ?? undefined),
      language: validateLanguage(language) ?? undefined,
      hotkey: validateHotkey(hotkey) ?? undefined,
    };
    setErrors(found);
    if (found.key ?? found.language ?? found.hotkey) {
      setNotice(null);
      return;
    }
    setSaving(true);
    setNotice(null);
    try {
      // The key first: if the keychain refuses, nothing else has changed.
      if (keyInput.trim() !== "") {
        const result = await commands.setApiKey(provider, keyInput);
        if (result.status === "error") {
          setNotice({ kind: "error", text: messageOf(result.error) });
          return;
        }
        setKeyInput("");
        keyCheck.current += 1; // any lookup still running predates this key
        setKeyStatus({ state: "saved" });
      }
      // An untouched checkbox follows the OS, so ask it once more: a save of
      // only the language must not undo a change made in System Settings.
      let launch = launchAtLogin;
      if (!launchEdited) {
        const current = await commands.getSettings();
        if (current.status === "ok") launch = current.data.launch_at_login;
      }
      const toSave: Settings_Serialize = {
        ...loaded,
        provider,
        // The model is kept unless the provider changes; then it becomes the
        // new provider's default (the form says so).
        model: provider === loaded.provider ? loaded.model : info.defaultModel,
        language: normalizeLanguage(language),
        launch_at_login: launch,
        transcription,
        local_model: localModel,
      };
      let saved = await commands.saveSettings(toSave);
      // The dictation key has its own writer: a typed one is set after the
      // rest is saved, and says so if it can't be used.
      const typedKey = hotkey.trim();
      if (saved.status === "ok" && typedKey !== loaded.hotkey) {
        const change = await saveHotkey(typedKey);
        if (change.error !== null) saved = { status: "error", error: change.error };
      }
      if (saved.status === "error") {
        // Something didn't take effect: the login item (the OS can refuse,
        // or want the user's approval) or the hotkey (the old one keeps
        // working). The message says which. The typed values stay, so the
        // user can retry.
        setNotice({ kind: "error", text: messageOf(saved.error) });
        // Show what the login item really is. If the app can't say, the box
        // stays the user's own unapplied choice rather than claiming to be
        // the OS's state.
        const actual = await commands.getSettings();
        if (actual.status === "ok") {
          setLaunchAtLogin(actual.data.launch_at_login);
          setLaunchEdited(false);
        }
        return;
      }
      // Reload, so the form shows what was stored (the canonical hotkey). If
      // that fails, what we just saved is still what is stored.
      const reread = await commands.getSettings();
      const stored = reread.status === "ok" ? reread.data : toSave;
      setLoaded(stored);
      setProvider(stored.provider);
      setTranscription(stored.transcription);
      setLocalModel(stored.local_model);
      setLanguage(stored.language ?? "");
      setHotkey(stored.hotkey);
      setLaunchAtLogin(stored.launch_at_login);
      setLaunchEdited(false);
      await checkKeySaved(stored.provider);
      setNotice({ kind: "ok", text: "Settings saved." });
    } catch {
      setNotice({ kind: "error", text: BROKEN });
    } finally {
      setSaving(false);
    }
  }

  /**
   * Turns sharing usage events on or off at once, without the Save button:
   * "off" has to stop sending now. It has a command of its own that changes
   * only this choice, and Save never changes it, so no Save can undo it.
   */
  async function setSharing(share: boolean) {
    if (loaded === null) return;
    setNotice(null);
    try {
      const result = await commands.setShareUsageEvents(share);
      if (result.status === "error") {
        setNotice({ kind: "error", text: messageOf(result.error) });
        return;
      }
      setLoaded((now) => (now === null ? now : { ...now, share_usage_events: share }));
    } catch {
      setNotice({ kind: "error", text: BROKEN });
    }
  }

  if (loadError !== null) {
    return (
      <main className="settings">
        <h1>Settings</h1>
        <p role="alert" className="notice error">
          Couldn&apos;t load the settings: {loadError}
        </p>
      </main>
    );
  }
  if (loaded === null) {
    return (
      <main className="settings">
        <h1>Settings</h1>
        <p>Loading…</p>
      </main>
    );
  }

  return (
    <main className="settings">
      <h1>Settings</h1>
      <form onSubmit={(event) => void save(event)} noValidate>
        <fieldset disabled={saving}>
          <fieldset className="field choice">
            <legend>Transcription</legend>
            <label className="check">
              <input
                type="radio"
                name="transcription"
                value="local"
                checked={transcription === "local"}
                onChange={() => {
                  if (saving) return;
                  setTranscription("local");
                  setNotice(null);
                }}
              />
              On this Mac
            </label>
            <label className="check">
              <input
                type="radio"
                name="transcription"
                value="cloud"
                checked={transcription === "cloud"}
                onChange={() => {
                  if (saving) return;
                  setTranscription("cloud");
                  setNotice(null);
                }}
              />
              Your own key
            </label>
            <p className="hint">
              {transcription === "local"
                ? "Speech is turned into text on this Mac with a model you download once. No key needed, and nothing is sent anywhere."
                : "Speech is sent to Groq or OpenAI with your own API key."}{" "}
              Takes effect when you save.
            </p>
          </fieldset>

          {transcription === "local" && (
            <div className="field">
              <LocalModels
                selected={localModel}
                onSelect={(id) => {
                  if (saving) return;
                  setLocalModel(id);
                  setNotice(null);
                }}
                disabled={saving}
                onModels={setCatalogue}
              />
              {notice !== null && (
                <p
                  role={notice.kind === "error" ? "alert" : "status"}
                  className={`notice ${notice.kind}`}
                >
                  {notice.text}
                </p>
              )}
            </div>
          )}

          {transcription === "cloud" && (
            <>
              <div className="field">
                <label htmlFor="provider">Provider</label>
                <select
                  id="provider"
                  value={provider}
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
                <p className="hint">
                  Speech is sent to this service to be turned into text.
                  {changesModel &&
                    ` Saving switches to ${info.name}'s default model, ${info.defaultModel}.`}
                </p>
              </div>

              <div className="field">
                <label htmlFor="api-key">{info.name} API key</label>
                <input
                  id="api-key"
                  type="password"
                  value={keyInput}
                  autoComplete="off"
                  autoCapitalize="off"
                  autoCorrect="off"
                  spellCheck={false}
                  placeholder={
                    keyStatus.state === "saved" ? "Paste a new key to replace it" : "Paste your key"
                  }
                  aria-invalid={errors.key !== undefined}
                  aria-describedby="api-key-status api-key-error"
                  onChange={(event) => {
                    changeKey(event.target.value);
                  }}
                />
                <p id="api-key-status" className="hint">
                  {keyStatus.state === "saved" ? (
                    <strong className="saved">Key saved</strong>
                  ) : keyStatus.state === "none" ? (
                    "No key saved yet."
                  ) : keyStatus.state === "checking" ? (
                    "Checking for a saved key…"
                  ) : (
                    <span role="alert" className="error">
                      Couldn&apos;t check for a saved key: {keyStatus.message}
                    </span>
                  )}{" "}
                  The key is kept in the system keychain and is never shown again.
                </p>
                <p id="api-key-error" role={errors.key ? "alert" : undefined} className="error">
                  {errors.key}
                </p>
                <div className="row">
                  <button type="button" disabled={testing || saving} onClick={() => void testKey()}>
                    {testing ? "Testing…" : "Test key"}
                  </button>
                  <span className="hint">
                    Tests the key in the box (without saving it), or the saved key if the box is
                    empty.
                  </span>
                  {keyStatus.state === "error" && (
                    <button
                      type="button"
                      onClick={() => {
                        void checkKeySaved(provider);
                      }}
                    >
                      Retry
                    </button>
                  )}
                </div>
                {notice !== null && (
                  <p
                    role={notice.kind === "error" ? "alert" : "status"}
                    className={`notice ${notice.kind}`}
                  >
                    {notice.text}
                  </p>
                )}
              </div>
            </>
          )}

          <div className="field">
            <span className="label" id="hotkey-label">
              Dictation key
            </span>
            <HotkeyRecorder
              value={loaded.hotkey}
              disabled={saving}
              onChange={recordHotkey}
              onRecording={(recording) => {
                setListening(recording);
                setHotkeyRecording(recording);
              }}
            />
            <label htmlFor="hotkey">Or type it</label>
            <input
              id="hotkey"
              type="text"
              value={hotkey}
              autoComplete="off"
              spellCheck={false}
              aria-invalid={errors.hotkey !== undefined}
              aria-describedby="hotkey-hint hotkey-error"
              onChange={(event) => {
                if (saving) return;
                setHotkey(event.target.value);
                setErrors((current) => ({ ...current, hotkey: undefined }));
              }}
            />
            <p id="hotkey-hint" className="hint">
              Hold it to talk. Typed, it looks like RightAlt or Ctrl+Shift+Space, and works as soon
              as you save. A recorded key works at once.
            </p>
            <p id="hotkey-error" role={errors.hotkey ? "alert" : undefined} className="error">
              {errors.hotkey}
            </p>
          </div>

          <div className="field">
            <label htmlFor="language">Language</label>
            <LanguagePicker
              id="language"
              value={language}
              onChange={(code) => {
                if (saving) return;
                setLanguage(code);
                setErrors((current) => ({ ...current, language: undefined }));
              }}
              supported={languageChoices}
              engineName={languageEngine}
              disabled={saving}
              describedBy="language-hint language-error"
              invalid={errors.language !== undefined}
            />
            <p id="language-hint" className="hint">
              {englishOnly
                ? `The English model understands English only. For Hindi and other languages, `
                : "Choose the language you speak, or let the app detect it."}
              {englishOnly && (
                <button
                  type="button"
                  className="link"
                  disabled={saving || multilingual === undefined}
                  onClick={() => {
                    if (multilingual !== undefined) setLocalModel(multilingual.id);
                  }}
                >
                  choose the multilingual model
                </button>
              )}
              {englishOnly && "."}
            </p>
            <p id="language-error" role={errors.language ? "alert" : undefined} className="error">
              {errors.language}
            </p>
          </div>

          <div className="field">
            <label className="check" htmlFor="launch-at-login">
              <input
                id="launch-at-login"
                type="checkbox"
                checked={launchAtLogin}
                aria-describedby="launch-at-login-hint"
                onChange={(event) => {
                  if (saving) return;
                  setLaunchAtLogin(event.target.checked);
                  setLaunchEdited(true);
                  setNotice(null);
                }}
              />
              Launch at login
            </label>
            <p id="launch-at-login-hint" className="hint">
              Starts Vocet in the menu bar when you log in. Takes effect when you save. You can also
              change it in System Settings → General → Login Items.
            </p>
          </div>

          <h2>Privacy</h2>
          <div className="field">
            <label className="check" htmlFor="share-usage-events">
              <input
                id="share-usage-events"
                type="checkbox"
                checked={loaded.share_usage_events}
                aria-describedby="share-usage-events-hint"
                onChange={(event) => {
                  void setSharing(event.target.checked);
                }}
              />
              Share usage and error events
            </label>
            <p id="share-usage-events-hint" className="hint">
              Helps us see what works and what breaks, and fix crashes. Never your voice, your text,
              your API keys or the apps you use. Takes effect at once; turning it off also clears
              what was waiting to be sent or reported.
            </p>
            <details>
              <summary>What&apos;s sent</summary>
              <ul className="hint">
                <li>That the app was opened, with its version, your macOS version and chip.</li>
                <li>Which onboarding steps you reached, and that you finished.</li>
                <li>Which speech engine you chose (for example Groq or OpenAI).</li>
                <li>
                  That a dictation worked, with its length and speed in rough ranges (not the
                  words).
                </li>
                <li>That a dictation failed, and the kind of failure (not the message).</li>
                <li>
                  Error reports, sent to Sentry (US): when the app crashes (a panic) or a dictation
                  fails, the error code (for example mic_denied), the app version, your macOS major
                  version and chip, and for a crash the code locations and function names where it
                  happened. Never the error message, your words, file paths, your user name or
                  computer name.
                </li>
                <li>Which setting you changed (not the new value).</li>
                <li>That the app updated, from which version to which.</li>
              </ul>
              <p className="hint">
                Each event carries a random ID made on this Mac, not tied to you or your computer.
                Turning sharing off deletes it.
              </p>
            </details>
          </div>

          {onTroubleshoot === undefined ? null : (
            <>
              <h2>Help</h2>
              <div className="field">
                <button type="button" onClick={onTroubleshoot}>
                  Troubleshoot…
                </button>
                <p className="hint">
                  Check the microphone, Accessibility, hotkey and engine, and report a problem.
                </p>
              </div>
            </>
          )}

          <div className="row actions">
            <button type="submit" className="primary" disabled={saving || testing || listening}>
              {saving ? "Saving…" : "Save"}
            </button>
          </div>
        </fieldset>
      </form>
    </main>
  );
}
