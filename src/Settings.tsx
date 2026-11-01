import { useCallback, useEffect, useRef, useState } from "react";
import type { SyntheticEvent } from "react";
import { commands } from "./bindings";
import type { Provider, Settings_Serialize } from "./bindings";
import { PROVIDERS, providerInfo } from "./providers";
import "./settings.css";
import { normalizeLanguage, validateHotkey, validateKey, validateLanguage } from "./validation";

interface Notice {
  kind: "ok" | "error";
  text: string;
}

interface FieldErrors {
  key?: string;
  language?: string;
  hotkey?: string;
}

/** Shown when a command fails without a message of its own (the IPC itself broke). */
const BROKEN = "Something went wrong talking to the app. Try again.";

/** The message of a failed command: the app's own text, or [`BROKEN`]. */
function messageOf(error: unknown): string {
  return typeof error === "string" && error !== "" ? error : BROKEN;
}

/**
 * The settings window: provider, API key (write-only), hotkey and language,
 * with a Test key button. The key lives in this component only while it is
 * being typed; it is cleared after a save and never loaded back. The only
 * thing the app tells us about a stored key is whether one exists.
 */
export function Settings() {
  // `null` until the saved settings load. Fields the form doesn't show
  // (version) are sent back as loaded.
  const [loaded, setLoaded] = useState<Settings_Serialize | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [provider, setProvider] = useState<Provider>("groq");
  const [language, setLanguage] = useState("");
  const [hotkey, setHotkey] = useState("");
  const [keyInput, setKeyInput] = useState("");
  const [keySaved, setKeySaved] = useState<boolean | null>(null);
  const [errors, setErrors] = useState<FieldErrors>({});
  const [notice, setNotice] = useState<Notice | null>(null);
  const [testing, setTesting] = useState(false);
  const [saving, setSaving] = useState(false);
  // Bumped whenever a test result would be out of date (a new test, or the
  // key or provider changed), so a slow reply can't show up under a newer key.
  const testRun = useRef(0);

  const checkKeySaved = useCallback(async (which: Provider) => {
    try {
      const result = await commands.hasApiKey(which);
      setKeySaved(result.status === "ok" ? result.data : null);
    } catch {
      setKeySaved(null);
    }
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
        setLanguage(result.data.language ?? "");
        setHotkey(result.data.hotkey);
        await checkKeySaved(result.data.provider);
      } catch {
        if (!state.cancelled) setLoadError(BROKEN);
      }
    })();
    return () => {
      state.cancelled = true;
    };
  }, [checkKeySaved]);

  const info = providerInfo(provider);

  function changeProvider(next: Provider) {
    testRun.current += 1;
    setTesting(false);
    setProvider(next);
    setKeyInput("");
    setKeySaved(null);
    setErrors({});
    setNotice(null);
    void checkKeySaved(next);
  }

  function changeKey(value: string) {
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
    if (typed === null && keySaved !== true) {
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
    let outcome: Notice;
    try {
      const result = await commands.testApiKey(provider, typed);
      outcome =
        result.status === "ok"
          ? { kind: "ok", text: `The key works. ${info.name} accepted it.` }
          : { kind: "error", text: result.error.message };
    } catch {
      outcome = { kind: "error", text: BROKEN };
    }
    if (run !== testRun.current) return;
    setTesting(false);
    setNotice(outcome);
  }

  async function save(event: SyntheticEvent) {
    event.preventDefault();
    if (loaded === null) return;
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
        setKeySaved(true);
      }
      const saved = await commands.saveSettings({
        ...loaded,
        provider,
        model: info.defaultModel,
        language: normalizeLanguage(language),
        hotkey: hotkey.trim(),
      });
      if (saved.status === "error") {
        setNotice({ kind: "error", text: messageOf(saved.error) });
        return;
      }
      // Reload, so the form shows what was stored (the canonical hotkey).
      const reread = await commands.getSettings();
      if (reread.status === "ok") {
        setLoaded(reread.data);
        setLanguage(reread.data.language ?? "");
        setHotkey(reread.data.hotkey);
      }
      await checkKeySaved(provider);
      setNotice({ kind: "ok", text: "Settings saved." });
    } catch {
      setNotice({ kind: "error", text: BROKEN });
    } finally {
      setSaving(false);
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
          <p className="hint">Speech is sent to this service to be turned into text.</p>
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
            placeholder={keySaved === true ? "Paste a new key to replace it" : "Paste your key"}
            aria-invalid={errors.key !== undefined}
            aria-describedby="api-key-status api-key-error"
            onChange={(event) => {
              changeKey(event.target.value);
            }}
          />
          <p id="api-key-status" className="hint">
            {keySaved === true ? (
              <strong className="saved">Key saved</strong>
            ) : keySaved === false ? (
              "No key saved yet."
            ) : (
              "Checking for a saved key…"
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
              Tests the key in the box (without saving it), or the saved key if the box is empty.
            </span>
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

        <div className="field">
          <label htmlFor="hotkey">Hotkey</label>
          <input
            id="hotkey"
            type="text"
            value={hotkey}
            autoComplete="off"
            spellCheck={false}
            aria-invalid={errors.hotkey !== undefined}
            aria-describedby="hotkey-hint hotkey-error"
            onChange={(event) => {
              setHotkey(event.target.value);
              setErrors((current) => ({ ...current, hotkey: undefined }));
            }}
          />
          <p id="hotkey-hint" className="hint">
            Hold it to talk. For example RightAlt (the right Option key) or Ctrl+Shift+Space.
          </p>
          <p id="hotkey-error" role={errors.hotkey ? "alert" : undefined} className="error">
            {errors.hotkey}
          </p>
        </div>

        <div className="field">
          <label htmlFor="language">Language</label>
          <input
            id="language"
            type="text"
            value={language}
            autoComplete="off"
            spellCheck={false}
            placeholder="Detect automatically"
            aria-invalid={errors.language !== undefined}
            aria-describedby="language-hint language-error"
            onChange={(event) => {
              setLanguage(event.target.value);
              setErrors((current) => ({ ...current, language: undefined }));
            }}
          />
          <p id="language-hint" className="hint">
            A code such as en, de or fr. Leave it empty to detect the language.
          </p>
          <p id="language-error" role={errors.language ? "alert" : undefined} className="error">
            {errors.language}
          </p>
        </div>

        <div className="row actions">
          <button type="submit" className="primary" disabled={saving || testing}>
            {saving ? "Saving…" : "Save"}
          </button>
        </div>
      </form>
    </main>
  );
}
