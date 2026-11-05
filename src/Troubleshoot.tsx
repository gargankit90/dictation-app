import { useCallback, useEffect, useState } from "react";
import { commands } from "./bindings";
import type { Check, FixAction } from "./bindings";
import { BROKEN, messageOf } from "./keyTest";
import type { Notice } from "./keyTest";

interface Props {
  /** Back to the settings form. */
  onBack: () => void;
  /** Opens the settings form (the fix for a missing key or model). */
  onOpenSettings: () => void;
  /** Opens Report a problem, which a failed check links to. */
  onReport: () => void;
}

type Checks = { state: "loading" } | { state: "ready"; checks: Check[] } | { state: "error" };

/** A code, out of the way until someone wants it. It stays in the report. */
function Details({ children }: { children: string }) {
  return (
    <details className="row-details">
      <summary>Details</summary>
      <code>{children}</code>
    </details>
  );
}

/** When the relaunch command itself can't be reached: what to do instead. */
const RELAUNCH_FAILED = "Couldn't relaunch Vocet. Quit it from the menu bar and open it again.";

/**
 * Troubleshoot (E1, U12): live checks with a ✓ or ✗ and a fix button each,
 * and a "Test connection" button that is the only thing that contacts the
 * provider. Codes sit behind Details; a failed check links to Report a
 * problem. Nothing here records audio or sends anything by itself.
 */
export function Troubleshoot({ onBack, onOpenSettings, onReport }: Props) {
  const [checks, setChecks] = useState<Checks>({ state: "loading" });
  const [connection, setConnection] = useState<(Notice & { code: string | null }) | null>(null);
  const [testing, setTesting] = useState(false);
  // Why Relaunch Vocet did nothing, with what to do instead.
  const [relaunchError, setRelaunchError] = useState<string | null>(null);
  // Bumped to run the checks again.
  const [round, setRound] = useState(0);
  const load = useCallback(() => {
    setRound((current) => current + 1);
  }, []);
  // The model on this Mac: there is no connection to test (U1), so the
  // engine row's readiness line stands in for the button.
  const localModel =
    checks.state === "ready"
      ? (checks.checks.find((check) => check.id === "engine")?.local_model ?? null)
      : null;
  const anyFailed =
    checks.state === "error" || (checks.state === "ready" && checks.checks.some((c) => !c.ok));

  useEffect(() => {
    const state = { cancelled: false };
    void (async () => {
      try {
        const result = await commands.runChecks();
        if (state.cancelled) return;
        setChecks(
          result.status === "ok" ? { state: "ready", checks: result.data } : { state: "error" },
        );
      } catch {
        if (!state.cancelled) setChecks({ state: "error" });
      }
    })();
    return () => {
      state.cancelled = true;
    };
  }, [round]);

  // A fix happens in System Settings: check again when the user returns.
  useEffect(() => {
    const refresh = () => {
      if (document.visibilityState !== "hidden") load();
    };
    window.addEventListener("focus", refresh);
    document.addEventListener("visibilitychange", refresh);
    return () => {
      window.removeEventListener("focus", refresh);
      document.removeEventListener("visibilitychange", refresh);
    };
  }, [load]);

  async function fix(action: FixAction) {
    try {
      switch (action) {
        case "open_microphone_settings":
          await commands.requestPermission("microphone");
          break;
        case "open_accessibility_settings":
          await commands.requestPermission("accessibility");
          break;
        case "open_settings":
          onOpenSettings();
          break;
        case "open_troubleshoot":
        case "try_again":
          break;
      }
    } catch {
      // The checks below say what is still wrong.
    }
    load();
  }

  async function testConnection() {
    setTesting(true);
    setConnection(null);
    try {
      const result = await commands.testConnection();
      if (result.status === "ok") {
        const { ok, code, message } = result.data;
        setConnection({ kind: ok ? "ok" : "error", text: message, code });
      } else {
        setConnection({ kind: "error", text: messageOf(result.error), code: null });
      }
    } catch {
      setConnection({ kind: "error", text: BROKEN, code: null });
    } finally {
      setTesting(false);
      load();
    }
  }

  async function relaunch() {
    setRelaunchError(null);
    try {
      const result = await commands.relaunchApp();
      if (result.status === "error") setRelaunchError(messageOf(result.error));
    } catch {
      setRelaunchError(RELAUNCH_FAILED);
    }
  }

  return (
    <main className="settings troubleshoot">
      <div className="page-body">
        <h1>Troubleshoot</h1>
        <p className="hint">
          These checks read the state of this Mac. They don&apos;t record audio or contact anyone.
        </p>

        {checks.state === "loading" ? <p>Checking…</p> : null}
        {checks.state === "error" ? (
          <p role="alert" className="notice error">
            Couldn&apos;t run the checks. {BROKEN}
          </p>
        ) : null}
        {checks.state === "ready" ? (
          <ul className="checks" aria-label="Checks">
            {checks.checks.map((check) => {
              const action = check.ok ? null : (check.fix ?? null);
              const button =
                action !== null && action !== "try_again" && action !== "open_troubleshoot";
              return (
                <li key={check.id} className="check-row" data-ok={check.ok}>
                  <span
                    className={check.ok ? "saved" : "error"}
                    role="img"
                    aria-label={check.ok ? "OK" : "Problem"}
                  >
                    {check.ok ? "✓" : "✗"}
                  </span>
                  <div className="check-body">
                    <strong>{check.label}</strong>
                    <p className="hint">{check.detail}</p>
                    {check.details == null ? null : <Details>{check.details}</Details>}
                  </div>
                  {button ? (
                    <button
                      type="button"
                      onClick={() => {
                        void fix(action);
                      }}
                    >
                      {check.fix_label}
                    </button>
                  ) : null}
                  {!check.ok && check.code === "mic_denied" ? (
                    <button
                      type="button"
                      onClick={() => {
                        void relaunch();
                      }}
                    >
                      Relaunch Vocet
                    </button>
                  ) : null}
                </li>
              );
            })}
          </ul>
        ) : null}

        {relaunchError !== null ? (
          <p role="alert" className="notice error">
            {relaunchError}
          </p>
        ) : null}

        <div className="row">
          <button type="button" onClick={load}>
            Check again
          </button>
          {checks.state !== "loading" && localModel === null ? (
            <button
              type="button"
              disabled={testing}
              onClick={() => {
                void testConnection();
              }}
            >
              {testing ? "Testing…" : "Test connection"}
            </button>
          ) : null}
        </div>
        {localModel !== null ? (
          <p className="hint" data-testid="local-readiness">
            Model on this Mac. {localModel}
          </p>
        ) : (
          <p className="hint">
            Test connection sends a short silent clip to your provider with your saved key, only
            when you press it.
          </p>
        )}
        {connection !== null ? (
          <>
            <p role="status" className={`notice ${connection.kind}`}>
              {connection.text}
            </p>
            {connection.code === null ? null : <Details>{connection.code}</Details>}
          </>
        ) : null}

        {anyFailed ? (
          <p className="hint">
            Still stuck?{" "}
            <button type="button" className="link" onClick={onReport}>
              Report a problem…
            </button>
          </p>
        ) : null}
      </div>
      <div className="row actions page-footer">
        <button type="button" onClick={onBack}>
          Back to Settings
        </button>
      </div>
    </main>
  );
}
