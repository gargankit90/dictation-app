import { useRef, useState } from "react";
import { commands } from "../bindings";
import type { Settings_Serialize } from "../bindings";
import { messageOf } from "../keyTest";
import { Group, Row, SaveStatus, Switch, useSave } from "./ui";
import type { SaveState } from "./ui";

/** What sharing sends, in words, with a link to the full notice. */
export function WhatsSent() {
  return (
    <details className="row-details">
      <summary>What&apos;s sent</summary>
      <ul className="hint">
        <li>That the app was opened, with its version, your macOS version and chip.</li>
        <li>Which onboarding steps you reached, and that you finished.</li>
        <li>Which speech engine you chose (for example Groq or OpenAI).</li>
        <li>That a dictation worked, with its length and speed in rough ranges (not the words).</li>
        <li>That a dictation failed, and the kind of failure (not the message).</li>
        <li>
          Error reports, sent to Sentry (US): when the app crashes (a panic) or a dictation fails,
          the error code (for example mic_denied), the app version, your macOS major version and
          chip, and for a crash the code locations and function names where it happened. Never the
          error message, your words, file paths, your user name or computer name.
        </li>
        <li>Which setting you changed (not the new value).</li>
        <li>That the app updated, from which version to which.</li>
      </ul>
      <p className="hint">
        Each event carries a random ID made on this Mac, not tied to you or your computer. Turning
        sharing off deletes it.
      </p>
      <button
        type="button"
        className="link"
        onClick={() => {
          void commands.openLink("privacy").catch(() => undefined);
        }}
      >
        Read the full privacy notice
      </button>
    </details>
  );
}

/**
 * The sharing switch's writes, one at a time and the last choice wins: the app
 * doesn't promise to apply overlapping commands in the order they were sent,
 * so a slow "on" could otherwise land after the user's final "off" (U8 review).
 * `save` stays "saving" until the queue is empty, and shows the outcome of the
 * last write.
 */
export function useSharingSwitch(
  stored: boolean,
  onStored: (settings: Settings_Serialize) => void,
) {
  const sharing = useSave();
  // Shown at once; the stored value comes back after the writes.
  const [shown, setShown] = useState<boolean | null>(null);
  const wanted = useRef<boolean | null>(null);
  const writing = useRef(false);

  async function setSharing(share: boolean) {
    setShown(share);
    wanted.current = share;
    if (writing.current) return;
    writing.current = true;
    await sharing.track(async () => {
      let problem: string | null = null;
      try {
        while (wanted.current !== null) {
          const next = wanted.current;
          wanted.current = null;
          const result = await commands.setShareUsageEvents(next);
          problem = result.status === "error" ? messageOf(result.error) : null;
        }
      } finally {
        wanted.current = null;
        writing.current = false;
      }
      // What the app stores now, not a copy of what this pane was given.
      const reread = await commands.getSettings();
      if (reread.status === "ok") onStored(reread.data);
      return problem;
    });
    setShown(null);
  }

  return { save: sharing.save, checked: shown ?? stored, setSharing };
}

export type SharingSwitch = ReturnType<typeof useSharingSwitch>;

/**
 * What a step's way forward must say while the switch isn't settled: leaving
 * can send events, so it waits for the user's choice to be saved.
 */
export function sharingBlock(save: SaveState, failed: string): string | null {
  if (save.state === "saving") return "Saving your choice…";
  return save.state === "error" ? failed : null;
}

/**
 * Privacy: "Share usage and error events" (A1's own command: off stops
 * sending at once), what is sent (from docs/privacy.md), and a link to the
 * full list.
 */
export function Privacy({
  settings,
  onStored,
}: {
  settings: Settings_Serialize;
  onStored: (settings: Settings_Serialize) => void;
}) {
  const { save, checked, setSharing } = useSharingSwitch(settings.share_usage_events, onStored);

  return (
    <Group>
      <Row
        label="Share usage and error events"
        labelId="share-usage-events-label"
        caption="Helps us see what works and what breaks, and fix crashes. Never your voice, your text, your API keys or the apps you use. Turning it off stops sending at once and clears what was waiting."
        captionId="share-usage-events-hint"
        below={
          <>
            <SaveStatus save={save} />
            <WhatsSent />
          </>
        }
      >
        <Switch
          id="share-usage-events"
          checked={checked}
          labelledBy="share-usage-events-label"
          describedBy="share-usage-events-hint"
          onChange={(share) => void setSharing(share)}
        />
      </Row>
    </Group>
  );
}
