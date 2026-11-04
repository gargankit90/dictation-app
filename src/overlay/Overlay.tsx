import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  HIDDEN,
  OVERLAY_EVENT,
  OVERLAY_LEVEL_EVENT,
  SILENT_LEVELS,
  announceListening,
  barHeight,
  parseLevel,
  parseView,
  pushLevel,
} from "./view";
import type { OverlayView } from "./view";
import "./overlay.css";

/** The hint while listening (U4). Esc cancels a recording (#40). */
export const LISTENING_HINT = "Release to paste · Esc to cancel";

/** Whether the view is a recording's, which shows the meter. */
function metered(view: OverlayView): boolean {
  return view.state === "recording" || view.state === "no_sound";
}

/**
 * The overlay window's content: follows {@link OVERLAY_EVENT}, and the input
 * level ({@link OVERLAY_LEVEL_EVENT}) while recording. The window itself is
 * shown and hidden from Rust; this only draws the state, and draws nothing
 * while hidden. Once it listens it says so through the document title
 * ({@link announceListening}), and the app sends the current state again.
 */
export function Overlay() {
  const [view, setView] = useState<OverlayView>(HIDDEN);
  const [levels, setLevels] = useState<readonly number[]>(SILENT_LEVELS);
  const current = useRef<OverlayView>(HIDDEN);

  useEffect(() => {
    let active = true;
    const views = listen<unknown>(OVERLAY_EVENT, (event) => {
      if (!active) {
        return;
      }
      const next = parseView(event.payload);
      // A new recording starts with a flat meter.
      if (metered(next) && !metered(current.current)) {
        setLevels(SILENT_LEVELS);
      }
      current.current = next;
      setView(next);
    });
    const meter = listen<unknown>(OVERLAY_LEVEL_EVENT, (event) => {
      const dbfs = parseLevel(event.payload);
      if (active && dbfs !== null && metered(current.current)) {
        setLevels((levels) => pushLevel(levels, dbfs));
      }
    });
    const subscribed = Promise.all([views, meter]);
    // Anything sent before now was lost: ask for the current view (#119).
    subscribed.then(
      () => {
        if (active) {
          announceListening();
        }
      },
      () => undefined,
    );
    return () => {
      active = false;
      for (const listener of [views, meter]) {
        listener.then(
          (unlisten) => {
            unlisten();
          },
          () => undefined,
        );
      }
    };
  }, []);

  return <OverlayContent view={view} levels={levels} />;
}

/**
 * The level meter: one bar per level, newest on the right. Decorative for
 * assistive tech; the label says what matters.
 */
export function Meter({ levels, idle }: { levels: readonly number[]; idle: boolean }) {
  return (
    <span
      className={idle ? "overlay-meter overlay-meter-idle" : "overlay-meter"}
      aria-hidden="true"
    >
      {levels.map((dbfs, index) => (
        <i
          // A fixed set of bars; the index is the bar.
          key={index}
          className="overlay-bar"
          data-testid="overlay-bar"
          style={{ height: `${String(barHeight(dbfs))}px` }}
        />
      ))}
    </span>
  );
}

/** One state, drawn as the pill. Never holds a button or a link. */
export function OverlayContent({
  view,
  levels = SILENT_LEVELS,
}: {
  view: OverlayView;
  levels?: readonly number[];
}) {
  switch (view.state) {
    case "hidden":
      return null;
    case "recording":
      return (
        <div className="overlay overlay-recording" role="status" data-state="recording">
          <span className="overlay-dot" aria-hidden="true" />
          <Meter levels={levels} idle={false} />
          <span className="overlay-label">Listening</span>
          <span className="overlay-hint">{LISTENING_HINT}</span>
        </div>
      );
    case "no_sound":
      return (
        <div className="overlay overlay-no-sound" role="status" data-state="no_sound">
          <span className="overlay-dot" aria-hidden="true" />
          <Meter levels={levels} idle={true} />
          <span className="overlay-label">No sound</span>
          <span className="overlay-hint">
            {view.device === undefined
              ? "Check your microphone"
              : `Check your microphone (${view.device})`}
          </span>
        </div>
      );
    case "transcribing":
      return (
        <div className="overlay overlay-transcribing" role="status" data-state="transcribing">
          <span className="overlay-spinner" aria-hidden="true" />
          <span className="overlay-label">Transcribing…</span>
        </div>
      );
    case "pasted":
      return (
        <div className="overlay overlay-pasted" role="status" data-state="pasted">
          <span className="overlay-check" aria-hidden="true">
            ✓
          </span>
          <span className="overlay-label">Pasted</span>
        </div>
      );
    case "error":
      return (
        <div className="overlay overlay-error" role="alert" data-state="error">
          <span className="overlay-icon" aria-hidden="true">
            !
          </span>
          <span className="overlay-message">{view.message}</span>
        </div>
      );
  }
}
