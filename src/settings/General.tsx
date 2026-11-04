import { useCallback, useEffect, useRef, useState } from "react";
import { commands } from "../bindings";
import type { AppInfo } from "../bindings";
import { BROKEN, messageOf } from "../keyTest";
import { Group, Row, Switch } from "./ui";

/** When updates were last checked, in words. */
export function lastChecked(ms: number | null, now: number = Date.now()): string {
  if (ms === null) return "Not checked yet since Vocet started.";
  const minutes = Math.floor((now - ms) / 60_000);
  if (minutes < 1) return "Last checked just now.";
  if (minutes < 60) return `Last checked ${String(minutes)} min ago.`;
  const at = new Date(ms).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  return `Last checked at ${at}.`;
}

/**
 * Open Vocet at login (S1): applied in the background, "Applying…" until
 * the OS answers, then its real state or an inline error. Held by the
 * Settings window, not the General pane, so switching sections while the
 * OS works loses nothing.
 */
export function useLaunchAtLogin(stored: boolean) {
  // The user's choice while it applies, then the OS's answer, until the
  // settings are read again; `null`: what was read.
  const [answer, setLaunchAtLogin] = useState<boolean | null>(null);
  const [seenStored, setSeenStored] = useState(stored);
  // The OS can take tens of seconds (#238); nothing waits for it.
  const [launchPending, setLaunchPending] = useState(false);
  const [launchError, setLaunchError] = useState<string | null>(null);
  // Counts the choices, so an answer to an older one is ignored.
  const launchChoice = useRef(0);

  // Settings read again (the window came back): follow the OS, unless the
  // user is in the middle of changing it.
  if (stored !== seenStored && !launchPending) {
    setSeenStored(stored);
    setLaunchAtLogin(null);
  }
  const launchAtLogin = answer ?? stored;

  async function changeLaunch(want: boolean) {
    const choice = ++launchChoice.current;
    setLaunchAtLogin(want);
    setLaunchPending(true);
    setLaunchError(null);
    let actual: boolean | null = null;
    let error: string | null = null;
    try {
      const result = await commands.setLaunchAtLogin(want);
      if (result.status === "ok") actual = result.data;
      else error = messageOf(result.error);
    } catch {
      error = BROKEN;
    }
    if (choice !== launchChoice.current) return;
    if (actual === null) {
      try {
        const current = await commands.getSettings();
        if (current.status === "ok") actual = current.data.launch_at_login;
      } catch {
        // Keep the user's choice shown; the error says it may not have applied.
      }
      if (choice !== launchChoice.current) return;
    }
    if (actual !== null) setLaunchAtLogin(actual);
    setLaunchError(error);
    setLaunchPending(false);
  }
  return { launchAtLogin, launchPending, launchError, changeLaunch };
}

export type LaunchAtLogin = ReturnType<typeof useLaunchAtLogin>;

/**
 * General: Open Vocet at login and Check for updates.
 */
export function General({ launch }: { launch: LaunchAtLogin }) {
  const { launchAtLogin, launchPending, launchError, changeLaunch } = launch;
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [checking, setChecking] = useState(false);

  const readInfo = useCallback(async () => {
    try {
      const result = await commands.getAppInfo();
      if (result.status === "ok") setInfo(result.data);
    } catch {
      // Keep what is shown.
    }
  }, []);

  useEffect(() => {
    const first = window.setTimeout(() => void readInfo(), 0);
    const onFocus = () => void readInfo();
    window.addEventListener("focus", onFocus);
    return () => {
      window.clearTimeout(first);
      window.removeEventListener("focus", onFocus);
    };
  }, [readInfo]);

  async function checkForUpdates() {
    setChecking(true);
    try {
      await commands.checkForUpdates();
    } catch {
      // The tray's check shows its own dialog; nothing to add here.
    }
    // The check answers in a dialog; its time shows once it is back.
    window.setTimeout(() => {
      setChecking(false);
      void readInfo();
    }, 3_000);
  }

  return (
    <Group>
      <Row
        label="Open Vocet at login"
        labelId="launch-at-login-label"
        caption="Starts Vocet in the menu bar when you log in. You can also change it in System Settings → General → Login Items."
        captionId="launch-at-login-hint"
        below={
          launchError === null ? null : (
            <p role="alert" className="error row-error">
              {launchError}
            </p>
          )
        }
      >
        {launchPending && (
          <span role="status" className="row-note">
            Applying…
          </span>
        )}
        <Switch
          id="launch-at-login"
          checked={launchAtLogin}
          labelledBy="launch-at-login-label"
          describedBy="launch-at-login-hint"
          onChange={(want) => void changeLaunch(want)}
        />
      </Row>
      <Row
        label="Check for updates"
        labelId="updates-label"
        caption={info === null ? undefined : lastChecked(info.last_update_check_ms ?? null)}
      >
        <button type="button" disabled={checking} onClick={() => void checkForUpdates()}>
          {checking ? "Checking…" : "Check now"}
        </button>
      </Row>
    </Group>
  );
}
