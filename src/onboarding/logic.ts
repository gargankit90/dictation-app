// Onboarding's decisions (U8), apart from its views: the recommended model,
// what the download strip says, whether practice must wait, and what Try it's
// stage line may claim. Each answer comes only from what the app reports.

import type {
  DictationProgressView,
  EngineStatus,
  LocalModelInfo_Serialize,
  Settings_Serialize,
} from "../bindings";
import { megabytes, modelTitle } from "../settings/modelRow";

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

/** A model as Ready names it (U10): "Parakeet English", "Qwen3-ASR Multilingual". */
export function spokenName(model: LocalModelInfo_Serialize): string {
  const family = model.name.split(" ")[0] ?? model.name;
  return `${family} ${englishOnly(model) ? "English" : "Multilingual"}`;
}

/** "212 of 477 MB". */
export function progressText(model: LocalModelInfo_Serialize): string {
  return `${String(Math.round(done(model) / 1_000_000))} of ${megabytes(size(model))}`;
}

/**
 * The model to recommend for `language` (an ISO code, `""` for automatic):
 * the English model for English, otherwise the multilingual one that speaks
 * it. `null` if no on-device model does.
 */
export function recommendedModel(
  models: readonly LocalModelInfo_Serialize[],
  language: string,
): LocalModelInfo_Serialize | null {
  if (language === "en") {
    return models.find(englishOnly) ?? null;
  }
  const multilingual = models.filter((model) => !englishOnly(model));
  if (language === "") return multilingual[0] ?? null;
  return multilingual.find((model) => model.language_codes.includes(language)) ?? null;
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
      text: `Downloading ${modelTitle(running)} · ${progressText(running)}`,
      note,
      percent: total > 0 ? Math.min(100, Math.round((bytes / total) * 100)) : 0,
      actions: ["cancel"],
      model: running,
    };
  }
  if (chosen === null || chosen.state === "ready") return null;
  if (chosen.error !== null) {
    return {
      text: `Download of ${modelTitle(chosen)} stopped: ${chosen.error}`,
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
        : `${modelTitle(chosen)} isn't downloaded yet`,
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

/** Whether the words reached the practice box, as far as can be told. */
export type PasteCheck = "unknown" | "landed" | "missed";

export type TryFix =
  | { kind: "none" }
  | { kind: "microphone"; message: string }
  | { kind: "key"; message: string }
  | { kind: "paste" }
  | { kind: "other"; message: string };

export interface TryView {
  /** A dictation started since the step opened. */
  started: boolean;
  heard: boolean;
  transcribed: boolean;
  pasted: boolean;
  fix: TryFix;
}

export const NO_SOUND = "Vocet didn't hear anything. Check System Settings → Sound.";
export const NO_WORDS = "Vocet heard you but found no words. Try a full sentence.";

/**
 * Try it's stage line and its one fix, from the app's reports since the step
 * opened (`baseline`). The practice box's text isn't an input here, except
 * through `paste`, which compares it with the words the app says it pasted
 * after a dictation completes, so typing can't advance anything.
 */
export function tryView(
  progress: DictationProgressView | null,
  baseline: DictationProgressView | null,
  paste: PasteCheck,
): TryView {
  const idle: TryView = {
    started: false,
    heard: false,
    transcribed: false,
    pasted: false,
    fix: { kind: "none" },
  };
  if (progress === null || baseline === null || progress.started <= baseline.started) return idle;
  const transcribed = progress.stages.includes("transcribed");
  const over = progress.finished === progress.started;
  const view = { ...idle, started: true, heard: progress.heard, transcribed };
  const message = progress.error ?? "";
  switch (progress.failure) {
    case "microphone":
      return { ...view, fix: { kind: "microphone", message } };
    case "key":
      return { ...view, fix: { kind: "key", message } };
    case "paste":
      return { ...view, fix: { kind: "paste" } };
    case "other":
      return { ...view, fix: { kind: "other", message } };
    case null:
      break;
  }
  if (paste === "landed") return { ...view, pasted: true };
  if (paste === "missed") return { ...view, fix: { kind: "paste" } };
  if (over && !transcribed) {
    return {
      ...view,
      fix: progress.heard
        ? { kind: "other", message: NO_WORDS }
        : { kind: "microphone", message: NO_SOUND },
    };
  }
  return view;
}

/** Whether `words` are in the box, whatever the spacing around them. */
export function holdsWords(box: string, words: string): boolean {
  const squeeze = (text: string) => text.replace(/\s+/g, " ").trim();
  const wanted = squeeze(words);
  return wanted !== "" && squeeze(box).includes(wanted);
}
