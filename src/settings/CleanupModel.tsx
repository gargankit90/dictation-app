// The on-device cleanup model (Q4, Gemma 4 E2B): part of "On this Mac",
// which is one download (owner): the speech model, then the cleanup model,
// shown as one row with one progress bar and one Cancel. The app treats the
// speech model's id as the whole: downloading it fetches both, cancelling it
// cancels both, deleting it deletes both.

import { useEffect, useState } from "react";
import { commands } from "../bindings";
import type { LocalModelInfo_Serialize } from "../bindings";

/** How often the cleanup model is read. */
export const CLEANUP_POLL_MS = 1_000;

/**
 * The cleanup models, read every second while `active`: their download
 * follows the speech model's, so it can start at any time.
 */
export function useCleanupModels(active: boolean): LocalModelInfo_Serialize[] | null {
  const [models, setModels] = useState<LocalModelInfo_Serialize[] | null>(null);
  useEffect(() => {
    if (!active) return;
    const state = { stopped: false };
    const read = async () => {
      try {
        const result = await commands.listCleanupModels();
        if (!state.stopped && result.status === "ok") setModels(result.data);
      } catch {
        // Nothing to show: the speech model is what dictation needs.
      }
    };
    const first = window.setTimeout(() => void read(), 0);
    const timer = window.setInterval(() => void read(), CLEANUP_POLL_MS);
    return () => {
      state.stopped = true;
      window.clearTimeout(first);
      window.clearInterval(timer);
    };
  }, [active]);
  return active ? models : null;
}

type Model = LocalModelInfo_Serialize;

/**
 * The on-device download as one: `speech` with `cleanup` added. Sizes and
 * progress are summed; the state, error and resume details are the speech
 * model's until it is in, then the cleanup model's. The id stays the speech
 * model's (see the top of this file).
 */
export function withCleanup(speech: Model, cleanup: Model | null | undefined): Model {
  if (cleanup === null || cleanup === undefined) return speech;
  const bytes = (model: Model) =>
    model.state === "ready" ? (model.size_bytes ?? 0) : (model.downloaded_bytes ?? 0);
  const part = speech.state === "ready" ? cleanup : speech;
  return {
    ...speech,
    size_bytes: (speech.size_bytes ?? 0) + (cleanup.size_bytes ?? 0),
    downloaded_bytes: bytes(speech) + bytes(cleanup),
    state: part.state,
    error: part.error,
    resumed_from_bytes: part.resumed_from_bytes,
    restarted: part.restarted,
  };
}
