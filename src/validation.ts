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
