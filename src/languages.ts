import type { Provider } from "./bindings";

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
  la: ["Latin", "Latina"],
  ml: ["Malayalam", "മലയാളം"],
  te: ["Telugu", "తెలుగు"],
  bn: ["Bengali", "বাংলা"],
  br: ["Breton", "Brezhoneg"],
  eu: ["Basque", "Euskara"],
  mn: ["Mongolian", "Монгол"],
  sq: ["Albanian", "Shqip"],
  pa: ["Punjabi", "ਪੰਜਾਬੀ"],
  si: ["Sinhala", "සිංහල"],
  km: ["Khmer", "ភាសាខ្មែរ"],
  sn: ["Shona", "chiShona"],
  yo: ["Yoruba", "Yorùbá"],
  so: ["Somali", "Soomaali"],
  oc: ["Occitan", "Occitan"],
  ka: ["Georgian", "ქართული"],
  tg: ["Tajik", "Тоҷикӣ"],
  sd: ["Sindhi", "سنڌي"],
  gu: ["Gujarati", "ગુજરાતી"],
  am: ["Amharic", "አማርኛ"],
  yi: ["Yiddish", "ייִדיש"],
  lo: ["Lao", "ລາວ"],
  uz: ["Uzbek", "Oʻzbekcha"],
  fo: ["Faroese", "Føroyskt"],
  ht: ["Haitian Creole", "Kreyòl ayisyen"],
  ps: ["Pashto", "پښتو"],
  tk: ["Turkmen", "Türkmençe"],
  nn: ["Nynorsk", "Nynorsk"],
  mt: ["Maltese", "Malti"],
  sa: ["Sanskrit", "संस्कृतम्"],
  lb: ["Luxembourgish", "Lëtzebuergesch"],
  my: ["Burmese", "မြန်မာ"],
  bo: ["Tibetan", "བོད་ཡིག"],
  mg: ["Malagasy", "Malagasy"],
  as: ["Assamese", "অসমীয়া"],
  tt: ["Tatar", "Татарча"],
  haw: ["Hawaiian", "ʻŌlelo Hawaiʻi"],
  ln: ["Lingala", "Lingála"],
  ha: ["Hausa", "Hausa"],
  ba: ["Bashkir", "Башҡортса"],
  jw: ["Javanese", "Basa Jawa"],
  su: ["Sundanese", "Basa Sunda"],
};

/**
 * The languages every Whisper model accepts: the upstream table
 * (openai/whisper, `whisper/tokenizer.py` LANGUAGES, without `yue`). Groq
 * documents 99+ languages for `whisper-large-v3` and
 * `whisper-large-v3-turbo`: https://console.groq.com/docs/model/whisper-large-v3-turbo
 * OpenAI's `whisper-1` takes the same ISO codes; its docs only list the
 * subset with good quality, but every code in the table is accepted.
 */
const WHISPER_LANGUAGES: readonly string[] = [
  "en",
  "zh",
  "de",
  "es",
  "ru",
  "ko",
  "fr",
  "ja",
  "pt",
  "tr",
  "pl",
  "ca",
  "nl",
  "ar",
  "sv",
  "it",
  "id",
  "hi",
  "fi",
  "vi",
  "he",
  "uk",
  "el",
  "ms",
  "cs",
  "ro",
  "da",
  "hu",
  "ta",
  "no",
  "th",
  "ur",
  "hr",
  "bg",
  "lt",
  "la",
  "mi",
  "ml",
  "cy",
  "sk",
  "te",
  "fa",
  "lv",
  "bn",
  "sr",
  "az",
  "sl",
  "kn",
  "et",
  "mk",
  "br",
  "eu",
  "is",
  "hy",
  "ne",
  "mn",
  "bs",
  "kk",
  "sq",
  "sw",
  "gl",
  "mr",
  "pa",
  "si",
  "km",
  "sn",
  "yo",
  "so",
  "af",
  "oc",
  "ka",
  "be",
  "tg",
  "sd",
  "gu",
  "am",
  "yi",
  "lo",
  "uz",
  "fo",
  "ht",
  "ps",
  "tk",
  "nn",
  "mt",
  "sa",
  "lb",
  "my",
  "bo",
  "tl",
  "mg",
  "as",
  "tt",
  "haw",
  "ln",
  "ha",
  "ba",
  "jw",
  "su",
];

/**
 * Cantonese (`yue`) was added with Whisper large-v3. Groq serves large-v3
 * models and documents it; OpenAI's `whisper-1` is large-v2, and nothing
 * says it accepts `yue`, so OpenAI is not offered it.
 */
const GROQ_LANGUAGES: readonly string[] = [...WHISPER_LANGUAGES, "yue"];

const BY_PROVIDER: Readonly<Record<Provider, readonly string[]>> = {
  groq: GROQ_LANGUAGES,
  openai: WHISPER_LANGUAGES,
};

/** The languages a cloud provider's Whisper models accept. */
export function cloudLanguages(provider: Provider): readonly string[] {
  return BY_PROVIDER[provider];
}

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
 * Lowercase with accents and combining marks removed, so "francais" finds
 * Français and the displayed Devanagari name compares equal to itself. Both
 * the query and the names go through this, so marks never split a word.
 */
function fold(text: string): string {
  return text
    .normalize("NFD")
    .replace(/\p{M}+/gu, "")
    .toLowerCase();
}

/**
 * Whether `query` finds `code`: it starts a word of either name
 * ("hin" finds Hindi, not Chinese). Case, accents and Unicode normal form
 * don't matter.
 */
export function languageMatches(code: string, query: string): boolean {
  const q = fold(query.trim());
  if (q === "") return true;
  const names = NAMES[code] ?? [languageLabel(code)];
  return names.some((name) =>
    fold(name)
      .split(/[^\p{L}\p{N}]+/u)
      .some((word) => word.startsWith(q)),
  );
}

/** `codes` sorted by English name. */
export function sortedByName(codes: readonly string[]): string[] {
  return [...codes].sort((a, b) => englishName(a).localeCompare(englishName(b)));
}
