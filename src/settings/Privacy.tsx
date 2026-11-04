import { useState } from "react";
import { commands } from "../bindings";
import type { Settings_Serialize } from "../bindings";
import { messageOf } from "../keyTest";
import { Group, Row, SaveStatus, Switch, useSave } from "./ui";

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
  const sharing = useSave();
  // Shown at once; the stored value comes back after the write.
  const [shown, setShown] = useState<boolean | null>(null);
  const checked = shown ?? settings.share_usage_events;

  async function setSharing(share: boolean) {
    setShown(share);
    await sharing.track(async () => {
      const result = await commands.setShareUsageEvents(share);
      if (result.status === "ok") onStored({ ...settings, share_usage_events: share });
      return result.status === "error" ? messageOf(result.error) : null;
    });
    setShown(null);
  }

  return (
    <Group>
      <Row
        label="Share usage and error events"
        labelId="share-usage-events-label"
        caption="Helps us see what works and what breaks, and fix crashes. Never your voice, your text, your API keys or the apps you use. Turning it off stops sending at once and clears what was waiting."
        captionId="share-usage-events-hint"
        below={
          <>
            <SaveStatus save={sharing.save} />
            <details className="row-details">
              <summary>What&apos;s sent</summary>
              <ul className="hint">
                <li>That the app was opened, with its version, your macOS version and chip.</li>
                <li>Which onboarding steps you reached, and that you finished.</li>
                <li>Which speech engine you chose (for example Groq or OpenAI).</li>
                <li>
                  That a dictation worked, with its length and speed in rough ranges (not the
                  words).
                </li>
                <li>That a dictation failed, and the kind of failure (not the message).</li>
                <li>
                  Error reports, sent to Sentry (US): when the app crashes (a panic) or a dictation
                  fails, the error code (for example mic_denied), the app version, your macOS major
                  version and chip, and for a crash the code locations and function names where it
                  happened. Never the error message, your words, file paths, your user name or
                  computer name.
                </li>
                <li>Which setting you changed (not the new value).</li>
                <li>That the app updated, from which version to which.</li>
              </ul>
              <p className="hint">
                Each event carries a random ID made on this Mac, not tied to you or your computer.
                Turning sharing off deletes it.
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
