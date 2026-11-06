import { useEffect, useState } from "react";
import { commands } from "../bindings";
import type {
  EngineStatus,
  LocalModelInfo_Serialize,
  OnboardingStatus,
  Settings_Serialize,
} from "../bindings";
import { keycapLabel } from "../hotkeyKeys";
import { BROKEN, messageOf } from "../keyTest";
import { providerInfo } from "../providers";
import { WhatsSent } from "../settings/Privacy";
import type { SharingSwitch } from "../settings/Privacy";
import { SaveStatus, Switch, useSave } from "../settings/ui";
import { modelTitle } from "../settings/modelRow";
import { chosenModel, modelWait } from "./logic";
import { StepFrame } from "./StepFrame";
import type { FrameParts } from "./StepFrame";

export const FINISH_REASON =
  "Start dictating unlocks once your choice is saved. Try the switch again.";
const NEVER = "Never audio, transcripts, typed text, keys or app names.";
const OFF_STOPS = "Off stops them and deletes what's waiting.";
const CHANGE = "Change it any time in Settings → Privacy.";
/** On, and it has been for all of setup. */
export const EVENTS_CAPTION = `On since setup began: anonymous usage events (with a random install ID) and error reports, so we can see where setup goes wrong. ${NEVER} ${OFF_STOPS} ${CHANGE}`;
/** On, after being off at some point of setup: "since setup began" would be false. */
export const EVENTS_FROM_NOW_CAPTION = `On from now: anonymous usage events (with a random install ID) and error reports, so we can see where things go wrong. ${NEVER} ${OFF_STOPS} ${CHANGE}`;
export const EVENTS_OFF_CAPTION = `Off: nothing more is sent, and there's no install ID. ${CHANGE}`;
export const NOT_TESTED = "Not tested yet: try it from the menu bar any time";

interface Props {
  parts: FrameParts;
  status: OnboardingStatus;
  settings: Settings_Serialize | null;
  onStored: (settings: Settings_Serialize) => void;
  sharing: SharingSwitch;
  models: LocalModelInfo_Serialize[] | null;
  engine: EngineStatus | null;
  onChangeHistory: () => void;
  onFinish: () => void;
}

/** What the history limit means, in words. */
export function historyLine(limit: number): string {
  if (limit === 0) return "Off";
  return `Keeps your last ${String(limit)} dictations on this Mac`;
}

/** "On this Mac · English · ✓ Ready", or where it really is. */
function useEngineLine(
  settings: Settings_Serialize | null,
  models: LocalModelInfo_Serialize[] | null,
  engine: EngineStatus | null,
): string {
  const [keySaved, setKeySaved] = useState<boolean | null>(null);
  const provider = settings?.provider ?? null;
  const cloud = settings?.transcription === "cloud";
  useEffect(() => {
    if (!cloud || provider === null) return;
    const state = { cancelled: false };
    void (async () => {
      let saved = false;
      try {
        const result = await commands.hasApiKey(provider);
        saved = result.status === "ok" && result.data;
      } catch {
        // Not connected, as far as this line can tell.
      }
      if (!state.cancelled) setKeySaved(saved);
    })();
    return () => {
      state.cancelled = true;
    };
  }, [cloud, provider]);

  if (settings === null) return "Loading…";
  if (settings.transcription === "cloud") {
    const name = providerInfo(settings.provider).name;
    if (keySaved === null) return `${name} · Checking…`;
    if (!keySaved) return `${name} · ! Needs a key`;
    return engine?.state === "failed"
      ? `${name} · ! ${engine.message ?? "Not working"}`
      : `${name} · ✓ Connected`;
  }
  const model = chosenModel(settings, models);
  const name = model === null ? "On-device model" : modelTitle(model);
  const wait = modelWait(settings, models, engine);
  return `On this Mac · ${name} · ${wait ?? "✓ Ready"}`;
}

/**
 * Ready: the real states, the menu-bar hint, launch at login, the sharing
 * switch (the only one in setup), and the way out.
 */
export function ReadyStep({
  parts,
  status,
  settings,
  onStored,
  sharing,
  models,
  engine,
  onChangeHistory,
  onFinish,
}: Props) {
  const login = useSave();
  const [shownLogin, setShownLogin] = useState<boolean | null>(null);
  const engineLine = useEngineLine(settings, models, engine);
  const atLogin = shownLogin ?? settings?.launch_at_login ?? false;

  // The app keeps whether sharing was off in this walk through setup, across a
  // quit. It is read once a second, so a switch turned off and on again here
  // counts at once.
  const [offHere, setOffHere] = useState(false);
  if (settings !== null && !sharing.checked && !offHere) setOffHere(true);
  const caption = !sharing.checked
    ? EVENTS_OFF_CAPTION
    : status.sharing_on_throughout && !offHere
      ? EVENTS_CAPTION
      : EVENTS_FROM_NOW_CAPTION;

  async function setLogin(enabled: boolean) {
    setShownLogin(enabled);
    await login.track(async () => {
      try {
        const result = await commands.setLaunchAtLogin(enabled);
        const reread = await commands.getSettings();
        if (reread.status === "ok") onStored(reread.data);
        return result.status === "error" ? messageOf(result.error) : null;
      } catch {
        return BROKEN;
      }
    });
    setShownLogin(null);
  }

  return (
    <StepFrame
      parts={parts}
      title="You're set"
      primary={{
        label: "Start dictating",
        onClick: onFinish,
        blockedBecause: null,
      }}
    >
      <dl className="ob-summary">
        <div>
          <dt>Key</dt>
          <dd>{keycapLabel(settings?.hotkey ?? "RightAlt")} (hold)</dd>
        </div>
        <div>
          <dt>Speech</dt>
          <dd>{engineLine}</dd>
        </div>
        {settings !== null && (
          <div>
            <dt>History</dt>
            <dd>
              {historyLine(settings.history_limit)} ·{" "}
              <button
                type="button"
                className="link"
                disabled={parts.exitBlock !== null}
                onClick={onChangeHistory}
              >
                Change
              </button>
            </dd>
          </div>
        )}
        <div className="ob-summary-switch">
          <dt id="ob-login-label">Open Vocet when I log in</dt>
          <dd>
            <Switch
              id="ob-login"
              checked={atLogin}
              labelledBy="ob-login-label"
              onChange={(enabled) => void setLogin(enabled)}
            />
          </dd>
          <dd className="ob-summary-note">
            <SaveStatus save={login.save} />
          </dd>
        </div>
        <div className="ob-summary-switch ob-share-row">
          <dt id="ob-share-ready-label">Share usage and error events</dt>
          <dd>
            <Switch
              id="ob-share-ready"
              checked={sharing.checked}
              disabled={settings === null}
              labelledBy="ob-share-ready-label"
              describedBy="ob-share-ready-caption"
              onChange={(share) => void sharing.setSharing(share)}
            />
          </dd>
          <dd className="ob-summary-note">
            <p className="ob-caption" id="ob-share-ready-caption">
              {settings === null ? null : caption}
            </p>
            <SaveStatus save={sharing.save} />
            <WhatsSent />
          </dd>
        </div>
      </dl>
      <div className="ob-notes">
        {status.try_outcome !== "passed" && <p className="ob-attention">{NOT_TESTED}</p>}
        <p className="ob-menubar">
          Find Vocet in the menu bar <span aria-hidden="true">↗</span>
        </p>
      </div>
    </StepFrame>
  );
}
