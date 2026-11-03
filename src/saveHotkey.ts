import { commands } from "./bindings";
import type { Settings_Serialize } from "./bindings";
import { BROKEN, messageOf } from "./keyTest";

/**
 * Sets `hotkey` as the dictation key, through the command that is its only
 * writer, and returns what the app now stores. The other settings are never
 * touched, so this can't undo a Save of the form or the other way round. The
 * app switches the running hotkey as it saves, so the key works at once; if
 * it can't, the old key keeps working and `error` says why. Never throws.
 */
export async function saveHotkey(
  hotkey: string,
): Promise<{ error: string | null; settings: Settings_Serialize | null }> {
  try {
    const saved = await commands.setHotkey(hotkey);
    // Even a failed change shows what the app really holds.
    const reread = await commands.getSettings();
    const settings = reread.status === "ok" ? reread.data : null;
    if (saved.status === "error") return { error: messageOf(saved.error), settings };
    return { error: null, settings };
  } catch {
    return { error: BROKEN, settings: null };
  }
}

/** Tells the app the user is (or no longer is) choosing a key. Never throws. */
export function setHotkeyRecording(recording: boolean): void {
  void commands.setHotkeyRecording(recording).catch(() => undefined);
}
