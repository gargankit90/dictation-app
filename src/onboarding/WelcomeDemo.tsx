import { useEffect, useState, useSyncExternalStore } from "react";
import { OverlayContent } from "../overlay/Overlay";
import { CAPTIONS, DEMO_QUESTION, DEMO_WORDS, FINAL_FRAME, frameAt } from "./demoScript";
import type { DemoFrame } from "./demoScript";

const REDUCE_MOTION = "(prefers-reduced-motion: reduce)";

function subscribeToMotion(changed: () => void): () => void {
  const query = window.matchMedia(REDUCE_MOTION);
  query.addEventListener("change", changed);
  return () => {
    query.removeEventListener("change", changed);
  };
}

/** Whether the system asks for less motion; follows it if it changes. */
function useReducedMotion(): boolean {
  return useSyncExternalStore(subscribeToMotion, () => window.matchMedia(REDUCE_MOTION).matches);
}

/**
 * The loop's current frame, or the final one with Reduce Motion. On animation
 * frames, so it stops by itself while the window is hidden, and stops for good
 * when the step goes. With Reduce Motion nothing is scheduled at all.
 */
function useDemoFrame(): DemoFrame {
  const reduced = useReducedMotion();
  const [frame, setFrame] = useState(() => frameAt(0));
  useEffect(() => {
    if (reduced) return;
    let start: number | null = null;
    let request = 0;
    const tick = (now: number) => {
      start ??= now;
      setFrame(frameAt(now - start));
      request = requestAnimationFrame(tick);
    };
    request = requestAnimationFrame(tick);
    return () => {
      cancelAnimationFrame(request);
    };
  }, [reduced]);
  return reduced ? FINAL_FRAME : frame;
}

/**
 * The demo card: a small app with the real pill over it, fed by a fixed script.
 * Only `OverlayContent` comes from the overlay, never `Overlay`, which listens
 * for the app's events and reads the microphone's level. Hidden from assistive
 * tech: the screen has one sentence that says the same.
 */
export function WelcomeDemo({ keyLabel }: { keyLabel: string }) {
  const { beat, view, levels, landed } = useDemoFrame();
  const lit = (index: number) => (beat === null || beat === index ? "ob-beat lit" : "ob-beat");
  return (
    <div className="ob-demo" aria-hidden="true">
      <div className="ob-demo-app">
        <div className="ob-demo-message">
          <span className="ob-demo-avatar">S</span>
          <span>{DEMO_QUESTION}</span>
        </div>
        <div className="ob-demo-box">
          {landed && <span className="ob-demo-words">{DEMO_WORDS}</span>}
          <span className="ob-demo-caret" />
        </div>
        <div className="ob-demo-pill">
          <OverlayContent view={view} levels={levels} />
        </div>
      </div>
      <ol className="ob-beats">
        <li className={lit(0)}>
          <span>{CAPTIONS[0]}</span>
          <kbd className="keycap">{keyLabel}</kbd>
        </li>
        <li className={lit(1)}>
          <span>{CAPTIONS[1]}</span>
        </li>
        <li className={lit(2)}>
          <span>{CAPTIONS[2]}</span>
        </li>
      </ol>
    </div>
  );
}
