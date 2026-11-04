import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  HIDDEN,
  LEVEL_COMMAND,
  LEVEL_POLL_MS,
  OVERLAY_EVENT,
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
 * Reads the input level for the meter while `on` (U4 round 2): on animation
 * frames, at most once per {@link LEVEL_POLL_MS}, and never while a read is
 * still out, so a page that stalls (or an app that doesn't answer) asks for
 * nothing more. Stops as soon as `on` goes false or the page goes; a late
 * answer is dropped. Each recording starts flat.
 */
function useLevels(on: boolean): readonly number[] {
  const [levels, setLevels] = useState<readonly number[]>(SILENT_LEVELS);

  useEffect(() => {
    if (!on) {
      return;
    }
    let active = true;
    let waiting = false;
    let last = Number.NEGATIVE_INFINITY;
    let frame = 0;
    const tick = (now: number) => {
      if (!active) {
        return;
      }
      if (!waiting && now - last >= LEVEL_POLL_MS) {
        waiting = true;
        last = now;
        invoke<unknown>(LEVEL_COMMAND).then(
          (value) => {
            waiting = false;
            const dbfs = parseLevel(value);
            if (active && dbfs !== null) {
              setLevels((levels) => pushLevel(levels, dbfs));
            }
          },
          () => {
            waiting = false;
          },
        );
      }
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => {
      active = false;
      cancelAnimationFrame(frame);
      // The meter went: the next recording starts flat.
      setLevels(SILENT_LEVELS);
    };
  }, [on]);

  return levels;
}

/**
 * The overlay window's content: follows {@link OVERLAY_EVENT}, and reads the
 * input level ({@link LEVEL_COMMAND}) while it shows the meter. The window
 * itself is shown and hidden from Rust; this only draws the state, and draws
 * nothing while hidden. Once it listens it says so through the document title
 * ({@link announceListening}), and the app sends the current state again.
 */
export function Overlay() {
  const [view, setView] = useState<OverlayView>(HIDDEN);
  const levels = useLevels(metered(view));

  useEffect(() => {
    let active = true;
    const views = listen<unknown>(OVERLAY_EVENT, (event) => {
      if (active) {
        setView(parseView(event.payload));
      }
    });
    // Anything sent before now was lost: ask for the current view (#119).
    views.then(
      () => {
        if (active) {
          announceListening();
        }
      },
      () => undefined,
    );
    return () => {
      active = false;
      views.then(
        (unlisten) => {
          unlisten();
        },
        () => undefined,
      );
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
    case "paste_sent":
      // "sent", not "pasted": the inserter can't tell whether the focused
      // app took the text (U4 review).
      return (
        <div className="overlay overlay-paste-sent" role="status" data-state="paste_sent">
          <span className="overlay-check" aria-hidden="true">
            ✓
          </span>
          <span className="overlay-label">Paste sent</span>
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
