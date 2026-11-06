// The own-key panel (U6), shared by Settings' sheet (U12) and onboarding's
// Speech step (U8): pick the provider, the how-to, the field and Test & Save
// (saved only if the test passes).

import { useCallback, useEffect, useRef, useState } from "react";
import type { KeyboardEvent } from "react";
import { commands } from "../bindings";
import type { Provider, Settings_Serialize } from "../bindings";
import { BROKEN, messageOf } from "../keyTest";
import { PROVIDERS, providerInfo } from "../providers";
import { validateKey } from "../validation";

/** Shown after Test & Save, once the provider accepted the key and the Keychain kept it. */
export const CONNECTED = "✓ Connected · saved in Keychain";
/** A key is saved and nothing has been tested since. */
export const SAVED_LINE = "A key is saved in your Keychain.";
/** A good key the Keychain wouldn't keep (U6 criterion 5). */
export const KEY_NOT_SAVED = "Key works, but couldn't be saved to your Keychain";

/** Where each provider's keys are made, and the steps to get one. */
const KEY_STEPS: Record<Provider, { link: "groq_keys" | "openai_keys"; site: string }> = {
  groq: { link: "groq_keys", site: "console.groq.com" },
  openai: { link: "openai_keys", site: "platform.openai.com" },
};

export type KeyStatus =
  | { state: "checking" }
  | { state: "saved" }
  | { state: "none" }
  | { state: "error"; message: string };

type KeyNotice = { kind: "ok" | "error"; text: string } | null;

/**
 * The key operations' generation, bumped by every Test & Save, edit of
 * the key field, provider change and Remove key; a test writes its key
 * only if it is still the newest. In module scope, not in the component,
 * so leaving Speech and coming back can't reset it while an old test is
 * still waiting for the provider (review round 2).
 */
const keyRun = { current: 0 };

/**
 * A Remove key still running, whichever panel started it. Asking whether a key
 * is saved waits for it, or a panel opened meanwhile could say "saved" about a
 * key that is being deleted (review round 1).
 */
const removal: { current: Promise<unknown> | null } = { current: null };

interface Options {
  settings: Settings_Serialize;
  /** The engine in use is on-device, so a provider change doesn't switch it. */
  onDevice: boolean;
  /** Makes `which` the engine. */
  switchToCloud: (which: Provider) => Promise<void>;
  /** Reads the engine's status again. */
  refreshEngine: () => Promise<void>;
  /** A key for `which` passed its test and the Keychain kept it. */
  onConnected?: (which: Provider) => Promise<void>;
  /**
   * Settings' sheet (U12): the popup only picks whose key to work on, and a
   * working engine stays until the chosen provider has a key. Setup never
   * switches here at all: it passes `onDevice` and switches on Continue (U11).
   */
  draftProvider?: boolean;
}

/** The panel's state and actions. */
export function useOwnKey({
  settings,
  onDevice,
  switchToCloud,
  refreshEngine,
  onConnected,
  draftProvider = false,
}: Options) {
  const [provider, setProvider] = useState<Provider>(settings.provider);
  const [keyInput, setKeyInput] = useState("");
  const [keyStatus, setKeyStatus] = useState<KeyStatus>({ state: "checking" });
  const [keyNotice, setKeyNotice] = useState<KeyNotice>(null);
  const [keyBusy, setKeyBusy] = useState<"testing" | "saving" | "removing" | null>(null);
  // A key write is running; edits don't cancel it (it is already sent).
  const writing = useRef(false);
  // Only the newest key lookup counts.
  const keyCheck = useRef(0);

  // The panel follows the stored provider when it changes elsewhere.
  const [seenProvider, setSeenProvider] = useState(settings.provider);
  if (settings.provider !== seenProvider) {
    setSeenProvider(settings.provider);
    setProvider(settings.provider);
  }

  const checkKeySaved = useCallback(async (which: Provider) => {
    keyCheck.current += 1;
    const run = keyCheck.current;
    setKeyStatus({ state: "checking" });
    let status: KeyStatus;
    try {
      await removal.current;
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
    const timer = window.setTimeout(() => void checkKeySaved(provider), 0);
    return () => {
      window.clearTimeout(timer);
    };
  }, [provider, checkKeySaved]);

  function changeProvider(next: Provider) {
    keyRun.current += 1;
    setKeyBusy(null);
    setProvider(next);
    setKeyInput("");
    setKeyNotice(null);
    if (onDevice) return;
    if (!draftProvider) {
      // Instant apply: while the own key is the engine, the provider is too.
      void switchToCloud(next);
      return;
    }
    // The sheet switches only to a provider with a saved key: selecting "Your
    // own key" with one does the same. Otherwise Test & Save decides.
    const run = keyRun.current;
    void (async () => {
      try {
        const saved = await commands.hasApiKey(next);
        if (run === keyRun.current && saved.status === "ok" && saved.data) {
          await switchToCloud(next);
        }
      } catch {
        // The engine stays; the status line says what the lookup found.
      }
    })();
  }

  function editKey(value: string) {
    if (writing.current) return;
    keyRun.current += 1;
    setKeyBusy(null);
    setKeyInput(value);
    setKeyNotice(null);
  }

  /**
   * The panel closes: an unfinished test is dropped, and so is what was typed.
   * So is a provider picked but not set up: the row goes back to the settings'
   * one. A key saved for it stays chosen while the engine is on this Mac, for
   * when "Your own key" is selected.
   */
  function abandon() {
    editKey("");
    if (draftProvider && !(onDevice && keyStatus.state === "saved")) {
      setProvider(settings.provider);
    }
  }

  async function testAndSave() {
    if (writing.current) return;
    const typed = keyInput.trim();
    if (typed === "") {
      setKeyNotice({ kind: "error", text: "Paste your API key." });
      return;
    }
    const problem = validateKey(typed);
    if (problem !== null) {
      setKeyNotice({ kind: "error", text: problem });
      return;
    }
    keyRun.current += 1;
    const run = keyRun.current;
    const which = provider;
    setKeyNotice(null);
    setKeyBusy("testing");
    let notice: KeyNotice;
    try {
      const tested = await commands.testApiKey(which, typed);
      if (tested.status === "error") {
        notice = { kind: "error", text: tested.error.message };
      } else {
        // Only the newest Test & Save may write, and only if no edit,
        // removal or newer save came since it started: an obsolete test
        // must never restore a removed key or replace a newer one.
        if (run !== keyRun.current) return;
        // From here the write is committed: the field and buttons wait for it.
        writing.current = true;
        setKeyBusy("saving");
        const saved = await commands.setApiKey(which, typed).finally(() => {
          writing.current = false;
        });
        notice =
          saved.status === "error"
            ? { kind: "error", text: `${KEY_NOT_SAVED}: ${messageOf(saved.error)}` }
            : { kind: "ok", text: CONNECTED };
      }
    } catch {
      notice = { kind: "error", text: BROKEN };
    }
    if (run !== keyRun.current) return;
    setKeyBusy(null);
    setKeyNotice(notice);
    if (notice.kind === "ok") {
      setKeyInput("");
      keyCheck.current += 1; // any lookup still running predates this key
      setKeyStatus({ state: "saved" });
      await onConnected?.(which);
      await refreshEngine();
    }
  }

  async function removeKey() {
    keyRun.current += 1;
    const run = keyRun.current;
    const which = provider;
    setKeyNotice(null);
    setKeyBusy("removing");
    // Committed once sent: the field, the popup and the sheet wait for it, and
    // its outcome always counts (review round 1).
    writing.current = true;
    let notice: KeyNotice = null;
    const deletion = commands.deleteApiKey(which);
    const running = deletion.catch(() => undefined);
    removal.current = running;
    try {
      const result = await deletion;
      if (result.status === "error") notice = { kind: "error", text: messageOf(result.error) };
    } catch {
      notice = { kind: "error", text: BROKEN };
    }
    writing.current = false;
    if (removal.current === running) removal.current = null;
    setKeyBusy(null);
    if (notice !== null || run === keyRun.current) {
      setKeyNotice(notice ?? { kind: "ok", text: "Key removed from your Keychain." });
    }
    void checkKeySaved(which);
    await refreshEngine();
  }

  return {
    provider,
    keyInput,
    keyStatus,
    keyNotice,
    keyBusy,
    /** The last Test & Save here passed and the Keychain kept the key. */
    connected: keyNotice?.text === CONNECTED,
    changeProvider,
    editKey,
    abandon,
    testAndSave,
    removeKey,
    checkKeySaved,
  };
}

export type OwnKey = ReturnType<typeof useOwnKey>;

/** The panel itself: provider, how-to, field, one status line, Test & Save, Remove key. */
export function OwnKeyPanel({
  ownKey,
  canRemove = true,
  providerOnly = false,
}: {
  ownKey: OwnKey;
  /** Setup has no Remove key (U11); it stays in Settings. */
  canRemove?: boolean;
  /** A saved key in setup (U11): only the Provider row, until Replace. */
  providerOnly?: boolean;
}) {
  const { provider, keyInput, keyStatus, keyNotice, keyBusy } = ownKey;
  const info = providerInfo(provider);
  const steps = KEY_STEPS[provider];
  const providerRow = (
    <div className="panel-row provider-row">
      <label htmlFor="provider">Provider</label>
      <select
        id="provider"
        value={provider}
        disabled={keyBusy === "saving" || keyBusy === "removing"}
        onChange={(event) => {
          ownKey.changeProvider(event.target.value as Provider);
        }}
      >
        {PROVIDERS.map((option) => (
          <option key={option.id} value={option.id}>
            {option.name}
          </option>
        ))}
      </select>
    </div>
  );
  if (providerOnly) {
    return (
      <div className="own-key-panel" id="own-key-panel">
        {providerRow}
      </div>
    );
  }
  return (
    <div className="own-key-panel" id="own-key-panel">
      {providerRow}
      <ol className="key-steps">
        <li>
          Open{" "}
          <button
            type="button"
            className="link"
            onClick={() => {
              void commands.openLink(steps.link).catch(() => undefined);
            }}
          >
            {steps.site}
          </button>{" "}
          and sign in.
        </li>
        <li>Create an API key and copy it.</li>
        <li>Paste it below and press Test &amp; Save.</li>
      </ol>
      <div className="panel-row">
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
          aria-describedby="api-key-status"
          aria-invalid={keyNotice?.kind === "error"}
          readOnly={keyBusy === "saving" || keyBusy === "removing"}
          onChange={(event) => {
            ownKey.editKey(event.target.value);
          }}
          onKeyDown={(event) => {
            if (event.key === "Enter") void ownKey.testAndSave();
          }}
        />
      </div>
      <KeyStatusLine id="api-key-status" ownKey={ownKey} />
      <div className="row own-key-actions">
        <button
          type="button"
          className="primary"
          disabled={keyBusy !== null}
          onClick={() => void ownKey.testAndSave()}
        >
          {keyBusy === "testing" ? "Testing…" : keyBusy === "saving" ? "Saving…" : "Test & Save"}
        </button>
        {canRemove && keyStatus.state === "saved" && (
          <button
            type="button"
            className="destructive"
            disabled={keyBusy !== null}
            onClick={() => void ownKey.removeKey()}
          >
            {keyBusy === "removing" ? "Removing…" : "Remove key"}
          </button>
        )}
        {keyStatus.state === "error" && (
          <button type="button" onClick={() => void ownKey.checkKeySaved(provider)}>
            Retry
          </button>
        )}
      </div>
    </div>
  );
}

/** The panel's one status line: what just happened, otherwise what is saved (U12). */
function KeyStatusLine({ id, ownKey }: { id: string; ownKey: OwnKey }) {
  const { keyStatus, keyNotice } = ownKey;
  if (keyNotice !== null) {
    return (
      <p
        id={id}
        role={keyNotice.kind === "error" ? "alert" : "status"}
        className={`notice ${keyNotice.kind}`}
      >
        {keyNotice.text}
      </p>
    );
  }
  if (keyStatus.state === "error") {
    return (
      <p id={id} role="alert" className="notice error">
        Couldn&apos;t check for a saved key: {keyStatus.message}
      </p>
    );
  }
  return (
    <p id={id} className="hint">
      {keyStatus.state === "saved"
        ? SAVED_LINE
        : keyStatus.state === "none"
          ? "No key saved yet."
          : "Checking for a saved key…"}
    </p>
  );
}

/**
 * The own-key panel as a sheet over the window (U12): modal, Tab stays inside,
 * Esc and Done close it, except while a key is being written or removed.
 */
export function OwnKeySheet({ ownKey, onDone }: { ownKey: OwnKey; onDone: () => void }) {
  const sheet = useRef<HTMLDivElement>(null);
  const writing = ownKey.keyBusy === "saving" || ownKey.keyBusy === "removing";
  useEffect(() => {
    sheet.current?.querySelector<HTMLInputElement>("#api-key")?.focus();
  }, []);
  function onKey(event: KeyboardEvent<HTMLDivElement>) {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      if (!writing) onDone();
    } else if (event.key === "Tab") {
      const focusable = [
        ...(sheet.current?.querySelectorAll<HTMLElement>(
          "button:not(:disabled), input, select:not(:disabled)",
        ) ?? []),
      ];
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (first === undefined || last === undefined) return;
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    }
  }
  return (
    <>
      <div className="confirm-backdrop" />
      <div
        ref={sheet}
        className="key-sheet"
        role="dialog"
        aria-modal="true"
        aria-labelledby="key-sheet-title"
        onKeyDown={onKey}
      >
        <h2 id="key-sheet-title">Your own key</h2>
        <OwnKeyPanel ownKey={ownKey} />
        <div className="confirm-actions">
          <button type="button" disabled={writing} onClick={onDone}>
            Done
          </button>
        </div>
      </div>
    </>
  );
}
