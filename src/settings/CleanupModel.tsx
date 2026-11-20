// Settings → Speech, under "On this Mac" (Q4): the on-device cleanup model
// (Gemma 4 E2B), which turns the raw transcript into send-ready text. It is
// a separate download from the speech model, through the same model
// commands; dictation works without it, uncleaned.

import { useCallback, useEffect, useState } from "react";
import { commands } from "../bindings";
import type { LocalModelInfo_Serialize } from "../bindings";
import { BROKEN, messageOf } from "../keyTest";
import { megabytes } from "./modelRow";
import { Chip, Group, Row } from "./ui";

/** How often the row asks for progress while a download runs. */
export const CLEANUP_POLL_MS = 1_000;

export const PRIVATE = "Private: nothing leaves this Mac";

/** "Adds punctuation, removes fillers · 3107 MB · Private: nothing leaves this Mac". */
export function cleanupCaption(model: LocalModelInfo_Serialize): string {
  return `Adds punctuation, removes fillers · ${megabytes(model.size_bytes ?? 0)} · ${PRIVATE}`;
}

type Action = "download" | "resume" | "cancel" | "delete";

/** What the row says and offers in each download state. */
export function cleanupRow(model: LocalModelInfo_Serialize): {
  chip: string | null;
  action: Action;
  progress: { value: number; max: number } | null;
} {
  const size = model.size_bytes ?? 0;
  const done = model.downloaded_bytes ?? 0;
  const of = `${String(Math.round(done / 1_000_000))} of ${megabytes(size)}`;
  switch (model.state) {
    case "downloading":
      return { chip: `↓ ${of}`, action: "cancel", progress: { value: done, max: size } };
    case "partial":
      return { chip: `Paused · ${of}`, action: "resume", progress: null };
    case "ready":
      return { chip: "✓ Ready", action: "delete", progress: null };
    case "missing":
      return { chip: null, action: "download", progress: null };
  }
}

const LABELS: Record<Action, string> = {
  download: "Download",
  resume: "Resume",
  cancel: "Cancel",
  delete: "Delete",
};

export function CleanupModel() {
  const [models, setModels] = useState<LocalModelInfo_Serialize[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const result = await commands.listCleanupModels();
      if (result.status === "ok") setModels(result.data);
      else setError(messageOf(result.error));
    } catch {
      setError(BROKEN);
    }
  }, []);

  useEffect(() => {
    // The first read; a timer, so no state changes inside the effect itself.
    const first = window.setTimeout(() => void refresh(), 0);
    return () => {
      window.clearTimeout(first);
    };
  }, [refresh]);

  const downloading = models?.some((model) => model.state === "downloading") ?? false;
  useEffect(() => {
    if (!downloading) return;
    const timer = setInterval(() => void refresh(), CLEANUP_POLL_MS);
    return () => {
      clearInterval(timer);
    };
  }, [downloading, refresh]);

  async function act(action: Action, model: LocalModelInfo_Serialize) {
    setError(null);
    try {
      const result =
        action === "cancel"
          ? await commands.cancelLocalModelDownload(model.id)
          : action === "delete"
            ? await commands.deleteLocalModel(model.id)
            : await commands.downloadLocalModel(model.id);
      if (result.status === "error") setError(messageOf(result.error));
    } catch {
      setError(BROKEN);
    }
    await refresh();
  }

  if (models === null && error === null) return null;
  return (
    <Group title="Cleanup on this Mac">
      {(models ?? []).map((model) => {
        const title = model.name;
        const row = cleanupRow(model);
        return (
          <Row
            key={model.id}
            label={title}
            caption={cleanupCaption(model)}
            below={
              <>
                {row.progress === null ? null : (
                  <progress
                    aria-label={`${title} download`}
                    max={row.progress.max}
                    value={row.progress.value}
                  />
                )}
                {model.error === null ? null : <p className="hint row-error">{model.error}</p>}
              </>
            }
          >
            {row.chip === null ? null : (
              <span aria-live="polite">
                <Chip tone={model.state === "ready" ? "ready" : "info"}>{row.chip}</Chip>
              </span>
            )}
            <button
              type="button"
              aria-description={title}
              className={row.action === "delete" ? "destructive" : undefined}
              onClick={() => void act(row.action, model)}
            >
              {LABELS[row.action]}
            </button>
          </Row>
        );
      })}
      {error === null ? null : (
        <p role="alert" className="error">
          {error}
        </p>
      )}
    </Group>
  );
}
