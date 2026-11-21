// The on-device cleanup model (Q4, Gemma 4 E2B): part of "On this Mac". It
// downloads after the speech model and goes with it when that is deleted, so
// it has no row of its own: one line under the speech model's row says where
// it is, and offers Download, Resume or Cancel. Dictation works without it,
// uncleaned.

import { useEffect, useState } from "react";
import { commands } from "../bindings";
import type { LocalModelInfo_Serialize } from "../bindings";
import { BROKEN, messageOf } from "../keyTest";
import { megabytes } from "./modelRow";

/** How often the cleanup model is read while on-device speech is chosen. */
export const CLEANUP_POLL_MS = 1_000;

export const PRIVATE = "Private: nothing leaves this Mac";

/**
 * The cleanup models, read every second while `active` (on-device speech is
 * chosen): their download follows the speech model's, so it can start at
 * any time.
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

type Action = "download" | "resume" | "cancel";

/** What the line says and offers in each download state. */
export function cleanupLine(model: LocalModelInfo_Serialize): {
  text: string;
  action: Action | null;
  progress: { value: number; max: number } | null;
} {
  const size = model.size_bytes ?? 0;
  const done = model.downloaded_bytes ?? 0;
  const of = `${String(Math.round(done / 1_000_000))} of ${megabytes(size)}`;
  const name = `Cleanup (punctuation, fillers) · ${megabytes(size)}`;
  switch (model.state) {
    case "downloading":
      return {
        text: `${name} · ↓ ${of}`,
        action: "cancel",
        progress: { value: done, max: size },
      };
    case "partial":
      return { text: `${name} · Paused · ${of}`, action: "resume", progress: null };
    case "ready":
      return { text: `${name} · ✓ Ready · ${PRIVATE}`, action: null, progress: null };
    case "missing":
      return { text: `${name} · not downloaded`, action: "download", progress: null };
  }
}

const LABELS: Record<Action, string> = {
  download: "Download cleanup",
  resume: "Resume cleanup",
  cancel: "Cancel cleanup",
};

/** The line under the "On this Mac" row. */
export function CleanupStatus({ model }: { model: LocalModelInfo_Serialize }) {
  const [error, setError] = useState<string | null>(null);
  const line = cleanupLine(model);
  const action = line.action;

  async function act(action: Action) {
    setError(null);
    try {
      const result =
        action === "cancel"
          ? await commands.cancelLocalModelDownload(model.id)
          : await commands.downloadLocalModel(model.id);
      if (result.status === "error") setError(messageOf(result.error));
    } catch {
      setError(BROKEN);
    }
  }

  const problem = error ?? model.error;
  return (
    <div className="hint" data-testid="cleanup-status">
      <span aria-live="polite">{line.text}</span>{" "}
      {action === null ? null : (
        <button type="button" onClick={() => void act(action)}>
          {LABELS[action]}
        </button>
      )}
      {line.progress === null ? null : (
        <progress
          aria-label="Cleanup model download"
          max={line.progress.max}
          value={line.progress.value}
        />
      )}
      {problem === null ? null : <p className="row-error">{problem}</p>}
    </div>
  );
}
