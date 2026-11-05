// Welcome's demo as data (U11): what the real pill shows at each moment of the
// loop, and what the box holds. An illustration: it hears nothing and reads no
// level from the app. The timings are the real pill's order, so the demo doesn't
// promise instant text.

import { HIDDEN, LEVEL_POLL_MS, SILENT_LEVELS, pushLevel } from "../overlay/view";
import type { OverlayView } from "../overlay/view";

/** The words that land, and the message they answer. */
export const DEMO_QUESTION = "Can you send the notes before Friday?";
export const DEMO_WORDS = "Yes, I’ll send them tomorrow morning.";

/** The three captions; which one is lit says where the demo is. */
export const CAPTIONS = [
  "Hold the key",
  "Say what you mean",
  "Let go. The words land at your cursor",
] as const;

/** The demo moves on at the rate the real meter does. */
export const DEMO_TICK_MS = LEVEL_POLL_MS;

/** When each part of the loop ends, in ms from its start. */
export const HOLD_END_MS = 900;
export const SPEAK_END_MS = 3000;
export const TRANSCRIBE_END_MS = 4000;
/** The real pill keeps its success label for 0.8 s. */
export const DONE_END_MS = 4800;
export const LOOP_MS = 8000;

/**
 * The sentence's loudness in dBFS, one level per tick, inside the real meter's
 * range (`METER_LOW_DBFS`..`METER_HIGH_DBFS`): each syllable rises and falls,
 * with short gaps between words and a longer one at the comma. Fixed on purpose:
 * bars from random numbers would look like noise, not like someone talking.
 */
export const SPEECH_LEVELS: readonly number[] = [
  // "Yes,"
  -36, -26, -21, -24, -33, -46, -49, -47,
  // "I'll"
  -34, -24, -23, -31, -44,
  // "send"
  -33, -23, -20, -25, -35, -45,
  // "them"
  -32, -27, -34, -46,
  // "tomorrow"
  -35, -30, -43, -30, -22, -21, -29, -28, -25, -36, -44,
  // "morning."
  -31, -23, -24, -30, -29, -33, -42, -49,
];

export interface DemoFrame {
  /** The caption to light; `null` lights all three (Reduce Motion). */
  beat: 0 | 1 | 2 | null;
  view: OverlayView;
  levels: readonly number[];
  /** The words are in the box. */
  landed: boolean;
}

/**
 * The loop, one frame per tick. Frames that look the same are the same object,
 * so React skips them: outside the speaking part nothing renders.
 */
function buildFrames(): readonly DemoFrame[] {
  const listening: OverlayView = { state: "recording" };
  const held: DemoFrame = { beat: 0, view: listening, levels: SILENT_LEVELS, landed: false };
  const frames: DemoFrame[] = [];
  let levels = SILENT_LEVELS;
  SPEECH_LEVELS.forEach((level) => {
    levels = pushLevel(levels, level);
    frames.push({ beat: 1, view: listening, levels, landed: false });
  });
  const speaking = frames.splice(0);
  const transcribing: DemoFrame = {
    beat: 2,
    view: { state: "transcribing" },
    levels: SILENT_LEVELS,
    landed: false,
  };
  const done: DemoFrame = {
    beat: 2,
    view: { state: "paste_sent" },
    levels: SILENT_LEVELS,
    landed: true,
  };
  const rest: DemoFrame = { beat: 2, view: HIDDEN, levels: SILENT_LEVELS, landed: true };
  for (let at = 0; at < LOOP_MS; at += DEMO_TICK_MS) {
    if (at < HOLD_END_MS) frames.push(held);
    else if (at < SPEAK_END_MS) {
      const index = Math.floor((at - HOLD_END_MS) / DEMO_TICK_MS);
      frames.push(speaking[index] ?? rest);
    } else if (at < TRANSCRIBE_END_MS) frames.push(transcribing);
    else if (at < DONE_END_MS) frames.push(done);
    else frames.push(rest);
  }
  return frames;
}

export const DEMO_FRAMES: readonly DemoFrame[] = buildFrames();

/**
 * With Reduce Motion: where the loop ends, the words in the box and the pill
 * gone, with every caption lit since no moment is the current one.
 */
export const FINAL_FRAME: DemoFrame = {
  beat: null,
  view: HIDDEN,
  levels: SILENT_LEVELS,
  landed: true,
};

/** The frame at `elapsed` ms after the loop started; it goes round forever. */
export function frameAt(elapsed: number): DemoFrame {
  const tick = Math.floor(elapsed / DEMO_TICK_MS) % DEMO_FRAMES.length;
  return DEMO_FRAMES[tick] ?? FINAL_FRAME;
}
