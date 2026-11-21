// The Speech section's state machine for one on-device model row (U6, U12):
// what the row says, its one action, and whether it can be chosen, from the
// model's download state, whether the settings choose it, and how the engine
// in use is doing. Choosing is the row's radio button (U12), so a ready row
// that nothing is wrong with says nothing at all.

import type { EngineStatus, LocalModelInfo_Serialize } from "../bindings";

/** `bytes` as whole megabytes, the way the design shows sizes ("477 MB"). */
export function megabytes(bytes: number): string {
  return `${String(Math.round(bytes / 1_000_000))} MB`;
}

/** Without the unit, for "212 of 477 MB". */
function mb(bytes: number): string {
  return String(Math.round(bytes / 1_000_000));
}

export type ChipTone = "plain" | "info" | "attention" | "ready" | "error";

/** What a row's one action does. */
export type ModelAction = "download" | "cancel" | "resume" | "retry" | "retry-load";

export interface ModelRow {
  /** A state worth saying: loading, progress, paused, failed. */
  chip: { text: string; tone: ChipTone } | null;
  actions: ModelAction[];
  /** The download's progress, while it runs. */
  progress: { value: number; max: number } | null;
  /** The "…" menu offers Delete: something is on disk and it isn't in use. */
  canDelete: boolean;
  /** The row has a "…" menu: its model is at least partly on this Mac. */
  hasMenu: boolean;
  /** The row is the engine in use, or is becoming it. */
  active: boolean;
  /** Why it can't be chosen now; `null` if it can. */
  blockedBecause: string | null;
}

export interface RowContext {
  /** The settings choose this model (transcription: local, this id). */
  chosen: boolean;
  /** The engine in use, as the app reports it. */
  engine: EngineStatus | null;
  /** Choosing it is running. */
  switching: boolean;
  /** Why the last try to choose it failed, if it did. */
  loadError: string | null;
}

/** A failure that is the network's: the download can continue later. */
function offline(error: string): boolean {
  return /internet|connection|network|offline/i.test(error);
}

/** The row for `model` (see the state tables in docs/history/milestones/U6.md and U12.md). */
export function modelRow(model: LocalModelInfo_Serialize, context: RowContext): ModelRow {
  // Sizes are doubles in the bindings, which allow null (NaN); never sent.
  const size = model.size_bytes ?? 0;
  const done = model.downloaded_bytes ?? 0;
  const row = (
    chip: ModelRow["chip"],
    actions: ModelAction[],
    extra: Partial<ModelRow> = {},
  ): ModelRow => ({
    chip,
    actions,
    progress: null,
    canDelete: false,
    hasMenu: false,
    active: false,
    blockedBecause: null,
    ...extra,
  });
  const partly = { canDelete: true, hasMenu: true };
  switch (model.state) {
    case "downloading":
      return row({ text: `↓ ${mb(done)} of ${megabytes(size)}`, tone: "info" }, ["cancel"], {
        progress: { value: done, max: size },
        blockedBecause: "You can choose it when the download finishes.",
      });
    case "partial":
      if (model.error !== null && !offline(model.error)) {
        return row({ text: "Download failed", tone: "error" }, ["retry"], {
          ...partly,
          blockedBecause: "Download it again to choose it.",
        });
      }
      // Pause keeps the part; downloading again continues it.
      if (model.error === null) {
        return row(
          { text: `Paused · ${mb(done)} of ${megabytes(size)}`, tone: "attention" },
          ["resume"],
          { ...partly, blockedBecause: "Finish the download to choose it." },
        );
      }
      return row({ text: "Paused: no connection", tone: "attention" }, ["retry"], {
        ...partly,
        blockedBecause: "Finish the download to choose it.",
      });
    case "missing":
      if (model.error !== null) {
        return row({ text: "Download failed", tone: "error" }, ["retry"], {
          blockedBecause: "Download it again to choose it.",
        });
      }
      return row(null, ["download"], { blockedBecause: "Download it to choose it." });
    case "ready":
      break;
  }
  const loading = { text: "Loading…", tone: "info" } as const;
  if (context.switching) return row(loading, [], { active: true, hasMenu: true });
  if (context.chosen) {
    const engine = context.engine?.state ?? "ready";
    if (engine === "loading") return row(loading, [], { active: true, hasMenu: true });
    if (engine === "failed") {
      return row({ text: "! Couldn't load", tone: "error" }, ["retry-load"], {
        ...partly,
        active: true,
      });
    }
    return row(null, [], { active: true, hasMenu: true });
  }
  if (context.loadError !== null) {
    return row({ text: "! Couldn't load", tone: "error" }, ["retry-load"], partly);
  }
  return row(null, [], partly);
}

/**
 * The on-device model's title: its language, not the checkpoint (Design
 * Lead, round 1). 0.2.0 is English-only.
 */
export const MODEL_TITLE = "English";

/**
 * The title and size together, as setup's card shows them (U11): "English ·
 * 477 MB". Settings shows the same two parts apart.
 */
export function modelLabel(model: LocalModelInfo_Serialize): string {
  return `${MODEL_TITLE} · ${megabytes(model.size_bytes ?? 0)}`;
}

/** The on-device row's title (owner, Q4). */
export const LOCAL_TITLE = "On this Mac";

/** The smaller line under it (owner, Q4). */
export const LOCAL_CAPTION = `${MODEL_TITLE} · works offline · nothing leaves your Mac`;

/** What one on-device download is (owner, Q4). */
export const DOWNLOADING = "Downloading on-device models (speech + cleanup)";

/** "Downloading on-device models (speech + cleanup) · 4.4 GB". */
export function downloadingText(model: LocalModelInfo_Serialize): string {
  const gigabytes = (model.size_bytes ?? 0) / 1_000_000_000;
  return `${DOWNLOADING} · ${gigabytes.toFixed(1)} GB`;
}

/** The cloud provider's name (Q2: `OpenRouter` only). */
export const PROVIDER_NAME = "OpenRouter";

/** Display names of the models in `crates/core/src/cloud_models.rs`. */
export const SPEECH_MODEL_NAME = "GPT-4o mini Transcribe (OpenAI)";
export const CLEANUP_MODEL_NAME = "GLM 5.3 Flash (Z.ai)";

/** The smaller line under the OpenRouter row. */
export function cloudCaption(connected: boolean): string {
  return connected
    ? `${PROVIDER_NAME} · speech is sent to ${PROVIDER_NAME} · ` +
        `speech: ${SPEECH_MODEL_NAME} · cleanup: ${CLEANUP_MODEL_NAME}`
    : "Uses internet · needs an OpenRouter account";
}

/**
 * What the "…" menu says about the model: the catalogue name, the languages,
 * and what stays local. Not "nothing is sent": usage events may be on; the
 * audio never leaves.
 */
export function modelDetails(model: LocalModelInfo_Serialize): string {
  return `${model.name} · ${model.languages} · works offline · audio stays on this Mac`;
}
