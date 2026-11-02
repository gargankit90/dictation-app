// Shared by Settings and onboarding (M5.2): testing an API key and the
// wording of the outcome.

import { commands } from "./bindings";
import type { Provider } from "./bindings";
import { providerInfo } from "./providers";

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

/**
 * Tests `typed` (used for this request only, not saved), or the saved key
 * if `typed` is `null`, against `provider`. Never throws.
 */
export async function testKey(provider: Provider, typed: string | null): Promise<Notice> {
  try {
    const result = await commands.testApiKey(provider, typed);
    return result.status === "ok"
      ? { kind: "ok", text: `The key works. ${providerInfo(provider).name} accepted it.` }
      : { kind: "error", text: result.error.message };
  } catch {
    return { kind: "error", text: BROKEN };
  }
}
