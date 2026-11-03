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
}

type Checks = { state: "loading" } | { state: "ready"; checks: Check[] } | { state: "error" };

/**
 * Troubleshoot (E1): live checks with a ✓ or ✗ and a fix button each, a
 * "Test connection" button that is the only thing that contacts the
 * provider, and Report a problem, which shows the exact report before it
 * can be saved, copied or opened as a GitHub issue. Nothing here records
 * audio or sends anything by itself.
 */
export function Troubleshoot({ onBack, onOpenSettings }: Props) {
  const [checks, setChecks] = useState<Checks>({ state: "loading" });
  const [connection, setConnection] = useState<Notice | null>(null);
  const [testing, setTesting] = useState(false);
  const [report, setReport] = useState<string | null>(null);
  const [reportNotice, setReportNotice] = useState<Notice | null>(null);
  const [building, setBuilding] = useState(false);
  // Bumped to run the checks again.
  const [round, setRound] = useState(0);
  const load = useCallback(() => {
    setRound((current) => current + 1);
  }, []);

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
        setConnection({
          kind: ok ? "ok" : "error",
          text: code === null ? message : `${message} (${code})`,
        });
      } else {
        setConnection({ kind: "error", text: messageOf(result.error) });
      }
    } catch {
      setConnection({ kind: "error", text: BROKEN });
    } finally {
      setTesting(false);
      load();
    }
  }

  async function buildReport() {
    setBuilding(true);
    setReportNotice(null);
    try {
      const result = await commands.buildReport();
      if (result.status === "ok") setReport(result.data);
      else setReportNotice({ kind: "error", text: messageOf(result.error) });
    } catch {
      setReportNotice({ kind: "error", text: BROKEN });
    } finally {
      setBuilding(false);
    }
  }

  async function saveReport() {
    try {
      const result = await commands.saveReport();
      if (result.status === "error")
        setReportNotice({ kind: "error", text: messageOf(result.error) });
      else if (result.data !== null)
        setReportNotice({ kind: "ok", text: `Saved as ${result.data}.` });
    } catch {
      setReportNotice({ kind: "error", text: BROKEN });
    }
  }

  async function copyReport() {
    if (report === null) return;
    try {
      await navigator.clipboard.writeText(report);
      setReportNotice({ kind: "ok", text: "Copied." });
    } catch {
      setReportNotice({
        kind: "error",
        text: "Couldn't copy. Select the text and copy it yourself.",
      });
    }
  }

  async function openIssue() {
    try {
      const result = await commands.openIssue();
      setReportNotice(
        result.status === "ok"
          ? { kind: "ok", text: "Opened in your browser. Read it there, then submit it yourself." }
          : { kind: "error", text: messageOf(result.error) },
      );
    } catch {
      setReportNotice({ kind: "error", text: BROKEN });
    }
  }

  return (
    <main className="settings troubleshoot">
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
                  <p className="hint">
                    {check.detail}
                    {check.code != null && check.id !== "last_error" ? ` (${check.code})` : null}
                  </p>
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
              </li>
            );
          })}
        </ul>
      ) : null}

      <div className="row">
        <button type="button" onClick={load}>
          Check again
        </button>
        <button
          type="button"
          disabled={testing}
          onClick={() => {
            void testConnection();
          }}
        >
          {testing ? "Testing…" : "Test connection"}
        </button>
      </div>
      <p className="hint">
        Test connection sends a short silent clip to your provider with your saved key, only when
        you press it. It does nothing for the model on this Mac.
      </p>
      {connection !== null ? (
        <p role="status" className={`notice ${connection.kind}`}>
          {connection.text}
        </p>
      ) : null}

      <h2>Report a problem</h2>
      <p className="hint">
        Builds a short report from a fixed list of facts: versions, permissions, your hotkey and
        engine, and the codes and times of recent errors. No audio, text, keys, app names, file
        paths or logs. Nothing is sent: you read it first, then save it, copy it or open a GitHub
        issue that you submit yourself.
      </p>
      {report === null ? (
        <div className="row">
          <button
            type="button"
            disabled={building}
            onClick={() => {
              void buildReport();
            }}
          >
            {building ? "Building…" : "Preview report"}
          </button>
        </div>
      ) : (
        <>
          <pre className="report" aria-label="Report" tabIndex={0}>
            {report}
          </pre>
          <div className="row">
            <button
              type="button"
              onClick={() => {
                void saveReport();
              }}
            >
              Save report…
            </button>
            <button
              type="button"
              onClick={() => {
                void copyReport();
              }}
            >
              Copy
            </button>
            <button
              type="button"
              className="primary"
              onClick={() => {
                void openIssue();
              }}
            >
              Open a GitHub issue
            </button>
          </div>
        </>
      )}
      {reportNotice !== null ? (
        <p role="status" className={`notice ${reportNotice.kind}`}>
          {reportNotice.text}
        </p>
      ) : null}

      <div className="row actions">
        <button type="button" onClick={onBack}>
          Back to Settings
        </button>
      </div>
    </main>
  );
}
