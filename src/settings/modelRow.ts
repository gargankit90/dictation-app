// The Speech section's state machine for one on-device model row (U6): a
// chip and at most two actions, from the model's download state, whether
// the settings choose it, and how the engine in use is doing.

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

/** What a row's one action (or two) does. */
export type ModelAction = "download" | "cancel" | "retry" | "use" | "retry-load" | "delete";

export interface ModelRow {
  chip: { text: string; tone: ChipTone };
  actions: ModelAction[];
  /** The download's progress, while it runs. */
  progress: { value: number; max: number } | null;
  /** The "…" menu offers Delete: something is on disk and it isn't active. */
  canDelete: boolean;
  /** The row is the engine in use (✓ Active), or is becoming it. */
  active: boolean;
}

export interface RowContext {
  /** The settings choose this model (transcription: local, this id). */
  chosen: boolean;
  /** The engine in use, as the app reports it. */
  engine: EngineStatus | null;
  /** "Use this" is running for this model. */
  switching: boolean;
  /** Why the last "Use this" for this model failed, if it did. */
  loadError: string | null;
}

/** A failure that is the network's: the download can continue later. */
function offline(error: string): boolean {
  return /internet|connection|network|offline/i.test(error);
}

/** The row for `model` (see the state table in docs/history/milestones/U6.md). */
export function modelRow(model: LocalModelInfo_Serialize, context: RowContext): ModelRow {
  // Sizes are doubles in the bindings, which allow null (NaN); never sent.
  const size = model.size_bytes ?? 0;
  const done = model.downloaded_bytes ?? 0;
  const row = (
    text: string,
    tone: ChipTone,
    actions: ModelAction[],
    extra: Partial<ModelRow> = {},
  ): ModelRow => ({
    chip: { text, tone },
    actions,
    progress: null,
    canDelete: false,
    active: false,
    ...extra,
  });
  switch (model.state) {
    case "downloading":
      return row(`↓ ${mb(done)} of ${megabytes(size)}`, "info", ["cancel"], {
        progress: { value: done, max: size },
      });
    case "partial":
      if (model.error !== null && !offline(model.error)) {
        return row("Download failed", "error", ["retry"], { canDelete: true });
      }
      return row(
        model.error === null
          ? `Paused at ${mb(done)} of ${megabytes(size)}`
          : "Paused: no connection",
        "attention",
        ["retry"],
        { canDelete: true },
      );
    case "missing":
      if (model.error !== null) return row("Download failed", "error", ["retry"]);
      return row(megabytes(size), "plain", ["download"]);
    case "ready":
      break;
  }
  if (context.switching) return row("Loading…", "info", [], { active: true });
  if (context.chosen) {
    const engine = context.engine?.state ?? "ready";
    if (engine === "loading") return row("Loading…", "info", [], { active: true });
    if (engine === "failed") {
      return row("! Couldn't load", "error", ["retry-load", "delete"], { active: true });
    }
    return row("✓ Active", "ready", [], { active: true });
  }
  if (context.loadError !== null) {
    return row("! Couldn't load", "error", ["retry-load", "delete"]);
  }
  return row("Downloaded", "plain", ["use"], { canDelete: true });
}

/** A model's title: the language choice, not the checkpoint (Design Lead, round 1). */
export function modelTitle(model: LocalModelInfo_Serialize): string {
  return model.language_codes.join() === "en" ? "English" : "Multilingual";
}

/** Its languages in words, counted from the codes the language picker uses. */
export function languagesLabel(codes: readonly string[]): string {
  if (codes.join() === "en") return "English only";
  const count = `${String(codes.length)} languages`;
  return codes.includes("en") && codes.includes("hi")
    ? `${count}, including English and Hindi`
    : count;
}

/**
 * The caption: the catalogue name, the languages, and what stays local.
 * Not "nothing is sent": usage events may be on; the audio never leaves.
 */
export function modelCaption(model: LocalModelInfo_Serialize): string {
  return `${model.name} · ${languagesLabel(model.language_codes)} · works offline · audio stays on this Mac`;
}
