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
import { chosenModel, modelWait, spokenName } from "./logic";
import { StepFrame } from "./StepFrame";
import type { FrameParts } from "./StepFrame";

export const FINISH_REASON =
  "Start dictating unlocks once your choice is saved. Try the switch again.";
export const EVENTS_CAPTION =
  "Anonymous usage events (with a random install ID) and error reports have been on since setup began, so we can see where setup goes wrong and fix what breaks. Never audio, transcripts, typed text, keys or app names. Off stops them from now on and deletes what's waiting and the install ID; it can't recall what was already sent.";
/** Off may be a stored choice, with nothing ever sent, so this can't say "since setup began". */
export const EVENTS_OFF_CAPTION =
  "Off: nothing more is sent, and there is no install ID. On shares anonymous usage events and error reports, so we can see where setup goes wrong and fix what breaks. Never audio, transcripts, typed text, keys or app names.";
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

/** "On this Mac · Parakeet English · ✓ Ready", or where it really is. */
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
  const name = model === null ? "On-device model" : spokenName(model);
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
      </dl>
      {status.try_outcome !== "passed" && <p className="ob-attention">{NOT_TESTED}</p>}
      <p className="ob-menubar">
        Find Vocet in the menu bar <span aria-hidden="true">↗</span>
      </p>
      <div className="ob-card">
        <div className="ob-card-head">
          <span className="ob-card-title" id="ob-login-label">
            Open Vocet when I log in
          </span>
          <Switch
            id="ob-login"
            checked={atLogin}
            labelledBy="ob-login-label"
            onChange={(enabled) => void setLogin(enabled)}
          />
        </div>
        <SaveStatus save={login.save} />
      </div>
      <div className="ob-card">
        <div className="ob-card-head">
          <span className="ob-card-title" id="ob-share-ready-label">
            Share usage and error events
          </span>
          <Switch
            id="ob-share-ready"
            checked={sharing.checked}
            disabled={settings === null}
            labelledBy="ob-share-ready-label"
            describedBy="ob-share-ready-caption"
            onChange={(share) => void sharing.setSharing(share)}
          />
        </div>
        <p className="ob-caption" id="ob-share-ready-caption">
          {sharing.checked ? EVENTS_CAPTION : EVENTS_OFF_CAPTION}
        </p>
        <SaveStatus save={sharing.save} />
        <WhatsSent />
        <p className="ob-caption">Change any time in Settings → Privacy.</p>
      </div>
    </StepFrame>
  );
}
