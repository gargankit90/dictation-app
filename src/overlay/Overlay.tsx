import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { HIDDEN, OVERLAY_EVENT, announceListening, parseView } from "./view";
import type { OverlayView } from "./view";
import "./overlay.css";

/**
 * The overlay window's content: follows {@link OVERLAY_EVENT}. The window
 * itself is shown and hidden from Rust; this only draws the state, and
 * draws nothing while hidden. Once it listens it says so through the
 * document title ({@link announceListening}), and the app sends the current
 * state again.
 */
export function Overlay() {
  const [view, setView] = useState<OverlayView>(HIDDEN);

  useEffect(() => {
    let active = true;
    const subscribed = listen<unknown>(OVERLAY_EVENT, (event) => {
      if (active) {
        setView(parseView(event.payload));
      }
    });
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
      subscribed.then(
        (unlisten) => {
          unlisten();
        },
        () => undefined,
      );
    };
  }, []);

  return <OverlayContent view={view} />;
}

/** One state, drawn as a small pill. */
export function OverlayContent({ view }: { view: OverlayView }) {
  switch (view.state) {
    case "hidden":
      return null;
    case "recording":
      return (
        <div className="overlay overlay-recording" role="status" data-state="recording">
          <span className="overlay-dot" aria-hidden="true" />
          Recording
        </div>
      );
    case "transcribing":
      return (
        <div className="overlay overlay-transcribing" role="status" data-state="transcribing">
          <span className="overlay-spinner" aria-hidden="true" />
          Transcribing…
        </div>
      );
    case "error":
      return (
        <div className="overlay overlay-error" role="alert" data-state="error">
          <span className="overlay-icon" aria-hidden="true">
            !
          </span>
          <span className="overlay-text">
            <span className="overlay-message">{view.message}</span>
            {view.code === undefined ? null : <span className="overlay-code">{view.code}</span>}
          </span>
        </div>
      );
  }
}
