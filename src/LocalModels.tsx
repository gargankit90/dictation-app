import { useCallback, useEffect, useRef, useState } from "react";
import { commands } from "./bindings";
import type { LocalModelInfo_Serialize } from "./bindings";
import { BROKEN, messageOf } from "./keyTest";

/** How often the list is read again while a download runs. */
export const POLL_MS = 500;

/** `bytes` as megabytes, the way the catalogue sizes are shown ("477 MB"). */
export function megabytes(bytes: number): string {
  return `${String(Math.ceil(bytes / 1_000_000))} MB`;
}

function status(model: LocalModelInfo_Serialize): string {
  // Sizes are doubles in the bindings, which allow null (NaN); never sent.
  const size = model.size_bytes ?? 0;
  const done = model.downloaded_bytes ?? 0;
  switch (model.state) {
    case "ready":
      return "Downloaded";
    case "downloading": {
      const percent = size > 0 ? Math.floor((100 * done) / size) : 0;
      return `Downloading… ${String(percent)}% of ${megabytes(size)}`;
    }
    case "partial":
      return `Paused at ${megabytes(done)} of ${megabytes(size)}`;
    case "missing":
      return `Not downloaded (${megabytes(size)})`;
  }
}

interface Props {
  /** The model chosen in the form (saved with the settings). */
  selected: string;
  onSelect: (id: string) => void;
  /** The form is saving. */
  disabled: boolean;
}

/**
 * The on-device models (L1): pick one, download, cancel or delete it, and
 * see its status. Downloads start at once (not on Save) and keep going while
 * the window is hidden; the list is polled while one runs.
 */
export function LocalModels({ selected, onSelect, disabled }: Props) {
  const [models, setModels] = useState<LocalModelInfo_Serialize[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Only the newest reply counts.
  const read = useRef(0);

  const refresh = useCallback(async () => {
    read.current += 1;
    const run = read.current;
    try {
      const result = await commands.listLocalModels();
      if (run !== read.current) return;
      if (result.status === "error") {
        setError(messageOf(result.error));
        return;
      }
      setModels(result.data);
    } catch {
      if (run === read.current) setError(BROKEN);
    }
  }, []);

  useEffect(() => {
    // The first read; the interval below takes over while a download runs.
    const timer = window.setTimeout(() => void refresh(), 0);
    return () => {
      window.clearTimeout(timer);
    };
  }, [refresh]);

  const downloading = models?.some((model) => model.state === "downloading") ?? false;
  useEffect(() => {
    if (!downloading) return;
    const timer = window.setInterval(() => void refresh(), POLL_MS);
    return () => {
      window.clearInterval(timer);
    };
  }, [downloading, refresh]);

  async function act(call: () => Promise<{ status: "ok" } | { status: "error"; error: string }>) {
    setError(null);
    try {
      const result = await call();
      if (result.status === "error") setError(messageOf(result.error));
    } catch {
      setError(BROKEN);
    }
    await refresh();
  }

  if (models === null) {
    return error === null ? (
      <p className="hint">Loading the models…</p>
    ) : (
      <p role="alert" className="error">
        Couldn&apos;t list the models: {error}
      </p>
    );
  }

  return (
    <div className="local-models">
      <ul aria-label="On-device models">
        {models.map((model) => (
          <li key={model.id}>
            <label className="check">
              <input
                type="radio"
                name="local-model"
                value={model.id}
                checked={selected === model.id}
                disabled={disabled}
                onChange={() => {
                  onSelect(model.id);
                }}
              />
              {model.name}
              {model.default && " (recommended)"} — {model.languages}
            </label>
            <p className="hint" aria-live="polite">
              {status(model)}
            </p>
            {model.state === "downloading" && (
              <progress
                aria-label={`${model.name} download`}
                max={model.size_bytes ?? 0}
                value={model.downloaded_bytes ?? 0}
              />
            )}
            {model.error !== null && (
              <p role="alert" className="error">
                {model.error}
              </p>
            )}
            <div className="row">
              {(model.state === "missing" || model.state === "partial") && (
                <button
                  type="button"
                  disabled={disabled || downloading}
                  onClick={() => void act(() => commands.downloadLocalModel(model.id))}
                >
                  {model.state === "partial" ? "Resume download" : "Download"}
                </button>
              )}
              {model.state === "downloading" && (
                <button
                  type="button"
                  onClick={() => void act(() => commands.cancelLocalModelDownload(model.id))}
                >
                  Cancel
                </button>
              )}
              {(model.state === "ready" || model.state === "partial") && (
                <button
                  type="button"
                  className="destructive"
                  disabled={disabled}
                  onClick={() => void act(() => commands.deleteLocalModel(model.id))}
                >
                  Delete
                </button>
              )}
            </div>
          </li>
        ))}
      </ul>
      {downloading && <p className="hint">One model downloads at a time.</p>}
      {error !== null && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
    </div>
  );
}
