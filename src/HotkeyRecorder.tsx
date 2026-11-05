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
  /**
   * Told when recording starts (`true`, and again every `RENEW_MS` while it
   * lasts) and ends (`false`), however it ends: the key chosen, Esc, Cancel,
   * the window losing focus or hiding, or this component going away. The app
   * uses it to keep the real hotkey from dictating meanwhile.
   */
  onRecording?: (recording: boolean) => void;
  /**
   * Settings' row (U12): the keycap and one "Change…" on one line, and only
   * while recording "Use default" and "Cancel". Without it, setup's layout.
   */
  compact?: boolean;
  /** What describes the compact row's button, by id. */
  describedBy?: string;
  /**
   * Try it (U11): the same row with a small plain "Change key" and no "Use
   * default". The step puts the focus back in its box, so the button doesn't
   * take it.
   */
  setup?: boolean;
}

/** How often recording tells the app it still goes on (its pause lapses). */
export const RENEW_MS = 10_000;

/**
 * The dictation key as a keycap, with "Record a different key": the next key
 * or combination pressed while this window is focused becomes the key. It
 * listens to the page's own key events, so it needs no extra permission.
 */
export function HotkeyRecorder({
  value,
  onChange,
  disabled = false,
  onRecording,
  compact = false,
  describedBy,
  setup = false,
}: Props) {
  const [recording, setRecording] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const change = useRef(onChange);
  const tell = useRef(onRecording);
  const changeButton = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    change.current = onChange;
    tell.current = onRecording;
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
      const step = recorderStep(state, {
        type,
        code: event.code,
        flags: {
          ctrl: event.ctrlKey,
          alt: event.altKey,
          shift: event.shiftKey,
          meta: event.metaKey,
        },
      });
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
    const hidden = () => {
      if (document.visibilityState === "hidden") setRecording(false);
    };
    tell.current?.(true);
    const renew = window.setInterval(() => {
      tell.current?.(true);
    }, RENEW_MS);
    window.addEventListener("keydown", down, true);
    window.addEventListener("keyup", up, true);
    window.addEventListener("blur", cancel);
    document.addEventListener("visibilitychange", hidden);
    return () => {
      window.clearInterval(renew);
      tell.current?.(false);
      document.removeEventListener("visibilitychange", hidden);
      window.removeEventListener("keydown", down, true);
      window.removeEventListener("keyup", up, true);
      window.removeEventListener("blur", cancel);
    };
  }, [recording]);

  // When recording ends, the button that started it is back; the keyboard goes there.
  const wasRecording = useRef(false);
  useEffect(() => {
    if (wasRecording.current && !recording && !setup) changeButton.current?.focus();
    wasRecording.current = recording;
  }, [recording, setup]);

  if (compact || setup) {
    const buttonClass = setup ? "link" : undefined;
    return (
      <div className={setup ? "hotkey-recorder compact setup" : "hotkey-recorder compact"}>
        <div className="recorder-line">
          <kbd className={recording ? "keycap recording" : "keycap"} aria-label="Dictation key">
            {recording ? "Press a key…" : keycapLabel(value)}
          </kbd>
          {recording ? (
            <>
              {!setup && (
                <button
                  type="button"
                  disabled={value === DEFAULT_HOTKEY}
                  onClick={() => {
                    setRecording(false);
                    setProblem(null);
                    void apply(DEFAULT_HOTKEY);
                  }}
                >
                  Use default
                </button>
              )}
              <button
                type="button"
                className={buttonClass}
                onClick={() => {
                  setRecording(false);
                }}
              >
                Cancel
              </button>
            </>
          ) : (
            <button
              ref={changeButton}
              type="button"
              className={buttonClass}
              aria-describedby={describedBy}
              disabled={disabled || busy}
              onClick={() => {
                setProblem(null);
                setRecording(true);
              }}
            >
              {setup ? "Change key" : "Change…"}
            </button>
          )}
        </div>
        {recording && (
          <p role="status" className="hint">
            A modifier alone works too, such as the right ⌘. Esc cancels.
          </p>
        )}
        <p role={problem === null ? undefined : "alert"} className="error">
          {problem}
        </p>
      </div>
    );
  }

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
