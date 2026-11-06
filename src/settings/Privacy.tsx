import { useLayoutEffect, useRef, useSyncExternalStore } from "react";
import { commands } from "../bindings";
import type { Settings_Serialize } from "../bindings";
import { sharingState, subscribeSharing, writeSharing } from "./sharingQueue";
import { Group, Row, SaveStatus, Switch, useSave } from "./ui";
import type { SaveState } from "./ui";

/** What sharing sends, in words, with a link to the full notice. */
export function WhatsSent() {
  return (
    <details className="row-details">
      <summary>What&apos;s sent</summary>
      <p className="hint">
        Sent while this is on, from the first setup screen. Off stops it at once and deletes what is
        waiting and the ID below; it can&apos;t recall what was already sent.
      </p>
      <ul className="hint">
        <li>That the app was opened, with its version, your macOS version and chip.</li>
        <li>Which setup steps you reached, where you stopped, and that you finished.</li>
        <li>
          What got in the way in setup, as a fixed reason: a failed download or key test, a
          permission that was refused (which one), a paste that missed, a skipped practice. Never
          the details.
        </li>
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
 * The sharing switch, for any view that shows it. The writes go through the
 * one queue in `sharingQueue`, which outlives the view. `save` stays "saving"
 * while any view's write is waiting, and holds the last failure until a later
 * write succeeds, whichever view made them.
 */
export function useSharingSwitch(
  stored: boolean,
  onStored: (settings: Settings_Serialize) => void,
) {
  const sharing = useSave();
  const queue = useSyncExternalStore(subscribeSharing, sharingState);

  // Settings read back after a write reach every view, once each: an older
  // copy must not overwrite what the view has learned since.
  const latestOnStored = useRef(onStored);
  const seen = useRef(queue.settings);
  useLayoutEffect(() => {
    latestOnStored.current = onStored;
  });
  useLayoutEffect(() => {
    if (queue.settings !== null && queue.settings !== seen.current) {
      seen.current = queue.settings;
      latestOnStored.current(queue.settings);
    }
  }, [queue.settings]);

  async function setSharing(share: boolean) {
    await sharing.track(() => writeSharing(share));
  }

  const save: SaveState = queue.pending
    ? { state: "saving" }
    : queue.problem !== null
      ? { state: "error", message: queue.problem }
      : sharing.save.state === "saved"
        ? sharing.save
        : { state: "idle" };

  return {
    save,
    checked: queue.pending && queue.latest !== null ? queue.latest : stored,
    setSharing,
  };
}

export type SharingSwitch = ReturnType<typeof useSharingSwitch>;

/**
 * What a step's way forward must say while the switch isn't settled: leaving
 * can send events, so it waits for the user's choice to be saved. `failed` is
 * what to say about a failed write, or `null` if this step can't hold on one.
 */
export function sharingBlock(save: SaveState, failed: string | null): string | null {
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
