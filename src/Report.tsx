import { useEffect, useRef, useState } from "react";
import { commands } from "./bindings";
import { BROKEN, messageOf } from "./keyTest";
import type { Notice } from "./keyTest";

interface Props {
  /** Says where Back goes: "Back to Settings", "Back to Troubleshoot". */
  backLabel: string;
  onBack: () => void;
}

/**
 * Report a problem (E1, its own screen since U12): the exact report, built
 * when the screen opens, and only then the choice to save it, copy it or open
 * a GitHub issue. Nothing is sent by itself.
 */
export function Report({ backLabel, onBack }: Props) {
  const [report, setReport] = useState<string | null>(null);
  const [notice, setNotice] = useState<Notice | null>(null);
  const [building, setBuilding] = useState(true);

  async function build() {
    setBuilding(true);
    setNotice(null);
    try {
      const result = await commands.buildReport();
      if (result.status === "ok") setReport(result.data);
      else setNotice({ kind: "error", text: messageOf(result.error) });
    } catch {
      setNotice({ kind: "error", text: BROKEN });
    } finally {
      setBuilding(false);
    }
  }

  const started = useRef(false);
  useEffect(() => {
    if (started.current) return;
    started.current = true;
    void build();
  }, []);

  async function save() {
    try {
      const result = await commands.saveReport();
      if (result.status === "error") setNotice({ kind: "error", text: messageOf(result.error) });
      else if (result.data !== null) setNotice({ kind: "ok", text: `Saved as ${result.data}.` });
    } catch {
      setNotice({ kind: "error", text: BROKEN });
    }
  }

  async function copy() {
    if (report === null) return;
    try {
      await navigator.clipboard.writeText(report);
      setNotice({ kind: "ok", text: "Copied." });
    } catch {
      setNotice({ kind: "error", text: "Couldn't copy. Select the text and copy it yourself." });
    }
  }

  async function openIssue() {
    try {
      const result = await commands.openIssue();
      setNotice(
        result.status === "ok"
          ? { kind: "ok", text: "Opened in your browser. Read it there, then submit it yourself." }
          : { kind: "error", text: messageOf(result.error) },
      );
    } catch {
      setNotice({ kind: "error", text: BROKEN });
    }
  }

  return (
    <main className="settings troubleshoot">
      <h1>Report a problem</h1>
      <p className="hint">
        A short report from a fixed list of facts: versions, permissions, your dictation key and
        speech engine, and the codes and times of recent errors. No audio, text, keys, app names,
        file paths or logs. Nothing is sent: you read it first, then save it, copy it or open a
        GitHub issue that you submit yourself.
      </p>
      {report === null ? (
        building ? (
          <p>Building…</p>
        ) : (
          <div className="row">
            <button
              type="button"
              onClick={() => {
                void build();
              }}
            >
              Try again
            </button>
          </div>
        )
      ) : (
        <>
          <pre className="report" aria-label="Report" tabIndex={0}>
            {report}
          </pre>
          <div className="row">
            <button
              type="button"
              onClick={() => {
                void save();
              }}
            >
              Save report…
            </button>
            <button
              type="button"
              onClick={() => {
                void copy();
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
      {notice !== null ? (
        <p role={notice.kind === "error" ? "alert" : "status"} className={`notice ${notice.kind}`}>
          {notice.text}
        </p>
      ) : null}

      <div className="row actions">
        <button type="button" onClick={onBack}>
          {backLabel}
        </button>
      </div>
    </main>
  );
}
