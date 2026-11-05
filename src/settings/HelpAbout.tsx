import { useEffect, useState } from "react";
import { commands } from "../bindings";
import type { Link } from "../bindings";
import { BROKEN, messageOf } from "../keyTest";
import { Group, Row } from "./ui";

/** What Help's buttons open; the app supplies each one it supports. */
export interface HelpActions {
  onTroubleshoot?: () => void;
  onReport?: () => void;
  /** Shows the log file; resolves to an error message, or `null`. */
  onShowLog?: () => Promise<string | null>;
  onSetup?: () => void;
}

/** Help: Troubleshoot, Report a problem, the log file, Run setup again. */
export function Help({ onTroubleshoot, onReport, onShowLog, onSetup }: HelpActions) {
  const [logError, setLogError] = useState<string | null>(null);
  return (
    <Group>
      {onTroubleshoot === undefined ? null : (
        <Row
          label="Troubleshoot"
          labelId="troubleshoot-label"
          caption="Checks the microphone, Accessibility, the dictation key and the speech engine."
        >
          <button type="button" aria-describedby="troubleshoot-label" onClick={onTroubleshoot}>
            Open…
          </button>
        </Row>
      )}
      {onReport === undefined ? null : (
        <Row
          label="Report a problem"
          labelId="report-label"
          caption="Builds a short report you can read before sending. Nothing is sent by itself."
        >
          <button type="button" aria-describedby="report-label" onClick={onReport}>
            Preview…
          </button>
        </Row>
      )}
      {onShowLog === undefined ? null : (
        <Row
          label="Log file"
          labelId="log-label"
          caption="Shows Vocet's log in Finder. It never holds your words or your keys."
          below={
            logError === null ? null : (
              <p role="alert" className="error row-error">
                {logError}
              </p>
            )
          }
        >
          <button
            type="button"
            aria-describedby="log-label"
            onClick={() => {
              setLogError(null);
              void onShowLog()
                .then(setLogError)
                .catch(() => {
                  setLogError(BROKEN);
                });
            }}
          >
            Show in Finder
          </button>
        </Row>
      )}
      {onSetup === undefined ? null : (
        <Row
          label="Run setup again"
          labelId="setup-label"
          caption="Go through the first-run steps again: speech, permissions and a test."
        >
          <button type="button" aria-describedby="setup-label" onClick={onSetup}>
            Start…
          </button>
        </Row>
      )}
    </Group>
  );
}

function LinkButton({ link, children }: { link: Link; children: string }) {
  const [error, setError] = useState<string | null>(null);
  return (
    <>
      <button
        type="button"
        onClick={() => {
          setError(null);
          void commands
            .openLink(link)
            .then((result) => {
              if (result.status === "error") setError(messageOf(result.error));
            })
            .catch(() => {
              setError(BROKEN);
            });
        }}
      >
        {children}
      </button>
      {error === null ? null : (
        <span role="alert" className="error">
          {" "}
          {error}
        </span>
      )}
    </>
  );
}

/** About: the version, the licence, the source, the credits. */
export function About() {
  const [version, setVersion] = useState<string | null>(null);
  useEffect(() => {
    void commands
      .getAppInfo()
      .then((result) => {
        if (result.status === "ok") setVersion(result.data.version);
      })
      .catch(() => undefined);
  }, []);
  return (
    <Group>
      <Row label="Vocet" caption={version === null ? "Version…" : `Version ${version}`} />
      <Row label="Licence" caption="Free and open source under the MIT licence.">
        <LinkButton link="licence">Read the licence</LinkButton>
      </Row>
      <Row label="Source code" caption="github.com/gargankit90/dictation-app">
        <LinkButton link="repository">Open on GitHub</LinkButton>
      </Row>
      <Row label="Credits" caption="Code adapted from other open-source projects, with thanks.">
        <LinkButton link="notice">Show credits</LinkButton>
      </Row>
    </Group>
  );
}
