/**
 * What the overlay window shows (M5.1, U4: the recording pill). The Rust side
 * sends one of these as the payload of {@link OVERLAY_EVENT}
 * (`src-tauri/src/overlay.rs`, whose tests pin the same JSON), and the input
 * level while recording as {@link OVERLAY_LEVEL_EVENT}. An error carries only
 * the user-facing message, never a code, the transcript or a key.
 */
export type OverlayView =
  | { state: "hidden" }
  | { state: "recording" }
  | { state: "no_sound"; device?: string }
  | { state: "transcribing" }
  | { state: "paste_sent" }
  | { state: "error"; message: string };

/** The view event (sent only to the overlay window). */
export const OVERLAY_EVENT = "overlay-view";

/**
 * The input level event while recording, about 20 a second: `{ dbfs }`
 * (`LEVEL_EVENT` in `src-tauri/src/overlay.rs`). A number only; the page
 * never stores or sends it.
 */
export const OVERLAY_LEVEL_EVENT = "overlay-level";

/**
 * The document title the overlay sets once its listeners are registered, with
 * a count after it. An event sent before that is lost, and the page may not
 * call commands or emit events, so the title is how it tells the app to send
 * the current view again (#119; `LISTENING_TITLE` in
 * `src-tauri/src/overlay.rs`).
 */
export const LISTENING_TITLE = "dictation-app overlay listening";

let listening = 0;

/** Sets a new listening title, so the app sees a change each time. */
export function announceListening() {
  listening += 1;
  document.title = `${LISTENING_TITLE} ${String(listening)}`;
}

export const HIDDEN: OverlayView = { state: "hidden" };

/** Reads an event payload; anything unexpected hides the overlay. */
export function parseView(payload: unknown): OverlayView {
  if (typeof payload !== "object" || payload === null || !("state" in payload)) {
    return HIDDEN;
  }
  switch (payload.state) {
    case "recording":
      return { state: "recording" };
    case "no_sound":
      return "device" in payload && typeof payload.device === "string" && payload.device !== ""
        ? { state: "no_sound", device: payload.device }
        : { state: "no_sound" };
    case "transcribing":
      return { state: "transcribing" };
    case "paste_sent":
      return { state: "paste_sent" };
    case "error":
      return "message" in payload && typeof payload.message === "string"
        ? { state: "error", message: payload.message }
        : HIDDEN;
    default:
      return HIDDEN;
  }
}

/** Reads a level payload: its dBFS, or `null` if it isn't one. */
export function parseLevel(payload: unknown): number | null {
  if (typeof payload !== "object" || payload === null || !("dbfs" in payload)) {
    return null;
  }
  const { dbfs } = payload;
  return typeof dbfs === "number" && Number.isFinite(dbfs) ? dbfs : null;
}

/** Bars in the meter. */
export const METER_BARS = 7;
/** A bar's height at or below {@link METER_LOW_DBFS}, in px. */
export const BAR_MIN_PX = 4;
/** A bar's height at or above {@link METER_HIGH_DBFS}, in px. */
export const BAR_MAX_PX = 18;
/** The level drawn as the shortest bar. */
export const METER_LOW_DBFS = -60;
/** The level drawn as the tallest bar. */
export const METER_HIGH_DBFS = -10;
/** Silence, for an empty meter (`LEVEL_FLOOR_DBFS` in `dictation-core`). */
export const FLOOR_DBFS = -100;

/**
 * A bar's height for a level: linear in dBFS from {@link METER_LOW_DBFS}
 * ({@link BAR_MIN_PX}) to {@link METER_HIGH_DBFS} ({@link BAR_MAX_PX}),
 * clamped, to the nearest pixel. So silence is flat and speech (about −40 to
 * −20 dBFS) moves the bars.
 */
export function barHeight(dbfs: number): number {
  const span = METER_HIGH_DBFS - METER_LOW_DBFS;
  const share = Math.min(1, Math.max(0, (dbfs - METER_LOW_DBFS) / span));
  return Math.round(BAR_MIN_PX + share * (BAR_MAX_PX - BAR_MIN_PX));
}

/** A silent meter: {@link METER_BARS} levels at the floor. */
export const SILENT_LEVELS: readonly number[] = Array.from(
  { length: METER_BARS },
  () => FLOOR_DBFS,
);

/**
 * The meter's levels after a new one: the newest on the right, the rest
 * scrolling left, always {@link METER_BARS} long.
 */
export function pushLevel(levels: readonly number[], dbfs: number): number[] {
  return [...levels, dbfs].slice(-METER_BARS);
}
