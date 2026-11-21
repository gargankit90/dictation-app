// The OpenRouter connection (Q3), shared by Settings' sheet and onboarding's
// key screen: Connect OpenRouter (sign in in the browser), what is saved
// (the masked key), Disconnect, Add credits, and "Paste a key instead" for
// when the browser can't come back.

import { useCallback, useEffect, useRef, useState } from "react";
import type { KeyboardEvent } from "react";
import { commands } from "../bindings";
import { BROKEN, messageOf } from "../messages";
import { validateKey } from "../validation";
import { PROVIDER_NAME } from "./modelRow";

const PROVIDER = "openrouter";

/** Shown once the key is saved. */
export const CONNECTED = "✓ Connected to OpenRouter";
/** What the first screen says before the button (owner-approved, Q3). */
export const EXPLAINER =
  "Vocet uses your OpenRouter account. You pay OpenRouter directly: about $0.70 per 1,000 dictations. New accounts need a few dollars of credit.";
/** A good key the Keychain wouldn't keep. */
export const KEY_NOT_SAVED = "Key works, but couldn't be saved to your Keychain";

export type Status =
  | { state: "checking" }
  | { state: "none" }
  | { state: "connected"; hint: string }
  | { state: "error"; message: string };

type Notice = { kind: "ok" | "error"; text: string } | null;
type Busy = "connecting" | "saving" | "disconnecting" | null;

interface Options {
  /** A key was saved (by sign-in or paste). */
  onConnected?: () => Promise<void>;
  /** Reads the engine's status again. */
  refreshEngine: () => Promise<void>;
}

/** The connection's state and actions. */
export function useConnection({ onConnected, refreshEngine }: Options) {
  const [status, setStatus] = useState<Status>({ state: "checking" });
  const [notice, setNotice] = useState<Notice>(null);
  const [busy, setBusy] = useState<Busy>(null);
  // Only the newest status lookup counts.
  const lookup = useRef(0);
  // The user pressed Cancel: the attempt's "cancelled" is not an error.
  const cancelled = useRef(false);

  const refresh = useCallback(async () => {
    lookup.current += 1;
    const run = lookup.current;
    let next: Status;
    try {
      const hint = await commands.savedKeyHint(PROVIDER);
      if (hint.status === "error") {
        next = { state: "error", message: messageOf(hint.error) };
      } else if (hint.data === null) {
        next = { state: "none" };
      } else {
        next = { state: "connected", hint: hint.data };
      }
    } catch {
      next = { state: "error", message: BROKEN };
    }
    if (run === lookup.current) setStatus(next);
  }, []);

  useEffect(() => {
    const timer = window.setTimeout(() => void refresh(), 0);
    return () => {
      window.clearTimeout(timer);
    };
  }, [refresh]);

  async function saved() {
    setNotice({ kind: "ok", text: CONNECTED });
    await refresh();
    await onConnected?.();
    await refreshEngine();
  }

  // Read through a function: the flag changes while the attempt is awaited.
  const wasCancelled = () => cancelled.current;

  async function connect() {
    if (busy !== null) return;
    cancelled.current = false;
    setNotice(null);
    setBusy("connecting");
    let failed: string | null = null;
    try {
      const result = await commands.connectOpenrouter();
      if (result.status === "error") failed = messageOf(result.error);
    } catch {
      failed = BROKEN;
    }
    setBusy(null);
    if (failed === null) {
      await saved();
    } else if (!wasCancelled()) {
      setNotice({ kind: "error", text: failed });
    }
  }

  /** Cancels the wait. `false` if it was too late: the key is being saved. */
  async function cancel(): Promise<boolean> {
    cancelled.current = true;
    try {
      const result = await commands.cancelConnect();
      if (result.status === "ok" && !result.data) {
        // The key has already arrived and is being saved, which always
        // finishes: say so, keep the exits shut, and report how it ends.
        cancelled.current = false;
        setBusy((now) => (now === "connecting" ? "saving" : now));
        return false;
      }
    } catch {
      // The attempt ends by itself at its own deadline.
    }
    return true;
  }

  /**
   * Every way out (Done, Esc, Back, "this Mac only") asks this first: a sign-in
   * still waiting is cancelled, and the answer says whether leaving is safe.
   * `false` while a key is being saved: stay, the window shows the outcome.
   */
  async function canLeave(): Promise<boolean> {
    if (busy === "saving" || busy === "disconnecting") return false;
    if (busy === "connecting") return await cancel();
    return true;
  }

  /** "Paste a key instead": tested, then saved only if it works. */
  async function pasteKey(typed: string) {
    if (busy !== null) return;
    const key = typed.trim();
    const problem = validateKey(key);
    if (problem !== null) {
      setNotice({ kind: "error", text: problem });
      return;
    }
    setNotice(null);
    setBusy("saving");
    let failed: string | null = null;
    try {
      const tested = await commands.testApiKey(PROVIDER, key);
      if (tested.status === "error") {
        failed = tested.error.message;
      } else {
        const stored = await commands.setApiKey(PROVIDER, key);
        if (stored.status === "error") failed = `${KEY_NOT_SAVED}: ${messageOf(stored.error)}`;
      }
    } catch {
      failed = BROKEN;
    }
    setBusy(null);
    if (failed === null) await saved();
    else setNotice({ kind: "error", text: failed });
  }

  async function disconnect() {
    if (busy !== null) return;
    setNotice(null);
    setBusy("disconnecting");
    let failed: string | null = null;
    try {
      const result = await commands.deleteApiKey(PROVIDER);
      if (result.status === "error") failed = messageOf(result.error);
    } catch {
      failed = BROKEN;
    }
    setBusy(null);
    setNotice(
      failed === null
        ? { kind: "ok", text: "Disconnected. The key is removed." }
        : { kind: "error", text: failed },
    );
    await refresh();
    await refreshEngine();
  }

  function addCredits() {
    void commands.openLink("openrouter_credits").catch(() => undefined);
  }

  return {
    status,
    notice,
    busy,
    /** The last sign-in or paste worked in this panel. */
    justConnected: notice?.text === CONNECTED,
    connect,
    cancel,
    canLeave,
    pasteKey,
    disconnect,
    addCredits,
    refresh,
    clearNotice: () => {
      setNotice(null);
    },
  };
}

export type Connection = ReturnType<typeof useConnection>;

/**
 * The panel: the explainer and Connect OpenRouter, or what is connected with
 * Disconnect and Add credits; "Paste a key instead" under either.
 */
export function ConnectPanel({
  connection,
  canDisconnect = true,
}: {
  connection: Connection;
  /** Setup has no Disconnect; it stays in Settings. */
  canDisconnect?: boolean;
}) {
  const { status, notice, busy } = connection;
  const [pasting, setPasting] = useState(false);
  const [typed, setTyped] = useState("");
  const connected = status.state === "connected";
  const working = busy !== null;
  return (
    <div className="own-key-panel" id="connect-panel">
      {connected ? (
        <>
          <p id="connect-status" className="hint">
            {`Connected: ${status.hint}`}
          </p>
        </>
      ) : (
        <p className="hint">{EXPLAINER}</p>
      )}
      <div className="row own-key-actions">
        {busy === "connecting" ? (
          <>
            <span role="status">Waiting for OpenRouter in your browser…</span>
            <button type="button" onClick={() => void connection.cancel()}>
              Cancel
            </button>
          </>
        ) : busy === "saving" ? (
          <span role="status">Saving your key…</span>
        ) : (
          <button
            type="button"
            className="primary"
            disabled={working || status.state === "checking"}
            onClick={() => void connection.connect()}
          >
            {connected ? "Connect again" : "Connect OpenRouter"}
          </button>
        )}
        <button type="button" onClick={connection.addCredits}>
          Add credits
        </button>
        {canDisconnect && connected && (
          <button
            type="button"
            className="destructive"
            disabled={working}
            onClick={() => void connection.disconnect()}
          >
            {busy === "disconnecting" ? "Disconnecting…" : "Disconnect"}
          </button>
        )}
        {status.state === "error" && (
          <button type="button" onClick={() => void connection.refresh()}>
            Retry
          </button>
        )}
      </div>
      {notice !== null && (
        <p
          id="connect-notice"
          role={notice.kind === "error" ? "alert" : "status"}
          className={`notice ${notice.kind}`}
        >
          {notice.text}
        </p>
      )}
      {status.state === "error" && (
        <p role="alert" className="notice error">
          Couldn&apos;t check for a saved key: {status.message}
        </p>
      )}
      {pasting ? (
        <div className="panel-row">
          <label htmlFor="api-key">{PROVIDER_NAME} API key</label>
          <input
            id="api-key"
            type="password"
            value={typed}
            autoComplete="off"
            autoCapitalize="off"
            autoCorrect="off"
            spellCheck={false}
            placeholder="Paste your key"
            readOnly={busy === "saving"}
            onChange={(event) => {
              setTyped(event.target.value);
              connection.clearNotice();
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter") void connection.pasteKey(typed);
            }}
          />
          <button
            type="button"
            disabled={working}
            onClick={() => {
              void connection.pasteKey(typed).then(() => {
                setTyped("");
              });
            }}
          >
            {busy === "saving" ? "Saving…" : "Test & Save"}
          </button>
        </div>
      ) : (
        <button
          type="button"
          className="link"
          onClick={() => {
            setPasting(true);
          }}
        >
          Paste a key instead
        </button>
      )}
    </div>
  );
}

/**
 * The connection as a sheet over the window (U12): modal, Tab stays inside,
 * Esc and Done close it, except while a key is being written.
 */
export function ConnectSheet({
  connection,
  onDone,
}: {
  connection: Connection;
  onDone: () => void;
}) {
  const sheet = useRef<HTMLDivElement>(null);
  const writing = connection.busy === "saving" || connection.busy === "disconnecting";
  useEffect(() => {
    sheet.current?.querySelector<HTMLElement>("button.primary")?.focus();
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
        <h2 id="key-sheet-title">OpenRouter</h2>
        <ConnectPanel connection={connection} />
        <div className="confirm-actions">
          <button type="button" disabled={writing} onClick={onDone}>
            Done
          </button>
        </div>
      </div>
    </>
  );
}
