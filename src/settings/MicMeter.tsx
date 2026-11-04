// U4's level meter, as Settings' microphone test and onboarding's Microphone
// card show it.

import { useEffect, useState } from "react";
import { commands } from "../bindings";
import { BROKEN, messageOf } from "../keyTest";
import { SILENT_LEVELS, barHeight, pushLevel } from "../overlay/view";

/** How often the test's level is read: as often as the pill's (U4). */
export const MIC_POLL_MS = 50;

/** The meter: U4's bars, newest on the right. Decorative; the text says what matters. */
export function MicMeter({ levels }: { levels: readonly number[] }) {
  return (
    <span className="mic-meter" aria-hidden="true">
      {levels.map((dbfs, index) => (
        <i
          // A fixed set of bars; the index is the bar.
          key={index}
          data-testid="mic-bar"
          style={{ height: `${String(barHeight(dbfs))}px` }}
        />
      ))}
    </span>
  );
}

/**
 * Listens for as long as `active`: the live levels, and the input's name.
 * The app ends each test after a few seconds (`mic_test::MAX_TEST`), so a
 * reading with no level starts the next one. Stops when no longer active.
 */
export function useLiveMeter(active: boolean) {
  const [levels, setLevels] = useState<readonly number[]>(SILENT_LEVELS);
  const [device, setDevice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!active) return;
    const state = { stopped: false, starting: false, listening: false };
    const start = async () => {
      if (state.starting) return;
      state.starting = true;
      try {
        const started = await commands.startMicrophoneTest();
        if (state.stopped) return;
        if (started.status === "error") {
          setError(messageOf(started.error));
          return;
        }
        setError(null);
        state.listening = true;
      } catch {
        if (!state.stopped) setError(BROKEN);
      } finally {
        state.starting = false;
      }
    };
    const read = async () => {
      if (!state.listening) {
        await start();
        return;
      }
      try {
        const reading = await commands.readMicrophoneTest();
        if (state.stopped || reading.status !== "ok") return;
        if (reading.data.device !== null) setDevice(reading.data.device);
        const level = reading.data.level;
        if (level === null) {
          // That test ended; the next tick starts another.
          state.listening = false;
          return;
        }
        setLevels((now) => pushLevel(now, level));
      } catch {
        // The next reading tries again.
      }
    };
    const timer = window.setInterval(() => void read(), MIC_POLL_MS);
    return () => {
      state.stopped = true;
      window.clearInterval(timer);
      void commands.stopMicrophoneTest().catch(() => undefined);
    };
  }, [active]);

  return { levels, device, error };
}
