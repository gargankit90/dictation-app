import { commands } from "./bindings";
import type { Settings_Serialize } from "./bindings";
import { BROKEN, messageOf } from "./keyTest";

/**
 * Saves `hotkey` as the dictation key and returns what the app now stores.
 * Only the key changes: everything else is what the app holds right now. The
 * app switches the running hotkey as it saves, so the key works at once; if
 * it can't, the old key keeps working and `error` says why. Never throws.
 */
export async function saveHotkey(
  hotkey: string,
): Promise<{ error: string | null; settings: Settings_Serialize | null }> {
  try {
    const current = await commands.getSettings();
    if (current.status === "error") return { error: messageOf(current.error), settings: null };
    const saved = await commands.saveSettings({ ...current.data, hotkey });
    // Even a failed save can leave the settings partly changed: show what the
    // app really holds.
    const reread = await commands.getSettings();
    const settings = reread.status === "ok" ? reread.data : null;
    if (saved.status === "error") return { error: messageOf(saved.error), settings };
    return { error: null, settings: settings ?? { ...current.data, hotkey } };
  } catch {
    return { error: BROKEN, settings: null };
  }
}
