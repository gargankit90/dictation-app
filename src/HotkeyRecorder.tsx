import { useEffect, useRef, useState } from "react";
import { DEFAULT_HOTKEY, INITIAL_RECORDER, keycapLabel, recorderStep } from "./hotkeyKeys";
import type { RecorderState } from "./hotkeyKeys";

interface Props {
  /** The dictation key now, as the stored spec text (`RightAlt`). */
  value: string;
  /**
   * Called with the spec the user recorded (or the default, for Reset). It
   * applies the key and returns a plain message if it couldn't, in which
   * case the old key stays.
   */
  onChange: (spec: string) => Promise<string | null>;
  disabled?: boolean;
}

/**
 * The dictation key as a keycap, with "Record a different key": the next key
 * or combination pressed while this window is focused becomes the key. It
 * listens to the page's own key events, so it needs no extra permission.
 */
export function HotkeyRecorder({ value, onChange, disabled = false }: Props) {
  const [recording, setRecording] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const change = useRef(onChange);
  useEffect(() => {
    change.current = onChange;
  });

  async function apply(spec: string) {
    setBusy(true);
    try {
      setProblem(await change.current(spec));
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => {
    if (!recording) return;
    let state: RecorderState = INITIAL_RECORDER;
    const handle = (type: "down" | "up") => (event: KeyboardEvent) => {
      // Nothing typed or shortcut while recording reaches the page.
      event.preventDefault();
      event.stopPropagation();
      if (event.repeat) return;
      const step = recorderStep(state, { type, code: event.code });
      state = step.state;
      const outcome = step.outcome;
      if (outcome === undefined) return;
      if (outcome.kind === "refused") {
        setProblem(outcome.reason); // stays recording: try another key
        return;
      }
      setRecording(false);
      if (outcome.kind === "recorded") void apply(outcome.spec);
    };
    const down = handle("down");
    const up = handle("up");
    // A modifier released in another window would never reach us.
    const cancel = () => {
      setRecording(false);
    };
    window.addEventListener("keydown", down, true);
    window.addEventListener("keyup", up, true);
    window.addEventListener("blur", cancel);
    return () => {
      window.removeEventListener("keydown", down, true);
      window.removeEventListener("keyup", up, true);
      window.removeEventListener("blur", cancel);
    };
  }, [recording]);

  return (
    <div className="hotkey-recorder">
      <kbd className={recording ? "keycap recording" : "keycap"} aria-label="Dictation key">
        {recording ? "Press a key…" : keycapLabel(value)}
      </kbd>
      <div className="row">
        {recording ? (
          <button
            type="button"
            onClick={() => {
              setRecording(false);
            }}
          >
            Cancel
          </button>
        ) : (
          <button
            type="button"
            disabled={disabled || busy}
            onClick={() => {
              setProblem(null);
              setRecording(true);
            }}
          >
            Record a different key
          </button>
        )}
        <button
          type="button"
          disabled={disabled || busy || recording || value === DEFAULT_HOTKEY}
          onClick={() => {
            setProblem(null);
            void apply(DEFAULT_HOTKEY);
          }}
        >
          Reset to default
        </button>
      </div>
      {recording && (
        <p role="status" className="hint">
          Press the key or combination to use. A modifier alone works too, such as the right ⌘. Esc
          cancels.
        </p>
      )}
      <p role={problem === null ? undefined : "alert"} className="error">
        {problem}
      </p>
    </div>
  );
}
