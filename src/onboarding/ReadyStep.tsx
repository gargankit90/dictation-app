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
import { SaveStatus, Switch, useSave } from "../settings/ui";
import { chosenModel, modelWait, spokenName } from "./logic";
import { StepFrame } from "./StepFrame";
import type { FrameParts } from "./StepFrame";

export const NOT_TESTED = "Not tested yet: try it from the menu bar any time";

interface Props {
  parts: FrameParts;
  status: OnboardingStatus;
  settings: Settings_Serialize | null;
  onStored: (settings: Settings_Serialize) => void;
  models: LocalModelInfo_Serialize[] | null;
  engine: EngineStatus | null;
  onChangeSharing: () => void;
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

/** Ready: the real states, the menu-bar hint, launch at login, and the way out. */
export function ReadyStep({
  parts,
  status,
  settings,
  onStored,
  models,
  engine,
  onChangeSharing,
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
      primary={{ label: "Start dictating", onClick: onFinish, blockedBecause: null }}
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
        <div>
          <dt>Usage and error events</dt>
          <dd>
            {settings?.share_usage_events === false ? "Off" : "On"} ·{" "}
            <button type="button" className="link" onClick={onChangeSharing}>
              Change
            </button>
          </dd>
        </div>
        {settings !== null && (
          <div>
            <dt>History</dt>
            <dd>
              {historyLine(settings.history_limit)} ·{" "}
              <button type="button" className="link" onClick={onChangeHistory}>
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
    </StepFrame>
  );
}
