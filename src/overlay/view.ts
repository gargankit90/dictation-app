/**
 * What the overlay window shows (M5.1). The Rust side sends one of these as
 * the payload of {@link OVERLAY_EVENT} (`src-tauri/src/overlay.rs`, whose
 * tests pin the same JSON). An error carries only the user-facing message,
 * never the transcript or a key.
 */
export type OverlayView =
  | { state: "hidden" }
  | { state: "recording" }
  | { state: "transcribing" }
  | { state: "error"; message: string };

/** The one event the overlay listens to (sent only to the overlay window). */
export const OVERLAY_EVENT = "overlay-view";

export const HIDDEN: OverlayView = { state: "hidden" };

/** Reads an event payload; anything unexpected hides the overlay. */
export function parseView(payload: unknown): OverlayView {
  if (typeof payload !== "object" || payload === null || !("state" in payload)) {
    return HIDDEN;
  }
  switch (payload.state) {
    case "recording":
      return { state: "recording" };
    case "transcribing":
      return { state: "transcribing" };
    case "error":
      return "message" in payload && typeof payload.message === "string"
        ? { state: "error", message: payload.message }
        : HIDDEN;
    default:
      return HIDDEN;
  }
}
