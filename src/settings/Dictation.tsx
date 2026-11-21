import { useCallback, useEffect, useState } from "react";
import { commands } from "../bindings";
import type { Settings_Serialize } from "../bindings";
import { HotkeyRecorder } from "../HotkeyRecorder";
import { saveHotkey, setHotkeyRecording } from "../saveHotkey";
import { Group, Row, SaveStatus, useSave } from "./ui";

/** How often the Dictation key row asks whether the hotkey still waits. */
export const HOTKEY_NOTICE_POLL_MS = 2_000;

interface Props {
  settings: Settings_Serialize;
  /** The app's settings after a change (what Settings shows from now). */
  onStored: (settings: Settings_Serialize) => void;
}

/**
 * Dictation: the dictation key (K1's recorder, applied at once), and where
 * the microphone is chosen.
 */
export function Dictation({ settings, onStored }: Props) {
  const hotkey = useSave();
  const [hotkeyNotice, setHotkeyNotice] = useState<string | null>(null);

  const readHotkeyNotice = useCallback(async () => {
    try {
      const result = await commands.getHotkeyNotice();
      if (result.status === "ok") setHotkeyNotice(result.data);
    } catch {
      // What is shown stays; the next read tries again.
    }
  }, []);

  // A hotkey waiting for Accessibility isn't a failed save, so no save says so:
  // the row does, until the key listens. That happens by itself once it's
  // allowed in System Settings, so the row keeps asking.
  useEffect(() => {
    const first = window.setTimeout(() => void readHotkeyNotice(), 0);
    const timer = window.setInterval(() => void readHotkeyNotice(), HOTKEY_NOTICE_POLL_MS);
    return () => {
      window.clearTimeout(first);
      window.clearInterval(timer);
    };
  }, [readHotkeyNotice]);

  async function recordHotkey(spec: string): Promise<string | null> {
    const outcome: { problem: string | null } = { problem: null };
    await hotkey.track(async () => {
      const result = await saveHotkey(spec);
      if (result.settings !== null) onStored(result.settings);
      outcome.problem = result.error;
      // The recorder shows its own message; the row shows only "Saved".
      return result.error === null ? null : "";
    });
    if (outcome.problem !== null) hotkey.clear();
    void readHotkeyNotice();
    return outcome.problem;
  }

  return (
    <>
      <Group>
        <Row
          label="Dictation key"
          labelId="hotkey-label"
          caption="Hold it to talk, release to paste."
          below={
            <>
              <SaveStatus save={hotkey.save} />
              {hotkeyNotice !== null && (
                <p role="status" className="notice attention">
                  {hotkeyNotice}
                </p>
              )}
            </>
          }
        >
          <HotkeyRecorder
            value={settings.hotkey}
            onChange={recordHotkey}
            onRecording={setHotkeyRecording}
            describedBy="hotkey-label"
          />
        </Row>
      </Group>
      <Group title="Microphone">
        <Row
          label="The Mac's default input"
          caption="Vocet uses the input chosen in System Settings → Sound. Change it there."
        />
      </Group>
    </>
  );
}
