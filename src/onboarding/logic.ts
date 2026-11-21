// Onboarding's decisions (U8), apart from its views: the recommended model,
// what the download strip says, and whether practice must wait. Each answer
// comes only from what the app reports.

import type { EngineStatus, LocalModelInfo_Serialize, Settings_Serialize } from "../bindings";
import { DOWNLOADING, megabytes } from "../settings/modelRow";

/** The bindings allow null for sizes (NaN); the app never sends it. */
function size(model: LocalModelInfo_Serialize): number {
  return model.size_bytes ?? 0;
}

function done(model: LocalModelInfo_Serialize): number {
  return model.downloaded_bytes ?? 0;
}

function englishOnly(model: LocalModelInfo_Serialize): boolean {
  return model.language_codes.join() === "en";
}

/** "212 of 477 MB". */
export function progressText(model: LocalModelInfo_Serialize): string {
  return `${String(Math.round(done(model) / 1_000_000))} of ${megabytes(size(model))}`;
}

/** The model to recommend (0.2.0 is English-only): the English model, else the first. */
export function recommendedModel(
  models: readonly LocalModelInfo_Serialize[],
): LocalModelInfo_Serialize | null {
  return models.find(englishOnly) ?? models[0] ?? null;
}

/** The on-device model the settings choose, if they choose one. */
export function chosenModel(
  settings: Settings_Serialize | null,
  models: readonly LocalModelInfo_Serialize[] | null,
): LocalModelInfo_Serialize | null {
  if (settings?.transcription !== "local" || models === null) return null;
  return models.find((model) => model.id === settings.local_model) ?? null;
}

export type StripAction = "cancel" | "resume" | "retry" | "use-key";

export interface StripView {
  text: string;
  /** What happened to an earlier download, when that needs saying. */
  note: string | null;
  /** 0–100 while downloading. */
  percent: number | null;
  actions: StripAction[];
  model: LocalModelInfo_Serialize;
}

/**
 * The download strip, or `null` for no strip. What it says about an earlier
 * part is what the server's answer established (`resumed_from_bytes`,
 * `restarted`), never a guess from the bytes so far.
 */
export function stripView(
  models: readonly LocalModelInfo_Serialize[] | null,
  chosen: LocalModelInfo_Serialize | null,
): StripView | null {
  const running = models?.find((model) => model.state === "downloading") ?? null;
  if (running !== null) {
    const bytes = done(running);
    const total = size(running);
    let note: string | null = null;
    if (running.restarted) {
      note = "The earlier download couldn't continue, so it started again.";
    } else if (running.resumed_from_bytes !== null) {
      note = `Continued from ${megabytes(running.resumed_from_bytes)}.`;
    }
    return {
      text: `${DOWNLOADING} · ${progressText(running)}`,
      note,
      percent: total > 0 ? Math.min(100, Math.round((bytes / total) * 100)) : 0,
      actions: ["cancel"],
      model: running,
    };
  }
  if (chosen === null || chosen.state === "ready") return null;
  if (chosen.error !== null) {
    return {
      text: `Download of the on-device models stopped: ${chosen.error}`,
      note: null,
      percent: null,
      actions: ["retry", "use-key"],
      model: chosen,
    };
  }
  return {
    text:
      chosen.state === "partial"
        ? `Download paused · ${progressText(chosen)}`
        : "The on-device models aren't downloaded yet",
    note: null,
    percent: null,
    actions: ["resume", "use-key"],
    model: chosen,
  };
}

/** Why practice has to wait, or `null` if dictation can run now. */
export function modelWait(
  settings: Settings_Serialize | null,
  models: readonly LocalModelInfo_Serialize[] | null,
  engine: EngineStatus | null,
): string | null {
  if (settings === null) return "Loading…";
  if (settings.transcription !== "local") return null;
  const model = chosenModel(settings, models);
  if (model === null) return "Getting the model ready…";
  if (model.state === "downloading") {
    const total = size(model);
    const percent = total > 0 ? Math.min(100, Math.round((done(model) / total) * 100)) : 0;
    return `Getting the model ready: ${String(percent)}%`;
  }
  if (model.state !== "ready") return "The model isn't downloaded yet. Practice starts once it is.";
  if (engine === null || engine.state === "loading") return "Getting the model ready: loading it…";
  if (engine.state === "failed") {
    return engine.message ?? "The model couldn't load. Choose another in the Speech step.";
  }
  return null;
}
