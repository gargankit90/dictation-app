import { useEffect, useRef, useState } from "react";
import type { Settings_Serialize } from "../bindings";
import { HotkeyRecorder } from "../HotkeyRecorder";
import { saveHotkey, setHotkeyRecording } from "../saveHotkey";
import { StepFrame } from "./StepFrame";
import type { FrameParts } from "./StepFrame";

/** How often the paused hotkey's lease is renewed (the app's lease is 30 s). */
export const HOLD_RENEW_MS = 10_000;

/** What the step suggests saying. */
export const PRACTICE_WORDS = "Vocet turns what I say into text, in any app.";

interface Props {
  parts: FrameParts;
  settings: Settings_Serialize | null;
  onStored: (settings: Settings_Serialize) => void;
  /** Why practice has to wait, or `null` if it can run. */
  wait: string | null;
  onDone: () => void;
}

/**
 * Try it: the key, the words to say, a box the dictation pastes into, and
 * Done. Nothing checks the box: what landed there is for the user to see.
 */
export function TryStep({ parts, settings, onStored, wait, onDone }: Props) {
  const [box, setBox] = useState("");
  const field = useRef<HTMLTextAreaElement>(null);
  const blocked = wait !== null;

  // The paste goes wherever the focus is: put it in the box, so the first
  // dictation works without a click.
  const focusBox = () => {
    if (field.current !== null && !field.current.disabled) field.current.focus();
  };
  useEffect(() => {
    if (!blocked) field.current?.focus();
  }, [blocked]);

  // While practice waits for the model the key does nothing here. The hotkey
  // recorder's pause, renewed; it lapses by itself if this window stops.
  useEffect(() => {
    if (!blocked) return;
    setHotkeyRecording(true);
    const timer = window.setInterval(() => {
      setHotkeyRecording(true);
    }, HOLD_RENEW_MS);
    return () => {
      window.clearInterval(timer);
      setHotkeyRecording(false);
    };
  }, [blocked]);

  return (
    <StepFrame
      parts={parts}
      title="Try it"
      primary={{ label: "Done", onClick: onDone, blockedBecause: null }}
    >
      <div className="ob-key-row">
        <span className="ob-label">Your dictation key</span>
        <HotkeyRecorder
          setup
          value={settings?.hotkey ?? "RightAlt"}
          disabled={settings === null}
          onRecording={(now) => {
            // While practice waits, the key stays paused after recording too.
            setHotkeyRecording(now || blocked);
            if (!now) focusBox();
          }}
          onChange={async (spec) => {
            const result = await saveHotkey(spec);
            if (result.settings !== null) onStored(result.settings);
            focusBox();
            return result.error;
          }}
        />
      </div>
      <label htmlFor="try-field" className="ob-label">
        Hold the key, say: “{PRACTICE_WORDS}”, then let go.
      </label>
      <textarea
        id="try-field"
        className="ob-practice"
        ref={field}
        rows={3}
        value={box}
        disabled={blocked}
        placeholder={wait ?? undefined}
        onChange={(event) => {
          setBox(event.target.value);
        }}
      />
      {wait !== null && (
        <p role="status" className="ob-sr">
          {wait}
        </p>
      )}
    </StepFrame>
  );
}
