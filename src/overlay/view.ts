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
  | { state: "error"; message: string; code?: string };

/** The one event the overlay listens to (sent only to the overlay window). */
export const OVERLAY_EVENT = "overlay-view";

/**
 * The document title the overlay sets once its listener is registered, with a
 * count after it. An event sent before that is lost, and the page may not
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
    case "transcribing":
      return { state: "transcribing" };
    case "error":
      return "message" in payload && typeof payload.message === "string"
        ? {
            state: "error",
            message: payload.message,
            ...("code" in payload && typeof payload.code === "string"
              ? { code: payload.code }
              : {}),
          }
        : HIDDEN;
    default:
      return HIDDEN;
  }
}
