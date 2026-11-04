// Language names for the picker (U2). Settings still stores the ISO code (or
// `null` for "detect"); the names exist only for display and search.

/** English name and the language's own name, by ISO 639 code. */
const NAMES: Readonly<Record<string, readonly [english: string, native: string]>> = {
  af: ["Afrikaans", "Afrikaans"],
  ar: ["Arabic", "العربية"],
  az: ["Azerbaijani", "Azərbaycanca"],
  be: ["Belarusian", "Беларуская"],
  bg: ["Bulgarian", "Български"],
  bs: ["Bosnian", "Bosanski"],
  ca: ["Catalan", "Català"],
  cs: ["Czech", "Čeština"],
  cy: ["Welsh", "Cymraeg"],
  da: ["Danish", "Dansk"],
  de: ["German", "Deutsch"],
  el: ["Greek", "Ελληνικά"],
  en: ["English", "English"],
  es: ["Spanish", "Español"],
  et: ["Estonian", "Eesti"],
  fa: ["Persian", "فارسی"],
  fi: ["Finnish", "Suomi"],
  fil: ["Filipino", "Filipino"],
  fr: ["French", "Français"],
  gl: ["Galician", "Galego"],
  he: ["Hebrew", "עברית"],
  hi: ["Hindi", "हिन्दी"],
  hr: ["Croatian", "Hrvatski"],
  hu: ["Hungarian", "Magyar"],
  hy: ["Armenian", "Հայերեն"],
  id: ["Indonesian", "Bahasa Indonesia"],
  is: ["Icelandic", "Íslenska"],
  it: ["Italian", "Italiano"],
  ja: ["Japanese", "日本語"],
  kk: ["Kazakh", "Қазақша"],
  kn: ["Kannada", "ಕನ್ನಡ"],
  ko: ["Korean", "한국어"],
  lt: ["Lithuanian", "Lietuvių"],
  lv: ["Latvian", "Latviešu"],
  mi: ["Maori", "Māori"],
  mk: ["Macedonian", "Македонски"],
  mr: ["Marathi", "मराठी"],
  ms: ["Malay", "Bahasa Melayu"],
  ne: ["Nepali", "नेपाली"],
  nl: ["Dutch", "Nederlands"],
  no: ["Norwegian", "Norsk"],
  pl: ["Polish", "Polski"],
  pt: ["Portuguese", "Português"],
  ro: ["Romanian", "Română"],
  ru: ["Russian", "Русский"],
  sk: ["Slovak", "Slovenčina"],
  sl: ["Slovenian", "Slovenščina"],
  sr: ["Serbian", "Српски"],
  sv: ["Swedish", "Svenska"],
  sw: ["Swahili", "Kiswahili"],
  ta: ["Tamil", "தமிழ்"],
  th: ["Thai", "ไทย"],
  tl: ["Tagalog", "Tagalog"],
  tr: ["Turkish", "Türkçe"],
  uk: ["Ukrainian", "Українська"],
  ur: ["Urdu", "اردو"],
  vi: ["Vietnamese", "Tiếng Việt"],
  yue: ["Cantonese", "粵語"],
  zh: ["Chinese", "中文"],
};

/** The languages Groq's and OpenAI's Whisper models accept. */
export const CLOUD_LANGUAGES: readonly string[] = [
  "af",
  "ar",
  "hy",
  "az",
  "be",
  "bs",
  "bg",
  "ca",
  "zh",
  "hr",
  "cs",
  "da",
  "nl",
  "en",
  "et",
  "fi",
  "fr",
  "gl",
  "de",
  "el",
  "he",
  "hi",
  "hu",
  "is",
  "id",
  "it",
  "ja",
  "kn",
  "kk",
  "ko",
  "lv",
  "lt",
  "mk",
  "ms",
  "mr",
  "mi",
  "ne",
  "no",
  "fa",
  "pl",
  "pt",
  "ro",
  "ru",
  "sr",
  "sk",
  "sl",
  "es",
  "sw",
  "sv",
  "tl",
  "ta",
  "th",
  "tr",
  "uk",
  "ur",
  "vi",
  "cy",
];

/** "Detect automatically": the option that stores no language. */
export const AUTO_LABEL = "Detect automatically";

/**
 * `code` in words: "Hindi (हिन्दी)", or just "English" when the native name is
 * the same. A code the table doesn't know is shown as "Unrecognised language
 * (xx)" so a hand-edited settings file stays visible and is not changed.
 */
export function languageLabel(code: string): string {
  const names = NAMES[code];
  if (names === undefined) return `Unrecognised language (${code})`;
  const [english, native] = names;
  return english === native ? english : `${english} (${native})`;
}

/** The English name alone, for sentences ("German, not supported by …"). */
export function englishName(code: string): string {
  return NAMES[code]?.[0] ?? languageLabel(code);
}

/**
 * Whether `query` finds `code`: it starts a word of either name,
 * case-insensitive ("hin" finds Hindi, not Chinese).
 */
export function languageMatches(code: string, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (q === "") return true;
  const names = NAMES[code] ?? [languageLabel(code)];
  return names.some((name) =>
    name
      .toLowerCase()
      .split(/[^\p{L}\p{N}]+/u)
      .some((word) => word.startsWith(q)),
  );
}

/** `codes` sorted by English name. */
export function sortedByName(codes: readonly string[]): string[] {
  return [...codes].sort((a, b) => englishName(a).localeCompare(englishName(b)));
}
