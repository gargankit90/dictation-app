// The one queue for the sharing switch, for the whole window. The app doesn't
// promise to apply overlapping commands in the order they were sent, so a slow
// "on" could otherwise land after the user's final "off". Every view that
// shows the switch writes through here: one write at a time, the last choice
// wins, and it outlives the view that started it (U8 review rounds 2 and 5).

import { commands } from "../bindings";
import type { Settings_Serialize } from "../bindings";
import { BROKEN, messageOf } from "../keyTest";

export interface SharingQueueState {
  /** A write is running or waiting. */
  pending: boolean;
  /** The user's last choice, while `pending`. */
  latest: boolean | null;
  /** Why the last write failed, until a later one succeeds. */
  problem: string | null;
  /** What the app stores, as read after the last write. */
  settings: Settings_Serialize | null;
}

const IDLE: SharingQueueState = { pending: false, latest: null, problem: null, settings: null };

let state = IDLE;
let wanted: boolean | null = null;
let running: Promise<string | null> | null = null;
// Bumped by the tests' reset, so a write a finished test left waiting is ignored.
let generation = 0;
const listeners = new Set<() => void>();

function publish(next: Partial<SharingQueueState>) {
  state = { ...state, ...next };
  for (const listener of listeners) listener();
}

/** A function, since a write can set `wanted` during any `await` in `drain`. */
function choiceWaiting(): boolean {
  return wanted !== null;
}

async function drain(mine: number): Promise<string | null> {
  let problem: string | null = null;
  let settings: Settings_Serialize | null = state.settings;
  do {
    while (wanted !== null) {
      const next = wanted;
      wanted = null;
      try {
        const result = await commands.setShareUsageEvents(next);
        problem = result.status === "error" ? messageOf(result.error) : null;
      } catch {
        problem = BROKEN;
      }
      if (mine !== generation) return problem;
    }
    // What the app stores now, not a copy of what a view was given.
    try {
      const reread = await commands.getSettings();
      if (reread.status === "ok") settings = reread.data;
    } catch {
      // What is shown stays; the next change reads again.
    }
    if (mine !== generation) return problem;
    // A choice made during that read goes round again.
  } while (choiceWaiting());
  running = null;
  publish({ pending: false, latest: null, problem, settings });
  return problem;
}

/**
 * Asks for `share`. Resolves once every write is done, with why the last one
 * failed, or `null` if it worked.
 */
export function writeSharing(share: boolean): Promise<string | null> {
  wanted = share;
  publish({ pending: true, latest: share, problem: null });
  running ??= drain(generation);
  return running;
}

export function subscribeSharing(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function sharingState(): SharingQueueState {
  return state;
}

/** For tests: forgets everything, and any write still waiting. */
export function resetSharingQueue() {
  generation += 1;
  state = IDLE;
  wanted = null;
  running = null;
}
