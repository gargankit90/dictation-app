// Quick checks so a typo is caught before a round trip. The Rust side
// (`Settings::validate`, `Config::set_api_key`) checks again and has the
// last word; these mirror its rules.

/** The longest API key `set_api_key` accepts. */
export const MAX_KEY_LEN = 512;

/** `null` if `key` could be an API key, otherwise what's wrong, for the user. */
export function validateKey(key: string): string | null {
  const trimmed = key.trim();
  if (trimmed === "") return "Paste your API key.";
  if (trimmed.length > MAX_KEY_LEN) return "That key is too long.";
  if (/\s/u.test(trimmed)) return "A key has no spaces; check what you pasted.";
  return null;
}

/** The language code as saved: trimmed, lowercase; `null` means detect. */
export function normalizeLanguage(language: string): string | null {
  const trimmed = language.trim().toLowerCase();
  return trimmed === "" ? null : trimmed;
}

/** `null` if `language` is empty (detect) or a code like `en`. */
export function validateLanguage(language: string): string | null {
  const code = normalizeLanguage(language);
  if (code === null || /^[a-z]{2,3}$/u.test(code)) return null;
  return "Use a two or three letter language code such as en, or leave it empty to detect.";
}

/** `null` if `hotkey` looks like `RightAlt` or `Ctrl+Shift+Space`. */
export function validateHotkey(hotkey: string): string | null {
  const trimmed = hotkey.trim();
  if (trimmed === "") return "Enter a hotkey, such as RightAlt or Ctrl+Shift+Space.";
  if (!/^[A-Za-z0-9]+(\+[A-Za-z0-9]+)*$/u.test(trimmed)) {
    return "Write the hotkey as keys joined by +, such as Ctrl+Shift+Space.";
  }
  return null;
}
