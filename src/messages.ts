// Shared wording for command failures.

/** A result to show under a form. */
export interface Notice {
  kind: "ok" | "error";
  text: string;
}

/** Shown when a command fails without a message of its own (the IPC itself broke). */
export const BROKEN = "Something went wrong talking to the app. Try again.";

/** The message of a failed command: the app's own text, or [`BROKEN`]. */
export function messageOf(error: unknown): string {
  return typeof error === "string" && error !== "" ? error : BROKEN;
}
