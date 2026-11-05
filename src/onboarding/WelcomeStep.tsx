import { useState } from "react";
import { commands } from "../bindings";
import type { Settings_Serialize } from "../bindings";
import { keycapLabel } from "../hotkeyKeys";
import { messageOf } from "../keyTest";
import { WhatsSent } from "../settings/Privacy";
import { SaveStatus, Switch, useSave } from "../settings/ui";
import { StepFrame } from "./StepFrame";
import type { FrameParts } from "./StepFrame";

export const EVENTS_CAPTION =
  "Anonymous usage events (with a random install ID) and error reports help us fix what breaks. Never audio, transcripts, typed text, keys or app names.";

/** Leaving Welcome starts sending, so the switch has to be saved first. */
export const SAVING_REASON = "Saving your choice…";
export const SAVE_FAILED_REASON =
  "Continue unlocks once your choice is saved. Try the switch again.";

interface Props {
  parts: FrameParts;
  settings: Settings_Serialize | null;
  onStored: (settings: Settings_Serialize) => void;
  onContinue: () => void;
}

/**
 * Welcome: what the app does, and the events switch (A1's command). The app
 * sends nothing until the user continues past this screen, and nothing at
 * all if they switch it off here (`ConsentGate`).
 */
export function WelcomeStep({ parts, settings, onStored, onContinue }: Props) {
  const sharing = useSave();
  const [shown, setShown] = useState<boolean | null>(null);
  const checked = shown ?? settings?.share_usage_events ?? true;
  const key = keycapLabel(settings?.hotkey ?? "RightAlt");

  async function setSharing(share: boolean) {
    setShown(share);
    await sharing.track(async () => {
      const result = await commands.setShareUsageEvents(share);
      const reread = await commands.getSettings();
      if (reread.status === "ok") onStored(reread.data);
      return result.status === "error" ? messageOf(result.error) : null;
    });
    setShown(null);
  }

  return (
    <StepFrame
      parts={parts}
      title="Talk instead of type"
      primary={{
        label: "Continue",
        onClick: onContinue,
        blockedBecause:
          settings === null
            ? "Loading…"
            : sharing.save.state === "saving"
              ? SAVING_REASON
              : sharing.save.state === "error"
                ? SAVE_FAILED_REASON
                : null,
      }}
    >
      <div className="ob-demo">
        <kbd className="keycap">{key}</kbd>
        <span>Hold {key}, speak, release, and the text lands at your cursor.</span>
      </div>
      <div className="ob-card">
        <div className="ob-card-head">
          <span className="ob-card-title" id="ob-share-label">
            Share usage and error events
          </span>
          <Switch
            id="ob-share"
            checked={checked}
            labelledBy="ob-share-label"
            describedBy="ob-share-caption"
            onChange={(share) => void setSharing(share)}
          />
        </div>
        <p className="ob-caption" id="ob-share-caption">
          {EVENTS_CAPTION}
        </p>
        <SaveStatus save={sharing.save} />
        <WhatsSent />
        <p className="ob-caption">Change any time in Settings → Privacy.</p>
      </div>
    </StepFrame>
  );
}
